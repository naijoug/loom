#![cfg(unix)]
use loom_core::*;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(1);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "loom-legacy-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }
    fn write(&self, path: &str, bytes: impl AsRef<[u8]>) {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    fn json(&self, path: &str, value: &Value) {
        self.write(path, serde_json::to_vec(value).unwrap());
    }
    fn chat(&self, id: &str, version: u32) -> Value {
        json!({"id":id,"projectPath":self.0,"agentId":"old-agent","title":"旧对话","permissionMode":"execute","messages":[
            {"id":"user","role":"user","content":"用户原文","status":"complete","createdAtMs":1},
            {"id":"assistant","role":"assistant","content":"","status":"streaming","createdAtMs":2}
        ],"createdAtMs":1,"updatedAtMs":2,"schemaVersion":version,"lastSeq":1,"revision":1,"turnStatus":"running","activeTurnId":"turn","resumeHandle":{"nativeSessionId":"secret-native-handle"}})
    }
    fn task(&self, id: &str) -> Value {
        json!({"id":id,"projectPath":self.0,"title":"旧任务","rawRequirement":"历史需求","status":"testing","createdAtMs":1,"updatedAtMs":2,"events":[],"commandRuns":[],"feedback":[]})
    }
    fn event(&self, id: &str, seq: u64, kind: &str, payload: Value) -> Value {
        json!({"schemaVersion":2,"projectKey":self.0,"sessionId":id,"seq":seq,"timestampMs":seq+1,"kind":kind,"payload":payload})
    }
    fn journal(&self, id: &str, events: &[Value], tail: &[u8]) {
        let mut bytes = Vec::new();
        for event in events {
            bytes.extend(serde_json::to_vec(event).unwrap());
            bytes.push(b'\n');
        }
        bytes.extend(tail);
        self.write(&format!(".loom/chat/v2/sessions/{id}/events.jsonl"), bytes);
    }
    fn inspect(&self) -> LegacyPreview {
        inspect_legacy_project(&self.0).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn hash(value: &Value) -> String {
    format!("{:x}", Sha256::digest(serde_json::to_vec(value).unwrap()))
}
fn has_issue(item: &LegacyItem, code: &str) -> bool {
    item.issues.iter().any(|i| i.code == code)
}
fn snapshot(path: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut result = BTreeMap::new();
    for entry in fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            result.extend(snapshot(&path));
        } else {
            result.insert(path.clone(), fs::read(path).unwrap());
        }
    }
    result
}

#[test]
fn reads_v1_bare_and_enveloped_tasks_without_modifying_source_or_exposing_resume() {
    let f = Fixture::new();
    f.json(".loom/chat/sessions/a.json", &f.chat("a", 1));
    f.json(".loom/tasks/b.json", &f.task("b"));
    f.json(
        ".loom/tasks/c.json",
        &json!({"schemaVersion":1,"data":f.task("c")}),
    );
    let before = snapshot(&f.0);
    let preview = f.inspect();
    assert_eq!(preview.items.len(), 3);
    assert!(preview.items.iter().all(|i| i.content_hash.is_some()));
    assert_eq!(preview.items[0].message_count, 2);
    assert!(has_issue(&preview.items[0], "runtime_reset"));
    assert!(!serde_json::to_string(&preview)
        .unwrap()
        .contains("secret-native-handle"));
    assert_eq!(snapshot(&f.0), before);
    assert_eq!(preview, f.inspect());
}

