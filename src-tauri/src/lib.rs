mod agent_adapter;
mod agent_diagnostics;
mod agents;
mod app_update;
mod attachments;
mod chat;
mod chat_context;
mod command_runner;
pub mod context_builder;
#[cfg(test)]
mod contracts;
#[cfg(target_os = "macos")]
mod desktop_menu;
mod diagnostics;
mod execution_policy;
mod implementation_review;
mod migrations;
pub mod models;
mod plan_html;
pub mod process_supervisor;
mod project_git;
mod project_preferences;
mod projects;
mod pty;
mod run_recovery;
mod session_capture;
mod settings;
mod storage;
pub mod task_repository;
pub mod task_state;
mod task_summary;
mod tasks;
mod terminals;
mod workbench;

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
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(app_update::AppUpdate::default())
        .manage(command_runner::CommandRegistry::default())
        .manage(execution_policy::ExecutionApprovalRegistry::default())
        .manage(pty::PtyRegistry::default())
        .manage(run_recovery::RunRecoveryRegistry::default())
        .manage(models::IdGenerator::default())
        .manage(workbench::Workbench::default())
        .invoke_handler(tauri::generate_handler![
            app_update::get_update_state,
            app_update::check_for_updates,
            app_update::download_update,
            app_update::install_update,
            app_update::load_update_drafts,
            app_update::save_update_drafts,
            health_check,
            workbench::workbench_capabilities,
            workbench::workbench_invoke,
            workbench::workbench_save_connection,
            workbench::workbench_save_permissions,
            workbench::workbench_permissions,
            workbench::workbench_watch,
            workbench::workbench_watch_workspace,
            workbench::workbench_unwatch,
            workbench::workbench_resume_queue,
            agents::create_agent,
            agents::delete_agent,
            agent_diagnostics::diagnose_agents,
            agents::list_agents,
            chat::chat_list_sessions,
            chat::chat_create,
            chat::chat_get,
            chat::chat_read_events,
            chat::chat_read_run_logs,
            chat::chat_export,
            chat::chat_set_agent,
            chat::chat_update_meta,
            chat::chat_clear_resume,
            chat::chat_promote_to_task,
            chat::chat_send,
            chat::chat_abort,
            agents::prepare_agent_invocation,
            agents::retry_planning_agent,
            agents::run_planning_discussion,
            agents::run_plan_reviews,
            agents::set_agent_enabled,
            agents::update_agent,
            command_runner::command_runner_ready,
            command_runner::read_command_run_logs,
            execution_policy::assess_execution,
            execution_policy::approve_execution,
            implementation_review::decide_implementation_review_finding,
            implementation_review::run_implementation_reviews,
            diagnostics::export_diagnostic_bundle,
            settings::load_app_settings,
            settings::save_app_settings,
            command_runner::start_command_run,
            command_runner::stop_command_run,
            pty::start_pty_run,
            pty::stop_pty_run,
            pty::write_pty,
            pty::resize_pty,
            terminals::list_terminal_slots,
            terminals::save_terminal_slots,
            terminals::suggest_terminal_slots,
            plan_html::open_plan_html,
            plan_html::open_planning_evidence,
            plan_html::open_plan_viewer,
            plan_html::read_plan_html,
            project_preferences::load_project_agent_preferences,
            project_preferences::save_project_agent_preferences,
            projects::list_recent_projects,
            projects::remove_recent_project,
            projects::register_project,
            tasks::testing::append_feedback,
            tasks::block_task,
            tasks::build_implementation_context,
            tasks::complete_task,
            tasks::complete_todo,
            tasks::confirm_plan,
            tasks::create_task,
            tasks::delete_task,
            tasks::testing::generate_repair_context,
            tasks::list_tasks,
            tasks::mark_ready_for_testing,
            tasks::pause_task,
            tasks::record_planning_decision,
            tasks::resume_task,
            tasks::cancel_task,
            tasks::start_todo,
            tasks::switch_primary_agent,
            task_summary::regenerate_task_summary,
            task_summary::export_task_summary
        ]);
    #[cfg(target_os = "macos")]
    let builder = builder
        .menu(desktop_menu::build)
        .on_menu_event(|app, event| {
            if event.id().as_ref() == desktop_menu::QUIT_ID {
                app.exit(0);
            }
        });
    let app = builder
        .build(tauri::generate_context!())
        .expect("error while building tauri application");
    chat::shutdown::install_signal_handlers(app.handle());
    app_update::setup(app.handle());
    app.run(|app, event| {
        if !app_update::handle_event(app, &event) && !workbench::handle_event(app, &event) {
            chat::shutdown::handle_event(app, event);
        }
    });
}
