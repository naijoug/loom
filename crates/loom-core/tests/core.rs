use loom_core::*;
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Barrier,
    },
};
static NEXT: AtomicU64 = AtomicU64::new(1);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "loom-core-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn open(&self) -> Core {
        Core::open(&self.0).unwrap()
    }
    fn session_dir(&self, key: &SessionKey) -> PathBuf {
        self.0
            .join("workspaces")
            .join(key.workspace_id.as_str())
            .join("sessions")
            .join(key.session_id.as_str())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn id(value: &str) -> Id {
    Id::new(value).unwrap()
}
fn key(session: &str) -> SessionKey {
    SessionKey {
        workspace_id: id("workspace"),
        session_id: id(session),
    }
}
fn create(core: &Core, session: &str) -> SessionKey {
    core.create_workspace(id("workspace"), "工作区".into())
        .unwrap();
    let key = key(session);
    core.create_session(CreateSession {
        key: key.clone(),
        title: None,
        project_id: None,
        permission_mode: PermissionMode::Ask,
        connection: Default::default(),
    })
    .unwrap();
    key
}
fn request(key: &SessionKey, client_id: &str, text: &str) -> SendMessage {
    SendMessage {
        key: key.clone(),
        client_request_id: id(client_id),
        text: text.into(),
        attachment_ids: vec![],
    }
}
fn binding(mode: PermissionMode) -> RuntimeBinding {
    RuntimeBinding {
        adapter: "fixture".into(),
        connection: Default::default(),
        permission_mode: mode,
        policy_version: 1,
        working_directory: None,
        connection_snapshot_digest: None,
        policy_snapshot_digest: None,
    }
}
fn text(part: &str, delta: &str) -> RuntimeUpdate {
    RuntimeUpdate::TextDelta {
        part_id: id(part),
        text_kind: TextKind::Text,
        delta: delta.into(),
    }
}
fn assistant_text(view: &SessionView) -> String {
    view.messages
        .iter()
        .filter(|m| m.role == Role::Assistant)
        .flat_map(|m| &m.parts)
        .filter_map(|part| match part {
            MessagePart::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn workspace_does_not_require_a_project_and_project_membership_is_enforced() {
    let f = Fixture::new();
    let core = f.open();
    let k = create(&core, "s");
    assert_eq!(core.get_session(&k).unwrap().metadata.project_id, None);
    core.create_workspace(id("other"), "Other".into()).unwrap();
    core.register_project(id("other"), id("project"), "Project".into(), &f.0)
        .unwrap();
    let bad = core
        .create_session(CreateSession {
            key: key("p"),
            title: None,
            project_id: Some(id("project")),
            permission_mode: PermissionMode::Ask,
            connection: Default::default(),
        })
        .unwrap_err();
    assert_eq!(bad.code, ErrorCode::NotFound);
    let project = core
        .register_project(id("workspace"), id("project"), "Project".into(), &f.0)
        .unwrap();
    let k = key("with-project");
    core.create_session(CreateSession {
        key: k.clone(),
        title: Some("Project chat".into()),
        project_id: Some(project.id.clone()),
        permission_mode: PermissionMode::Ask,
        connection: Default::default(),
    })
    .unwrap();
    core.send_message(request(&k, "send", "hello")).unwrap();
    assert!(core
        .claim_next_turn(&k, binding(PermissionMode::Ask))
        .is_err());
    let mut valid = binding(PermissionMode::Ask);
    valid.working_directory = Some(project.directory);
    let lease = core.claim_next_turn(&k, valid).unwrap();
    core.finish_turn(&lease, FinishStatus::Completed, None)
        .unwrap();
}
#[test]
fn concurrent_send_has_one_durable_receipt_and_restart_preserves_it() {
    let f = Fixture::new();
    let core = f.open();
    let k = create(&core, "s");
    let barrier = Arc::new(Barrier::new(12));
    let joins: Vec<_> = (0..12)
        .map(|_| {
            let core = core.clone();
            let k = k.clone();
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                core.send_message(request(&k, "same", "相同请求"))
            })
        })
        .collect();
    let sends: Vec<_> = joins
        .into_iter()
        .map(|j| j.join().unwrap().unwrap())
        .collect();
    assert_eq!(sends.iter().filter(|s| !s.already_accepted).count(), 1);
    assert!(sends.iter().all(|s| s.receipt == sends[0].receipt));
    assert_eq!(core.get_session(&k).unwrap().messages.len(), 2);
    drop(core);
    let core = f.open();
    let retry = core.send_message(request(&k, "same", "相同请求")).unwrap();
    assert!(retry.already_accepted);
    assert_eq!(
        core.send_message(request(&k, "same", "changed"))
            .unwrap_err()
            .code,
        ErrorCode::Conflict
    );
}
#[test]
fn queue_keeps_accepted_configuration_and_only_one_runtime_owner() {
    let f = Fixture::new();
    let core = f.open();
    let k = create(&core, "s");
    let first = core.send_message(request(&k, "first", "a")).unwrap();
    let second = core.send_message(request(&k, "second", "b")).unwrap();
    let before = core.get_session(&k).unwrap();
    core.patch_session(
        &k,
        before.last_seq,
        SessionPatch {
            permission_mode: Some(PermissionMode::Execute),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(core
        .claim_next_turn(&k, binding(PermissionMode::Execute))
        .is_err());
    let lease = core
        .claim_next_turn(&k, binding(PermissionMode::Ask))
        .unwrap();
    assert_eq!(lease.turn_id(), &first.receipt.turn_id);
    assert!(core
        .claim_next_turn(&k, binding(PermissionMode::Ask))
        .is_err());
    let running = core.get_session(&k).unwrap();
    assert_eq!(
        core.patch_session(
            &k,
            running.last_seq,
            SessionPatch {
                connection: Some(Default::default()),
                ..Default::default()
            }
        )
        .unwrap_err()
        .code,
        ErrorCode::Busy
    );
    core.finish_turn(&lease, FinishStatus::Completed, None)
        .unwrap();
    let next = core
        .claim_next_turn(&k, binding(PermissionMode::Ask))
        .unwrap();
    assert_eq!(next.turn_id(), &second.receipt.turn_id);
    core.finish_turn(&next, FinishStatus::Completed, None)
        .unwrap();
}
#[test]
fn runtime_frames_are_idempotent_and_bound_to_an_unforgeable_live_owner() {
    let f = Fixture::new();
    let core = f.open();
    let a = create(&core, "a");
    let b = create(&core, "b");
    core.send_message(request(&a, "send", "a")).unwrap();
    core.send_message(request(&b, "send", "b")).unwrap();
    let lease = core
        .claim_next_turn(&a, binding(PermissionMode::Ask))
        .unwrap();
    let seq = core
        .apply_runtime(&lease, id("input"), text("text", "你好🌍"))
        .unwrap();
    assert_eq!(
        seq,
        core.apply_runtime(&lease, id("input"), text("text", "你好🌍"))
            .unwrap()
    );
    assert_eq!(assistant_text(&core.get_session(&a).unwrap()), "你好🌍");
    assert!(core
        .apply_runtime(&lease, id("input"), text("text", "changed"))
        .is_err());
    assert!(assistant_text(&core.get_session(&b).unwrap()).is_empty());
    let other = Fixture::new();
    let other_core = other.open();
    assert_eq!(
        other_core
            .apply_runtime(&lease, id("foreign"), text("text", "wrong core"))
            .unwrap_err()
            .code,
        ErrorCode::StaleLease
    );
    let final_view = core
        .finish_turn(&lease, FinishStatus::Completed, None)
        .unwrap();
    assert_eq!(
        core.apply_runtime(&lease, id("late"), text("text", "late"))
            .unwrap_err()
            .code,
        ErrorCode::StaleLease
    );
    assert_eq!(core.get_session(&a).unwrap().last_seq, final_view.last_seq);
}
#[test]
fn tool_lifecycle_blocks_premature_completion_and_survives_cache_loss() {
    let f = Fixture::new();
    let core = f.open();
    let k = create(&core, "s");
    core.send_message(request(&k, "send", "tool")).unwrap();
    let lease = core
        .claim_next_turn(&k, binding(PermissionMode::Ask))
        .unwrap();
    core.apply_runtime(
        &lease,
        id("tool-start"),
        RuntimeUpdate::ToolStarted {
            tool_call_id: id("call"),
            name: "Read".into(),
            input: json!({"path":"hello.txt"}),
        },
    )
    .unwrap();
    assert!(core
        .finish_turn(&lease, FinishStatus::Completed, None)
        .is_err());
    assert!(core
        .apply_runtime(
            &lease,
            id("bad-result"),
            RuntimeUpdate::ToolFinished {
                tool_call_id: id("missing"),
                status: ToolStatus::Succeeded,
                output: json!("bad")
            }
        )
        .is_err());
    core.apply_runtime(
        &lease,
        id("result"),
        RuntimeUpdate::ToolFinished {
            tool_call_id: id("call"),
            status: ToolStatus::Succeeded,
            output: json!({"text":"read content"}),
        },
    )
    .unwrap();
    core.apply_runtime(&lease, id("delta"), text("reply", "完成"))
        .unwrap();
    let view = core
        .finish_turn(&lease, FinishStatus::Completed, None)
        .unwrap();
    drop(lease);
    drop(core);
    for file in ["summary.json", "snapshot.json"] {
        fs::remove_file(f.session_dir(&k).join(file)).unwrap();
    }
    let core = f.open();
    assert_eq!(core.get_session(&k).unwrap(), view);
    assert_eq!(
        core.list_sessions(&k.workspace_id).unwrap()[0]
            .summary
            .as_ref()
            .unwrap()
            .last_seq,
        view.last_seq
    );
}
#[test]
fn cancellation_rejects_late_output_and_dropped_owner_interrupts_tools() {
    let f = Fixture::new();
    let core = f.open();
    let k = create(&core, "s");
    let receipt = core
        .send_message(request(&k, "send", "hello"))
        .unwrap()
        .receipt;
    let lease = core
        .claim_next_turn(&k, binding(PermissionMode::Ask))
        .unwrap();
    core.apply_runtime(
        &lease,
        id("tool"),
        RuntimeUpdate::ToolStarted {
            tool_call_id: id("call"),
            name: "Bash".into(),
            input: json!({"command":"sleep"}),
        },
    )
    .unwrap();
    core.cancel_turn(&k, &receipt.turn_id).unwrap();
    assert!(lease.cancel_requested().unwrap());
    assert!(core
        .apply_runtime(&lease, id("late"), text("reply", "late"))
        .is_err());
    assert!(core
        .finish_turn(&lease, FinishStatus::Completed, None)
        .is_err());
    drop(lease);
    let view = core.get_session(&k).unwrap();
    assert_eq!(view.turns[0].status, TurnStatus::Interrupted);
    assert_eq!(view.tools[&id("call")].status, ToolStatus::Interrupted);
    assert_eq!(view.active_turn_id, None);
    let retry = core.cancel_turn(&k, &receipt.turn_id).unwrap();
    assert_eq!(retry.last_seq, view.last_seq);
}
#[test]
fn queued_cancellation_does_not_start_a_runtime() {
    let f = Fixture::new();
    let core = f.open();
    let k = create(&core, "s");
    let receipt = core
        .send_message(request(&k, "send", "queued"))
        .unwrap()
        .receipt;
    let view = core.cancel_turn(&k, &receipt.turn_id).unwrap();
    assert_eq!(view.turns[0].status, TurnStatus::Cancelled);
    assert!(core
        .claim_next_turn(&k, binding(PermissionMode::Ask))
        .is_err());
}
#[test]
fn archive_preserves_history_and_duplicate_send_is_safe_after_archive() {
    let f = Fixture::new();
    let core = f.open();
    let k = create(&core, "s");
    core.send_message(request(&k, "send", "hello")).unwrap();
    let before = core.get_session(&k).unwrap();
    let archived = core
        .patch_session(
            &k,
            before.last_seq,
            SessionPatch {
                archived: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(archived.messages, before.messages);
    assert!(
        core.send_message(request(&k, "send", "hello"))
            .unwrap()
            .already_accepted
    );
    assert!(core.send_message(request(&k, "new", "hello")).is_err());
    assert!(core
        .claim_next_turn(&k, binding(PermissionMode::Ask))
        .is_err());
    assert!(core
        .patch_session(
            &k,
            before.last_seq,
            SessionPatch {
                title: Change::Set("stale".into()),
                ..Default::default()
            }
        )
        .is_err());
}
#[test]
fn subscribers_receive_only_their_session_and_slow_clients_can_replay() {
    let f = Fixture::new();
    let core = f.open();
    let a = create(&core, "a");
    let b = create(&core, "b");
    let sub = core.subscribe(&a).unwrap();
    let other = core.subscribe(&b).unwrap();
    core.send_message(request(&a, "send", "stream")).unwrap();
    let lease = core
        .claim_next_turn(&a, binding(PermissionMode::Ask))
        .unwrap();
    for index in 0..90 {
        core.apply_runtime(&lease, id(&format!("input-{index}")), text("reply", "界"))
            .unwrap();
    }
    core.finish_turn(&lease, FinishStatus::Completed, None)
        .unwrap();
    assert!(other.events.try_recv().is_err());
    let delivered: Vec<_> = sub.events.try_iter().collect();
    assert_eq!(delivered.len(), 64);
    assert!(matches!(
        sub.events.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Disconnected)
    ));
    let after = delivered
        .last()
        .map(|e| e.seq)
        .unwrap_or(sub.snapshot.last_seq);
    let page = core.read_events(&a, after, 500).unwrap();
    assert!(!page.has_more);
    assert_eq!(page.events[0].seq, after + 1);
    assert_eq!(page.events.last().unwrap().seq, page.last_seq);
    assert!(core.read_events(&a, page.last_seq + 1, 1).is_err());
}
#[test]
fn incomplete_tail_is_preserved_then_repaired_but_complete_corruption_is_not() {
    let f = Fixture::new();
    let core = f.open();
    let k = create(&core, "s");
    let view = core.get_session(&k).unwrap();
    drop(core);
    let path = f.session_dir(&k).join("events.jsonl");
    let original = fs::read(&path).unwrap();
    let mut interrupted = original.clone();
    interrupted.extend_from_slice(b"{\"seq\":2");
    fs::write(&path, &interrupted).unwrap();
    let core = f.open();
    assert_eq!(core.get_session(&k).unwrap(), view);
    assert_eq!(fs::read(&path).unwrap(), original);
    assert!(fs::read_dir(f.session_dir(&k)).unwrap().any(|e| e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with("incomplete-tail-")));
    drop(core);
    let corrupted = String::from_utf8(original).unwrap().replace("todo", "done");
    fs::write(&path, &corrupted).unwrap();
    let core = f.open();
    assert_eq!(
        core.get_session(&k).unwrap_err().code,
        ErrorCode::CorruptStore
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), corrupted);
}
#[test]
fn truncated_committed_history_is_rejected_even_with_valid_remaining_records() {
    let f = Fixture::new();
    let core = f.open();
    let k = create(&core, "s");
    let path = f.session_dir(&k).join("events.jsonl");
    let prefix = fs::read(&path).unwrap();
    core.send_message(request(&k, "send", "durable")).unwrap();
    drop(core);
    fs::write(&path, prefix).unwrap();
    let core = f.open();
    assert_eq!(
        core.get_session(&k).unwrap_err().code,
        ErrorCode::CorruptStore
    );
}
#[test]
fn failed_journal_write_does_not_reserve_request_or_publish_success() {
    let f = Fixture::new();
    let core = f.open();
    let k = create(&core, "s");
    let sub = core.subscribe(&k).unwrap();
    let path = f.session_dir(&k).join("events.jsonl");
    let saved = path.with_extension("saved");
    fs::rename(&path, &saved).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(core.send_message(request(&k, "send", "hello")).is_err());
    assert!(sub.events.try_recv().is_err());
    drop(core);
    fs::remove_dir(&path).unwrap();
    fs::rename(saved, &path).unwrap();
    let core = f.open();
    assert!(
        !core
            .send_message(request(&k, "send", "hello"))
            .unwrap()
            .already_accepted
    );
}
#[test]
fn uncertain_commit_is_reconciled_without_reaccepting_or_reexecuting() {
    let f = Fixture::new();
    let core = f.open();
    let k = create(&core, "s");
    let sub = core.subscribe(&k).unwrap();
    let head = f.session_dir(&k).join("commit.json");
    let saved = head.with_extension("saved");
    fs::rename(&head, &saved).unwrap();
    fs::create_dir(&head).unwrap();
    assert!(core.send_message(request(&k, "send", "uncertain")).is_err());
    assert!(sub.events.try_recv().is_err());
    assert_eq!(
        core.get_session(&k).unwrap_err().code,
        ErrorCode::StorageUnavailable
    );
    drop(core);
    fs::remove_dir(&head).unwrap();
    fs::rename(saved, &head).unwrap();
    let core = f.open();
    let retry = core.send_message(request(&k, "send", "uncertain")).unwrap();
    assert!(retry.already_accepted);
    let view = core.get_session(&k).unwrap();
    assert_eq!(view.messages.len(), 2);
    assert_eq!(view.turns[0].status, TurnStatus::Queued);
}
#[test]
fn invalid_identifiers_and_cross_scope_journals_are_rejected() {
    for value in ["", "../outside", "a/b", "a\\b", ".", "中文"] {
        assert!(Id::new(value).is_err());
        assert!(serde_json::from_value::<Id>(json!(value)).is_err());
    }
    let f = Fixture::new();
    let core = f.open();
    let a = create(&core, "a");
    let b = create(&core, "b");
    drop(core);
    fs::copy(
        f.session_dir(&a).join("events.jsonl"),
        f.session_dir(&b).join("events.jsonl"),
    )
    .unwrap();
    let core = f.open();
    assert_eq!(
        core.get_session(&b).unwrap_err().code,
        ErrorCode::CorruptStore
    );
    assert!(core.get_session(&a).is_ok());
}
#[test]
fn writer_lock_is_exclusive_and_released_on_drop() {
    let f = Fixture::new();
    let core = f.open();
    assert!(matches!(
        Core::open(&f.0),
        Err(CoreError {
            code: ErrorCode::Busy,
            ..
        })
    ));
    core.create_workspace(id("w"), "Writer".into()).unwrap();
    drop(core);
    assert_eq!(f.open().list_workspaces().unwrap().len(), 1);
}
#[test]
fn crash_worker() {
    let Some(root) = std::env::var_os("LOOM_CORE_CRASH_FIXTURE") else {
        return;
    };
    let core = Core::open(Path::new(&root)).unwrap();
    let k = key("crash");
    let lease = core
        .claim_next_turn(&k, binding(PermissionMode::Ask))
        .unwrap();
    core.apply_runtime(&lease, id("delta"), text("reply", "partial before crash"))
        .unwrap();
    std::process::exit(0);
}
#[test]
fn crash_releases_writer_and_recovery_interrupts_without_replaying() {
    let f = Fixture::new();
    let core = f.open();
    let k = create(&core, "crash");
    core.send_message(request(&k, "send", "run")).unwrap();
    drop(core);
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "crash_worker", "--nocapture"])
        .env("LOOM_CORE_CRASH_FIXTURE", &f.0)
        .status()
        .unwrap();
    assert!(status.success());
    let core = f.open();
    let view = core.get_session(&k).unwrap();
    assert_eq!(view.turns[0].status, TurnStatus::Interrupted);
    assert_eq!(assistant_text(&view), "partial before crash");
    assert!(
        core.send_message(request(&k, "send", "run"))
            .unwrap()
            .already_accepted
    );
    assert!(core
        .claim_next_turn(&k, binding(PermissionMode::Ask))
        .is_err());
}
#[cfg(unix)]
#[test]
fn symlink_session_and_derived_cache_cannot_redirect_storage() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    let outside = Fixture::new();
    let core = f.open();
    let k = create(&core, "s");
    let target = outside.0.join("outside.json");
    fs::write(&target, "untouched").unwrap();
    let summary = f.session_dir(&k).join("summary.json");
    fs::remove_file(&summary).unwrap();
    symlink(&target, &summary).unwrap();
    let view = core.get_session(&k).unwrap();
    core.patch_session(
        &k,
        view.last_seq,
        SessionPatch {
            flagged: Some(true),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(fs::read_to_string(&target).unwrap(), "untouched");
    assert!(!core.diagnostics().unwrap().is_empty());
    drop(core);
    let directory = f.session_dir(&key("link"));
    symlink(&outside.0, &directory).unwrap();
    let core = f.open();
    assert_eq!(
        core.get_session(&key("link")).unwrap_err().code,
        ErrorCode::CorruptStore
    );
}

#[test]
fn unread_cursor_does_not_move_backwards_and_empty_cancelled_turn_is_not_unread() {
    let f = Fixture::new();
    let core = f.open();
    let k = create(&core, "read");
    let mut receipts = Vec::new();
    for (request_id, answer) in [("one", "first"), ("two", "second")] {
        let receipt = core
            .send_message(request(&k, request_id, "hello"))
            .unwrap()
            .receipt;
        let lease = core
            .claim_next_turn(&k, binding(PermissionMode::Ask))
            .unwrap();
        core.apply_runtime(&lease, id("text"), text("reply", answer))
            .unwrap();
        core.finish_turn(&lease, FinishStatus::Completed, None)
            .unwrap();
        assert!(
            core.list_sessions(&k.workspace_id).unwrap()[0]
                .summary
                .as_ref()
                .unwrap()
                .has_unread
        );
        core.mark_read(&k, receipt.assistant_message_id.clone())
            .unwrap();
        receipts.push(receipt);
    }
    let latest = core.get_session(&k).unwrap();
    let late = core
        .mark_read(&k, receipts[0].assistant_message_id.clone())
        .unwrap();
    assert_eq!(late.last_seq, latest.last_seq);
    let queued = core
        .send_message(request(&k, "three", "cancel before running"))
        .unwrap();
    core.cancel_turn(&k, &queued.receipt.turn_id).unwrap();
    assert!(
        !core.list_sessions(&k.workspace_id).unwrap()[0]
            .summary
            .as_ref()
            .unwrap()
            .has_unread
    );
}
#[test]
fn invalid_create_configuration_does_not_poison_the_session_id() {
    let f = Fixture::new();
    let core = f.open();
    core.create_workspace(id("workspace"), "Workspace".into())
        .unwrap();
    let k = key("retry-create");
    let mut input = CreateSession {
        key: k.clone(),
        title: None,
        project_id: None,
        permission_mode: PermissionMode::Ask,
        connection: ConnectionSelection {
            model: Some("\n".into()),
            ..Default::default()
        },
    };
    assert!(core.create_session(input.clone()).is_err());
    assert!(!f.session_dir(&k).exists());
    input.connection.model = Some("valid-model".into());
    assert!(core.create_session(input.clone()).is_ok());
    assert!(core.create_session(input).is_ok());
}
#[cfg(unix)]
#[test]
fn project_replaced_by_symlink_cannot_be_claimed_as_its_old_directory() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    let project = Fixture::new();
    let outside = Fixture::new();
    let core = f.open();
    core.create_workspace(id("workspace"), "Workspace".into())
        .unwrap();
    let registered = core
        .register_project(id("workspace"), id("project"), "Project".into(), &project.0)
        .unwrap();
    let k = key("s");
    core.create_session(CreateSession {
        key: k.clone(),
        title: None,
        project_id: Some(registered.id),
        permission_mode: PermissionMode::Ask,
        connection: Default::default(),
    })
    .unwrap();
    core.send_message(request(&k, "send", "work")).unwrap();
    fs::remove_dir(&project.0).unwrap();
    symlink(&outside.0, &project.0).unwrap();
    let mut selected = binding(PermissionMode::Ask);
    selected.working_directory = Some(registered.directory);
    assert_eq!(
        core.claim_next_turn(&k, selected).err().unwrap().code,
        ErrorCode::Conflict
    );
    assert_eq!(
        core.get_session(&k).unwrap().turns[0].status,
        TurnStatus::Queued
    );
    fs::remove_file(&project.0).unwrap();
}

fn run_host(root: &Path, requests: &[serde_json::Value]) -> Vec<serde_json::Value> {
    use std::io::Write;
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_loom-core-host"))
        .arg(root)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    for request in requests {
        writeln!(stdin, "{}", request).unwrap();
    }
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
#[test]
fn jsonl_transport_persists_real_commands_and_exposes_no_runtime_mutation() {
    let f = Fixture::new();
    let scope = json!({"workspaceId":"workspace","sessionId":"transport"});
    let req = |id: &str, command: serde_json::Value| json!({"protocolVersion":1,"requestId":id,"command":command});
    let send = json!({"method":"send_message","params":{"key":scope,"clientRequestId":"send","text":"Through JSONL"}});
    let replies = run_host(
        &f.0,
        &[
            req(
                "w",
                json!({"method":"create_workspace","params":{"id":"workspace","name":"Workspace"}}),
            ),
            req(
                "s",
                json!({"method":"create_session","params":{"key":scope}}),
            ),
            req("send", send.clone()),
            req("retry", send),
            req(
                "events",
                json!({"method":"read_events","params":{"key":scope,"after":0,"limit":500}}),
            ),
            req(
                "forged",
                json!({"method":"runtime_applied","params":{"key":scope,"text":"forged"}}),
            ),
            json!({"protocolVersion":99,"requestId":"future","command":{"method":"create_workspace","params":{"id":"future","name":"Future"}}}),
            req(
                "extra",
                json!({"method":"get_session","params":{"key":scope,"unexpected":"reject"}}),
            ),
        ],
    );
    assert_eq!(replies.len(), 8);
    assert_eq!(replies[5]["requestId"], "forged");
    assert_eq!(replies[7]["requestId"], "extra");
    assert_eq!(replies[2]["result"]["data"]["alreadyAccepted"], false);
    assert_eq!(replies[3]["result"]["data"]["alreadyAccepted"], true);
    assert_eq!(
        replies[4]["result"]["data"]["events"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    for reply in &replies[5..] {
        assert_eq!(reply["status"], "failure");
    }
    let restored = run_host(
        &f.0,
        &[
            req("list", json!({"method":"list_workspaces"})),
            req(
                "get",
                json!({"method":"get_session","params":{"key":scope}}),
            ),
        ],
    );
    assert_eq!(restored[0]["result"]["data"].as_array().unwrap().len(), 1);
    assert_eq!(
        restored[1]["result"]["data"]["turns"][0]["status"],
        "queued"
    );
    assert!(restored[1]["result"]["data"]["activeTurnId"].is_null());
}

#[test]
fn streaming_does_not_conflict_with_metadata_edits_but_other_metadata_edits_do() {
    let f = Fixture::new();
    let core = f.open();
    let k = create(&core, "edit-during-stream");
    core.send_message(request(&k, "send", "hello")).unwrap();
    let lease = core
        .claim_next_turn(&k, binding(PermissionMode::Ask))
        .unwrap();
    let before = core.get_session(&k).unwrap();
    core.apply_runtime(&lease, id("delta"), text("reply", "live text"))
        .unwrap();
    let renamed = core
        .patch_session(
            &k,
            before.last_seq,
            SessionPatch {
                title: Change::Set("Renamed while streaming".into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        renamed.metadata.title.as_deref(),
        Some("Renamed while streaming")
    );
    assert_eq!(
        core.patch_session(
            &k,
            before.last_seq,
            SessionPatch {
                title: Change::Set("Stale overwrite".into()),
                ..Default::default()
            }
        )
        .unwrap_err()
        .code,
        ErrorCode::Conflict
    );
    core.finish_turn(&lease, FinishStatus::Completed, None)
        .unwrap();
}

#[test]
fn protocol_and_message_budgets_reject_without_mutating_state() {
    let f = Fixture::new();
    let core = f.open();
    let k = create(&core, "budget");
    let before = core.get_session(&k).unwrap();
    assert!(matches!(
        core.dispatch_json(&vec![b' '; 8 * 1024 * 1024 + 1]).outcome,
        Outcome::Failure {
            error: CoreError {
                code: ErrorCode::InvalidInput,
                ..
            }
        }
    ));
    assert_eq!(
        core.send_message(request(&k, "oversized", &"x".repeat(MAX_MESSAGE_BYTES + 1)))
            .unwrap_err()
            .code,
        ErrorCode::InvalidInput
    );
    assert_eq!(core.get_session(&k).unwrap(), before);
}

#[test]
fn session_list_stays_lightweight_when_notes_are_large() {
    let f = Fixture::new();
    let core = f.open();
    let k = create(&core, "notes");
    let before = core.get_session(&k).unwrap();
    let notes = "n".repeat(64 * 1024);
    core.patch_session(
        &k,
        before.last_seq,
        SessionPatch {
            notes: Some(notes.clone()),
            ..Default::default()
        },
    )
    .unwrap();
    drop(core);
    let core = f.open();
    let list = core.list_sessions(&k.workspace_id).unwrap();
    let wire = serde_json::to_string(&list).unwrap();
    assert!(wire.len() < 4096);
    assert!(
        serde_json::to_value(&list).unwrap()[0]["summary"]["metadata"]
            .get("notes")
            .is_none()
    );
    assert_eq!(core.get_session(&k).unwrap().notes, notes);
}

#[test]
fn evicted_sessions_and_active_owners_remain_consistent() {
    let f = Fixture::new();
    let core = f.open();
    let active = create(&core, "active");
    core.send_message(request(&active, "send", "keep running"))
        .unwrap();
    let lease = core
        .claim_next_turn(&active, binding(PermissionMode::Ask))
        .unwrap();
    for i in 0..40 {
        let k = create(&core, &format!("session-{i}"));
        core.send_message(request(&k, "send", &format!("message {i}")))
            .unwrap();
    }
    assert_eq!(
        core.get_session(&key("session-0")).unwrap().messages.len(),
        2
    );
    core.apply_runtime(&lease, id("delta"), text("reply", "still owned"))
        .unwrap();
    core.finish_turn(&lease, FinishStatus::Completed, None)
        .unwrap();
    assert_eq!(core.list_sessions(&active.workspace_id).unwrap().len(), 41);
    assert_eq!(
        assistant_text(&core.get_session(&active).unwrap()),
        "still owned"
    );
}

#[test]
fn workspace_notifications_cover_background_lifecycle_without_stream_flooding() {
    let f = Fixture::new();
    let core = f.open();
    let key = create(&core, "observed");
    core.create_workspace(id("other"), "Other".into()).unwrap();
    let workspace = core.subscribe_workspace(&key.workspace_id).unwrap();
    let other = core.subscribe_workspace(&id("other")).unwrap();
    core.send_message(request(&key, "send", "background"))
        .unwrap();
    let lease = core
        .claim_next_turn(&key, binding(PermissionMode::Ask))
        .unwrap();
    assert_eq!(workspace.events.try_iter().count(), 2);
    core.apply_runtime(&lease, id("delta"), text("reply", "background output"))
        .unwrap();
    assert!(workspace.events.try_recv().is_err());
    core.finish_turn(&lease, FinishStatus::Completed, None)
        .unwrap();
    let notice = workspace.events.recv().unwrap();
    assert_eq!(notice.session_id, Some(key.session_id.clone()));
    assert_eq!(notice.seq, core.get_session(&key).unwrap().last_seq);
    assert!(other.events.try_recv().is_err());
    drop(workspace);
    core.register_project(key.workspace_id, id("project"), "Project".into(), &f.0)
        .unwrap();
}

#[test]
fn prepared_claim_and_preparation_failure_are_exact_queue_operations_and_durable() {
    let f = Fixture::new();
    let core = Core::open(&f.0).unwrap();
    let key = create(&core, "session");
    let first = core
        .send_message(request(&key, "first", "first"))
        .unwrap()
        .receipt;
    let second = core
        .send_message(request(&key, "second", "second"))
        .unwrap()
        .receipt;
    assert!(core
        .fail_queued_turn(&key, &second.turn_id, "wrong queue entry".into())
        .is_err());
    core.cancel_turn(&key, &first.turn_id).unwrap();
    assert!(core
        .claim_prepared_turn(&key, &first.turn_id, binding(PermissionMode::Ask))
        .is_err());
    assert!(core
        .fail_queued_turn(&key, &first.turn_id, "late error".into())
        .is_err());
    let view = core
        .fail_queued_turn(&key, &second.turn_id, "worker unavailable".into())
        .unwrap();
    assert_eq!(view.turns[1].status, TurnStatus::Failed);
    assert!(
        view.turns[1].started_at_ms.is_none()
            && view.turns[1].binding.is_none()
            && view.active_turn_id.is_none()
    );
    assert_eq!(
        core.fail_queued_turn(&key, &second.turn_id, "worker unavailable".into())
            .unwrap(),
        view
    );
    drop(core);
    let core = Core::open(&f.0).unwrap();
    assert_eq!(core.get_session(&key).unwrap(), view);
}