#[test]
fn replays_stream_parts_patches_and_leaves_partial_tail_and_stale_snapshot_unchanged() {
    let f = Fixture::new();
    let initial = f.chat("a", 2);
    let events=vec![f.event("a",1,"session_created",initial.clone()),f.event("a",2,"stream",json!({"sessionId":"a","turnId":"turn","messageId":"assistant","delta":"你好","done":false,"part":{"type":"tool","name":"Read","status":"complete"}})),f.event("a",3,"stream",json!({"sessionId":"a","turnId":"turn","messageId":"assistant","delta":"IGNORED-DONE","done":true})),f.event("a",4,"session_patch",json!({"fields":{"title":"更名","activeTurnId":null,"turnStatus":"idle"},"changedMessages":[],"appendedMessages":[]}))];
    f.journal("a", &events, b"{partial");
    f.json(".loom/chat/v2/sessions/a/snapshot.json", &initial);
    let before = snapshot(&f.0);
    let preview = f.inspect();
    let item = &preview.items[0];
    let mut expected = initial;
    expected["title"] = "更名".into();
    expected["activeTurnId"] = Value::Null;
    expected["turnStatus"] = "idle".into();
    expected["lastSeq"] = 4.into();
    expected["revision"] = 4.into();
    expected["updatedAtMs"] = 4.into();
    expected["messages"][1]["content"] = "你好".into();
    expected["messages"][1]["parts"] = json!([{"type":"tool","name":"Read","status":"complete"}]);
    assert_eq!(item.content_hash.as_deref(), Some(hash(&expected).as_str()));
    assert!(has_issue(item, "incomplete_tail"));
    assert!(has_issue(item, "stale_snapshot"));
    assert_eq!(snapshot(&f.0), before);
}

#[test]
fn v2_always_wins_and_corruption_never_falls_back_to_stale_v1() {
    let f = Fixture::new();
    f.json(".loom/chat/sessions/a.json", &f.chat("a", 1));
    f.journal("a", &[], b"corrupt\n");
    f.json(".loom/chat/sessions/b.json", &f.chat("b", 1));
    let before = snapshot(&f.0);
    let p = f.inspect();
    assert_eq!(p.items.len(), 2);
    assert_eq!(p.items[0].source_version, 2);
    assert!(p.items[0].content_hash.is_none());
    assert!(p.items[1].content_hash.is_some());
    assert!(p.issues.iter().any(|i| i.code == "v1_shadowed"));
    assert_eq!(snapshot(&f.0), before);
}

#[test]
fn rejects_journal_scope_gaps_unknown_patch_duplicate_messages_and_lost_history() {
    for scenario in 0..8 {
        let f = Fixture::new();
        let initial = f.chat("a", 2);
        let create = f.event("a", 1, "session_created", initial.clone());
        let mut next = f.event(
            "a",
            2,
            "session_patch",
            json!({"fields":{"title":"changed"}}),
        );
        match scenario {
            0=>next["seq"]=3.into(),1=>next["projectKey"]="/other".into(),2=>next["sessionId"]="other".into(),
            3=>next["payload"]["fields"]=json!({"messages":[]}),
            4=>next["payload"]["appendedMessages"]=json!([initial["messages"][0].clone()]),
            5=>next=f.event("a",2,"stream",json!({"sessionId":"a","turnId":"other","messageId":"assistant","done":false,"delta":"bad"})),
            6=>next["schemaVersion"]=3.into(),
            7=>{let mut snapshot=initial.clone();snapshot["lastSeq"]=99.into();snapshot["messages"]=false.into();f.json(".loom/chat/v2/sessions/a/snapshot.json",&snapshot);},
            _=>unreachable!()
        }
        f.journal("a", &[create, next], b"");
        assert!(
            f.inspect().items[0].content_hash.is_none(),
            "scenario {scenario}"
        );
    }
}

#[test]
fn warns_for_invalid_snapshot_without_repair_and_errors_on_newer_schema() {
    let f = Fixture::new();
    f.journal(
        "a",
        &[f.event("a", 1, "session_created", f.chat("a", 2))],
        b"",
    );
    f.write(".loom/chat/v2/sessions/a/snapshot.json", b"bad JSON");
    let before = snapshot(&f.0);
    let p = f.inspect();
    assert!(p.items[0].content_hash.is_some());
    assert!(has_issue(&p.items[0], "invalid_snapshot"));
    assert_eq!(snapshot(&f.0), before);
    f.json(
        ".loom/tasks/a.json",
        &json!({"schemaVersion":99,"data":f.task("a")}),
    );
    assert!(f
        .inspect()
        .items
        .iter()
        .find(|i| i.kind == "task")
        .unwrap()
        .content_hash
        .is_none());
}

