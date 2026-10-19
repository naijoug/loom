use loom_core::*;
use serde_json::json;
use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Barrier,
    },
    time::Duration,
};
static NEXT: AtomicU64 = AtomicU64::new(1);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "loom-approvals-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn open(&self) -> Core {
        Core::open(&self.0).unwrap()
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
fn start(core: &Core, session: &str, mode: PermissionMode) -> (SessionKey, TurnLease) {
    core.create_workspace(id("w"), "workspace".into()).unwrap();
    let key = SessionKey {
        workspace_id: id("w"),
        session_id: id(session),
    };
    core.create_session(CreateSession {
        key: key.clone(),
        title: None,
        project_id: None,
        permission_mode: mode,
        connection: Default::default(),
    })
    .unwrap();
    core.send_message(SendMessage {
        key: key.clone(),
        client_request_id: id("send"),
        text: "fixture".into(),
        attachment_ids: vec![],
    })
    .unwrap();
    let lease = core
        .claim_next_turn(
            &key,
            RuntimeBinding {
                adapter: "approval-fixture".into(),
                connection: Default::default(),
                permission_mode: mode,
                policy_version: 7,
                working_directory: None,
                connection_snapshot_digest: None,
                policy_snapshot_digest: None,
            },
        )
        .unwrap();
    (key, lease)
}
fn spec(request: &str, tool: &str) -> ToolApprovalSpec {
    ToolApprovalSpec {
        request_id: id(request),
        tool_call_id: id(tool),
        tool_name: "Write".into(),
        input: json!({"file_path":"example.txt","content":"secret-file-content","api_key":"secret-credential-value"}),
        ttl_ms: 30_000,
        remember: None,
    }
}

#[test]
fn grant_requires_durable_decision_then_one_consumption_with_exact_arguments() {
    let f = Fixture::new();
    let core = f.open();
    let (key, lease) = start(&core, "s", PermissionMode::Ask);
    let request = spec("request", "tool");
    let pending = core.request_tool_approval(&lease, &request).unwrap();
    assert_eq!(pending.status, ApprovalStatus::Pending);
    assert_eq!(pending.policy_version, 7);
    assert!(core.consume_tool_approval(&lease, &request).is_err());
    assert!(core
        .apply_runtime(
            &lease,
            id("bypass"),
            RuntimeUpdate::ToolStarted {
                tool_call_id: request.tool_call_id.clone(),
                name: "Write".into(),
                input: json!({})
            }
        )
        .is_err());
    assert!(core
        .finish_turn(&lease, FinishStatus::Completed, None)
        .is_err());
    let allowed = core
        .respond_tool_approval(&key, &request.request_id, &pending.binding_digest, true)
        .unwrap();
    assert_eq!(allowed.status, ApprovalStatus::Allowed);
    let mut changed = spec("request", "tool");
    changed.input["file_path"] = "other.txt".into();
    assert!(core.consume_tool_approval(&lease, &changed).is_err());
    changed = spec("request", "tool");
    changed.tool_name = "Bash".into();
    assert!(core.consume_tool_approval(&lease, &changed).is_err());
    let permit = core.consume_tool_approval(&lease, &request).unwrap();
    assert!(permit.is_live().unwrap());
    assert_eq!(permit.tool_call_id(), &id("tool"));
    assert_eq!(permit.digest(), pending.binding_digest);
    assert!(core.consume_tool_approval(&lease, &request).is_err());
    assert_eq!(
        core.respond_tool_approval(&key, &request.request_id, &pending.binding_digest, true)
            .unwrap()
            .status,
        ApprovalStatus::Consumed
    );
    assert!(core
        .finish_turn(&lease, FinishStatus::Completed, None)
        .is_err());
    core.apply_runtime(
        &lease,
        id("finished"),
        RuntimeUpdate::ToolFinished {
            tool_call_id: id("tool"),
            status: ToolStatus::Succeeded,
            output: json!({"ok":true}),
        },
    )
    .unwrap();
    core.finish_turn(&lease, FinishStatus::Completed, None)
        .unwrap();
    assert!(!permit.is_live().unwrap());
    let log = fs::read_to_string(f.0.join("workspaces/w/sessions/s/events.jsonl")).unwrap();
    assert!(!log.contains("secret-file-content"));
    assert!(!log.contains("secret-credential-value"));
    assert!(log.contains("example.txt"));
}

