use crate::migrations::{
    AGENT_STORE_SCHEMA_VERSION, PROJECT_PREFERENCES_SCHEMA_VERSION, SETTINGS_SCHEMA_VERSION,
    TASK_SCHEMA_VERSION, TERMINAL_STORE_SCHEMA_VERSION,
};
use crate::{
    models::{AgentConfig, AppSettings, CommandRun, Task, TerminalSlot},
    project_preferences::ProjectAgentPreferences,
};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;

const COMMANDS: &[&str] = &[
    "health_check",
    "create_agent",
    "delete_agent",
    "diagnose_agents",
    "list_agents",
    "chat_list_sessions",
    "chat_create",
    "chat_get",
    "chat_set_agent",
    "chat_clear_resume",
    "chat_send",
    "chat_abort",
    "prepare_agent_invocation",
    "retry_planning_agent",
    "run_planning_discussion",
    "run_plan_reviews",
    "set_agent_enabled",
    "update_agent",
    "command_runner_ready",
    "read_command_run_logs",
    "assess_execution",
    "approve_execution",
    "decide_implementation_review_finding",
    "run_implementation_reviews",
    "export_diagnostic_bundle",
    "load_app_settings",
    "save_app_settings",
    "start_command_run",
    "stop_command_run",
    "start_pty_run",
    "stop_pty_run",
    "write_pty",
    "resize_pty",
    "list_terminal_slots",
    "save_terminal_slots",
    "suggest_terminal_slots",
    "open_plan_html",
    "open_planning_evidence",
    "open_plan_viewer",
    "read_plan_html",
    "load_project_agent_preferences",
    "save_project_agent_preferences",
    "list_recent_projects",
    "remove_recent_project",
    "register_project",
    "append_feedback",
    "block_task",
    "build_implementation_context",
    "complete_task",
    "complete_todo",
    "confirm_plan",
    "create_task",
    "delete_task",
    "generate_repair_context",
    "list_tasks",
    "mark_ready_for_testing",
    "pause_task",
    "record_planning_decision",
    "resume_task",
    "cancel_task",
    "start_todo",
    "switch_primary_agent",
    "regenerate_task_summary",
    "export_task_summary",
];

const EVENTS: &[&str] = &[
    "loom://command-finished",
    "loom://command-log",
    "loom://planning-agent-log",
    "loom://planning-agent-status",
    "loom://pty-output",
    "loom://chat-stream",
    "loom://chat-turn-finished",
];

const TASK_STATUSES: &[&str] = &[
    "drafting_requirements",
    "planning",
    "plan_review",
    "ready_to_implement",
    "implementing",
    "reviewing",
    "debugging",
    "fixing",
    "verifying",
    "completed",
    "blocked",
    "cancelled",
];

const COMMAND_RUN_STATUSES: &[&str] =
    &["running", "succeeded", "failed", "cancelled", "interrupted"];

const PLAN_TODO_STATUSES: &[&str] = &["pending", "implementing", "done", "blocked"];

fn manifest() -> Value {
    json!({
        "schemaVersion": 1,
        "commands": COMMANDS,
        "events": EVENTS,
        "taskStatuses": TASK_STATUSES,
        "commandRunStatuses": COMMAND_RUN_STATUSES,
        "planTodoStatuses": PLAN_TODO_STATUSES,
        "storeSchemaVersions": {
            "task": TASK_SCHEMA_VERSION,
            "agents": AGENT_STORE_SCHEMA_VERSION,
            "settings": SETTINGS_SCHEMA_VERSION,
            "terminalSlots": TERMINAL_STORE_SCHEMA_VERSION,
            "projectAgentPreferences": PROJECT_PREFERENCES_SCHEMA_VERSION,
        }
    })
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
    let fixture: Value = serde_json::from_str(include_str!("../../contracts/tauri-contract.json"))
        .expect("canonical contract fixture");
    let samples = &fixture["modelSamples"];
    assert_model_sample::<AppSettings>(samples, "appSettings");
    assert_model_sample::<AgentConfig>(samples, "agentConfig");
    assert_model_sample::<TerminalSlot>(samples, "terminalSlot");
    assert_model_sample::<ProjectAgentPreferences>(samples, "projectAgentPreferences");
    assert_model_sample::<CommandRun>(samples, "commandRun");
    assert_model_sample::<Task>(samples, "task");
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
        include_str!("agents.rs"),
        include_str!("command_runner.rs"),
        include_str!("pty.rs"),
    ]
    .join("\n");
    for event in EVENTS {
        assert!(sources.contains(event), "missing backend event {event}");
    }
}

#[test]
fn status_manifest_matches_rust_parsers() {
    for status in TASK_STATUSES {
        status
            .parse::<crate::models::TaskStatus>()
            .unwrap_or_else(|error| panic!("task status {status}: {error}"));
    }
    for status in COMMAND_RUN_STATUSES {
        status
            .parse::<crate::models::CommandRunStatus>()
            .unwrap_or_else(|error| panic!("command status {status}: {error}"));
    }
    for status in PLAN_TODO_STATUSES {
        status
            .parse::<crate::models::PlanTodoStatus>()
            .unwrap_or_else(|error| panic!("todo status {status}: {error}"));
    }
}