#[test]
fn resource_hashes_missing_files_and_traversal_are_reported_without_outside_reads() {
    let f = Fixture::new();
    let outside = Fixture::new();
    outside.write("secret", b"outside-secret");
    f.write(".loom/tasks/a/attachments/image.png", b"PNG fixture");
    f.write("docs/plans/old.md", b"old plan");
    let mut task = f.task("a");
    task["finalPlanPath"] = json!(f.0.join("docs/plans/old.md"));
    task["feedback"] = json!([{"attachments":[{"storedPath":".loom/tasks/a/attachments/image.png"},{"storedPath":".loom/tasks/a/attachments/missing.png"},{"storedPath":"../escape"},{"storedPath":outside.0.join("secret")}]}]);
    f.json(".loom/tasks/a.json", &task);
    let p = f.inspect();
    let item = &p.items[0];
    assert!(has_issue(item, "missing_resource"));
    assert!(has_issue(item, "unsafe_resource"));
    assert!(has_issue(item, "unreadable_resource"));
    assert!(p
        .files
        .iter()
        .any(|f| f.path == "docs/plans/old.md" && f.sha256.is_some()));
    assert!(!p.files.iter().any(|f| f.path.contains("secret")));
    let before = p.fingerprint;
    f.write("docs/plans/old.md", b"changed plan");
    assert_ne!(before, f.inspect().fingerprint);
}

#[test]
fn rejects_symlinked_directories_files_and_fifo_without_following_or_blocking() {
    use std::{
        ffi::CString,
        os::unix::{ffi::OsStrExt, fs::symlink},
    };
    let f = Fixture::new();
    let outside = Fixture::new();
    outside.json("a.json", &f.chat("a", 1));
    fs::create_dir_all(f.0.join(".loom/chat")).unwrap();
    symlink(&outside.0, f.0.join(".loom/chat/sessions")).unwrap();
    assert!(f
        .inspect()
        .issues
        .iter()
        .any(|i| i.code == "directory_unreadable"));
    fs::remove_file(f.0.join(".loom/chat/sessions")).unwrap();
    fs::create_dir(f.0.join(".loom/chat/sessions")).unwrap();
    symlink(
        outside.0.join("a.json"),
        f.0.join(".loom/chat/sessions/a.json"),
    )
    .unwrap();
    let fifo = f.0.join(".loom/chat/sessions/b.json");
    let name = CString::new(fifo.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    let p = f.inspect();
    assert_eq!(p.items.len(), 2);
    assert!(p.items.iter().all(|i| i.content_hash.is_none()));
}

#[test]
fn inaccessible_v2_disables_v1_fallback() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    let outside = Fixture::new();
    f.json(".loom/chat/sessions/a.json", &f.chat("a", 1));
    symlink(&outside.0, f.0.join(".loom/chat/v2")).unwrap();
    let p = f.inspect();
    assert!(p.items[0].content_hash.is_none());
    assert!(p.issues.iter().any(|i| i.path == ".loom/chat/v2/sessions"));
}

#[test]
fn registered_project_replaced_by_symlink_cannot_redirect_preview() {
    use std::os::unix::fs::symlink;
    let data = Fixture::new();
    let container = Fixture::new();
    let outside = Fixture::new();
    let path = container.0.join("project");
    fs::create_dir(&path).unwrap();
    let core = Core::open(&data.0).unwrap();
    let id = |s: &str| Id::new(s).unwrap();
    core.create_workspace(id("w"), "workspace".into()).unwrap();
    core.register_project(id("w"), id("p"), "project".into(), &path)
        .unwrap();
    fs::remove_dir(&path).unwrap();
    symlink(&outside.0, &path).unwrap();
    let reply = core.dispatch(CoreRequest {
        protocol_version: 1,
        request_id: id("request"),
        command: CoreCommand::PreviewLegacyImport {
            workspace_id: id("w"),
            project_id: id("p"),
        },
    });
    assert!(matches!(
        reply.outcome,
        Outcome::Failure {
            error: CoreError {
                code: ErrorCode::Conflict,
                ..
            }
        }
    ));
}

#[test]
fn oversized_sparse_source_is_rejected_before_allocation() {
    let f = Fixture::new();
    f.write(".loom/tasks/a.json", b"");
    let file = fs::OpenOptions::new()
        .write(true)
        .open(f.0.join(".loom/tasks/a.json"))
        .unwrap();
    file.set_len(33 * 1024 * 1024).unwrap();
    let p = f.inspect();
    assert!(p.items[0].content_hash.is_none());
    assert!(p.items[0]
        .issues
        .iter()
        .any(|i| i.message.contains("budget")));
}