#[test]
fn duplicate_request_does_not_extend_deadline_and_reusing_identity_with_new_input_fails() {
    let f = Fixture::new();
    let core = f.open();
    let (_, lease) = start(&core, "s", PermissionMode::Ask);
    let mut request = spec("request", "tool");
    let first = core.request_tool_approval(&lease, &request).unwrap();
    request.ttl_ms = 300_000;
    assert_eq!(core.request_tool_approval(&lease, &request).unwrap(), first);
    request.input["file_path"] = "other".into();
    assert_eq!(
        core.request_tool_approval(&lease, &request)
            .unwrap_err()
            .code,
        ErrorCode::Conflict
    );
    assert!(core
        .request_tool_approval(&lease, &spec("other-request", "tool"))
        .is_err());
}

#[test]
fn deny_wrong_scope_and_late_allow_cannot_produce_a_permit() {
    let f = Fixture::new();
    let core = f.open();
    let (key, lease) = start(&core, "s", PermissionMode::Ask);
    let (other, foreign) = start(&core, "other", PermissionMode::Ask);
    let request = spec("request", "tool");
    let pending = core.request_tool_approval(&lease, &request).unwrap();
    assert!(core
        .respond_tool_approval(&other, &request.request_id, &pending.binding_digest, true)
        .is_err());
    assert!(core.consume_tool_approval(&foreign, &request).is_err());
    assert!(core
        .respond_tool_approval(&key, &request.request_id, "different-digest", true)
        .is_err());
    assert_eq!(
        core.respond_tool_approval(&key, &request.request_id, &pending.binding_digest, false)
            .unwrap()
            .status,
        ApprovalStatus::Denied
    );
    assert!(core
        .respond_tool_approval(&key, &request.request_id, &pending.binding_digest, true)
        .is_err());
    assert!(core.consume_tool_approval(&lease, &request).is_err());
    core.finish_turn(&lease, FinishStatus::Completed, None)
        .unwrap();
}

#[test]
fn cancellation_revokes_pending_allowed_and_already_consumed_permits() {
    for allow in [false, true] {
        let f = Fixture::new();
        let core = f.open();
        let (key, lease) = start(&core, "s", PermissionMode::Ask);
        let request = spec("request", "tool");
        let pending = core.request_tool_approval(&lease, &request).unwrap();
        if allow {
            core.respond_tool_approval(&key, &request.request_id, &pending.binding_digest, true)
                .unwrap();
        }
        core.cancel_turn(&key, lease.turn_id()).unwrap();
        assert_eq!(
            core.get_session(&key).unwrap().approvals[&request.request_id].status,
            ApprovalStatus::Cancelled
        );
        assert!(core
            .respond_tool_approval(&key, &request.request_id, &pending.binding_digest, true)
            .is_err());
        assert!(core.consume_tool_approval(&lease, &request).is_err());
    }
    let f = Fixture::new();
    let core = f.open();
    let (key, lease) = start(&core, "s", PermissionMode::Ask);
    let request = spec("request", "tool");
    let pending = core.request_tool_approval(&lease, &request).unwrap();
    core.respond_tool_approval(&key, &request.request_id, &pending.binding_digest, true)
        .unwrap();
    let permit = core.consume_tool_approval(&lease, &request).unwrap();
    core.cancel_turn(&key, lease.turn_id()).unwrap();
    assert!(!permit.is_live().unwrap());
}

