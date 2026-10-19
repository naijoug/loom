use loom_core::*;
use serde_json::json;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "loom-permissions-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path.canonicalize().unwrap())
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
fn id(v: &str) -> Id {
    Id::new(v).unwrap()
}
fn setup(core: &Core) -> SessionKey {
    core.create_workspace(id("w"), "Workspace".into()).unwrap();
    let key = SessionKey {
        workspace_id: id("w"),
        session_id: id("s"),
    };
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
fn save(
    core: &Core,
    scope: PermissionScope,
    operation: &str,
    revision: Option<u64>,
    config: Option<serde_json::Value>,
) -> Result<PermissionDocument> {
    core.save_permission_document(SavePermissionRequest {
        scope,
        operation_id: id(operation),
        expected_revision: revision,
        config,
    })
}
#[test]
fn permissions_are_scoped_conflict_checked_idempotent_resettable_and_durable() {
    let f = Fixture::new();
    let core = f.open();
    setup(&core);
    core.create_workspace(id("other"), "Other".into()).unwrap();
    let app = save(
        &core,
        PermissionScope::Application,
        "app",
        None,
        Some(json!({"allowedBashPatterns":["^ls"]})),
    )
    .unwrap();
    let scope = PermissionScope::Workspace {
        workspace_id: id("w"),
    };
    let config = Some(json!({"allowedWritePaths":["/tmp/work/**"]}));
    let ws = save(&core, scope.clone(), "create", None, config.clone()).unwrap();
    assert_eq!(
        save(&core, scope.clone(), "create", None, config).unwrap(),
        ws
    );
    assert!(save(&core, scope.clone(), "create", None, None).is_err());
    assert!(save(&core, scope.clone(), "stale", None, None).is_err());
    let source = PermissionScope::Source {
        workspace_id: id("w"),
        source_id: id("src"),
    };
    save(&core, source, "source", None, Some(json!({}))).unwrap();
    assert_eq!(core.permission_documents(&id("other")).unwrap(), vec![app]);
    let reset = save(&core, scope, "reset", Some(1), None).unwrap();
    assert_eq!(reset.revision, 2);
    assert!(reset.config.is_none());
    let before = core.permission_documents(&id("w")).unwrap();
    drop(core);
    assert_eq!(f.open().permission_documents(&id("w")).unwrap(), before);
}
#[test]
fn invalid_scopes_and_configs_cannot_publish_and_user_wire_cannot_bypass_host_validation() {
    let f = Fixture::new();
    let core = f.open();
    setup(&core);
    assert!(save(
        &core,
        PermissionScope::Workspace {
            workspace_id: id("missing")
        },
        "bad",
        None,
        Some(json!({}))
    )
    .is_err());
    assert!(save(
        &core,
        PermissionScope::Application,
        "array",
        None,
        Some(json!([]))
    )
    .is_err());
    assert!(save(
        &core,
        PermissionScope::Application,
        "oversize",
        None,
        Some(json!({"large":"x".repeat(256*1024)}))
    )
    .is_err());
    assert!(core.permission_documents(&id("w")).unwrap().is_empty());
    let reply=core.dispatch_json(br#"{"protocolVersion":1,"requestId":"forged","command":{"method":"save_permission_document","params":{}}}"#);
    assert!(matches!(reply.outcome, Outcome::Failure { .. }));
}
#[test]
fn failed_catalog_write_does_not_publish_permissions() {
    let f = Fixture::new();
    let core = f.open();
    setup(&core);
    let path = f.0.join("catalog/events.jsonl");
    let contents = fs::read(&path).unwrap();
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(save(
        &core,
        PermissionScope::Application,
        "fail",
        None,
        Some(json!({}))
    )
    .is_err());
    fs::remove_dir(&path).unwrap();
    fs::write(&path, contents).unwrap();
    drop(core);
    assert!(f.open().permission_documents(&id("w")).unwrap().is_empty());
}
#[test]
fn policy_snapshot_is_immutable_hashed_turn_scoped_and_survives_restart() {
    let f = Fixture::new();
    let core = f.open();
    let key = setup(&core);
    let snapshot = json!({"schemaVersion":1,"rules":{"allowed":["read"],"scope":"w/s"}});
    let digest = core.save_policy_snapshot(&key, &snapshot).unwrap();
    assert_eq!(core.save_policy_snapshot(&key, &snapshot).unwrap(), digest);
    let turn = core
        .send_message(SendMessage {
            key: key.clone(),
            client_request_id: id("send"),
            text: "hello".into(),
            attachment_ids: vec![],
        })
        .unwrap()
        .receipt
        .turn_id;
    let lease = core
        .claim_next_turn(
            &key,
            RuntimeBinding {
                adapter: "fixture".into(),
                connection: Default::default(),
                permission_mode: PermissionMode::Ask,
                policy_version: 1,
                working_directory: None,
                connection_snapshot_digest: None,
                policy_snapshot_digest: Some(digest.clone()),
            },
        )
        .unwrap();
    core.finish_turn(&lease, FinishStatus::Completed, None)
        .unwrap();
    drop(lease);
    assert_eq!(core.read_policy_snapshot(&key, &turn).unwrap(), snapshot);
    assert!(core
        .read_policy_snapshot(&key, &id("foreign-turn"))
        .is_err());
    drop(core);
    let core = f.open();
    assert_eq!(core.read_policy_snapshot(&key, &turn).unwrap(), snapshot);
    let path =
        f.0.join(format!("workspaces/w/sessions/s/policies/{digest}.json"));
    fs::write(&path, b"{\"tampered\":true}").unwrap();
    assert!(core.read_policy_snapshot(&key, &turn).is_err());
    assert!(core.save_policy_snapshot(&key, &snapshot).is_err());
}
#[cfg(unix)]
#[test]
fn snapshots_reject_linked_directories_and_files() {
    let f = Fixture::new();
    let outside = Fixture::new();
    let core = f.open();
    let key = setup(&core);
    let value = json!({"rules":[]});
    let digest = core.save_policy_snapshot(&key, &value).unwrap();
    let base = f.0.join("workspaces/w/sessions/s/policies");
    let file = base.join(format!("{digest}.json"));
    let destination = outside.0.join("snapshot.json");
    fs::rename(&file, &destination).unwrap();
    std::os::unix::fs::symlink(&destination, &file).unwrap();
    assert!(core.save_policy_snapshot(&key, &value).is_err());
    fs::remove_file(&file).unwrap();
    fs::remove_dir(&base).unwrap();
    std::os::unix::fs::symlink(&outside.0, &base).unwrap();
    assert!(core.save_policy_snapshot(&key, &value).is_err());
}