#[test]
fn preview_protocol_requires_project_membership_and_creates_no_sessions() {
    let data = Fixture::new();
    let project = Fixture::new();
    project.json(".loom/chat/sessions/a.json", &project.chat("a", 1));
    let core = Core::open(&data.0).unwrap();
    let id = |s: &str| Id::new(s).unwrap();
    core.create_workspace(id("w"), "workspace".into()).unwrap();
    core.create_workspace(id("other"), "other".into()).unwrap();
    core.register_project(id("w"), id("p"), "project".into(), &project.0)
        .unwrap();
    let request = |workspace: &str| CoreRequest {
        protocol_version: 1,
        request_id: id("request"),
        command: CoreCommand::PreviewLegacyImport {
            workspace_id: id(workspace),
            project_id: id("p"),
        },
    };
    match core.dispatch(request("w")).outcome {
        Outcome::Success {
            result: CoreResponse::LegacyPreview(p),
        } => assert_eq!(p.items.len(), 1),
        reply => panic!("{reply:?}"),
    }
    assert!(matches!(
        core.dispatch(request("other")).outcome,
        Outcome::Failure {
            error: CoreError {
                code: ErrorCode::NotFound,
                ..
            }
        }
    ));
    assert!(core.list_sessions(&id("w")).unwrap().is_empty());
}

#[test]
fn scanner_handles_the_repository_export_fixture_as_relocated_project_history() {
    let f = Fixture::new();
    let mut sample: Value = serde_json::from_str(include_str!(
        "../../../docs/dogfood/samples/chat-export/session.json"
    ))
    .unwrap();
    sample["projectPath"] = json!(f.0);
    let id = sample["id"].as_str().unwrap().to_owned();
    // This test relocates the sample into the supported project layout; standalone
    // export import is not silently treated as the original project's directory.
    f.journal(&id, &[f.event(&id, 1, "session_created", sample)], b"");
    let p = f.inspect();
    assert!(p.items[0].content_hash.is_some());
    assert_eq!(p.items[0].message_count, 2);
    assert!(has_issue(&p.items[0], "missing_resource"));
}

#[test]
fn rejects_mutation_of_previously_accepted_receipts_and_user_content() {
    for scenario in 0..2 {
        let f = Fixture::new();
        let mut initial = f.chat("a", 2);
        initial["sendReceipts"] = json!([{"clientRequestId":"request","turnId":"turn","userMessageId":"user","assistantMessageId":"assistant"}]);
        let payload = if scenario == 0 {
            json!({"fields":{"sendReceipts":[]}})
        } else {
            let mut message = initial["messages"][0].clone();
            message["content"] = "rewritten".into();
            json!({"fields":{},"changedMessages":[message]})
        };
        f.journal(
            "a",
            &[
                f.event("a", 1, "session_created", initial),
                f.event("a", 2, "session_patch", payload),
            ],
            b"",
        );
        assert!(f.inspect().items[0].content_hash.is_none());
    }
}

fn imported_setup(data: &Fixture, source: &Fixture) -> (Core, LegacyImportPlan) {
    let core = Core::open(&data.0).unwrap();
    let w = Id::new("w").unwrap();
    let p = Id::new("p").unwrap();
    core.create_workspace(w.clone(), "workspace".into())
        .unwrap();
    core.register_project(w.clone(), p.clone(), "project".into(), &source.0)
        .unwrap();
    let plan = core
        .prepare_legacy_import(&w, &p, &source.inspect().fingerprint)
        .unwrap();
    (core, plan)
}