#[test]
fn expired_requests_and_unconsumed_grants_are_durably_revoked() {
    for allow in [false, true] {
        let f = Fixture::new();
        let core = f.open();
        let (key, lease) = start(&core, "s", PermissionMode::Ask);
        let mut request = spec("request", "tool");
        request.ttl_ms = 250;
        let pending = core.request_tool_approval(&lease, &request).unwrap();
        if allow {
            core.respond_tool_approval(&key, &request.request_id, &pending.binding_digest, true)
                .unwrap();
        }
        std::thread::sleep(Duration::from_millis(300));
        let view = core.get_session(&key).unwrap();
        assert_eq!(
            view.approvals[&request.request_id].status,
            ApprovalStatus::Expired
        );
        assert!(core.consume_tool_approval(&lease, &request).is_err());
        assert!(core
            .respond_tool_approval(&key, &request.request_id, &pending.binding_digest, true)
            .is_err());
    }
}

#[test]
fn competing_frontend_responses_have_only_one_durable_winner() {
    let f = Fixture::new();
    let core = f.open();
    let (key, lease) = start(&core, "s", PermissionMode::Ask);
    let request = spec("request", "tool");
    let pending = core.request_tool_approval(&lease, &request).unwrap();
    let barrier = Arc::new(Barrier::new(3));
    let workers = [true, false].map(|allowed| {
        let core = core.clone();
        let key = key.clone();
        let request_id = request.request_id.clone();
        let digest = pending.binding_digest.clone();
        let barrier = barrier.clone();
        std::thread::spawn(move || {
            barrier.wait();
            core.respond_tool_approval(&key, &request_id, &digest, allowed)
        })
    });
    barrier.wait();
    let results = workers.map(|thread| thread.join().unwrap());
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert!(matches!(
        core.get_session(&key).unwrap().approvals[&request.request_id].status,
        ApprovalStatus::Allowed | ApprovalStatus::Denied
    ));
}

