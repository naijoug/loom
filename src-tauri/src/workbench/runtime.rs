//! SDK scheduling for the real desktop Host; no frontend commands choose binaries.
use super::host::Host;
use loom_core::{
    ConnectionProvider, Core, FinishStatus, RuntimeBinding, SessionKey, Turn, TurnStatus,
};
use loom_runtime::{
    policy::{PermissionPolicy, PolicyScope},
    SessionRunOptions, WorkerCommand,
};
use std::{
    path::PathBuf,
    sync::{
        atomic::Ordering,
        {Arc, Mutex},
    },
    time::Duration,
};

struct ReaRunner {
    client: Mutex<Option<loom_runtime::ReaMcpClient>>,
}

impl ReaRunner {
    fn new() -> Result<Self, String> {
        let root = std::env::var_os("LOOM_REA_PACKAGE_ROOT")
            .map(PathBuf::from)
            .ok_or("REA 6.5.0 官方包路径未配置")?;
        let executable = loom_runtime::rea_executable(&root).map_err(|e| e.to_string())?;
        let client =
            loom_runtime::ReaMcpClient::start(&executable, std::time::Duration::from_secs(30))
                .map_err(|e| e.to_string())?;
        Ok(Self {
            client: Mutex::new(Some(client)),
        })
    }
}

impl loom_runtime::ExternalToolRunner for ReaRunner {
    fn call(
        &mut self,
        name: &str,
        input: &serde_json::Value,
    ) -> Result<serde_json::Value, loom_runtime::RuntimeError> {
        let mut guard = self.client.lock().map_err(|_| {
            loom_runtime::RuntimeError::new("rea_source", "REA client lock unavailable")
        })?;
        let Some(client) = guard.as_mut() else {
            return Err(loom_runtime::RuntimeError::new(
                "rea_source",
                "REA client already stopped",
            ));
        };
        let action = loom_runtime::split_tool_name(name)?;
        let mut result = client.call_tool(action, input, std::time::Duration::from_secs(30))?;
        // Keep the model-facing result compact; the host Evidence repository can
        // later retain the complete record without expanding every context.
        if let Some(content) = result
            .get_mut("structuredContent")
            .and_then(|value| value.as_object_mut())
        {
            content.retain(|key, _| matches!(key.as_str(), "result" | "_meta"));
        }
        Ok(result)
    }
}

impl Drop for ReaRunner {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.client.lock() {
            drop(guard.take());
        }
    }
}