#[test]
fn runtime_context_reads_imported_chat_without_native_resume_or_task_raw_records() {
    let data = Fixture::new();
    let source = Fixture::new();
    source.json(".loom/chat/sessions/chat.json", &source.chat("chat", 1));
    let (core, plan) = imported_setup(&data, &source);
    let report = core
        .execute_legacy_import(&plan.workspace_id, &plan.id)
        .unwrap();
    let key = report.results[0].session_key.as_ref().unwrap();
    let id = |s: &str| Id::new(s).unwrap();
    let connection = core
        .save_connection_metadata(SaveConnectionMetadata {
            id: id("runtime"),
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
                models: vec!["fixture".into()],
                default_model: "fixture".into(),
                supports_images: false,
                mid_stream_behavior: MidStreamBehavior::Queue,
            },
        })
        .unwrap();
    let selection = ConnectionSelection {
        connection_id: Some(connection.id.clone()),
        model: Some("fixture".into()),
        thinking: None,
    };
    let view = core.get_session(key).unwrap();
    core.patch_session(
        key,
        view.last_seq,
        SessionPatch {
            connection: Some(selection.clone()),
            ..Default::default()
        },
    )
    .unwrap();
    core.send_message(SendMessage {
        key: key.clone(),
        client_request_id: id("continue"),
        text: "Continue imported conversation".into(),
        attachment_ids: vec![],
    })
    .unwrap();
    let lease = core
        .claim_next_turn(
            key,
            RuntimeBinding {
                adapter: "fixture".into(),
                connection: selection,
                permission_mode: view.metadata.permission_mode,
                policy_version: 1,
                working_directory: Some(core.runtime_working_directory(key).unwrap()),
                connection_snapshot_digest: Some(connection.snapshot_digest().unwrap()),
                policy_snapshot_digest: None,
            },
        )
        .unwrap();
    let context = core.runtime_context(&lease, 100_000).unwrap();
    assert_eq!(context.history.len(), 2);
    assert_eq!(context.history[0].text, "用户原文");
    assert!(context.history[1].text.contains("interrupted"));
    assert!(!context
        .history
        .iter()
        .any(|m| m.text.contains("secret-native-handle")));
    assert_eq!(context.prompt, "Continue imported conversation");
}

#[test]
fn actual_import_survives_restart_preserves_binary_resources_and_never_creates_live_turns() {
    let data = Fixture::new();
    let source = Fixture::new();
    source.journal(
        "chat",
        &[source.event("chat", 1, "session_created", source.chat("chat", 2))],
        b"unfinished tail",
    );
    let mut task = source.task("task");
    task["feedback"] = json!([{"attachments":[{"storedPath":".loom/tasks/task/attachments/file.bin"},{"storedPath":"missing.png"}]}]);
    source.json(
        ".loom/tasks/task.json",
        &json!({"schemaVersion":1,"data":task}),
    );
    source.write(".loom/tasks/task/attachments/file.bin", [0, 255, 1, 127]);
    let before = snapshot(&source.0);
    let (core, plan) = imported_setup(&data, &source);
    drop(core);
    let core = Core::open(&data.0).unwrap();
    let report = core
        .execute_legacy_import(&plan.workspace_id, &plan.id)
        .unwrap();
    assert_eq!(report.results.len(), 2);
    assert!(report.results.iter().all(|r| r.status == "imported"));
    assert_eq!(snapshot(&source.0), before);
    for result in &report.results {
        let key = result.session_key.as_ref().unwrap();
        let view = core.get_session(key).unwrap();
        assert!(view.turns.is_empty() && view.messages.is_empty() && view.active_turn_id.is_none());
        assert_eq!(view.metadata.permission_mode, PermissionMode::Explore);
        assert_eq!(view.metadata.connection, ConnectionSelection::default());
        assert!(!serde_json::to_string(&view)
            .unwrap()
            .contains("secret-native-handle"));
        let page = core.read_import_history(key, 0, 1).unwrap();
        if result.kind == "chat" {
            assert_eq!(page.messages.len(), 1);
            assert!(page.has_more);
            let next = core.read_import_history(key, 1, 1).unwrap();
            assert_eq!(next.messages[0].status, "interrupted");
            assert_eq!(page.messages[0].content, "用户原文");
        } else {
            assert!(view.metadata.archived);
            assert!(page.messages.is_empty());
            let resource = page
                .resources
                .iter()
                .find(|r| r.source_path.ends_with("file.bin"))
                .unwrap();
            let chunk = core.read_import_resource(key, &resource.id, 0).unwrap();
            assert_eq!(chunk.bytes, [0, 255, 1, 127]);
            assert!(!chunk.has_more);
            assert!(result.issues.iter().any(|i| i.code == "missing_resource"));
        }
    }
    let restored = core
        .legacy_import_report(&plan.workspace_id, &plan.id)
        .unwrap();
    assert!(restored.completed_at_ms > 0);
    assert!(restored.results.iter().all(|r| r.status == "imported"));
    drop(core);
    let core = Core::open(&data.0).unwrap();
    assert_eq!(core.list_sessions(&plan.workspace_id).unwrap().len(), 2);
    let again = core
        .execute_legacy_import(&plan.workspace_id, &plan.id)
        .unwrap();
    assert!(again.results.iter().all(|r| r.status == "already_imported"));
    assert_eq!(snapshot(&source.0), before);
}

