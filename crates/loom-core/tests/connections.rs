use loom_core::*;
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
            "loom-connections-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
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
fn config() -> ConnectionConfig {
    ConnectionConfig {
        name: "Local fixture".into(),
        provider: ConnectionProvider::PiCompat,
        auth: ConnectionAuth::None,
        endpoint: Some("http://127.0.0.1:11434/v1".into()),
        pi_provider: None,
        custom_api: Some(EndpointApi::OpenaiCompletions),
        models: vec!["model-a".into(), "model-b".into()],
        default_model: "model-a".into(),
        supports_images: false,
        mid_stream_behavior: MidStreamBehavior::Queue,
    }
}
fn save(
    core: &Core,
    operation: &str,
    revision: Option<u64>,
    config: ConnectionConfig,
) -> Result<ModelConnection> {
    core.save_connection_metadata(SaveConnectionMetadata {
        id: id("connection"),
        operation_id: id(operation),
        expected_revision: revision,
        config,
        credential_ref: None,
    })
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
        connection: ConnectionSelection {
            connection_id: Some(id("connection")),
            model: None,
            thinking: None,
        },
    })
    .unwrap();
    key
}
fn send(core: &Core, key: &SessionKey, request: &str) -> Result<SendResult> {
    core.send_message(SendMessage {
        key: key.clone(),
        client_request_id: id(request),
        text: "fixture".into(),
        attachment_ids: vec![],
    })
}

#[test]
fn catalog_save_is_idempotent_conflict_checked_and_durable() {
    let f = Fixture::new();
    let core = f.open();
    let first = save(&core, "create", None, config()).unwrap();
    assert_eq!(first.revision, 1);
    assert_eq!(save(&core, "create", None, config()).unwrap(), first);
    assert!(save(&core, "new-operation", None, config()).is_err());
    let mut renamed = config();
    renamed.name = "Renamed".into();
    assert!(save(&core, "create", None, renamed.clone()).is_err());
    let next = save(&core, "rename", Some(1), renamed).unwrap();
    assert_eq!(next.revision, 2);
    assert_eq!(next.created_at_ms, first.created_at_ms);
    drop(core);
    let core = f.open();
    assert_eq!(core.list_connections().unwrap(), vec![next]);
}

#[test]
fn urls_reject_embedded_secrets_non_loopback_http_and_invalid_provider_combinations() {
    for endpoint in [
        "http://example.com/v1",
        "https://user:secret@example.com",
        "https://example.com?api_key=secret",
        "https://example.com/#secret",
        "file:///etc/passwd",
        "https:///",
        "http://localhost.evil.com",
        "http://127.0.0.1.evil.com",
    ] {
        let mut c = config();
        c.endpoint = Some(endpoint.into());
        assert!(c.validate().is_err(), "{endpoint}");
    }
    for endpoint in [
        "http://localhost:8080/v1",
        "http://[::1]:8080",
        "https://models.example.com/v1",
    ] {
        let mut c = config();
        c.endpoint = Some(endpoint.into());
        c.validate().unwrap();
    }
    let mut c = config();
    c.provider = ConnectionProvider::Claude;
    assert!(c.validate().is_err());
    c.custom_api = None;
    c.auth = ConnectionAuth::ApiKey;
    c.validate().unwrap();
    c.default_model = "missing".into();
    assert!(c.validate().is_err());
}

#[test]
fn queued_turn_freezes_the_connection_and_default_model() {
    let f = Fixture::new();
    let core = f.open();
    let first = save(&core, "create", None, config()).unwrap();
    let key = setup(&core);
    let sent = send(&core, &key, "first").unwrap();
    let mut revised = config();
    revised.endpoint = Some("https://new.example.com/v1".into());
    revised.default_model = "model-b".into();
    let second = save(&core, "update", Some(1), revised).unwrap();
    send(&core, &key, "second").unwrap();
    let view = core.get_session(&key).unwrap();
    assert_eq!(
        view.turns[0].configuration.resolved_connection.as_deref(),
        Some(&first)
    );
    assert_eq!(
        view.turns[1].configuration.resolved_connection.as_deref(),
        Some(&second)
    );
    assert!(send(&core, &key, "first").unwrap().already_accepted);
    assert_eq!(
        core.get_session(&key).unwrap().turns[0].receipt,
        sent.receipt
    );
    let mut binding = RuntimeBinding {
        adapter: "fixture".into(),
        connection: ConnectionSelection {
            connection_id: Some(id("connection")),
            model: Some("model-a".into()),
            thinking: None,
        },
        permission_mode: PermissionMode::Ask,
        policy_version: 1,
        working_directory: None,
        connection_snapshot_digest: Some(second.snapshot_digest().unwrap()),
        policy_snapshot_digest: None,
    };
    assert!(core.claim_next_turn(&key, binding.clone()).is_err());
    binding.connection_snapshot_digest = Some(first.snapshot_digest().unwrap());
    binding.connection.model = Some("model-b".into());
    assert!(core.claim_next_turn(&key, binding.clone()).is_err());
    binding.connection.model = Some("model-a".into());
    binding.policy_snapshot_digest = Some("invalid-policy-hash".into());
    assert!(core.claim_next_turn(&key, binding.clone()).is_err());
    binding.policy_snapshot_digest = None;
    let lease = core.claim_next_turn(&key, binding).unwrap();
    core.finish_turn(&lease, FinishStatus::Completed, None)
        .unwrap();
}

