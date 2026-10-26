mod agent_adapter;
mod agent_diagnostics;
mod agents;
mod app_update;
mod chat;
mod chat_context;
#[cfg(test)]
mod contracts;
#[cfg(target_os = "macos")]
mod desktop_menu;
mod diagnostics;
mod migrations;
pub mod models;
pub mod process_supervisor;
mod projects;
mod session_capture;
mod settings;
pub mod skills;
mod storage;

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
        .invoke_handler(tauri::generate_handler![
            app_update::get_update_state,
            app_update::check_for_updates,
            app_update::download_update,
            app_update::install_update,
            app_update::load_update_drafts,
            app_update::save_update_drafts,
            health_check,
            agents::create_agent,
            agents::delete_agent,
            agent_diagnostics::diagnose_agents,
            agents::list_agents,
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
            agents::set_agent_enabled,
            agents::update_agent,
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
        if !app_update::handle_event(app, &event) {
            chat::shutdown::handle_event(app, event);
        }
    });
}