#[test]
fn duplicate_import_preserves_new_notes_and_messages_but_changed_source_never_overwrites() {
    let data = Fixture::new();
    let source = Fixture::new();
    source.json(".loom/chat/sessions/a.json", &source.chat("a", 1));
    let (core, plan) = imported_setup(&data, &source);
    let report = core
        .execute_legacy_import(&plan.workspace_id, &plan.id)
        .unwrap();
    let key = report.results[0].session_key.clone().unwrap();
    let current = core.get_session(&key).unwrap();
    core.patch_session(
        &key,
        current.last_seq,
        SessionPatch {
            notes: Some("new notes".into()),
            ..Default::default()
        },
    )
    .unwrap();
    core.send_message(SendMessage {
        key: key.clone(),
        client_request_id: Id::new("new-request").unwrap(),
        text: "new message".into(),
        attachment_ids: vec![],
    })
    .unwrap();
    let again = core
        .execute_legacy_import(&plan.workspace_id, &plan.id)
        .unwrap();
    assert_eq!(again.results[0].status, "already_imported");
    let view = core.get_session(&key).unwrap();
    assert_eq!(view.notes, "new notes");
    assert_eq!(view.turns.len(), 1);
    let mut changed = source.chat("a", 1);
    changed["title"] = "changed source".into();
    source.json(".loom/chat/sessions/a.json", &changed);
    let next = core
        .prepare_legacy_import(
            &plan.workspace_id,
            &plan.project_id,
            &source.inspect().fingerprint,
        )
        .unwrap();
    let conflicting = core
        .execute_legacy_import(&plan.workspace_id, &next.id)
        .unwrap();
    assert_eq!(conflicting.results[0].status, "failed");
    assert!(conflicting.results[0]
        .error
        .as_ref()
        .unwrap()
        .contains("different content"));
    assert_eq!(core.list_sessions(&plan.workspace_id).unwrap().len(), 1);
    assert_eq!(core.get_session(&key).unwrap().notes, "new notes");
}

#[test]
fn stale_previews_plans_and_changed_resources_reject_before_any_publication() {
    let data = Fixture::new();
    let source = Fixture::new();
    let mut task = source.task("a");
    task["finalPlanPath"] = "plan.md".into();
    source.json(".loom/tasks/a.json", &task);
    source.write("plan.md", b"first");
    let (core, plan) = imported_setup(&data, &source);
    source.write("plan.md", b"second");
    assert_eq!(
        core.prepare_legacy_import(
            &plan.workspace_id,
            &plan.project_id,
            &plan.preview.fingerprint
        )
        .unwrap_err()
        .code,
        ErrorCode::Conflict
    );
    assert_eq!(
        core.execute_legacy_import(&plan.workspace_id, &plan.id)
            .unwrap_err()
            .code,
        ErrorCode::Conflict
    );
    assert!(core.list_sessions(&plan.workspace_id).unwrap().is_empty());
    let plan_path = data
        .0
        .join(format!("workspaces/w/imports/plans/{}.json", plan.id));
    let mut record: Value = serde_json::from_slice(&fs::read(&plan_path).unwrap()).unwrap();
    record["plan"]["projectId"] = "other".into();
    fs::write(plan_path, serde_json::to_vec(&record).unwrap()).unwrap();
    assert_eq!(
        core.execute_legacy_import(&plan.workspace_id, &plan.id)
            .unwrap_err()
            .code,
        ErrorCode::CorruptStore
    );
}

