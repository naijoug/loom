use loom_core::*;
use serde_json::json;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(1);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "loom-context-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p.canonicalize().unwrap())
    }
    fn setup(&self) -> (Core, SessionKey, ModelConnection) {
        let core = Core::open(&self.0).unwrap();
        core.create_workspace(id("w"), "Workspace".into()).unwrap();
        let connection = core
            .save_connection_metadata(SaveConnectionMetadata {
                id: id("connection"),
                operation_id: id("create"),
                expected_revision: None,
                credential_ref: None,
                config: ConnectionConfig {
                    name: "Fixture".into(),
                    provider: ConnectionProvider::PiCompat,
                    auth: ConnectionAuth::None,
                    endpoint: Some("http://127.0.0.1:1".into()),
                    pi_provider: None,
                    custom_api: Some(EndpointApi::OpenaiCompletions),
                    models: vec!["a".into(), "b".into()],
                    default_model: "a".into(),
                    supports_images: false,
                    mid_stream_behavior: MidStreamBehavior::Queue,
                },
            })
            .unwrap();
        let key = SessionKey {
            workspace_id: id("w"),
            session_id: id("s"),
        };
        core.create_session(CreateSession {
            key: key.clone(),
            title: None,
            project_id: None,
            permission_mode: PermissionMode::Ask,
            connection: ConnectionSelection {
                connection_id: Some(connection.id.clone()),
                ..Default::default()
            },
        })
        .unwrap();
        (core, key, connection)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn id(s: &str) -> Id {
    Id::new(s).unwrap()
}
fn send(core: &Core, key: &SessionKey, text: &str) {
    core.send_message(SendMessage {
        key: key.clone(),
        client_request_id: id(text),
        text: text.into(),
        attachment_ids: vec![],
    })
    .unwrap();
}
fn binding(core: &Core, key: &SessionKey, connection: &ModelConnection) -> RuntimeBinding {
    RuntimeBinding {
        adapter: "fixture".into(),
        connection: ConnectionSelection {
            connection_id: Some(connection.id.clone()),
            model: Some(connection.config.default_model.clone()),
            thinking: None,
        },
        permission_mode: PermissionMode::Ask,
        policy_version: 1,
        working_directory: Some(core.runtime_working_directory(key).unwrap()),
        connection_snapshot_digest: Some(connection.snapshot_digest().unwrap()),
        policy_snapshot_digest: None,
    }
}
fn text(core: &Core, lease: &TurnLease, name: &str, kind: TextKind, delta: &str) {
    core.apply_runtime(
        lease,
        id(name),
        RuntimeUpdate::TextDelta {
            part_id: id(name),
            text_kind: kind,
            delta: delta.into(),
        },
    )
    .unwrap();
}

#[test]
fn context_freezes_connection_and_excludes_future_queue_and_private_thinking() {
    let f = Fixture::new();
    let (core, key, connection) = f.setup();
    send(&core, &key, "first");
    let lease = core
        .claim_next_turn(&key, binding(&core, &key, &connection))
        .unwrap();
    text(
        &core,
        &lease,
        "thinking",
        TextKind::Thinking,
        "private-thought",
    );
    text(&core, &lease, "answer", TextKind::Text, "first-answer");
    core.apply_runtime(
        &lease,
        id("tool-start"),
        RuntimeUpdate::ToolStarted {
            tool_call_id: id("read"),
            name: "read".into(),
            input: json!({"path":"fixture"}),
        },
    )
    .unwrap();
    core.apply_runtime(
        &lease,
        id("tool-end"),
        RuntimeUpdate::ToolFinished {
            tool_call_id: id("read"),
            status: ToolStatus::Succeeded,
            output: json!("file-observation"),
        },
    )
    .unwrap();
    send(&core, &key, "second");
    core.finish_turn(&lease, FinishStatus::Completed, None)
        .unwrap();
    drop(lease);
    let lease = core
        .claim_next_turn(&key, binding(&core, &key, &connection))
        .unwrap();
    let mut revised = connection.config.clone();
    revised.default_model = "b".into();
    core.save_connection_metadata(SaveConnectionMetadata {
        id: connection.id.clone(),
        operation_id: id("edit"),
        expected_revision: Some(1),
        config: revised,
        credential_ref: None,
    })
    .unwrap();
    send(&core, &key, "future-input");
    let context = core.runtime_context(&lease, 100_000).unwrap();
    assert_eq!(context.connection, connection);
    assert_eq!(context.prompt, "second");
    assert_eq!(context.history.len(), 2);
    assert_eq!(context.history[0].text, "first");
    assert!(context.history[1].text.contains("first-answer"));
    assert!(context.history[1].text.contains("file-observation"));
    let joined = context
        .history
        .iter()
        .map(|m| m.text.as_str())
        .collect::<String>();
    assert!(!joined.contains("private-thought") && !joined.contains("future-input"));
    assert_eq!(
        Some(&context.working_directory),
        context.binding.working_directory.as_ref()
    );
    assert!(std::path::Path::new(&context.runtime_directory).is_dir());
    assert!(core.runtime_context(&lease, 3).is_err());
    core.cancel_turn(&key, lease.turn_id()).unwrap();
    assert!(core.runtime_context(&lease, 100_000).is_err());
}

#[test]
fn projectless_runtime_requires_its_own_explicit_directory() {
    let f = Fixture::new();
    let (core, key, connection) = f.setup();
    send(&core, &key, "first");
    let mut bound = binding(&core, &key, &connection);
    bound.working_directory = Some(f.0.to_str().unwrap().into());
    assert!(core.claim_next_turn(&key, bound.clone()).is_err());
    bound.working_directory = None;
    let lease = core.claim_next_turn(&key, bound).unwrap();
    assert!(core.runtime_context(&lease, 10_000).is_err());
}

#[cfg(unix)]
#[test]
fn replacing_projectless_work_directory_with_a_symlink_is_rejected() {
    let f = Fixture::new();
    let (core, key, connection) = f.setup();
    send(&core, &key, "first");
    let bound = binding(&core, &key, &connection);
    let directory = bound.working_directory.as_ref().unwrap();
    fs::remove_dir(directory).unwrap();
    std::os::unix::fs::symlink(&f.0, directory).unwrap();
    assert!(core.runtime_working_directory(&key).is_err());
    assert!(core.claim_next_turn(&key, bound).is_err());
}

#[test]
fn usage_is_cumulative_per_turn_durable_and_backwards_compatible() {
    let f = Fixture::new();
    let (core, key, connection) = f.setup();
    send(&core, &key, "first");
    let lease = core
        .claim_next_turn(&key, binding(&core, &key, &connection))
        .unwrap();
    let apply = |name: &str, input, output| {
        core.apply_runtime(
            &lease,
            id(name),
            RuntimeUpdate::Usage {
                usage: TokenUsage {
                    input_tokens: input,
                    output_tokens: output,
                },
            },
        )
    };
    apply("usage-1", 10, 2).unwrap();
    apply("usage-2", 10, 5).unwrap();
    assert!(apply("decreasing", 9, 5).is_err());
    assert!(apply("unsafe-integer", u64::MAX, 10).is_err());
    core.finish_turn(&lease, FinishStatus::Completed, None)
        .unwrap();
    drop(lease);
    send(&core, &key, "second");
    let lease = core
        .claim_next_turn(&key, binding(&core, &key, &connection))
        .unwrap();
    core.apply_runtime(
        &lease,
        id("usage-1"),
        RuntimeUpdate::Usage {
            usage: TokenUsage {
                input_tokens: 1,
                output_tokens: 1,
            },
        },
    )
    .unwrap();
    core.finish_turn(&lease, FinishStatus::Completed, None)
        .unwrap();
    drop(lease);
    drop(core);
    let core = Core::open(&f.0).unwrap();
    let view = core.get_session(&key).unwrap();
    assert_eq!(
        view.turns[0].usage,
        Some(TokenUsage {
            input_tokens: 10,
            output_tokens: 5
        })
    );
    assert_eq!(
        view.turns[1].usage,
        Some(TokenUsage {
            input_tokens: 1,
            output_tokens: 1
        })
    );
    let mut old = serde_json::to_value(&view.turns[0]).unwrap();
    old.as_object_mut().unwrap().remove("usage");
    let decoded: Turn = serde_json::from_value(old.clone()).unwrap();
    assert!(decoded.usage.is_none());
    assert_eq!(serde_json::to_value(decoded).unwrap(), old);
}

#[test]
fn accepted_cancellation_wins_over_late_runtime_completion_or_failure() {
    for terminal in [FinishStatus::Completed, FinishStatus::Failed] {
        let f = Fixture::new();
        let (core, key, connection) = f.setup();
        send(&core, &key, "first");
        let lease = core
            .claim_next_turn(&key, binding(&core, &key, &connection))
            .unwrap();
        core.cancel_turn(&key, lease.turn_id()).unwrap();
        let view = core
            .settle_runtime_turn(&lease, terminal, Some("late error".into()))
            .unwrap();
        assert_eq!(view.turns[0].status, TurnStatus::Cancelled);
        assert!(view.turns[0].error.is_none() && view.active_turn_id.is_none());
    }
}

#[test]
fn known_secrets_are_removed_before_preview_truncation_without_changing_authority() {
    let f = Fixture::new();
    let (core, key, connection) = f.setup();
    send(&core, &key, "first");
    let lease = core
        .claim_next_turn(&key, binding(&core, &key, &connection))
        .unwrap();
    let secret = "private-甲".repeat(300);
    let spec = ToolApprovalSpec {
        request_id: id("approve"),
        tool_call_id: id("write"),
        tool_name: "write".into(),
        input: json!({secret.clone():secret.clone(), "path":"ordinary-task-file.txt", "content": "private body"}),
        ttl_ms: 10_000,
        remember: None,
    };
    let approval = core
        .request_tool_approval_redacted(&lease, &spec, std::slice::from_ref(&secret))
        .unwrap();
    assert!(!approval.preview.to_string().contains("private-甲"));
    assert_eq!(approval.preview["path"], "ordinary-task-file.txt");
    core.respond_tool_approval(&key, &spec.request_id, &approval.binding_digest, true)
        .unwrap();
    let changed = ToolApprovalSpec {
        input: approval.preview.clone(),
        ..spec
    };
    assert!(core.consume_tool_approval(&lease, &changed).is_err());
    let spec = ToolApprovalSpec {
        input: json!({secret.clone():secret.clone(), "path":"ordinary-task-file.txt", "content": "private body"}),
        ..changed
    };
    core.consume_tool_approval(&lease, &spec).unwrap();
    let denied = ToolApprovalSpec {
        request_id: id("automatic"),
        tool_call_id: id("denied"),
        ..spec
    };
    core.record_tool_policy(
        &lease,
        id("decision"),
        &denied,
        ToolPolicyDecision {
            allowed: false,
            reason: secret.clone(),
        },
        std::slice::from_ref(&secret),
    )
    .unwrap();
    let events = serde_json::to_string(&core.read_events(&key, 0, 100).unwrap()).unwrap();
    assert!(!events.contains("private-甲") && !events.contains("private body"));
    assert_eq!(
        core.get_session(&key).unwrap().tools[&id("denied")].status,
        ToolStatus::Denied
    );
}
