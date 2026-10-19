//! Synthetic integration policy for the real supervisor and SDK worker.
//! Never used as the application's production permission classifier.
use loom_core::*;
use loom_runtime::{
    policy::{PermissionPolicy, PolicyLayers, PolicyScope, RulePattern},
    Authorization, RuntimeError, SessionRunOptions, ToolPolicy, WorkerCommand,
};
use serde_json::json;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
fn id(s: &str) -> Id {
    Id::new(s).unwrap()
}
struct FixturePolicy(PermissionPolicy);
impl ToolPolicy for FixturePolicy {
    fn remember_offer(
        &self,
        context: &RuntimeContext,
        name: &str,
        input: &serde_json::Value,
    ) -> std::result::Result<Option<RememberOffer>, RuntimeError> {
        self.0.remember_offer(context, name, input)
    }
    fn validate_context(&self, context: &RuntimeContext) -> std::result::Result<(), RuntimeError> {
        self.0.validate_context(context)
    }
    fn evaluate(
        &mut self,
        context: &RuntimeContext,
        name: &str,
        input: &serde_json::Value,
    ) -> std::result::Result<Authorization, RuntimeError> {
        let path = input
            .get("file_path")
            .or_else(|| input.get("path"))
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| RuntimeError::new("fixture", "missing write path"))?;
        let cwd = std::path::Path::new(&context.working_directory);
        let target = if std::path::Path::new(path).is_absolute() {
            PathBuf::from(path)
        } else {
            cwd.join(path)
        };
        if name != "write"
            || target != cwd.join("probe.txt")
            || input["content"] != "loom-sdk-probe\n"
        {
            return Err(RuntimeError::new(
                "fixture",
                "tool outside authorized fixture scope",
            ));
        }
        self.0.evaluate(context, name, input)
    }
}
// Models a real independent UI client. The production handler owns approval
// consumption, worker replies, normalized events and terminal persistence.
fn answer_approval(
    core: &Core,
    key: &SessionKey,
    turn_id: &Id,
    cwd: &std::path::Path,
    scenario: &str,
    done: &AtomicBool,
) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    while !done.load(Ordering::Acquire) {
        let view = core.get_session(key)?;
        if let Some(approval) = view
            .approvals
            .values()
            .find(|a| a.status == ApprovalStatus::Pending)
        {
            println!(
                "{}",
                json!({"kind":"approvalRequested","requestId":approval.id,"toolCallId":approval.tool_call_id,"toolName":approval.tool_name,"bindingDigest":approval.binding_digest})
            );
            std::thread::sleep(Duration::from_millis(80));
            if cwd.join("probe.txt").exists() {
                return Err("SDK side effect before Rust authorization".into());
            }
            if scenario == "cancel" {
                core.cancel_turn(key, turn_id)?;
            } else {
                if scenario == "stale" {
                    if core
                        .respond_tool_approval(key, &approval.id, "stale-binding", true)
                        .is_ok()
                    {
                        return Err("stale approval accepted".into());
                    }
                    println!(
                        "{}",
                        json!({"kind":"protocolError","message":"Core rejected stale approval binding"})
                    );
                }
                core.respond_tool_approval(
                    key,
                    &approval.id,
                    &approval.binding_digest,
                    scenario != "deny",
                )?;
            }
            println!(
                "{}",
                json!({"kind":"approvalSettled","allowed":!matches!(scenario, "deny" | "cancel")})
            );
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Err("runtime ended without requesting an approval".into())
}
fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if !(6..=7).contains(&args.len()) {
        return Err("Usage: sdk_runtime_probe <bun> <worker> <claude|pi> <loopback endpoint> <cwd> <allow|deny|cancel|stale>".into());
    }
    if !["claude", "pi"].contains(&args[2].as_str())
        || !args[3].starts_with("http://127.0.0.1:")
        || ![
            "allow",
            "deny",
            "cancel",
            "stale",
            "explore",
            "execute",
            "allowlist",
        ]
        .contains(&args[5].as_str())
    {
        return Err("invalid synthetic fixture configuration".into());
    }
    let anonymous = args.get(6).is_some_and(|value| value == "none");
    if anonymous && args[2] != "pi" {
        return Err("anonymous fixture requires Pi".into());
    }
    let literal = args.get(6).is_some_and(|value| value == "literal");
    let credential = if literal {
        "!printf should-not-execute > credential-effect"
    } else {
        "loom-fixture-not-secret"
    };
    let cwd = PathBuf::from(&args[4]).canonicalize()?;
    let core = Core::open(cwd.join("runtime-core"))?;
    core.create_workspace(id("workspace"), "SDK runtime fixture".into())?;
    core.register_project(id("workspace"), id("project"), "Fixture".into(), &cwd)?;
    let config = ConnectionConfig {
        name: "SDK fixture".into(),
        provider: if args[2] == "claude" {
            ConnectionProvider::Claude
        } else {
            ConnectionProvider::PiCompat
        },
        auth: if anonymous {
            ConnectionAuth::None
        } else {
            ConnectionAuth::ApiKey
        },
        endpoint: Some(args[3].clone()),
        pi_provider: None,
        custom_api: if args[2] == "pi" {
            Some(EndpointApi::OpenaiCompletions)
        } else {
            None
        },
        models: vec![if args[2] == "claude" {
            "claude-sonnet-4-6"
        } else {
            "loom-fixture"
        }
        .into()],
        default_model: if args[2] == "claude" {
            "claude-sonnet-4-6"
        } else {
            "loom-fixture"
        }
        .into(),
        supports_images: false,
        mid_stream_behavior: MidStreamBehavior::Queue,
    };
    let connection = core.save_connection_metadata(SaveConnectionMetadata {
        id: id("connection"),
        operation_id: id("create"),
        expected_revision: None,
        config: config.clone(),
        credential_ref: if anonymous {
            None
        } else {
            Some(id("synthetic-key"))
        },
    })?;
    let key = SessionKey {
        workspace_id: id("workspace"),
        session_id: id("session"),
    };
    let selection = ConnectionSelection {
        connection_id: Some(connection.id.clone()),
        model: Some(config.default_model.clone()),
        thinking: None,
    };
    let automatic = ["explore", "execute", "allowlist"].contains(&args[5].as_str());
    let mode = match args[5].as_str() {
        "explore" => PermissionMode::Explore,
        "execute" => PermissionMode::Execute,
        _ => PermissionMode::Ask,
    };
    let mut layers = PolicyLayers::default();
    if args[5] == "allowlist" {
        layers.workspace.allowed_write_paths.push(RulePattern::Text(
            cwd.join("probe.txt").to_string_lossy().into(),
        ));
    }
    let mut policy = FixturePolicy(PermissionPolicy::new(
        layers,
        PolicyScope::native(cwd.clone(), cwd.clone()),
    )?);
    core.create_session(CreateSession {
        key: key.clone(),
        title: None,
        project_id: Some(id("project")),
        permission_mode: mode,
        connection: selection.clone(),
    })?;
    let binding = RuntimeBinding {
        adapter: format!("runtime-{}", args[2]),
        connection: selection,
        permission_mode: mode,
        policy_version: 1,
        working_directory: Some(cwd.to_string_lossy().into()),
        connection_snapshot_digest: Some(connection.snapshot_digest()?),
        policy_snapshot_digest: Some(policy.0.snapshot_digest().into()),
    };
    // Seed a completed historical turn to test actual SDK request assembly.
    core.send_message(SendMessage {
        key: key.clone(),
        client_request_id: id("history"),
        text: "history-user-marker".into(),
        attachment_ids: vec![],
    })?;
    let prior = core.claim_next_turn(&key, binding.clone())?;
    core.apply_runtime(
        &prior,
        id("history-answer"),
        RuntimeUpdate::TextDelta {
            part_id: id("answer"),
            text_kind: TextKind::Text,
            delta: "history-answer-marker".into(),
        },
    )?;
    core.finish_turn(&prior, FinishStatus::Completed, None)?;
    drop(prior);
    let prompt="Write exactly loom-sdk-probe followed by a newline to probe.txt using the write tool, then report the result.";
    let receipt = core
        .send_message(SendMessage {
            key: key.clone(),
            client_request_id: id("send"),
            text: prompt.into(),
            attachment_ids: vec![],
        })?
        .receipt;
    // A subsequent global edit must not redirect the accepted turn.
    let mut changed = config.clone();
    changed.endpoint = Some("http://127.0.0.1:2".into());
    changed.models.push("future-model".into());
    changed.default_model = "future-model".into();
    core.save_connection_metadata(SaveConnectionMetadata {
        id: connection.id.clone(),
        operation_id: id("changed"),
        expected_revision: Some(1),
        config: changed,
        credential_ref: connection.credential_ref.clone(),
    })?;
    core.send_message(SendMessage {
        key: key.clone(),
        client_request_id: id("future"),
        text: "future-queued-marker".into(),
        attachment_ids: vec![],
    })?;
    let lease = core.claim_next_turn(&key, binding)?;
    let command = WorkerCommand {
        executable: PathBuf::from(&args[0]),
        script: PathBuf::from(&args[1]),
        timeout: Duration::from_secs(45),
        exit_grace: Duration::from_secs(3),
    };
    let stop = Arc::new(AtomicBool::new(false));
    let result = std::thread::scope(|threads| {
        let responder = threads.spawn(|| {
            if automatic {
                return Ok(());
            }
            let result = answer_approval(&core, &key, &receipt.turn_id, &cwd, &args[5], &stop);
            if result.is_err() {
                stop.store(true, Ordering::Release);
            }
            result
        });
        let result = loom_runtime::run_session(
            &core,
            lease,
            &command,
            SessionRunOptions {
                run_id: id("runtime-probe"),
                tools: vec!["write".into()],
                external_tools: Vec::new(),
                max_turns: 3,
                context_bytes: 512 * 1024,
            },
            if anonymous { None } else { Some(credential) },
            stop.clone(),
            &mut policy,
            None,
        );
        stop.store(true, Ordering::Release);
        responder
            .join()
            .expect("fixture responder panicked")
            .map_err(|e| RuntimeError::new("fixture", e.to_string()))?;
        result
    });
    let view = core.get_session(&key)?;
    for event in core.read_events(&key, 0, 500)?.events {
        if let SessionEvent::RuntimeApplied {
            update: RuntimeUpdate::ToolPolicy { allowed, .. },
            ..
        } = event.payload
        {
            println!("{}", json!({"kind":"policyDecision","allowed":allowed}));
        }
    }
    let turn = view.turns.iter().find(|turn| turn.id == receipt.turn_id);
    println!(
        "{}",
        json!({"kind":"coreAudit","approvalStatuses":view.approvals.values().map(|a|a.status).collect::<Vec<_>>(),"turnStatus":turn.map(|t|t.status),"usage":turn.and_then(|t|t.usage.as_ref()),"activeTurnId":view.active_turn_id,"lastSeq":view.last_seq,"toolCount":view.tools.len()})
    );
    result?;
    Ok(())
}