#[test]
fn partial_bad_items_do_not_block_valid_import_and_unpublished_staging_recovers() {
    let data = Fixture::new();
    let source = Fixture::new();
    source.json(".loom/chat/sessions/good.json", &source.chat("good", 1));
    source.write(".loom/tasks/bad.json", b"corrupt");
    let (core, plan) = imported_setup(&data, &source);
    let stage = data
        .0
        .join(format!("workspaces/w/imports/staging/{}", plan.id));
    fs::create_dir_all(&stage).unwrap();
    fs::write(stage.join("partial-copy"), b"uncommitted").unwrap();
    assert!(core.list_sessions(&plan.workspace_id).unwrap().is_empty());
    let report = core
        .execute_legacy_import(&plan.workspace_id, &plan.id)
        .unwrap();
    assert_eq!(
        report
            .results
            .iter()
            .filter(|r| r.status == "imported")
            .count(),
        1
    );
    assert_eq!(
        report
            .results
            .iter()
            .filter(|r| r.status == "skipped")
            .count(),
        1
    );
    assert!(!stage.exists());
}

#[test]
fn corrupted_or_redirected_import_resources_are_rejected_after_restart() {
    use std::os::unix::fs::symlink;
    let data = Fixture::new();
    let source = Fixture::new();
    source.json(".loom/chat/sessions/a.json", &source.chat("a", 1));
    let (core, plan) = imported_setup(&data, &source);
    let report = core
        .execute_legacy_import(&plan.workspace_id, &plan.id)
        .unwrap();
    let key = report.results[0].session_key.as_ref().unwrap();
    let page = core.read_import_history(key, 0, 100).unwrap();
    let resource = &page.resources[0];
    let file = data.0.join(format!(
        "workspaces/w/sessions/{}/import/resources/{}",
        key.session_id, resource.id
    ));
    fs::write(&file, b"tampered").unwrap();
    drop(core);
    let core = Core::open(&data.0).unwrap();
    assert_eq!(
        core.read_import_resource(key, &resource.id, 0)
            .unwrap_err()
            .code,
        ErrorCode::CorruptStore
    );
    fs::remove_file(&file).unwrap();
    symlink(source.0.join(".loom/chat/sessions/a.json"), &file).unwrap();
    assert!(core.read_import_resource(key, &resource.id, 0).is_err());
    assert!(core
        .read_import_resource(key, &Id::new("foreign-resource").unwrap(), 0)
        .is_err());
    let again = core
        .execute_legacy_import(&plan.workspace_id, &plan.id)
        .unwrap();
    assert_eq!(again.results[0].status, "failed");
}

#[test]
fn import_resource_chunking_is_exact_and_checks_bounds_and_workspace_scope() {
    let data = Fixture::new();
    let source = Fixture::new();
    let mut task = source.task("a");
    task["finalPlanPath"] = "large.bin".into();
    source.json(".loom/tasks/a.json", &task);
    let bytes: Vec<u8> = (0..100_000).map(|i| (i % 251) as u8).collect();
    source.write("large.bin", &bytes);
    let (core, plan) = imported_setup(&data, &source);
    let report = core
        .execute_legacy_import(&plan.workspace_id, &plan.id)
        .unwrap();
    let key = report.results[0].session_key.as_ref().unwrap();
    let page = core.read_import_history(key, 0, 100).unwrap();
    let resource = page
        .resources
        .iter()
        .find(|r| r.source_path == "large.bin")
        .unwrap();
    let first = core.read_import_resource(key, &resource.id, 0).unwrap();
    assert_eq!(first.bytes.len(), 65_536);
    assert!(first.has_more);
    let next = core
        .read_import_resource(key, &resource.id, first.bytes.len() as u64)
        .unwrap();
    assert!(!next.has_more);
    let mut readback = first.bytes;
    readback.extend(next.bytes);
    assert_eq!(readback, bytes);
    assert_eq!(
        core.read_import_resource(key, &resource.id, u64::MAX)
            .unwrap_err()
            .code,
        ErrorCode::InvalidInput
    );
    core.create_workspace(Id::new("other").unwrap(), "other".into())
        .unwrap();
    let foreign = SessionKey {
        workspace_id: Id::new("other").unwrap(),
        session_id: key.session_id.clone(),
    };
    assert!(core.read_import_history(&foreign, 0, 100).is_err());
    assert!(core
        .execute_legacy_import(&foreign.workspace_id, &plan.id)
        .is_err());
}
