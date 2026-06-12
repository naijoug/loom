mod agents;
mod command_runner;
mod models;
mod plan_html;
mod projects;
mod storage;
mod tasks;

use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HealthCheckResult {
    status: &'static str,
    app: &'static str,
    version: &'static str,
    backend: &'static str,
    timestamp_ms: u128,
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

#[tauri::command]
fn health_check() -> HealthCheckResult {
    HealthCheckResult {
        status: "ok",
        app: "Loom",
        version: env!("CARGO_PKG_VERSION"),
        backend: "tauri",
        timestamp_ms: now_ms(),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(command_runner::CommandRegistry::default())
        .manage(models::IdGenerator::default())
        .invoke_handler(tauri::generate_handler![
            health_check,
            agents::create_agent,
            agents::delete_agent,
            agents::list_agents,
            agents::retry_planning_agent,
            agents::run_planning_discussion,
            agents::run_plan_reviews,
            agents::set_agent_enabled,
            agents::update_agent,
            command_runner::command_runner_ready,
            command_runner::start_command_run,
            command_runner::stop_command_run,
            plan_html::open_plan_html,
            plan_html::open_planning_evidence,
            plan_html::open_plan_viewer,
            plan_html::read_plan_html,
            projects::list_recent_projects,
            projects::register_project,
            tasks::append_feedback,
            tasks::complete_task,
            tasks::complete_todo,
            tasks::confirm_plan,
            tasks::create_task,
            tasks::delete_task,
            tasks::generate_repair_context,
            tasks::list_tasks,
            tasks::mark_ready_for_testing,
            tasks::record_planning_decision,
            tasks::start_todo
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
