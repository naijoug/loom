use crate::{
    migrations::{AGENT_STORE_SCHEMA_VERSION, SETTINGS_SCHEMA_VERSION},
    models::{AgentConfig, AppSettings},
};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;
const COMMANDS: &[&str] = &[
    "get_update_state",
    "check_for_updates",
    "download_update",
    "install_update",
    "load_update_drafts",
    "save_update_drafts",
    "health_check",
    "create_agent",
    "delete_agent",
    "diagnose_agents",
    "list_agents",
    "chat_list_sessions",
    "chat_list_project_groups",
    "chat_create",
    "chat_get",
    "chat_read_events",
    "chat_read_run_logs",
    "chat_export",
    "chat_set_agent",
    "chat_update_meta",
    "chat_clear_resume",
    "chat_send",
    "chat_abort",
    "set_agent_enabled",
    "update_agent",
    "export_diagnostic_bundle",
    "load_app_settings",
    "save_app_settings",
    "list_recent_projects",
    "remove_recent_project",
    "register_project",
    "skills_list_library",
    "skills_get_detail",
    "skills_add_local_source",
    "skills_remove_source",
    "skills_import_repository",
    "skills_refresh_source",
    "skills_list_operations",
    "skills_cancel_operation",
    "skills_list_install_targets",
    "skills_get_install_status",
    "skills_list_installations",
    "skills_plan_install",
    "skills_plan_uninstall",
    "skills_apply_install",
    "skills_list_pending",
    "skills_recover_installation",
    "skills_list_translation_providers",
    "skills_save_translation_config",
    "skills_plan_migration",
    "skills_translate",
    "skills_translation_history",
    "skills_translation_result",
];
const EVENTS: &[&str] = &[
    "loom-update-state",
    "loom://chat-event",
    "loom://chat-error",
    "loom://skills-operation",
];
fn manifest() -> Value {
    json!({"schemaVersion":1,"commands":COMMANDS,"events":EVENTS,"storeSchemaVersions":{"agents":AGENT_STORE_SCHEMA_VERSION,"settings":SETTINGS_SCHEMA_VERSION,"chat":crate::chat::CHAT_SCHEMA_VERSION}})
}
fn registered_commands(source: &str) -> Vec<String> {
    let body = source
        .split("tauri::generate_handler![")
        .nth(1)
        .and_then(|value| value.split("])").next())
        .expect("generate_handler registration block");
    body.split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.rsplit("::").next().unwrap_or(value).to_string())
        .collect()
}

#[test]
fn canonical_contract_fixture_matches_backend_manifest() {
    let mut fixture: Value =
        serde_json::from_str(include_str!("../../contracts/tauri-contract.json"))
            .expect("canonical contract fixture");
    fixture
        .as_object_mut()
        .expect("contract fixture object")
        .remove("modelSamples")
        .expect("model samples");
    assert_eq!(fixture, manifest());
}

fn assert_model_sample<T>(samples: &Value, name: &str)
where
    T: DeserializeOwned + Serialize,
{
    let expected = samples
        .get(name)
        .unwrap_or_else(|| panic!("missing {name} sample"));
    let model: T = serde_json::from_value(expected.clone())
        .unwrap_or_else(|error| panic!("invalid {name} sample: {error}"));
    assert_eq!(
        serde_json::to_value(model).expect("serialize contract model"),
        *expected,
        "{name} wire shape drifted"
    );
}

#[test]
fn persisted_model_samples_round_trip_through_rust_types() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../contracts/tauri-contract.json")).unwrap();
    let samples = &fixture["modelSamples"];
    assert_model_sample::<AppSettings>(samples, "appSettings");
    assert_model_sample::<AgentConfig>(samples, "agentConfig");
    assert_model_sample::<crate::chat::ChatSession>(samples, "chatSession");
    assert_model_sample::<crate::chat::ChatEvent>(samples, "chatEvent");
    assert_model_sample::<crate::chat::ChatSendInput>(samples, "chatSendInput");
    assert_model_sample::<crate::chat::ChatSendReceipt>(samples, "chatSendReceipt");
    assert_model_sample::<crate::chat::ChatResumeHandle>(samples, "chatResumeHandle");
    assert_model_sample::<crate::chat::ChatTurn>(samples, "chatTurn");
    assert_model_sample::<crate::chat::ChatLogPage>(samples, "chatLogPage");
    assert_model_sample::<crate::chat::ChatExportInput>(samples, "chatExportInput");
    assert_model_sample::<crate::chat::ChatExportResult>(samples, "chatExportResult");
    assert_model_sample::<crate::chat::ChatMessagePart>(samples, "chatToolPart");
    assert_model_sample::<crate::agent_diagnostics::AgentDiagnostic>(samples, "agentDiagnostic");
    assert_model_sample::<crate::skills::migration::MigrationPreview>(
        samples,
        "skillsMigrationPreview",
    );
    assert_model_sample::<crate::skills::translation::TranslationResult>(
        samples,
        "skillsTranslationResult",
    );
    assert_model_sample::<crate::skills::translation::TranslationSummary>(
        samples,
        "skillsTranslationSummary",
    );
    assert_model_sample::<crate::skills::translation::config::ProviderList>(
        samples,
        "skillsTranslationProviders",
    );
    assert_model_sample::<crate::skills::repository::LibraryView>(samples, "skillsLibrary");
    assert_model_sample::<crate::skills::repository::Source>(samples, "skillsGithubApiSource");
    assert_model_sample::<crate::skills::operations::HttpRequestRecord>(
        samples,
        "skillsHttpRequest",
    );
    assert_model_sample::<crate::skills::operations::OperationRecord>(samples, "skillsOperation");
    assert_model_sample::<crate::skills::targets::InstallTarget>(samples, "skillsInstallTarget");
    assert_model_sample::<crate::skills::installers::Installation>(samples, "skillsInstallation");
    assert_model_sample::<crate::skills::installers::InstallPlan>(samples, "skillsInstallPlan");
    assert_model_sample::<crate::skills::installers::InstallPlan>(
        samples,
        "skillsCoordinatedInstallPlan",
    );
    assert_model_sample::<crate::skills::instructions::InstructionCheck>(
        samples,
        "skillsInstructionCheck",
    );
    assert_model_sample::<crate::skills::instructions::ResourcePreview>(
        samples,
        "skillsResourcePreview",
    );
    assert_model_sample::<crate::skills::installers::InstallReceipt>(
        samples,
        "skillsInstallReceipt",
    );
}
#[test]
fn chat_send_requires_a_client_request_identity() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../contracts/tauri-contract.json")).unwrap();
    let mut input = fixture["modelSamples"]["chatSendInput"].clone();
    input.as_object_mut().unwrap().remove("clientRequestId");
    assert!(serde_json::from_value::<crate::chat::ChatSendInput>(input).is_err());
}

#[test]
fn command_manifest_matches_tauri_registration() {
    let registered = registered_commands(include_str!("lib.rs"));
    assert_eq!(registered, COMMANDS);
    assert_eq!(
        registered.iter().collect::<BTreeSet<_>>().len(),
        registered.len(),
        "command names must be unique"
    );
}

#[test]
fn event_manifest_matches_backend_emitters() {
    let sources = [
        include_str!("app_update.rs"),
        include_str!("chat/mod.rs"),
        include_str!("chat/service.rs"),
        include_str!("skills/commands.rs"),
    ]
    .join("\n");
    for event in EVENTS {
        assert!(sources.contains(event), "missing event {event}");
    }
}