#[test]
fn user_wire_can_only_respond_and_cannot_forge_runtime_approval_requests() {
    let f = Fixture::new();
    let core = f.open();
    let (key, lease) = start(&core, "s", PermissionMode::Ask);
    let request = spec("request", "tool");
    let pending = core.request_tool_approval(&lease, &request).unwrap();
    let reply=core.dispatch_json(&serde_json::to_vec(&json!({"protocolVersion":1,"requestId":"reply","command":{"method":"respond_tool_approval","params":{"key":key,"requestId":"request","expectedDigest":pending.binding_digest,"allowed":true}}})).unwrap());
    assert!(matches!(
        reply.outcome,
        Outcome::Success {
            result: CoreResponse::ToolApproval(approval)
        } if approval.status==ApprovalStatus::Allowed
    ));
    let reply=core.dispatch_json(br#"{"protocolVersion":1,"requestId":"forge","command":{"method":"request_tool_approval","params":{}}}"#);
    assert!(matches!(reply.outcome, Outcome::Failure { .. }));
}

#[test]
fn approval_write_failure_never_returns_a_grant_or_permission_to_start() {
    let f = Fixture::new();
    let core = f.open();
    let (key, lease) = start(&core, "s", PermissionMode::Ask);
    let request = spec("request", "tool");
    let pending = core.request_tool_approval(&lease, &request).unwrap();
    let path = f.0.join("workspaces/w/sessions/s/commit.json");
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(core
        .respond_tool_approval(&key, &request.request_id, &pending.binding_digest, true)
        .is_err());
    assert!(core.consume_tool_approval(&lease, &request).is_err());
}

#[test]
fn interactive_prompts_do_not_create_a_policy_bypass_in_explore_or_execute() {
    for mode in [PermissionMode::Explore, PermissionMode::Execute] {
        let f = Fixture::new();
        let core = f.open();
        let (_, lease) = start(&core, "s", mode);
        assert!(core
            .request_tool_approval(&lease, &spec("request", "tool"))
            .is_err());
    }
}

#[test]
fn preview_preserves_task_paths_without_exposing_credential_shaped_values() {
    let f = Fixture::new();
    let core = f.open();
    let (_, lease) = start(&core, "s", PermissionMode::Ask);
    let mut request = spec("request", "tool");
    request.input = json!({"file_path":"tasks/task-list.md","command":"git diff -- tasks/task-list.md","nested":{"value":"sk-live-private-value"},"headers":{"x-api-key":"arbitrary-value"}});
    let approval = core.request_tool_approval(&lease, &request).unwrap();
    assert_eq!(approval.preview["file_path"], "tasks/task-list.md");
    assert_eq!(
        approval.preview["command"],
        "git diff -- tasks/task-list.md"
    );
    let text = serde_json::to_string(&approval).unwrap();
    assert!(!text.contains("sk-live-private-value"));
    assert!(!text.contains("arbitrary-value"));
}

#[test]
fn approval_crash_writer() {
    let Some(path) = std::env::var_os("LOOM_APPROVAL_CRASH_FIXTURE") else {
        return;
    };
    let core = Core::open(PathBuf::from(path)).unwrap();
    let (key, lease) = start(&core, "s", PermissionMode::Ask);
    let request = spec("request", "tool");
    let pending = core.request_tool_approval(&lease, &request).unwrap();
    core.respond_tool_approval(&key, &request.request_id, &pending.binding_digest, true)
        .unwrap();
    std::process::exit(23);
}

#[test]
fn restart_cancels_a_previously_allowed_request_and_cannot_reconstruct_a_permit() {
    let f = Fixture::new();
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "approval_crash_writer", "--nocapture"])
        .env("LOOM_APPROVAL_CRASH_FIXTURE", &f.0)
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(23));
    let core = f.open();
    let key = SessionKey {
        workspace_id: id("w"),
        session_id: id("s"),
    };
    let view = core.get_session(&key).unwrap();
    assert_eq!(view.turns[0].status, TurnStatus::Interrupted);
    let approval = &view.approvals[&id("request")];
    assert_eq!(approval.status, ApprovalStatus::Cancelled);
    assert!(core
        .respond_tool_approval(&key, &id("request"), &approval.binding_digest, true)
        .is_err());
    assert!(view.tools.is_empty());
}