#[derive(Clone)]
pub(super) struct SdkRuntime {
    pub(super) command: WorkerCommand,
    pub(super) home: PathBuf,
}
impl SdkRuntime {
    pub(super) fn development() -> Result<Option<Self>, String> {
        // Release packaging gets its own verified resource locator; never search
        // a user's cwd or let renderer input select an executable/script.
        if !cfg!(debug_assertions) || std::env::var("LOOM_WORKBENCH_SDK").as_deref() != Ok("1") {
            return Ok(None);
        }
        let executable = std::env::var_os("LOOM_WORKBENCH_BUN")
            .map(PathBuf::from)
            .ok_or("SDK 启动需要宿主配置的 Bun 绝对路径")?;
        let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../packages/provider-workers/src/runtime-worker.ts")
            .canonicalize()
            .map_err(|_| "SDK worker 文件不存在")?;
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or("SDK 运行缺少用户目录")?;
        let runtime = Self {
            command: WorkerCommand {
                executable,
                script,
                timeout: Duration::from_secs(15 * 60),
                exit_grace: Duration::from_secs(2),
            },
            home,
        };
        runtime.validate()?;
        Ok(Some(runtime))
    }
    fn validate(&self) -> Result<(), String> {
        for path in [&self.command.executable, &self.command.script] {
            if !path.is_absolute() || !path.is_file() {
                return Err("SDK 程序和 worker 必须是存在的绝对文件路径".into());
            }
        }
        if !self.home.is_absolute() {
            return Err("SDK 用户目录必须是绝对路径".into());
        }
        Ok(())
    }
    pub(super) fn schedule(&self, host: &Arc<Host>, key: SessionKey) {
        if host.stopping.load(Ordering::Acquire) {
            return;
        }
        let Ok(operation) = crate::process_supervisor::supervisor().begin_operation() else {
            return;
        };
        let Ok(mut running) = host.running.lock() else {
            return;
        };
        if !running.insert(key.clone()) {
            return;
        }
        let host = host.clone();
        let runtime = self.clone();
        drop(running);
        let failed_host = host.clone();
        let failed_key = key.clone();
        if std::thread::Builder::new()
            .name("loom-sdk-session".into())
            .spawn(move || {
                let _operation = operation;
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    runtime.queue(&host, &key)
                }));
                if result.is_err() {
                    host.fault("SDK 调度任务异常退出".into());
                    if let Ok(mut running) = host.running.lock() {
                        running.remove(&key);
                    }
                }
            })
            .is_err()
        {
            failed_host.fault("无法创建 SDK 调度线程".into());
            if let Ok(mut running) = failed_host.running.lock() {
                running.remove(&failed_key);
            }
        }
    }
    fn queue(&self, host: &Arc<Host>, key: &SessionKey) {
        loop {
            // Sending/scheduling and empty-queue release share this lock, so a
            // send arriving during cleanup cannot lose its wakeup.
            let Ok(mut running) = host.running.lock() else {
                return;
            };
            if host.stopping.load(Ordering::Acquire) {
                running.remove(key);
                return;
            }
            let view = match host.core.get_session(key) {
                Ok(v) => v,
                Err(e) => {
                    host.fault(e.to_string());
                    running.remove(key);
                    return;
                }
            };
            if view.metadata.archived || view.active_turn_id.is_some() {
                running.remove(key);
                return;
            }
            let Some(turn) = view
                .turns
                .iter()
                .find(|t| t.status == TurnStatus::Queued)
                .cloned()
            else {
                running.remove(key);
                return;
            };
            drop(running);
            match self.prepare(&host.core, key, &turn) {
                Ok((binding, mut policy)) => {
                    let lease = match host.core.claim_prepared_turn(key, &turn.id, binding) {
                        Ok(l) => l,
                        Err(e) => {
                            if Self::already_settled(&host.core, key, &turn) {
                                continue;
                            }
                            if let Err(failure) =
                                host.core.fail_queued_turn(key, &turn.id, e.to_string())
                            {
                                host.fault(failure.to_string());
                                Self::release(host, key);
                                return;
                            }
                            continue;
                        }
                    };
                    if host.stopping.load(Ordering::Acquire) {
                        let _ =
                            host.core
                                .settle_runtime_turn(&lease, FinishStatus::Cancelled, None);
                        Self::release(host, key);
                        return;
                    }
                    let connection = turn
                        .configuration
                        .resolved_connection
                        .as_deref()
                        .expect("prepared connection");
                    let secret = match host.connections.runtime_secret(connection) {
                        Ok(s) => s,
                        Err(e) => {
                            if let Err(error) =
                                host.core
                                    .settle_runtime_turn(&lease, FinishStatus::Failed, Some(e))
                            {
                                host.fault(error.to_string());
                                Self::release(host, key);
                                return;
                            }
                            continue;
                        }
                    };
                    let credential = secret
                        .as_ref()
                        .map(|s| std::str::from_utf8(s).expect("validated UTF-8 credential"));
                    let external_tools =
                        if loom_runtime::rea_source_active(&view.metadata.enabled_sources) {
                            match loom_runtime::rea_external_tools() {
                                Ok(tools) => tools,
                                Err(error) => {
                                    if let Err(failure) = host.core.settle_runtime_turn(
                                        &lease,
                                        FinishStatus::Failed,
                                        Some(error.to_string()),
                                    ) {
                                        host.fault(failure.to_string());
                                    }
                                    Self::release(host, key);
                                    return;
                                }
                            }
                        } else {
                            Vec::new()
                        };
                    let mut external_runner: Option<Box<dyn loom_runtime::ExternalToolRunner>> =
                        if external_tools.is_empty() {
                            None
                        } else {
                            match ReaRunner::new() {
                                Ok(runner) => Some(Box::new(runner)),
                                Err(error) => {
                                    if let Err(failure) = host.core.settle_runtime_turn(
                                        &lease,
                                        FinishStatus::Failed,
                                        Some(error),
                                    ) {
                                        host.fault(failure.to_string());
                                    }
                                    Self::release(host, key);
                                    return;
                                }
                            }
                        };
                    let tools = match connection.config.provider {
                        ConnectionProvider::Claude => {
                            vec!["read", "write", "edit", "bash", "glob", "grep"]
                        }
                        _ => vec!["read", "write", "edit", "bash", "find", "grep", "ls"],
                    }
                    .into_iter()
                    .map(str::to_owned)
                    .collect();
                    let result = loom_runtime::run_session(
                        &host.core,
                        lease,
                        &self.command,
                        SessionRunOptions {
                            run_id: turn.id.clone(),
                            tools,
                            external_tools,
                            max_turns: 100,
                            context_bytes: 768 * 1024,
                        },
                        credential,
                        host.stopping.clone(),
                        &mut policy,
                        external_runner.as_mut().map(|runner| {
                            &mut **runner as &mut dyn loom_runtime::ExternalToolRunner
                        }),
                    );
                    if result.is_err() && !Self::already_settled(&host.core, key, &turn) {
                        host.fault("SDK 回合终态未能保存，请检查执行记录".into());
                        Self::release(host, key);
                        return;
                    }
                }
                Err(error) => {
                    if Self::already_settled(&host.core, key, &turn) {
                        continue;
                    }
                    if let Err(e) = host.core.fail_queued_turn(key, &turn.id, error) {
                        host.fault(e.to_string());
                        Self::release(host, key);
                        return;
                    }
                }
            }
        }
    }
    fn release(host: &Host, key: &SessionKey) {
        if let Ok(mut running) = host.running.lock() {
            running.remove(key);
        }
    }
    fn already_settled(core: &Core, key: &SessionKey, turn: &Turn) -> bool {
        core.get_session(key).is_ok_and(|v| {
            v.turns
                .iter()
                .any(|t| t.id == turn.id && t.status.is_terminal())
        })
    }
    fn prepare(
        &self,
        core: &Core,
        key: &SessionKey,
        turn: &Turn,
    ) -> Result<(RuntimeBinding, PermissionPolicy), String> {
        self.validate()?;
        let connection = turn
            .configuration
            .resolved_connection
            .as_ref()
            .ok_or("请选择已保存的模型连接")?;
        let cwd = core
            .runtime_working_directory(key)
            .map_err(|e| e.to_string())?;
        let view = core.get_session(key).map_err(|e| e.to_string())?;
        let policy = PermissionPolicy::new(super::permissions::layers(core, key)?, {
            let mut scope = PolicyScope::native(PathBuf::from(&cwd), self.home.clone());
            if loom_runtime::rea_source_active(&view.metadata.enabled_sources) {
                scope
                    .built_in_sources
                    .insert(loom_core::Id::new("rea").unwrap());
            }
            scope
        })
        .map_err(|e| e.to_string())?;
        let stored = core
            .save_policy_snapshot(key, policy.snapshot())
            .map_err(|e| e.to_string())?;
        if stored != policy.snapshot_digest() {
            return Err("策略快照摘要不一致".into());
        }
        let mut selection = turn.configuration.connection.clone();
        if selection.model.is_none() {
            selection.model = Some(connection.config.default_model.clone());
        }
        let binding = RuntimeBinding {
            adapter: match connection.config.provider {
                ConnectionProvider::Claude => "claude-sdk",
                _ => "pi-sdk",
            }
            .into(),
            connection: selection,
            permission_mode: turn.configuration.permission_mode,
            policy_version: policy.version(),
            working_directory: Some(cwd),
            connection_snapshot_digest: Some(
                connection.snapshot_digest().map_err(|e| e.to_string())?,
            ),
            policy_snapshot_digest: Some(policy.snapshot_digest().into()),
        };
        Ok((binding, policy))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loom_core::*;
    use std::{fs, sync::atomic::AtomicU64, time::Instant};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "loom-sdk-host-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&p).unwrap();
            let p = p.canonicalize().unwrap();
            fs::write(p.join("worker.py"), PYTHON).unwrap();
            Self(p)
        }
        fn host(&self) -> Arc<Host> {
            let mut host = Host::open(&self.0.join("data"), false).unwrap();
            Arc::get_mut(&mut host)
                .unwrap()
                .configure_sdk(Some(SdkRuntime {
                    command: WorkerCommand {
                        executable: PathBuf::from("/usr/bin/python3"),
                        script: self.0.join("worker.py"),
                        timeout: Duration::from_secs(5),
                        exit_grace: Duration::from_millis(150),
                    },
                    home: self.0.clone(),
                }));
            host
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn id(v: &str) -> Id {
        Id::new(v).unwrap()
    }
    fn setup(host: &Host, mode: PermissionMode, connection: bool) -> SessionKey {
        host.core
            .create_workspace(id("w"), "Workspace".into())
            .unwrap();
        if connection {
            host.core
                .save_connection_metadata(SaveConnectionMetadata {
                    id: id("c"),
                    operation_id: id("save"),
                    expected_revision: None,
                    credential_ref: None,
                    config: ConnectionConfig {
                        name: "Fixture".into(),
                        provider: ConnectionProvider::PiCompat,
                        auth: ConnectionAuth::None,
                        endpoint: Some("http://127.0.0.1:1".into()),
                        pi_provider: None,
                        custom_api: Some(EndpointApi::OpenaiCompletions),
                        models: vec!["loom-fixture".into()],
                        default_model: "loom-fixture".into(),
                        supports_images: false,
                        mid_stream_behavior: MidStreamBehavior::Queue,
                    },
                })
                .unwrap();
        }
        let key = SessionKey {
            workspace_id: id("w"),
            session_id: id("s"),
        };
        host.core
            .create_session(CreateSession {
                key: key.clone(),
                title: None,
                project_id: None,
                permission_mode: mode,
                connection: ConnectionSelection {
                    connection_id: connection.then(|| id("c")),
                    ..Default::default()
                },
            })
            .unwrap();
        key
    }
    fn send(host: &Arc<Host>, key: &SessionKey, name: &str) -> SendReceipt {
        let reply = host
            .invoke(CoreRequest {
                protocol_version: 1,
                request_id: id("rpc"),
                command: CoreCommand::SendMessage(SendMessage {
                    key: key.clone(),
                    client_request_id: id(name),
                    text: name.into(),
                    attachment_ids: vec![],
                }),
            })
            .unwrap();
        match reply.outcome {
            Outcome::Success {
                result: CoreResponse::Send(sent),
            } => sent.receipt,
            _ => panic!("send failed"),
        }
    }
    fn until(mut p: impl FnMut() -> bool) {
        let end = Instant::now() + Duration::from_secs(15);
        while !p() {
            assert!(Instant::now() < end, "SDK host test timed out");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    fn view(host: &Host, key: &SessionKey) -> SessionView {
        host.core.get_session(key).unwrap()
    }
    fn directory(host: &Host, key: &SessionKey) -> PathBuf {
        PathBuf::from(host.core.runtime_working_directory(key).unwrap())
    }
    #[test]
    fn sdk_host_runs_accepted_queue_once_in_order_and_persists_failure_without_retry() {
        let f = Fixture::new();
        let host = f.host();
        let key = setup(&host, PermissionMode::Ask, true);
        let first = send(&host, &key, "first");
        send(&host, &key, "fail");
        send(&host, &key, "third");
        assert_eq!(send(&host, &key, "first"), first);
        until(|| {
            view(&host, &key)
                .turns
                .iter()
                .filter(|t| t.status.is_terminal())
                .count()
                == 3
        });
        host.drain(Duration::from_secs(2)).unwrap();
        let saved = view(&host, &key);
        assert_eq!(
            saved.turns.iter().map(|t| t.status).collect::<Vec<_>>(),
            vec![
                TurnStatus::Completed,
                TurnStatus::Failed,
                TurnStatus::Completed
            ]
        );
        assert_eq!(
            fs::read_to_string(directory(&host, &key).join("invocations")).unwrap(),
            "first\nfail\nthird\n"
        );
        drop(host);
        let restored = Core::open(f.0.join("data")).unwrap();
        assert_eq!(restored.get_session(&key).unwrap(), saved);
    }
    #[test]
    fn sdk_preparation_failure_is_terminal_without_a_process_or_execution_binding() {
        let f = Fixture::new();
        let host = f.host();
        let key = setup(&host, PermissionMode::Ask, false);
        send(&host, &key, "missing");
        until(|| view(&host, &key).turns[0].status == TurnStatus::Failed);
        let turn = &view(&host, &key).turns[0];
        assert!(turn.started_at_ms.is_none() && turn.binding.is_none());
        assert!(!directory(&host, &key).join("invocations").exists());
        host.drain(Duration::from_secs(2)).unwrap();
    }
    #[test]
    fn saved_permissions_apply_to_next_turn_and_each_turn_keeps_its_auditable_snapshot() {
        let f = Fixture::new();
        let host = f.host();
        let key = setup(&host, PermissionMode::Ask, true);
        send(&host, &key, "write");
        until(|| !view(&host, &key).approvals.is_empty());
        let first = view(&host, &key);
        let approval = first.approvals.values().next().unwrap().clone();
        let original = host
            .core
            .read_policy_snapshot(&key, &first.turns[0].id)
            .unwrap();
        super::super::permissions::save(
            &host.core,
            SavePermissionRequest {
                scope: PermissionScope::Workspace {
                    workspace_id: key.workspace_id.clone(),
                },
                operation_id: id("rules"),
                expected_revision: None,
                config: Some(
                    serde_json::json!({"allowedWritePaths":[directory(&host,&key).join("effect")]}),
                ),
            },
            host.root.clone(),
        )
        .unwrap();
        assert_eq!(
            view(&host, &key).approvals[&approval.id].status,
            ApprovalStatus::Pending
        );
        send(&host, &key, "write-again");
        host.invoke(CoreRequest {
            protocol_version: 1,
            request_id: id("deny"),
            command: CoreCommand::RespondToolApproval {
                key: key.clone(),
                request_id: approval.id,
                expected_digest: approval.binding_digest,
                allowed: false,
            },
        })
        .unwrap();
        until(|| {
            view(&host, &key)
                .turns
                .iter()
                .all(|t| t.status.is_terminal())
        });
        let saved = view(&host, &key);
        assert!(directory(&host, &key).join("effect").exists());
        assert_eq!(saved.approvals.len(), 1);
        let updated = host
            .core
            .read_policy_snapshot(&key, &saved.turns[1].id)
            .unwrap();
        assert_ne!(original, updated);
        assert_eq!(
            original["layers"]["workspace"]["allowedWritePaths"],
            serde_json::json!([])
        );
        assert_eq!(
            updated["layers"]["workspace"]["allowedWritePaths"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        host.drain(Duration::from_secs(2)).unwrap();
        drop(host);
        let core = Core::open(f.0.join("data")).unwrap();
        assert_eq!(
            core.read_policy_snapshot(&key, &saved.turns[0].id).unwrap(),
            original
        );
        assert_eq!(
            core.read_policy_snapshot(&key, &saved.turns[1].id).unwrap(),
            updated
        );
    }
    #[test]
    fn sdk_sessions_run_independently_while_one_waits_for_approval() {
        let f = Fixture::new();
        let host = f.host();
        let first = setup(&host, PermissionMode::Ask, true);
        let second = SessionKey {
            workspace_id: first.workspace_id.clone(),
            session_id: id("other"),
        };
        host.core
            .create_session(CreateSession {
                key: second.clone(),
                title: None,
                project_id: None,
                permission_mode: PermissionMode::Ask,
                connection: ConnectionSelection {
                    connection_id: Some(id("c")),
                    ..Default::default()
                },
            })
            .unwrap();
        send(&host, &first, "write");
        until(|| !view(&host, &first).approvals.is_empty());
        send(&host, &second, "independent");
        until(|| view(&host, &second).turns[0].status == TurnStatus::Completed);
        assert_eq!(view(&host, &first).turns[0].status, TurnStatus::Running);
        assert_ne!(directory(&host, &first), directory(&host, &second));
        host.drain(Duration::from_secs(3)).unwrap();
    }
    #[test]
    fn sdk_shutdown_revokes_pending_approval_and_leaves_the_rest_of_queue_unexecuted() {
        let f = Fixture::new();
        let host = f.host();
        let key = setup(&host, PermissionMode::Ask, true);
        send(&host, &key, "write");
        send(&host, &key, "queued");
        until(|| !view(&host, &key).approvals.is_empty());
        host.drain(Duration::from_secs(3)).unwrap();
        let saved = view(&host, &key);
        assert_eq!(saved.turns[0].status, TurnStatus::Cancelled);
        assert_eq!(saved.turns[1].status, TurnStatus::Queued);
        assert_eq!(
            saved.approvals.values().next().unwrap().status,
            ApprovalStatus::Cancelled
        );
        assert!(!directory(&host, &key).join("effect").exists());
        assert_eq!(
            fs::read_to_string(directory(&host, &key).join("invocations")).unwrap(),
            "write\n"
        );
        drop(host);
        let host = f.host();
        assert_eq!(view(&host, &key).turns[1].status, TurnStatus::Queued);
        host.schedule(key.clone());
        until(|| view(&host, &key).turns[1].status == TurnStatus::Completed);
        assert_eq!(
            fs::read_to_string(directory(&host, &key).join("invocations")).unwrap(),
            "write\nqueued\n"
        );
        host.drain(Duration::from_secs(2)).unwrap();
    }
    #[test]
    fn sdk_approval_and_cancellation_use_the_same_wire_as_the_desktop() {
        for allowed in [Some(true), Some(false), None] {
            let f = Fixture::new();
            let host = f.host();
            let key = setup(&host, PermissionMode::Ask, true);
            let first = send(&host, &key, "write");
            send(&host, &key, "after");
            until(|| !view(&host, &key).approvals.is_empty());
            let approval = view(&host, &key).approvals.values().next().unwrap().clone();
            assert!(!directory(&host, &key).join("effect").exists());
            let command = match allowed {
                Some(allowed) => CoreCommand::RespondToolApproval {
                    key: key.clone(),
                    request_id: approval.id,
                    expected_digest: approval.binding_digest,
                    allowed,
                },
                None => CoreCommand::CancelTurn {
                    key: key.clone(),
                    turn_id: first.turn_id,
                },
            };
            host.invoke(CoreRequest {
                protocol_version: 1,
                request_id: id("answer"),
                command,
            })
            .unwrap();
            until(|| {
                view(&host, &key)
                    .turns
                    .iter()
                    .all(|t| t.status.is_terminal())
            });
            assert_eq!(
                directory(&host, &key).join("effect").exists(),
                allowed == Some(true)
            );
            assert_eq!(view(&host, &key).turns[1].status, TurnStatus::Completed);
            host.drain(Duration::from_secs(2)).unwrap();
        }
    }
    #[test]
    fn sdk_remembered_folder_grant_reuses_only_this_session_and_clear_requires_a_new_prompt() {
        let f = Fixture::new();
        let host = f.host();
        let key = setup(&host, PermissionMode::Ask, true);
        send(&host, &key, "write");
        until(|| !view(&host, &key).approvals.is_empty());
        let approval = view(&host, &key).approvals.values().next().unwrap().clone();
        assert!(approval.remember.is_some());
        host.invoke(CoreRequest {
            protocol_version: 1,
            request_id: id("remember"),
            command: CoreCommand::RememberToolApproval {
                key: key.clone(),
                request_id: approval.id.clone(),
                expected_digest: approval.binding_digest,
            },
        })
        .unwrap();
        until(|| view(&host, &key).turns[0].status == TurnStatus::Completed);
        send(&host, &key, "write-second");
        until(|| {
            view(&host, &key).turns.len() == 2 && view(&host, &key).turns[1].status.is_terminal()
        });
        let saved = view(&host, &key);
        assert_eq!(saved.turns[1].status, TurnStatus::Completed);
        assert_eq!(saved.approvals.len(), 1);
        assert_eq!(saved.remembered_grants.len(), 1);
        assert!(host.core.read_events(&key,0,200).unwrap().events.iter().any(|e|matches!(&e.payload,SessionEvent::RuntimeApplied{update:RuntimeUpdate::ToolPolicy{authorization_ids,..},..}if authorization_ids==&vec![approval.id.clone()])));
        host.invoke(CoreRequest {
            protocol_version: 1,
            request_id: id("clear"),
            command: CoreCommand::ClearRememberedGrants { key: key.clone() },
        })
        .unwrap();
        send(&host, &key, "write-third");
        until(|| view(&host, &key).approvals.len() == 2);
        let pending = view(&host, &key)
            .approvals
            .values()
            .find(|a| a.status == ApprovalStatus::Pending)
            .unwrap()
            .clone();
        host.invoke(CoreRequest {
            protocol_version: 1,
            request_id: id("deny"),
            command: CoreCommand::RespondToolApproval {
                key: key.clone(),
                request_id: pending.id,
                expected_digest: pending.binding_digest,
                allowed: false,
            },
        })
        .unwrap();
        until(|| view(&host, &key).turns[2].status.is_terminal());
        host.drain(Duration::from_secs(3)).unwrap();
    }
    #[test]
    fn sdk_remembered_authority_is_not_restored_when_the_host_restarts() {
        let f = Fixture::new();
        let host = f.host();
        let key = setup(&host, PermissionMode::Ask, true);
        send(&host, &key, "write");
        until(|| !view(&host, &key).approvals.is_empty());
        let a = view(&host, &key).approvals.values().next().unwrap().clone();
        host.invoke(CoreRequest {
            protocol_version: 1,
            request_id: id("remember"),
            command: CoreCommand::RememberToolApproval {
                key: key.clone(),
                request_id: a.id,
                expected_digest: a.binding_digest,
            },
        })
        .unwrap();
        until(|| view(&host, &key).turns[0].status.is_terminal());
        host.drain(Duration::from_secs(3)).unwrap();
        drop(host);
        let host = f.host();
        assert!(view(&host, &key)
            .remembered_grants
            .values()
            .all(|g| g.revoked_at_ms.is_some()));
        send(&host, &key, "write-after-restart");
        until(|| view(&host, &key).approvals.len() == 2);
        host.drain(Duration::from_secs(3)).unwrap();
    }
    #[test]
    #[ignore = "Explicit real Pi SDK with injected fetch; two turns and no model account/network"]
    fn actual_pi_sdk_remembers_a_directory_across_two_turns() {
        let f = Fixture::new();
        let mut host = f.host();
        let bun = std::env::var_os("LOOM_TEST_BUN")
            .map(PathBuf::from)
            .expect("LOOM_TEST_BUN");
        let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../packages/provider-workers/tests/pi-fetch-fixture.ts")
            .canonicalize()
            .unwrap();
        Arc::get_mut(&mut host)
            .unwrap()
            .configure_sdk(Some(SdkRuntime {
                command: WorkerCommand {
                    executable: bun,
                    script,
                    timeout: Duration::from_secs(30),
                    exit_grace: Duration::from_secs(2),
                },
                home: f.0.clone(),
            }));
        let key = setup(&host, PermissionMode::Ask, true);
        send(&host, &key, "write");
        until(|| !view(&host, &key).approvals.is_empty());
        let a = view(&host, &key).approvals.values().next().unwrap().clone();
        host.invoke(CoreRequest {
            protocol_version: 1,
            request_id: id("remember"),
            command: CoreCommand::RememberToolApproval {
                key: key.clone(),
                request_id: a.id,
                expected_digest: a.binding_digest,
            },
        })
        .unwrap();
        until(|| view(&host, &key).turns[0].status.is_terminal());
        send(&host, &key, "write-again");
        until(|| {
            view(&host, &key).turns.len() == 2 && view(&host, &key).turns[1].status.is_terminal()
        });
        let v = view(&host, &key);
        assert!(v.turns.iter().all(|t| t.status == TurnStatus::Completed));
        assert_eq!(v.approvals.len(), 1);
        assert_eq!(v.tools.len(), 2);
        assert_eq!(
            fs::read_to_string(directory(&host, &key).join("probe.txt")).unwrap(),
            "loom-sdk-probe\n"
        );
        host.drain(Duration::from_secs(3)).unwrap();
    }
    #[test]
    #[ignore = "Explicit real Pi SDK integration with injected fetch; no HTTP listener or model account"]
    fn actual_pi_sdk_runs_through_desktop_host_modes_and_approvals() {
        for mode in [
            PermissionMode::Explore,
            PermissionMode::Ask,
            PermissionMode::Execute,
        ] {
            let f = Fixture::new();
            let mut host = f.host();
            let bun = std::env::var_os("LOOM_TEST_BUN")
                .map(PathBuf::from)
                .expect("LOOM_TEST_BUN must identify Bun");
            let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../packages/provider-workers/tests/pi-fetch-fixture.ts")
                .canonicalize()
                .unwrap();
            Arc::get_mut(&mut host)
                .unwrap()
                .configure_sdk(Some(SdkRuntime {
                    command: WorkerCommand {
                        executable: bun,
                        script,
                        timeout: Duration::from_secs(30),
                        exit_grace: Duration::from_secs(2),
                    },
                    home: f.0.clone(),
                }));
            let key = setup(&host, mode, true);
            send(&host, &key, "write");
            if mode == PermissionMode::Ask {
                until(|| !view(&host, &key).approvals.is_empty());
                let a = view(&host, &key).approvals.values().next().unwrap().clone();
                assert!(!directory(&host, &key).join("probe.txt").exists());
                host.invoke(CoreRequest {
                    protocol_version: 1,
                    request_id: id("approve"),
                    command: CoreCommand::RespondToolApproval {
                        key: key.clone(),
                        request_id: a.id,
                        expected_digest: a.binding_digest,
                        allowed: true,
                    },
                })
                .unwrap();
            }
            until(|| view(&host, &key).turns[0].status.is_terminal());
            let saved = view(&host, &key);
            assert_eq!(saved.turns[0].status, TurnStatus::Completed);
            assert_eq!(
                directory(&host, &key).join("probe.txt").exists(),
                mode != PermissionMode::Explore
            );
            if mode != PermissionMode::Explore {
                assert_eq!(
                    fs::read_to_string(directory(&host, &key).join("probe.txt")).unwrap(),
                    "loom-sdk-probe\n"
                );
            }
            assert!(saved.turns[0]
                .binding
                .as_ref()
                .unwrap()
                .policy_snapshot_digest
                .is_some());
            host.drain(Duration::from_secs(3)).unwrap();
        }
    }
    const PYTHON: &str = r#"
import json,sys,time,os
start=json.loads(sys.stdin.readline());config=start['config'];scope=config['scope'];seq=0;case=config['prompt']
with open('invocations','a') as f:f.write(case+'\n')
def emit(kind,**fields):
 global seq
 seq+=1
 print(json.dumps(dict(protocolVersion=start['protocolVersion'],scope=scope,seq=seq,event=dict(kind=kind,**fields))),flush=True)
emit('ready')
if case=='fail':emit('failed',code='fixture',message='Synthetic worker failure');sys.exit(1)
if case.startswith('write'):
 call='call-'+scope['turnId'];request='request-'+scope['turnId']
 emit('tool_request',requestId=request,toolCallId=call,name='write',input={'path':'effect','content':'authorized'})
 answer=json.loads(sys.stdin.readline())
 if answer['kind']=='tool_decision' and answer['decision']['allowed']:
  open('effect','w').write('authorized');emit('tool_result',toolCallId=call,failed=False,output={'ok':True})
else:time.sleep(.05)
emit('text',partId='reply',textKind='text',delta='reply '+case)
emit('completed',cancelled=False)
"#;
}