#[test]
fn session_locks_connection_identity_but_can_change_model_for_later_turns() {
    let f = Fixture::new();
    let core = f.open();
    save(&core, "create", None, config()).unwrap();
    let key = setup(&core);
    send(&core, &key, "first").unwrap();
    let view = core.get_session(&key).unwrap();
    assert!(core
        .patch_session(
            &key,
            view.last_seq,
            SessionPatch {
                connection: Some(ConnectionSelection::default()),
                ..Default::default()
            }
        )
        .is_err());
    core.patch_session(
        &key,
        view.last_seq,
        SessionPatch {
            connection: Some(ConnectionSelection {
                connection_id: Some(id("connection")),
                model: Some("model-b".into()),
                thinking: None,
            }),
            ..Default::default()
        },
    )
    .unwrap();
    send(&core, &key, "second").unwrap();
    assert_eq!(
        core.get_session(&key).unwrap().turns[1]
            .configuration
            .connection
            .model
            .as_deref(),
        Some("model-b")
    );
}

#[test]
fn missing_connection_or_model_does_not_accept_a_message() {
    let f = Fixture::new();
    let core = f.open();
    let key = setup(&core);
    assert!(send(&core, &key, "first").is_err());
    assert!(core.get_session(&key).unwrap().turns.is_empty());
    save(&core, "create", None, config()).unwrap();
    let view = core.get_session(&key).unwrap();
    core.patch_session(
        &key,
        view.last_seq,
        SessionPatch {
            connection: Some(ConnectionSelection {
                connection_id: Some(id("connection")),
                model: Some("unknown-model".into()),
                thinking: None,
            }),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(send(&core, &key, "first").is_err());
    assert!(core.get_session(&key).unwrap().turns.is_empty());
}

#[test]
fn public_wire_lists_connections_but_cannot_publish_vault_references() {
    let f = Fixture::new();
    let core = f.open();
    save(&core, "create", None, config()).unwrap();
    let reply = core.dispatch_json(
        br#"{"protocolVersion":1,"requestId":"list","command":{"method":"list_connections"}}"#,
    );
    assert!(
        matches!(reply.outcome,Outcome::Success{result:CoreResponse::Connections(items)} if items.len()==1)
    );
    let reply=core.dispatch_json(br#"{"protocolVersion":1,"requestId":"forge","command":{"method":"save_connection_metadata","params":{"credentialRef":"foreign"}}}"#);
    assert!(matches!(reply.outcome, Outcome::Failure { .. }));
}

#[test]
fn credential_input_type_errors_do_not_echo_sensitive_values() {
    let error = serde_json::from_str::<CredentialChange>(r#"{"kind":"replace","value":123456789}"#)
        .err()
        .unwrap()
        .to_string();
    assert!(!error.contains("123456789"));
    assert!(error.contains("credential must be a string"));
}

#[test]
fn new_optional_snapshot_fields_preserve_old_journal_serialization() {
    let configuration:TurnConfiguration=serde_json::from_str(r#"{"connection":{"connectionId":null,"model":null,"thinking":null},"permissionMode":"ask"}"#).unwrap();
    let value = serde_json::to_value(configuration).unwrap();
    assert!(value.get("resolvedConnection").is_none());
    let binding:RuntimeBinding=serde_json::from_str(r#"{"adapter":"old","connection":{"connectionId":null,"model":null,"thinking":null},"permissionMode":"ask","policyVersion":1,"workingDirectory":null}"#).unwrap();
    let serialized = serde_json::to_value(binding).unwrap();
    assert!(serialized.get("connectionSnapshotDigest").is_none());
    assert!(serialized.get("policySnapshotDigest").is_none());
}
