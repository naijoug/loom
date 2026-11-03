//! Startup-only readiness and bounded recovery. Page-load success is not UI readiness.
//! Never automatically reload a window after the application shell has acknowledged readiness.
use serde::Deserialize;
use serde_json::{json, Value};
use std::{sync::Mutex, time::Duration};
use tauri::{AppHandle, Manager, Webview, WebviewWindow};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

const READY_TIMEOUT: Duration = Duration::from_secs(15);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Loading,
    Reloading,
    Ready,
    Failed,
}
#[derive(Debug)]
struct Policy {
    generation: u64,
    phase: Phase,
    automatic_reloads: u8,
    ever_ready: bool,
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            generation: 0,
            phase: Phase::Loading,
            automatic_reloads: 0,
            ever_ready: false,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    None,
    Reload(u64),
    Notify(u64),
}
impl Policy {
    fn navigation(&mut self) -> u64 {
        self.generation += 1;
        self.phase = Phase::Loading;
        self.generation
    }
    fn ready(&mut self, generation: u64) -> bool {
        if generation != self.generation || self.phase != Phase::Loading {
            return false;
        }
        self.phase = Phase::Ready;
        self.ever_ready = true;
        true
    }
    fn fail(&mut self, generation: u64) -> Action {
        if generation != self.generation || matches!(self.phase, Phase::Ready | Phase::Failed) {
            return Action::None;
        }
        if !self.ever_ready && self.automatic_reloads == 0 {
            self.automatic_reloads = 1;
            self.generation += 1;
            self.phase = Phase::Reloading;
            Action::Reload(self.generation)
        } else {
            self.phase = Phase::Failed;
            Action::Notify(self.generation)
        }
    }
    fn manual_reload(&mut self) -> u64 {
        self.generation += 1;
        self.phase = Phase::Reloading;
        self.generation
    }
}
#[derive(Default)]
pub(crate) struct Startup {
    policy: Mutex<Policy>,
    journal: Mutex<Option<Vec<Value>>>,
}
fn record(app: &AppHandle, event: &'static str) {
    let state = app.state::<Startup>();
    let Ok(policy) = state.policy.lock() else {
        return;
    };
    let entry = json!({"at":crate::now_ms(),"pid":std::process::id(),"event":event,"generation":policy.generation,"phase":format!("{:?}",policy.phase),"automaticReloads":policy.automatic_reloads});
    drop(policy);
    eprintln!("[loom-startup] {entry}");
    // Diagnostic-only, bounded and without URLs, user content, argv or exception text.
    let Ok(dir) = crate::storage::global_config_dir(app) else {
        return;
    };
    let path = dir.join("webview-startup.json");
    let Ok(mut journal) = state.journal.lock() else {
        return;
    };
    let entries = journal.get_or_insert_with(|| {
        std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice::<Vec<Value>>(&b).ok())
            .unwrap_or_default()
    });
    entries.push(entry);
    if entries.len() > 128 {
        entries.drain(..entries.len() - 128);
    }
    if crate::storage::atomic_write_json(&path, entries).is_err() {
        eprintln!("[loom-startup] diagnostic_write_failed");
    }
}
fn arm(app: &AppHandle, generation: u64) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(READY_TIMEOUT).await;
        if crate::agent::is_shutting_down() {
            return;
        }
        fail(&app, generation, "ready_timeout");
    });
}
fn fail(app: &AppHandle, generation: u64, event: &'static str) {
    let action = app
        .state::<Startup>()
        .policy
        .lock()
        .map(|mut p| p.fail(generation))
        .unwrap_or(Action::None);
    if action == Action::None {
        return;
    }
    record(app, event);
    match action {
        Action::Reload(epoch) => {
            record(app, "automatic_reload");
            reload(app, epoch);
        }
        Action::Notify(epoch) => notify_failure(app, epoch),
        Action::None => {}
    }
}
fn reload(app: &AppHandle, epoch: u64) {
    if crate::agent::is_shutting_down() {
        return;
    }
    match app
        .get_webview_window("main")
        .ok_or_else(|| "main window missing".to_string())
        .and_then(|w| w.reload().map_err(|_| "reload failed".to_string()))
    {
        Ok(()) => arm(app, epoch),
        Err(_) => {
            record(app, "reload_failed");
            fail(app, epoch, "recovery_failed");
        }
    }
}
fn notify_failure(app: &AppHandle, epoch: u64) {
    if crate::agent::is_shutting_down() {
        return;
    }
    record(app, "recovery_exhausted");
    let app = app.clone();
    let dialog = app.dialog().message("Loom could not open its interface. Saved chats and project data have not been removed. Retry loading, or close Loom.\n\nLoom 未能打开界面。已保存的会话与项目数据未被删除。可以重试加载，或关闭 Loom。")
        .title("Loom").kind(MessageDialogKind::Error)
        .buttons(MessageDialogButtons::OkCancelCustom("Retry / 重试".into(),"Close / 关闭".into()));
    #[cfg(desktop)]
    let dialog = if let Some(window) = app.get_webview_window("main") {
        dialog.parent(&window)
    } else {
        dialog
    };
    dialog.show(move |retry| {
        if crate::agent::is_shutting_down() {
            return;
        }
        let current = app
            .state::<Startup>()
            .policy
            .lock()
            .is_ok_and(|p| p.generation == epoch && p.phase == Phase::Failed);
        if !current {
            return;
        }
        if retry {
            manual_reload(&app);
        } else {
            app.exit(0);
        }
    });
}
fn manual_reload(app: &AppHandle) {
    let epoch = app
        .state::<Startup>()
        .policy
        .lock()
        .map(|mut p| p.manual_reload())
        .ok();
    if let Some(epoch) = epoch {
        record(app, "manual_reload");
        reload(app, epoch);
    }
}
/// Native menu works even if JavaScript/React/the WebContent process cannot show a button.
#[cfg(target_os = "macos")]
pub(crate) fn request_reload(app: &AppHandle) {
    record(app, "manual_reload_requested");
    let app = app.clone();
    let dialog = app.dialog().message("Reload the Loom interface? Active Agent work keeps running. Text not saved yet may be lost.\n\n重新加载 Loom 界面？进行中的 Agent 工作会继续，尚未保存的输入可能丢失。")
        .title("Loom").buttons(MessageDialogButtons::OkCancelCustom("Reload / 重新加载".into(),"Cancel / 取消".into()));
    let dialog = if let Some(window) = app.get_webview_window("main") {
        dialog.parent(&window)
    } else {
        dialog
    };
    dialog.show(move |confirmed| {
        if confirmed && !crate::agent::is_shutting_down() {
            manual_reload(&app)
        }
    });
}
pub(crate) fn started(app: &AppHandle) {
    record(app, "native_ready");
    let epoch = app
        .state::<Startup>()
        .policy
        .lock()
        .map(|p| p.generation)
        .unwrap_or(0);
    arm(app, epoch);
}
pub(crate) fn page_load(webview: &Webview, event: tauri::webview::PageLoadEvent) {
    if webview.label() != "main" {
        return;
    }
    let app = webview.app_handle();
    match event {
        tauri::webview::PageLoadEvent::Started => {
            let epoch = app
                .state::<Startup>()
                .policy
                .lock()
                .map(|mut p| p.navigation())
                .unwrap_or(0);
            record(app, "navigation_started");
            arm(app, epoch);
        }
        tauri::webview::PageLoadEvent::Finished => record(app, "navigation_finished"),
    }
}
#[cfg(any(target_os = "macos", target_os = "ios"))]
pub(crate) fn content_terminated(webview: &Webview) {
    if webview.label() != "main" {
        return;
    }
    let app = webview.app_handle();
    record(app, "content_process_terminated");
    let epoch = app
        .state::<Startup>()
        .policy
        .lock()
        .map(|p| p.generation)
        .unwrap_or(0);
    // Ready is terminal for automatic recovery. Existing interactive sessions are never auto-reloaded.
    fail(app, epoch, "content_terminated_before_ready");
}
fn authorize(window: &WebviewWindow) -> Result<(), String> {
    if window.label() == "main" {
        Ok(())
    } else {
        Err("Startup readiness requires the main window".into())
    }
}
#[tauri::command]
pub(crate) fn webview_startup_context(
    app: AppHandle,
    window: WebviewWindow,
) -> Result<u64, String> {
    authorize(&window)?;
    app.state::<Startup>()
        .policy
        .lock()
        .map(|p| p.generation)
        .map_err(|_| "Startup state unavailable".into())
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FrontendStatus {
    Ready,
    Failed,
}
#[tauri::command]
pub(crate) fn report_webview_startup(
    app: AppHandle,
    window: WebviewWindow,
    generation: u64,
    status: FrontendStatus,
) -> Result<bool, String> {
    authorize(&window)?;
    match status {
        FrontendStatus::Ready => {
            let accepted = app
                .state::<Startup>()
                .policy
                .lock()
                .map(|mut p| p.ready(generation))
                .map_err(|_| "Startup state unavailable")?;
            if accepted {
                record(&app, "interface_ready");
            }
            Ok(accepted)
        }
        FrontendStatus::Failed => {
            fail(&app, generation, "frontend_boot_failed");
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_ui_gets_one_retry_then_native_failure_without_looping() {
        let mut p = Policy::default();
        let first = p.navigation();
        let Action::Reload(retry) = p.fail(first) else {
            panic!("must recover")
        };
        assert_eq!(p.fail(first), Action::None);
        let second = p.navigation();
        assert!(second > retry);
        assert_eq!(p.fail(second), Action::Notify(second));
        assert_eq!(p.fail(second), Action::None);
        assert_eq!(p.automatic_reloads, 1);
    }
    #[test]
    fn completed_navigation_is_not_interface_readiness() {
        let mut p = Policy::default();
        let g = p.navigation();
        assert_eq!(p.phase, Phase::Loading);
        assert!(matches!(p.fail(g), Action::Reload(_)));
    }
    #[test]
    fn healthy_interface_never_auto_reloads_even_after_later_navigation() {
        let mut p = Policy::default();
        let first = p.navigation();
        assert!(p.ready(first));
        assert_eq!(p.fail(first), Action::None);
        let manual = p.navigation();
        assert_eq!(p.fail(manual), Action::Notify(manual));
        assert_eq!(p.automatic_reloads, 0);
    }
    #[test]
    fn stale_ack_and_timer_cannot_complete_or_reload_new_document() {
        let mut p = Policy::default();
        let first = p.navigation();
        p.fail(first);
        let second = p.navigation();
        assert!(!p.ready(first));
        assert_eq!(p.fail(first), Action::None);
        assert!(p.ready(second));
        assert_eq!(p.fail(second), Action::None);
    }
    #[test]
    fn explicit_retry_can_recover_without_restoring_automatic_budget() {
        let mut p = Policy::default();
        p.fail(0);
        let next = p.navigation();
        p.fail(next);
        p.manual_reload();
        let latest = p.navigation();
        assert!(p.ready(latest));
        assert_eq!(p.automatic_reloads, 1);
    }
    #[test]
    fn reload_that_never_starts_navigation_is_bounded() {
        let mut p = Policy::default();
        let Action::Reload(g) = p.fail(0) else {
            panic!()
        };
        assert_eq!(p.fail(g), Action::Notify(g));
        assert!(!p.ready(g));
    }
}
