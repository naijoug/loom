use super::*;
use crate::skills::{coordination::PendingTransaction, tests::Fixture};
use serde_json::json;
use std::fs;
fn setup() -> (Fixture, Repository) {
    let f = Fixture::new();
    let r = Repository::open(&f.0.join("data"), &f.0.join("coord")).unwrap();
    (f, r)
}
fn input(id: ProviderId, revision: u64, key: KeyChange, model: Option<&str>) -> ConfigInput {
    ConfigInput {
        provider_id: id,
        expected_revision: revision,
        key,
        model: model.map(str::to_owned),
    }
}
#[test]
fn provider_projection_never_returns_keys_and_reports_environment_precedence() {
    let (_f, r) = setup();
    let runtime = RuntimeConfig::default();
    let initial = list(&r, &runtime).unwrap();
    assert_eq!(initial.providers.len(), 4);
    assert_eq!(initial.revision, 0);
    assert!(!r.root.join("translation-config.json").exists());
    let saved = save(
        &r,
        input(
            ProviderId::OpenAi,
            0,
            KeyChange::Set("stored-secret-marker".into()),
            Some("gpt-5"),
        ),
        &runtime,
    )
    .unwrap();
    assert_eq!(saved.providers[0].key_source, KeySource::Stored);
    let mut env = RuntimeConfig::default();
    env.keys
        .insert(ProviderId::OpenAi, "environment-secret-marker".into());
    env.models.insert(ProviderId::OpenAi, "gpt-5-mini".into());
    env.local.insert(ProviderId::Codex, true);
    let view = list(&r, &env).unwrap();
    assert_eq!(view.providers[0].model.as_deref(), Some("gpt-5-mini"));
    assert_eq!(view.providers[0].stored_model.as_deref(), Some("gpt-5"));
    assert!(view.providers[0].stored_key && view.providers[0].model_from_environment);
    assert_eq!(view.providers[0].key_source, KeySource::Environment);
    assert!(view.providers[2].configured);
    assert!(!view.providers[3].configured);
    for text in [
        serde_json::to_string(&view).unwrap(),
        format!("{view:?}"),
        format!("{:?}", load(&r).unwrap()),
    ] {
        assert!(!text.contains("secret-marker"));
    }
    let raw = fs::read_to_string(r.root.join("translation-config.json")).unwrap();
    assert!(raw.contains("stored-secret-marker"));
    assert!(!raw.contains("environment-secret-marker"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(r.root.join("translation-config.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}
#[test]
fn explicit_key_changes_and_revision_conflicts_preserve_other_providers() {
    let (_f, r) = setup();
    let env = RuntimeConfig::default();
    save(
        &r,
        input(
            ProviderId::OpenAi,
            0,
            KeyChange::Set("one-secret".into()),
            Some("gpt-5"),
        ),
        &env,
    )
    .unwrap();
    let before = fs::read(r.root.join("translation-config.json")).unwrap();
    assert_eq!(
        save(
            &r,
            input(
                ProviderId::OpenRouter,
                0,
                KeyChange::Set("two-secret".into()),
                None
            ),
            &env
        )
        .unwrap_err()
        .code,
        ErrorCode::Conflict
    );
    assert_eq!(
        fs::read(r.root.join("translation-config.json")).unwrap(),
        before
    );
    save(
        &r,
        input(
            ProviderId::OpenRouter,
            1,
            KeyChange::Set("two-secret".into()),
            None,
        ),
        &env,
    )
    .unwrap();
    save(
        &r,
        input(ProviderId::OpenAi, 2, KeyChange::Keep, Some("gpt-5-mini")),
        &env,
    )
    .unwrap();
    assert_eq!(
        load(&r).unwrap().providers[&ProviderId::OpenAi]
            .api_key
            .as_deref(),
        Some("one-secret")
    );
    let view = save(
        &r,
        input(ProviderId::OpenAi, 3, KeyChange::Clear, None),
        &env,
    )
    .unwrap();
    assert!(!view.providers[0].configured);
    assert!(view.providers[1].configured);
    assert_eq!(view.providers[0].model.as_deref(), Some("gpt-5"));
    assert_eq!(view.revision, 4);
    // Clearing a stored key cannot silently unset the user's process environment.
    let mut override_env = RuntimeConfig::default();
    override_env
        .keys
        .insert(ProviderId::OpenAi, "env-secret".into());
    assert_eq!(
        list(&r, &override_env).unwrap().providers[0].key_source,
        KeySource::Environment
    );
}
#[test]
fn corrupt_future_and_linked_configuration_is_never_overwritten() {
    let (f, r) = setup();
    let path = r.root.join("translation-config.json");
    let env = RuntimeConfig::default();
    for data in [
        "{broken",
        r#"{"schemaVersion":2,"revision":0,"providers":{}}"#,
        r#"{"schemaVersion":1,"revision":0,"providers":{"amp":{}}}"#,
        r#"{"schemaVersion":1,"revision":0,"providers":{"codex":{"apiKey":"secret"}}}"#,
    ] {
        fs::write(&path, data).unwrap();
        assert!(list(&r, &env).is_err());
        assert!(save(
            &r,
            input(ProviderId::OpenAi, 0, KeyChange::Clear, None),
            &env
        )
        .is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), data);
    }
    #[cfg(unix)]
    {
        fs::remove_file(&path).unwrap();
        let target = f.0.join("untouched");
        std::os::unix::fs::symlink(&target, &path).unwrap();
        assert!(save(
            &r,
            input(ProviderId::OpenAi, 0, KeyChange::Clear, None),
            &env
        )
        .is_err());
        assert!(!target.exists());
        assert!(fs::symlink_metadata(path).unwrap().file_type().is_symlink());
    }
}
#[test]
fn invalid_inputs_reject_without_echo_or_implicit_key_deletion() {
    let (_f, r) = setup();
    let env = RuntimeConfig::default();
    for request in [
        input(ProviderId::Codex, 0, KeyChange::Set("secret".into()), None),
        input(
            ProviderId::OpenAi,
            0,
            KeyChange::Set("secret\nAuthorization: bad".into()),
            None,
        ),
        input(ProviderId::OpenAi, 0, KeyChange::Set("".into()), None),
        input(
            ProviderId::OpenAi,
            0,
            KeyChange::Keep,
            Some("model\nsecret"),
        ),
    ] {
        let err = save(&r, request, &env).unwrap_err();
        assert!(!err.message.contains("secret"));
        assert!(!r.root.join("translation-config.json").exists());
    }
    assert!(serde_json::from_value::<ConfigInput>(
        json!({"providerId":"openai","expectedRevision":0,"model":null})
    )
    .is_err());
    assert!(serde_json::from_value::<ConfigInput>(json!({"providerId":"openai","expectedRevision":0,"key":{"action":"keep"},"apiBaseUrl":"https://other.invalid"})).is_err());
    let result = save(
        &r,
        input(ProviderId::ClaudeCode, 0, KeyChange::Keep, Some("sonnet")),
        &env,
    )
    .unwrap();
    assert_eq!(result.providers[3].model.as_deref(), Some("sonnet"));
    assert!(!result.providers[3].supports_api_key);
}
#[test]
fn updater_and_pending_transaction_block_config_writes() {
    let (_f, r) = setup();
    let env = RuntimeConfig::default();
    let update = r.coordinator.try_update().unwrap();
    assert_eq!(
        save(
            &r,
            input(ProviderId::OpenAi, 0, KeyChange::Clear, None),
            &env
        )
        .unwrap_err()
        .code,
        ErrorCode::Updating
    );
    drop(update);
    let path = r.root.join("translation-config.json");
    let lease = r.coordinator.enter().unwrap();
    let writer = r.coordinator.writer(&lease).unwrap();
    let journal = r.root.join("prepare.json");
    fs::write(&journal, b"{}").unwrap();
    let pending = PendingTransaction::prepare(
        "fixture-translation",
        std::slice::from_ref(&path),
        &r.root,
        &journal,
    )
    .unwrap();
    writer.publish(&pending).unwrap();
    drop(writer);
    drop(lease);
    assert_eq!(
        save(
            &r,
            input(ProviderId::OpenAi, 0, KeyChange::Clear, None),
            &env
        )
        .unwrap_err()
        .code,
        ErrorCode::RecoveryRequired
    );
    assert!(!path.exists());
}
