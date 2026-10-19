use loom_core::{
    Core, CoreCommand, CoreReply, CoreRequest, FinishStatus, Id, Outcome, RuntimeBinding,
    RuntimeUpdate, SessionKey, SessionView, TextKind, TurnStatus, WatchMessage,
};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::RecvTimeoutError,
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

type WatchKey = (String, Id);
pub(super) struct Host {
    pub(super) root: std::path::PathBuf,
    pub(super) core: Core,
    pub(super) connections: super::connections::Connections,
    fixture: bool,
    approval_fixture: bool,
    pub(super) stopping: Arc<AtomicBool>,
    pub(super) running: Mutex<HashSet<SessionKey>>,
    sdk: Option<super::runtime::SdkRuntime>,
    watches: Mutex<HashMap<WatchKey, Arc<AtomicBool>>>,
    faults: Mutex<Vec<String>>,
}
impl Host {
    pub(super) fn open(root: &Path, fixture: bool) -> loom_core::Result<Arc<Self>> {
        let core = Core::open(root)?;
        let connections = super::connections::Connections::new(core.clone(), root)
            .map_err(std::io::Error::other)?;
        Ok(Arc::new(Self {
            root: root.canonicalize()?,
            core,
            connections,
            fixture,
            approval_fixture: fixture
                && cfg!(debug_assertions)
                && std::env::var("LOOM_WORKBENCH_APPROVAL_FIXTURE").as_deref() == Ok("1"),
            stopping: Arc::new(AtomicBool::new(false)),
            sdk: None,
            running: Mutex::new(HashSet::new()),
            watches: Mutex::new(HashMap::new()),
            faults: Mutex::new(Vec::new()),
        }))
    }
    #[cfg(test)]
    pub(super) fn fixture_enabled(&self) -> bool {
        self.fixture
    }
    pub(super) fn configure_sdk(&mut self, sdk: Option<super::runtime::SdkRuntime>) {
        self.sdk = sdk;
    }
    pub(super) fn runtime_name(&self) -> &'static str {
        if self.fixture {
            "fixture"
        } else if self.sdk.is_some() {
            "sdk"
        } else {
            "unavailable"
        }
    }
    pub(super) fn runtime_available(&self) -> bool {
        self.fixture || self.sdk.is_some()
    }
    pub(super) fn invoke(self: &Arc<Self>, request: CoreRequest) -> Result<CoreReply, String> {
        let _operation = crate::process_supervisor::supervisor().begin_operation()?;
        if self.stopping.load(Ordering::Acquire) {
            return Err("Workbench is shutting down".into());
        }
        let key = match &request.command {
            CoreCommand::SendMessage(input) => Some(input.key.clone()),
            _ => None,
        };
        let reply = self.core.dispatch(request);
        if self.runtime_available() && matches!(reply.outcome, Outcome::Success { .. }) {
            if let Some(key) = key {
                self.schedule(key);
            }
        }
        Ok(reply)
    }
    pub(super) fn watch(
        self: &Arc<Self>,
        window: String,
        id: Id,
        key: SessionKey,
        send: impl Fn(WatchMessage) -> Result<(), String> + Send + 'static,
    ) -> Result<SessionView, String> {
        if self.stopping.load(Ordering::Acquire) {
            return Err("Workbench is shutting down".into());
        }
        let watch_key = (window, id);
        let cancelled = Arc::new(AtomicBool::new(false));
        {
            let mut watches = self
                .watches
                .lock()
                .map_err(|_| "watch registry unavailable")?;
            if watches.contains_key(&watch_key) {
                return Err("watch ID already exists".into());
            }
            if watches.len() >= 128 {
                return Err("too many active watches".into());
            }
            watches.insert(watch_key.clone(), cancelled.clone());
        }
        let subscription = match self.core.subscribe(&key) {
            Ok(subscription) => subscription,
            Err(error) => {
                self.unwatch(&watch_key.0, &watch_key.1);
                return Err(error.to_string());
            }
        };
        let snapshot = subscription.snapshot.clone();
        let host = Arc::downgrade(self);
        std::thread::spawn(move || {
            loop {
                if cancelled.load(Ordering::Acquire) {
                    break;
                }
                match subscription.events.recv_timeout(Duration::from_millis(100)) {
                    Ok(event) => {
                        if cancelled.load(Ordering::Acquire)
                            || send(WatchMessage::Event {
                                event: Box::new(event),
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => continue,
                    Err(RecvTimeoutError::Disconnected) => {
                        let _ = send(WatchMessage::ResyncRequired);
                        break;
                    }
                }
            }
            drop(subscription);
            if let Some(host) = host.upgrade() {
                if let Ok(mut watches) = host.watches.lock() {
                    if watches
                        .get(&watch_key)
                        .is_some_and(|token| Arc::ptr_eq(token, &cancelled))
                    {
                        watches.remove(&watch_key);
                    }
                }
            }
        });
        Ok(snapshot)
    }
    pub(super) fn watch_workspace(
        self: &Arc<Self>,
        window: String,
        id: Id,
        workspace_id: Id,
        send: impl Fn(WatchMessage) -> Result<(), String> + Send + 'static,
    ) -> Result<(), String> {
        if self.stopping.load(Ordering::Acquire) {
            return Err("Workbench is shutting down".into());
        }
        let watch_key = (window, id);
        let cancelled = Arc::new(AtomicBool::new(false));
        {
            let mut watches = self
                .watches
                .lock()
                .map_err(|_| "watch registry unavailable")?;
            if watches.contains_key(&watch_key) || watches.len() >= 128 {
                return Err("duplicate watch or watch limit".into());
            }
            watches.insert(watch_key.clone(), cancelled.clone());
        }
        let subscription = match self.core.subscribe_workspace(&workspace_id) {
            Ok(subscription) => subscription,
            Err(error) => {
                self.unwatch(&watch_key.0, &watch_key.1);
                return Err(error.to_string());
            }
        };
        let host = Arc::downgrade(self);
        std::thread::spawn(move || {
            loop {
                if cancelled.load(Ordering::Acquire) {
                    break;
                }
                match subscription.events.recv_timeout(Duration::from_millis(100)) {
                    Ok(notice) => {
                        if cancelled.load(Ordering::Acquire)
                            || send(WatchMessage::WorkspaceChanged { notice }).is_err()
                        {
                            break;
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => continue,
                    Err(RecvTimeoutError::Disconnected) => {
                        let _ = send(WatchMessage::ResyncRequired);
                        break;
                    }
                }
            }
            drop(subscription);
            if let Some(host) = host.upgrade() {
                if let Ok(mut watches) = host.watches.lock() {
                    if watches
                        .get(&watch_key)
                        .is_some_and(|token| Arc::ptr_eq(token, &cancelled))
                    {
                        watches.remove(&watch_key);
                    }
                }
            }
        });
        Ok(())
    }
    pub(super) fn unwatch(&self, window: &str, id: &Id) {
        if let Ok(mut watches) = self.watches.lock() {
            if let Some(token) = watches.remove(&(window.to_owned(), id.clone())) {
                token.store(true, Ordering::Release);
            }
        }
    }
    pub(super) fn close_window(&self, window: &str) {
        if let Ok(mut watches) = self.watches.lock() {
            watches.retain(|(label, _), token| {
                if label == window {
                    token.store(true, Ordering::Release);
                    false
                } else {
                    true
                }
            });
        }
    }
    pub(super) fn fault(&self, error: String) {
        if let Ok(mut faults) = self.faults.lock() {
            if faults.len() < 100 {
                faults.push(error);
            }
        }
    }
    pub(super) fn schedule(self: &Arc<Self>, key: SessionKey) {
        if let Some(sdk) = &self.sdk {
            sdk.schedule(self, key);
            return;
        }
        if !self.fixture || self.stopping.load(Ordering::Acquire) {
            return;
        }
        let Ok(operation) = crate::process_supervisor::supervisor().begin_operation() else {
            return;
        };
        let Ok(mut running) = self.running.lock() else {
            return;
        };
        if !running.insert(key.clone()) {
            return;
        }
        let host = self.clone();
        drop(running);
        std::thread::spawn(move || {
            let _operation = operation;
            loop {
                // Serialize queue-empty observation with scheduling so a new send
                // cannot lose its wake-up between inspection and owner removal.
                let Ok(mut running) = host.running.lock() else {
                    return;
                };
                if host.stopping.load(Ordering::Acquire) {
                    running.remove(&key);
                    return;
                }
                let view = match host.core.get_session(&key) {
                    Ok(view) => view,
                    Err(error) => {
                        host.fault(error.to_string());
                        running.remove(&key);
                        return;
                    }
                };
                if view.metadata.archived {
                    running.remove(&key);
                    return;
                }
                let Some(turn) = view.turns.iter().find(|t| t.status == TurnStatus::Queued) else {
                    running.remove(&key);
                    return;
                };
                let directory = view
                    .metadata
                    .project_id
                    .as_ref()
                    .and_then(|id| {
                        host.core
                            .list_projects(&key.workspace_id)
                            .ok()?
                            .into_iter()
                            .find(|p| &p.id == id)
                    })
                    .map(|p| p.directory);
                let connection_snapshot_digest = match turn
                    .configuration
                    .resolved_connection
                    .as_ref()
                    .map(|connection| connection.snapshot_digest())
                    .transpose()
                {
                    Ok(digest) => digest,
                    Err(error) => {
                        host.fault(error.to_string());
                        running.remove(&key);
                        return;
                    }
                };
                let mut selection = turn.configuration.connection.clone();
                if selection.model.is_none() {
                    selection.model = turn
                        .configuration
                        .resolved_connection
                        .as_ref()
                        .map(|c| c.config.default_model.clone());
                }
                let binding = RuntimeBinding {
                    adapter: "desktop-fixture".into(),
                    connection: selection,
                    permission_mode: turn.configuration.permission_mode,
                    policy_version: 1,
                    working_directory: directory,
                    connection_snapshot_digest,
                    policy_snapshot_digest: None,
                };
                let lease = match host.core.claim_next_turn(&key, binding) {
                    Ok(lease) => lease,
                    Err(error) => {
                        host.fault(error.to_string());
                        running.remove(&key);
                        return;
                    }
                };
                drop(running);
                let approval_error = if host.approval_fixture
                    && matches!(
                        lease_configuration(&host.core, &key),
                        Some(loom_core::PermissionMode::Ask | loom_core::PermissionMode::Guarded)
                    ) {
                    host.run_approval_fixture(&lease).err()
                } else {
                    None
                };
                let response="这是本地验证模式生成的测试回复。\n\n工作区、会话、流式事件和历史记录均由新的 Rust 内核保存；这段回复没有调用真实模型。";
                let mut cancelled = false;
                let mut failed = approval_error;
                for (index, ch) in response.chars().enumerate() {
                    if host.stopping.load(Ordering::Acquire)
                        || lease.cancel_requested().unwrap_or(true)
                    {
                        cancelled = true;
                        break;
                    }
                    if failed.is_some() {
                        break;
                    }
                    let update = RuntimeUpdate::TextDelta {
                        part_id: Id::new("reply").unwrap(),
                        text_kind: TextKind::Text,
                        delta: ch.to_string(),
                    };
                    if let Err(error) = host.core.apply_runtime(
                        &lease,
                        Id::new(format!("delta-{index}")).unwrap(),
                        update,
                    ) {
                        if lease.cancel_requested().unwrap_or(false) {
                            cancelled = true;
                        } else {
                            failed = Some(error.to_string());
                        }
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(18));
                }
                let status = if cancelled {
                    FinishStatus::Cancelled
                } else if failed.is_some() {
                    FinishStatus::Failed
                } else {
                    FinishStatus::Completed
                };
                if let Err(error) = host.core.finish_turn(&lease, status, failed) {
                    if lease.cancel_requested().unwrap_or(false) {
                        if let Err(error) =
                            host.core.finish_turn(&lease, FinishStatus::Cancelled, None)
                        {
                            host.fault(error.to_string());
                        }
                    } else {
                        host.fault(error.to_string());
                    }
                }
            }
        });
    }
    pub(super) fn begin_shutdown(&self) {
        self.stopping.store(true, Ordering::Release);
        if let Ok(mut watches) = self.watches.lock() {
            for (_, token) in watches.drain() {
                token.store(true, Ordering::Release);
            }
        }
    }
    fn run_approval_fixture(&self, lease: &loom_core::TurnLease) -> Result<(), String> {
        let spec = loom_core::ToolApprovalSpec {
            request_id: Id::new(format!("approval-{}", lease.turn_id()))
                .map_err(|e| e.to_string())?,
            tool_call_id: Id::new(format!("tool-{}", lease.turn_id()))
                .map_err(|e| e.to_string())?,
            tool_name: "FixtureApproval · 无副作用测试".into(),
            input: serde_json::json!({"operation":"仅验证允许/拒绝，不写文件、不执行命令","api_key":"fixture-only-secret"}),
            ttl_ms: 120_000,
            remember: Some(loom_core::RememberOffer {
                keys: vec![loom_core::RememberKey::new(
                    loom_core::RememberKind::Tool,
                    "fixture-approval",
                    "无副作用测试的同类请求".into(),
                )
                .map_err(|e| e.to_string())?],
            }),
        };
        if self
            .core
            .try_remembered_tool(
                lease,
                Id::new(format!("remembered-{}", lease.turn_id())).map_err(|e| e.to_string())?,
                &spec,
                &[],
            )
            .map_err(|e| e.to_string())?
        {
            self.core
                .apply_runtime(
                    lease,
                    Id::new(format!("result-{}", lease.turn_id())).map_err(|e| e.to_string())?,
                    RuntimeUpdate::ToolFinished {
                        tool_call_id: spec.tool_call_id,
                        status: loom_core::ToolStatus::Succeeded,
                        output: serde_json::json!({"fixture":true,"sideEffects":false}),
                    },
                )
                .map_err(|e| e.to_string())?;
            return Ok(());
        }
        self.core
            .request_tool_approval(lease, &spec)
            .map_err(|e| e.to_string())?;
        loop {
            if self.stopping.load(Ordering::Acquire)
                || lease.cancel_requested().map_err(|e| e.to_string())?
            {
                return Ok(());
            }
            let view = self
                .core
                .get_session(lease.key())
                .map_err(|e| e.to_string())?;
            match view.approvals[&spec.request_id].status {
                loom_core::ApprovalStatus::Pending => std::thread::sleep(Duration::from_millis(30)),
                loom_core::ApprovalStatus::Allowed => {
                    let permit = match self.core.consume_tool_approval(lease, &spec) {
                        Ok(permit) => permit,
                        Err(_) if lease.cancel_requested().unwrap_or(true) => return Ok(()),
                        Err(e) => return Err(e.to_string()),
                    };
                    if !permit.is_live().map_err(|e| e.to_string())? {
                        return Ok(());
                    }
                    self.core
                        .apply_runtime(
                            lease,
                            Id::new(format!("result-{}", lease.turn_id()))
                                .map_err(|e| e.to_string())?,
                            RuntimeUpdate::ToolFinished {
                                tool_call_id: spec.tool_call_id.clone(),
                                status: loom_core::ToolStatus::Succeeded,
                                output: serde_json::json!({"fixture":true,"sideEffects":false}),
                            },
                        )
                        .map_err(|e| e.to_string())?;
                    return Ok(());
                }
                _ => return Ok(()),
            }
        }
    }
    pub(super) fn drain(&self, budget: Duration) -> Result<(), String> {
        self.begin_shutdown();
        let deadline = Instant::now() + budget;
        loop {
            if self
                .running
                .lock()
                .map_err(|_| "runtime registry unavailable")?
                .is_empty()
            {
                break;
            }
            if Instant::now() >= deadline {
                return Err("runtime did not settle before exit budget".into());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let faults = self
            .faults
            .lock()
            .map_err(|_| "runtime faults unavailable")?;
        if !faults.is_empty() {
            return Err(faults.join("; "));
        }
        Ok(())
    }
}
fn lease_configuration(core: &Core, key: &SessionKey) -> Option<loom_core::PermissionMode> {
    let view = core.get_session(key).ok()?;
    view.turns
        .iter()
        .find(|turn| Some(&turn.id) == view.active_turn_id.as_ref())
        .map(|turn| turn.configuration.permission_mode)
}

#[cfg(test)]
mod tests {
    use super::*;
    use loom_core::{
        ConnectionSelection, CreateSession, PermissionMode, SendMessage, SessionPatch,
    };
    use std::sync::atomic::AtomicU64;
    static NEXT: AtomicU64 = AtomicU64::new(1);
    struct Fixture(std::path::PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "loom-host-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn id(s: &str) -> Id {
        Id::new(s).unwrap()
    }
    fn prepare(host: &Host) -> SessionKey {
        host.core
            .create_workspace(id("w"), "Workspace".into())
            .unwrap();
        let key = SessionKey {
            workspace_id: id("w"),
            session_id: id("s"),
        };
        host.core
            .create_session(CreateSession {
                key: key.clone(),
                title: None,
                project_id: None,
                permission_mode: PermissionMode::Ask,
                connection: ConnectionSelection::default(),
            })
            .unwrap();
        key
    }
    fn send(key: &SessionKey, request_id: &str) -> CoreRequest {
        CoreRequest {
            protocol_version: 1,
            request_id: id("rpc"),
            command: CoreCommand::SendMessage(SendMessage {
                key: key.clone(),
                client_request_id: id(request_id),
                text: "测试消息".into(),
                attachment_ids: vec![],
            }),
        }
    }
    fn until(mut predicate: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(12);
        while !predicate() {
            assert!(Instant::now() < deadline, "host test timed out");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    #[test]
    fn fixture_is_opt_in_and_wire_requests_cannot_enable_it() {
        let f = Fixture::new();
        let host = Host::open(&f.0, false).unwrap();
        let key = prepare(&host);
        host.invoke(send(&key, "send")).unwrap();
        host.schedule(key.clone());
        assert!(host.running.lock().unwrap().is_empty());
        assert_eq!(
            host.core.get_session(&key).unwrap().turns[0].status,
            TurnStatus::Queued
        );
        assert!(!host.fixture_enabled());
        host.drain(Duration::from_secs(1)).unwrap();
    }
    #[test]
    fn approval_fixture_waits_for_a_real_core_decision_and_shutdown_revokes_it() {
        for allowed in [Some(true), Some(false), None] {
            let f = Fixture::new();
            let mut host = Host::open(&f.0, true).unwrap();
            Arc::get_mut(&mut host).unwrap().approval_fixture = true;
            let key = prepare(&host);
            host.invoke(send(&key, "approval-send")).unwrap();
            until(|| !host.core.get_session(&key).unwrap().approvals.is_empty());
            let view = host.core.get_session(&key).unwrap();
            let approval = view.approvals.values().next().unwrap();
            assert!(view.tools.is_empty());
            if let Some(allowed) = allowed {
                host.invoke(CoreRequest {
                    protocol_version: 1,
                    request_id: id("decision"),
                    command: CoreCommand::RespondToolApproval {
                        key: key.clone(),
                        request_id: approval.id.clone(),
                        expected_digest: approval.binding_digest.clone(),
                        allowed,
                    },
                })
                .unwrap();
                until(|| {
                    host.core.get_session(&key).unwrap().turns[0]
                        .status
                        .is_terminal()
                });
                let view = host.core.get_session(&key).unwrap();
                assert_eq!(
                    view.approvals.values().next().unwrap().status,
                    if allowed {
                        loom_core::ApprovalStatus::Consumed
                    } else {
                        loom_core::ApprovalStatus::Denied
                    }
                );
                assert_eq!(view.tools.len(), usize::from(allowed));
            }
            host.drain(Duration::from_secs(2)).unwrap();
            let view = host.core.get_session(&key).unwrap();
            assert!(view.active_turn_id.is_none());
            if allowed.is_none() {
                assert_eq!(
                    view.approvals.values().next().unwrap().status,
                    loom_core::ApprovalStatus::Cancelled
                );
            }
        }
    }
    #[test]
    fn desktop_fixture_uses_core_receipts_stream_and_terminal_persistence() {
        let f = Fixture::new();
        let host = Host::open(&f.0, true).unwrap();
        let key = prepare(&host);
        let (tx, rx) = std::sync::mpsc::channel();
        let snapshot = host
            .watch("main".into(), id("watch"), key.clone(), move |event| {
                tx.send(event).map_err(|e| e.to_string())
            })
            .unwrap();
        assert_eq!(snapshot.last_seq, 1);
        host.invoke(send(&key, "send")).unwrap();
        host.invoke(send(&key, "send")).unwrap();
        until(|| {
            host.core
                .get_session(&key)
                .unwrap()
                .turns
                .first()
                .is_some_and(|turn| turn.status == TurnStatus::Completed)
        });
        let view = host.core.get_session(&key).unwrap();
        assert_eq!(view.turns.len(), 1);
        assert_eq!(view.messages.len(), 2);
        assert!(!view.messages[1].parts.is_empty());
        let events: Vec<_> = rx
            .try_iter()
            .filter_map(|msg| match msg {
                WatchMessage::Event { event } => Some(event),
                _ => None,
            })
            .collect();
        assert!(events.len() > 5);
        assert!(events.windows(2).all(|pair| pair[1].seq == pair[0].seq + 1));
        host.drain(Duration::from_secs(1)).unwrap();
        drop(host);
        let restored = Core::open(&f.0).unwrap();
        assert_eq!(restored.get_session(&key).unwrap(), view);
    }
    #[test]
    fn watch_cleanup_is_window_scoped_and_old_generation_cannot_remove_new_watch() {
        let f = Fixture::new();
        let host = Host::open(&f.0, false).unwrap();
        let key = prepare(&host);
        host.watch("main".into(), id("watch"), key.clone(), |_| Ok(()))
            .unwrap();
        host.unwatch("different-window", &id("watch"));
        assert_eq!(host.watches.lock().unwrap().len(), 1);
        host.unwatch("main", &id("watch"));
        host.watch("main".into(), id("watch"), key.clone(), |_| Ok(()))
            .unwrap();
        std::thread::sleep(Duration::from_millis(160));
        assert_eq!(host.watches.lock().unwrap().len(), 1);
        host.close_window("main");
        until(|| host.watches.lock().unwrap().is_empty());
        host.drain(Duration::from_secs(1)).unwrap();
    }
    #[test]
    fn shutdown_settles_current_turn_and_does_not_run_remaining_queue() {
        let f = Fixture::new();
        let host = Host::open(&f.0, true).unwrap();
        let key = prepare(&host);
        host.invoke(send(&key, "first")).unwrap();
        until(|| {
            host.core
                .get_session(&key)
                .unwrap()
                .active_turn_id
                .is_some()
        });
        host.invoke(send(&key, "second")).unwrap();
        host.drain(Duration::from_secs(3)).unwrap();
        let view = host.core.get_session(&key).unwrap();
        assert_eq!(view.turns[0].status, TurnStatus::Cancelled);
        assert_eq!(view.turns[1].status, TurnStatus::Queued);
        assert_eq!(view.active_turn_id, None);
        assert!(host.invoke(send(&key, "third")).is_err());
    }
    #[test]
    fn archived_duplicate_send_cannot_create_a_runtime_fault() {
        let f = Fixture::new();
        let host = Host::open(&f.0, true).unwrap();
        let key = prepare(&host);
        host.core
            .send_message(match send(&key, "send").command {
                CoreCommand::SendMessage(input) => input,
                _ => unreachable!(),
            })
            .unwrap();
        let view = host.core.get_session(&key).unwrap();
        host.core
            .patch_session(
                &key,
                view.last_seq,
                SessionPatch {
                    archived: Some(true),
                    ..Default::default()
                },
            )
            .unwrap();
        host.invoke(send(&key, "send")).unwrap();
        until(|| host.running.lock().unwrap().is_empty());
        assert_eq!(
            host.core.get_session(&key).unwrap().turns[0].status,
            TurnStatus::Queued
        );
        host.drain(Duration::from_secs(1)).unwrap();
    }
}