fn offer(selector: &str) -> RememberOffer {
    RememberOffer {
        keys: vec![RememberKey::new(
            RememberKind::Directory,
            selector,
            "同一测试目录内的写入".into(),
        )
        .unwrap()],
    }
}
fn grant(core: &Core, key: &SessionKey, lease: &TurnLease, request: &ToolApprovalSpec) {
    let pending = core.request_tool_approval(lease, request).unwrap();
    core.respond_tool_approval_with_memory(
        key,
        &request.request_id,
        &pending.binding_digest,
        true,
        true,
    )
    .unwrap();
    core.consume_tool_approval(lease, request).unwrap();
    core.apply_runtime(
        lease,
        id(&format!("finish-{}", request.request_id)),
        RuntimeUpdate::ToolFinished {
            tool_call_id: request.tool_call_id.clone(),
            status: ToolStatus::Succeeded,
            output: json!({"ok":true}),
        },
    )
    .unwrap();
}
#[test]
fn remembered_authority_is_exact_scoped_revocable_and_records_its_grant_id() {
    let f = Fixture::new();
    let core = f.open();
    let (key, lease) = start(&core, "s", PermissionMode::Ask);
    let mut first = spec("remember", "first");
    first.remember = Some(offer("folder-A"));
    grant(&core, &key, &lease, &first);
    let mut next = spec("next", "next-tool");
    next.remember = Some(offer("folder-B"));
    assert!(!core
        .try_remembered_tool(&lease, id("unmatched"), &next, &[])
        .unwrap());
    next.remember = Some(offer("folder-A"));
    assert!(core
        .try_remembered_tool(&lease, id("matched"), &next, &[])
        .unwrap());
    let events = core.read_events(&key, 0, 100).unwrap();
    assert!(events.events.iter().any(|e|matches!(&e.payload,SessionEvent::RuntimeApplied{update:RuntimeUpdate::ToolPolicy{authorization_ids,..},..} if authorization_ids==&vec![id("remember")])));
    assert_eq!(core.get_session(&key).unwrap().approvals.len(), 1);
    core.clear_remembered_grants(&key).unwrap();
    let later = ToolApprovalSpec {
        tool_call_id: id("later"),
        ..next
    };
    assert!(!core
        .try_remembered_tool(&lease, id("revoked"), &later, &[])
        .unwrap());
    assert!(
        core.get_session(&key).unwrap().remembered_grants[&id("remember")]
            .revoked_at_ms
            .is_some()
    );
}
#[test]
fn remembered_authority_survives_turns_but_not_a_policy_change_or_application_restart() {
    let f = Fixture::new();
    let core = f.open();
    let (key, lease) = start(&core, "s", PermissionMode::Ask);
    let mut request = spec("remember", "first");
    request.remember = Some(offer("folder-A"));
    grant(&core, &key, &lease, &request);
    core.finish_turn(&lease, FinishStatus::Completed, None)
        .unwrap();
    drop(lease);
    core.send_message(SendMessage {
        key: key.clone(),
        client_request_id: id("matching-turn"),
        text: "matching".into(),
        attachment_ids: vec![],
    })
    .unwrap();
    let matching = core
        .claim_next_turn(
            &key,
            RuntimeBinding {
                adapter: "approval-fixture".into(),
                connection: Default::default(),
                permission_mode: PermissionMode::Ask,
                policy_version: 7,
                working_directory: None,
                connection_snapshot_digest: None,
                policy_snapshot_digest: None,
            },
        )
        .unwrap();
    let matching_spec = ToolApprovalSpec {
        request_id: id("matching-request"),
        tool_call_id: id("matching-tool"),
        tool_name: "Write".into(),
        input: json!({"path":"next"}),
        ttl_ms: 30000,
        remember: Some(offer("folder-A")),
    };
    assert!(core
        .try_remembered_tool(&matching, id("admitted"), &matching_spec, &[])
        .unwrap());
    core.apply_runtime(
        &matching,
        id("matching-finished"),
        RuntimeUpdate::ToolFinished {
            tool_call_id: matching_spec.tool_call_id,
            status: ToolStatus::Succeeded,
            output: json!({}),
        },
    )
    .unwrap();
    core.finish_turn(&matching, FinishStatus::Completed, None)
        .unwrap();
    drop(matching);
    core.send_message(SendMessage {
        key: key.clone(),
        client_request_id: id("second"),
        text: "second".into(),
        attachment_ids: vec![],
    })
    .unwrap();
    let lease = core
        .claim_next_turn(
            &key,
            RuntimeBinding {
                adapter: "approval-fixture".into(),
                connection: Default::default(),
                permission_mode: PermissionMode::Ask,
                policy_version: 8,
                working_directory: None,
                connection_snapshot_digest: None,
                policy_snapshot_digest: None,
            },
        )
        .unwrap();
    let next = ToolApprovalSpec {
        request_id: id("second-request"),
        tool_call_id: id("second-tool"),
        ..request
    };
    assert!(!core
        .try_remembered_tool(&lease, id("changed-policy"), &next, &[])
        .unwrap());
    core.finish_turn(&lease, FinishStatus::Completed, None)
        .unwrap();
    drop(lease);
    drop(core);
    let core = f.open();
    let restored = core.get_session(&key).unwrap();
    assert_eq!(restored.remembered_grants.len(), 1);
    assert!(restored.remembered_grants[&id("remember")]
        .revoked_at_ms
        .is_some());
    let seq = restored.last_seq;
    assert_eq!(core.get_session(&key).unwrap().last_seq, seq);
}
#[test]
fn remember_choice_requires_an_offered_scope_and_cannot_be_added_to_a_settled_single_approval() {
    let f = Fixture::new();
    let core = f.open();
    let (key, lease) = start(&core, "s", PermissionMode::Ask);
    let request = spec("single", "single-tool");
    let pending = core.request_tool_approval(&lease, &request).unwrap();
    assert!(core
        .respond_tool_approval_with_memory(
            &key,
            &request.request_id,
            &pending.binding_digest,
            true,
            true
        )
        .is_err());
    core.respond_tool_approval(&key, &request.request_id, &pending.binding_digest, true)
        .unwrap();
    assert!(core
        .respond_tool_approval_with_memory(
            &key,
            &request.request_id,
            &pending.binding_digest,
            true,
            true
        )
        .is_err());
    assert!(core.get_session(&key).unwrap().remembered_grants.is_empty());
}
#[test]
fn remembered_scope_is_bound_to_approval_and_failed_persistence_creates_no_live_grant() {
    let f = Fixture::new();
    let core = f.open();
    let (key, lease) = start(&core, "s", PermissionMode::Ask);
    let mut request = spec("remember", "tool");
    request.remember = Some(offer("folder-A"));
    let pending = core.request_tool_approval(&lease, &request).unwrap();
    let mut changed = spec("remember", "tool");
    changed.remember = Some(offer("folder-B"));
    assert!(core.request_tool_approval(&lease, &changed).is_err());
    let path = f.0.join("workspaces/w/sessions/s/events.jsonl");
    let bytes = fs::read(&path).unwrap();
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(core
        .respond_tool_approval_with_memory(
            &key,
            &request.request_id,
            &pending.binding_digest,
            true,
            true
        )
        .is_err());
    fs::remove_dir(&path).unwrap();
    fs::write(&path, bytes).unwrap();
    drop(lease);
    drop(core);
    assert!(f
        .open()
        .get_session(&key)
        .unwrap()
        .remembered_grants
        .is_empty());
}

