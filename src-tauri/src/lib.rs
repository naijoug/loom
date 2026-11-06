mod agent;
mod app_update;
mod browser_host;
mod chat;
mod chat_context;
#[cfg(test)]
mod contracts;
#[cfg(target_os = "macos")]
mod desktop_menu;
mod diagnostics;
mod discuss;
mod git_fetch;
mod learn;
mod migrations;
pub mod models;
pub mod process_supervisor;
mod projects;
mod rea;
mod repository_sources;
mod research;
mod research_sources;
mod session_capture;
mod settings;
pub mod skills;
mod source_files;
mod storage;
mod tool_process;
mod webview_startup;

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
        .manage(models::IdGenerator::default())
        .manage(webview_startup::Startup::default())
        .on_page_load(|webview, payload| webview_startup::page_load(webview, payload.event()))
        .invoke_handler(tauri::generate_handler![
            app_update::get_update_state,
            app_update::check_for_updates,
            app_update::download_update,
            app_update::install_update,
            app_update::load_update_drafts,
            app_update::save_update_drafts,
            health_check,
            webview_startup::webview_startup_context,
            webview_startup::report_webview_startup,
            agent::registry::create_agent,
            agent::registry::delete_agent,
            agent::diagnostics::diagnose_agents,
            agent::registry::list_agents,
            chat::chat_list_sessions,
            chat::groups::chat_list_project_groups,
            chat::chat_create,
            chat::chat_get,
            chat::chat_read_events,
            chat::chat_read_run_logs,
            chat::chat_export,
            chat::chat_set_agent,
            chat::chat_update_meta,
            chat::chat_clear_resume,
            chat::chat_send,
            chat::chat_abort,
            agent::registry::set_agent_enabled,
            agent::registry::update_agent,
            diagnostics::export_diagnostic_bundle,
            settings::load_app_settings,
            settings::save_app_settings,
            projects::list_recent_projects,
            projects::remove_recent_project,
            projects::register_project,
            skills::commands::skills_list_library,
            skills::commands::skills_get_detail,
            skills::commands::skills_add_local_source,
            skills::commands::skills_remove_source,
            skills::commands::skills_import_repository,
            skills::commands::skills_refresh_source,
            skills::commands::skills_list_operations,
            skills::commands::skills_cancel_operation,
            skills::commands::skills_list_install_targets,
            skills::commands::skills_get_install_status,
            skills::commands::skills_list_installations,
            skills::commands::skills_plan_install,
            skills::commands::skills_plan_uninstall,
            skills::commands::skills_apply_install,
            skills::commands::skills_list_pending,
            skills::commands::skills_recover_installation,
            skills::commands::skills_list_translation_providers,
            skills::commands::skills_save_translation_config,
            skills::commands::skills_translate,
            skills::commands::skills_translation_history,
            skills::commands::skills_translation_result,
            learn::commands::learn_list_studies,
            learn::commands::learn_create_study,
            learn::commands::learn_get_study,
            learn::commands::learn_read_page,
            learn::commands::learn_generate,
            learn::commands::learn_cancel_job,
            learn::commands::learn_read_job_logs,
            learn::commands::learn_list_tree,
            learn::commands::learn_read_source,
            learn::commands::learn_delete_study,
            research::commands::research_list,
            research::commands::research_create,
            research::commands::research_get,
            research::commands::research_read_turn,
            research::commands::research_ask,
            research::commands::research_retry,
            research::commands::research_cancel,
            research::commands::research_read_job_logs,
            research::commands::research_list_tree,
            research::commands::research_read_source,
            research::commands::research_delete,
            research::target_commands::research_preflight,
            research::target_commands::research_start_run,
            research::target_commands::research_get_run,
            research::target_commands::research_list_runs,
            research::target_commands::research_cancel_run,
            research::target_commands::research_read_report,
            research::target_commands::research_list_report_tree,
            research::target_commands::research_read_report_source,
            research::target_commands::research_list_evidence,
            research::target_commands::research_read_evidence,
            research::target_commands::research_ask_run,
            research::target_commands::research_retry_run,
            research::target_commands::research_resume_run,
            research::target_commands::research_export,
            discuss::commands::discuss_list,
            discuss::commands::discuss_create,
            discuss::commands::discuss_get,
            discuss::commands::discuss_read_report,
            discuss::commands::discuss_retry,
            discuss::commands::discuss_cancel,
            discuss::commands::discuss_read_job_logs,
            discuss::commands::discuss_read_source,
            discuss::commands::discuss_list_tree,
            discuss::commands::discuss_delete,
        ]);
    #[cfg(target_os = "macos")]
    let builder = builder
        .on_web_content_process_terminate(webview_startup::content_terminated)
        .menu(desktop_menu::build)
        .on_menu_event(|app, event| {
            if event.id().as_ref() == desktop_menu::QUIT_ID {
                app.exit(0);
            } else if event.id().as_ref() == desktop_menu::RELOAD_ID {
                webview_startup::request_reload(app);
            }
        });
    let app = builder
        .build(tauri::generate_context!())
        .expect("error while building tauri application");
    chat::shutdown::install_signal_handlers(app.handle());
    app_update::setup(app.handle());
    app.run(|app, event| {
        if matches!(&event, tauri::RunEvent::Ready) {
            webview_startup::started(app);
        }
        if !app_update::handle_event(app, &event) {
            chat::shutdown::handle_event(app, event);
        }
    });
}