#[test]
fn remembered_domains_require_every_target_and_never_cross_sessions() {
    let f = Fixture::new();
    let core = f.open();
    let (key, lease) = start(&core, "s", PermissionMode::Ask);
    let domain = |host: &str| {
        RememberKey::new(RememberKind::Domain, host, format!("网络目标：{host}")).unwrap()
    };
    let mut a = spec("a", "tool-a");
    a.remember = Some(RememberOffer {
        keys: vec![domain("a.example")],
    });
    grant(&core, &key, &lease, &a);
    let mut both = spec("both", "both-tool");
    both.remember = Some(RememberOffer {
        keys: vec![domain("a.example"), domain("b.example")],
    });
    assert!(!core
        .try_remembered_tool(&lease, id("not-all"), &both, &[])
        .unwrap());
    let mut b = spec("b", "tool-b");
    b.remember = Some(RememberOffer {
        keys: vec![domain("b.example")],
    });
    grant(&core, &key, &lease, &b);
    assert!(core
        .try_remembered_tool(&lease, id("all"), &both, &[])
        .unwrap());
    let other = SessionKey {
        workspace_id: id("w"),
        session_id: id("other"),
    };
    core.create_session(CreateSession {
        key: other.clone(),
        title: None,
        project_id: None,
        permission_mode: PermissionMode::Ask,
        connection: Default::default(),
    })
    .unwrap();
    core.send_message(SendMessage {
        key: other.clone(),
        client_request_id: id("send"),
        text: "other".into(),
        attachment_ids: vec![],
    })
    .unwrap();
    let lease2 = core
        .claim_next_turn(
            &other,
            RuntimeBinding {
                adapter: "approval-fixture".into(),
                connection: Default::default(),
                permission_mode: PermissionMode::Ask,
                policy_version: 7,
                working_directory: None,
                connection_snapshot_digest: None,
                policy_snapshot_digest: None,
            },
        )
        .unwrap();
    assert!(!core
        .try_remembered_tool(&lease2, id("foreign"), &both, &[])
        .unwrap());
}
