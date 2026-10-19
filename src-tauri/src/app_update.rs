//! App-owned updater. Renderer never supplies endpoints, keys or install paths.
use crate::{process_supervisor::supervisor, storage};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf, sync::Mutex, time::Duration};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};
use tauri_plugin_updater::{Update, UpdaterExt};

const EVENT: &str = "loom-update-state";
pub(crate) type Drafts = BTreeMap<String, BTreeMap<String, String>>;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Snapshot {
    revision: u64,
    phase: &'static str,
    current_version: String,
    version: Option<String>,
    notes: Option<String>,
    downloaded: u64,
    total: Option<u64>,
    error: Option<String>,
    checked_at: Option<u128>,
}
impl Default for Snapshot {
    fn default() -> Self {
        Self {
            revision: 0,
            phase: "idle",
            current_version: env!("CARGO_PKG_VERSION").into(),
            version: None,
            notes: None,
            downloaded: 0,
            total: None,
            error: None,
            checked_at: None,
        }
    }
}
#[derive(Default)]
struct Candidate {
    update: Option<Update>,
    bytes: Option<Vec<u8>>,
}
#[derive(Default)]
pub(crate) struct AppUpdate {
    snapshot: Mutex<Snapshot>,
    candidate: tokio::sync::Mutex<Candidate>,
    journal: Mutex<Vec<serde_json::Value>>,
    drafts: Mutex<()>,
}
fn authorize(window: &WebviewWindow) -> Result<(), String> {
    if window.label() == "main" {
        Ok(())
    } else {
        Err("更新仅允许主窗口操作".into())
    }
}
fn directory(app: &AppHandle) -> Result<PathBuf, String> {
    storage::global_config_dir(app)
        .map(|dir| dir.join("updates"))
        .map_err(|_| "更新记录目录不可用".into())
}
fn configured(app: &AppHandle) -> bool {
    cfg!(target_os = "macos")
        && app.config().plugins.0.get("updater").is_some_and(|value| {
            value
                .get("pubkey")
                .and_then(|v| v.as_str())
                .is_some_and(|v| !v.is_empty())
                && value
                    .get("endpoints")
                    .and_then(|v| v.as_array())
                    .is_some_and(|v| !v.is_empty())
        })
}
impl AppUpdate {
    fn snapshot(&self) -> Result<Snapshot, String> {
        self.snapshot
            .lock()
            .map(|v| v.clone())
            .map_err(|_| "更新状态不可用".into())
    }
    fn change(&self, app: &AppHandle, edit: impl FnOnce(&mut Snapshot)) {
        if let Ok(mut snapshot) = self.snapshot.lock() {
            edit(&mut snapshot);
            snapshot.revision += 1;
            let _ = app.emit_to("main", EVENT, &*snapshot);
        }
    }
    // Persist bounded stage evidence, never raw plugin errors, URLs or notes.
    fn record(&self, app: &AppHandle, stage: &str) -> Result<(), String> {
        let mut journal = self.journal.lock().map_err(|_| "更新日志不可用")?;
        let snapshot = self.snapshot()?;
        journal.push(serde_json::json!({"at":crate::now_ms(),"stage":stage,"currentVersion":snapshot.current_version,"version":snapshot.version}));
        if journal.len() > 100 {
            journal.remove(0);
        }
        storage::atomic_write_json(
            &directory(app)?.join("history.json"),
            &serde_json::json!({"schemaVersion":1,"data":*journal}),
        )
        .map_err(|_| "更新日志保存失败".into())
    }
    fn fail(&self, app: &AppHandle, phase: &'static str, message: &str, stage: &str) -> String {
        self.change(app, |s| {
            s.phase = phase;
            s.error = Some(message.into());
        });
        let _ = self.record(app, stage);
        message.into()
    }
}

pub(crate) fn setup(app: &AppHandle) {
    let service = app.state::<AppUpdate>();
    if let Ok(dir) = directory(app) {
        if let Ok(value) = storage::read_json_value(&dir.join("history.json")) {
            if let Some(entries) = value.get("data").and_then(|v| v.as_array()) {
                if let Ok(mut journal) = service.journal.lock() {
                    *journal = entries.iter().rev().take(99).cloned().collect();
                    journal.reverse();
                }
            }
        }
    }
    if !configured(app) {
        service.change(app, |s| s.phase = "disabled");
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(30)).await;
        loop {
            let _ = check(&app, true).await;
            tokio::time::sleep(Duration::from_secs(6 * 60 * 60)).await;
        }
    });
}
async fn check(app: &AppHandle, background: bool) -> Result<Snapshot, String> {
    let service = app.state::<AppUpdate>();
    if !configured(app) {
        return Err("此构建尚未配置 macOS 更新源。".into());
    }
    let mut candidate = service
        .candidate
        .try_lock()
        .map_err(|_| "更新操作正在进行")?;
    // A background check must never discard a verified download or offered version.
    if background && candidate.update.is_some() {
        return service.snapshot();
    }
    if candidate.bytes.is_some() {
        return service.snapshot();
    }
    service.change(app, |s| {
        s.phase = "checking";
        s.error = None;
    });
    let result = async {
        app.updater_builder()
            .timeout(Duration::from_secs(30))
            .build()?
            .check()
            .await
    }
    .await;
    match result {
        Ok(update) => {
            if update
                .as_ref()
                .is_some_and(|u| u.download_url.scheme() != "https")
            {
                return Err(service.fail(
                    app,
                    "idle",
                    "更新包地址必须使用 HTTPS。",
                    "invalid_transport",
                ));
            }
            service.change(app, |s| {
                s.phase = if update.is_some() {
                    "available"
                } else {
                    "current"
                };
                s.version = update.as_ref().map(|u| u.version.clone());
                s.notes = update
                    .as_ref()
                    .and_then(|u| u.body.clone())
                    .map(|s| s.chars().take(20_000).collect());
                s.checked_at = Some(crate::now_ms());
                s.downloaded = 0;
                s.total = None;
            });
            candidate.update = update;
            let _ = service.record(app, "checked");
            service.snapshot()
        }
        Err(_) => Err(service.fail(
            app,
            if candidate.update.is_some() {
                "available"
            } else {
                "idle"
            },
            "检查更新失败，请检查网络后重试。",
            "check_failed",
        )),
    }
}
#[tauri::command]
pub(crate) fn get_update_state(app: AppHandle, window: WebviewWindow) -> Result<Snapshot, String> {
    authorize(&window)?;
    app.state::<AppUpdate>().snapshot()
}
#[tauri::command]
pub(crate) async fn check_for_updates(
    app: AppHandle,
    window: WebviewWindow,
) -> Result<Snapshot, String> {
    authorize(&window)?;
    check(&app, false).await
}
#[tauri::command]
pub(crate) async fn download_update(
    app: AppHandle,
    window: WebviewWindow,
) -> Result<Snapshot, String> {
    authorize(&window)?;
    let service = app.state::<AppUpdate>();
    let mut candidate = service
        .candidate
        .try_lock()
        .map_err(|_| "更新操作正在进行")?;
    if candidate.bytes.is_some() {
        return service.snapshot();
    }
    let mut update = candidate.update.clone().ok_or("请先检查更新")?;
    update.timeout = Some(Duration::from_secs(10 * 60));
    service.change(&app, |s| {
        s.phase = "downloading";
        s.error = None;
        s.downloaded = 0;
        s.total = None;
    });
    let _ = service.record(&app, "download_started");
    let mut downloaded = 0u64;
    let mut last = std::time::Instant::now();
    match update
        .download(
            |chunk, total| {
                downloaded = downloaded.saturating_add(chunk as u64);
                if last.elapsed() >= Duration::from_millis(100) {
                    service.change(&app, |s| {
                        s.downloaded = downloaded;
                        s.total = total;
                    });
                    last = std::time::Instant::now();
                }
            },
            || {},
        )
        .await
    {
        Ok(bytes) => {
            service.change(&app, |s| {
                s.phase = "ready";
                s.downloaded = bytes.len() as u64;
                s.total = Some(bytes.len() as u64);
            });
            candidate.bytes = Some(bytes);
            let _ = service.record(&app, "download_verified");
            service.snapshot()
        }
        Err(_) => Err(service.fail(
            &app,
            "available",
            "下载或签名校验失败，请重试下载。",
            "download_or_signature_failed",
        )),
    }
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DraftFile {
    schema_version: u32,
    data: Drafts,
}
fn validate_drafts(drafts: &Drafts) -> Result<(), String> {
    if drafts
        .keys()
        .any(|s| !matches!(s.as_str(), "workbench" | "workbench-notes" | "legacy-chat"))
        || serde_json::to_vec(drafts)
            .map_err(|_| "草稿编码失败")?
            .len()
            > 2 * 1024 * 1024
    {
        return Err("草稿格式无效或超过 2 MiB，更新前请先保存。".into());
    }
    Ok(())
}
fn save_drafts(app: &AppHandle, drafts: Drafts) -> Result<(), String> {
    validate_drafts(&drafts)?;
    let service = app.state::<AppUpdate>();
    let _guard = service.drafts.lock().map_err(|_| "草稿状态不可用")?;
    storage::atomic_write_json(
        &directory(app)?.join("drafts.json"),
        &DraftFile {
            schema_version: 1,
            data: drafts,
        },
    )
    .map_err(|_| "草稿保存失败，已取消更新。".into())
}
#[tauri::command]
pub(crate) fn load_update_drafts(app: AppHandle, window: WebviewWindow) -> Result<Drafts, String> {
    authorize(&window)?;
    let path = directory(&app)?.join("drafts.json");
    if !path.exists() {
        return Ok(Drafts::new());
    }
    let value =
        storage::read_json_value(&path).map_err(|_| "更新草稿读取失败，请检查本地存储。")?;
    let file: DraftFile = serde_json::from_value(value).map_err(|_| "更新草稿格式无效")?;
    if file.schema_version != 1 {
        return Err("不支持此草稿版本".into());
    }
    validate_drafts(&file.data)?;
    Ok(file.data)
}
#[tauri::command]
pub(crate) fn save_update_drafts(
    app: AppHandle,
    window: WebviewWindow,
    drafts: Drafts,
) -> Result<(), String> {
    authorize(&window)?;
    save_drafts(&app, drafts)
}

#[tauri::command]
pub(crate) async fn install_update(
    app: AppHandle,
    window: WebviewWindow,
    drafts: Drafts,
) -> Result<(), String> {
    authorize(&window)?;
    let worker_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let app = worker_app;
        let service = app.state::<AppUpdate>();
        let candidate = service
            .candidate
            .try_lock()
            .map_err(|_| "更新操作正在进行")?;
        let update = candidate.update.as_ref().ok_or("请先下载更新")?;
        let bytes = candidate.bytes.as_ref().ok_or("请先下载更新")?;
        let _admission = supervisor()
            .begin_update()
            .map_err(|message| service.fail(&app, "ready", &message, "installation_deferred"))?;
        // The guard remains live through install/restart; failure releases admission.
        let prepare = (|| {
            check_install_location(&std::env::current_exe().map_err(|_| "无法定位应用")?)?;
            save_drafts(&app, drafts)?;
            service.record(&app, "install_started")
        })();
        if let Err(message) = prepare {
            return Err(service.fail(&app, "ready", &message, "prepare_failed"));
        }
        service.change(&app, |s| {
            s.phase = "installing";
            s.error = None;
        });
        if update.install(bytes).is_err() {
            return Err(service.fail(
                &app,
                "ready",
                "安装失败，请检查应用目录权限和磁盘空间后重试。",
                "install_failed",
            ));
        }
        service.change(&app, |s| s.phase = "restarting");
        let _ = service.record(&app, "installed");
        app.restart();
    })
    .await
    .map_err(|_| {
        app.state::<AppUpdate>().fail(
            &app,
            "ready",
            "安装任务异常，请重试或重新打开 Loom。",
            "install_task_failed",
        )
    })?
}
fn check_install_location(executable: &std::path::Path) -> Result<(), String> {
    if !cfg!(target_os = "macos") {
        return Err("当前仅支持 macOS 应用内更新。".into());
    }
    let bundle = executable
        .ancestors()
        .find(|p| p.extension().is_some_and(|ext| ext == "app"))
        .ok_or("请从已安装的 Loom.app 更新。")?;
    if bundle.starts_with("/Volumes") || bundle.to_string_lossy().contains("/AppTranslocation/") {
        return Err("请将 Loom 拖入应用程序目录后再更新。".into());
    }
    Ok(())
}
/// Restart cannot be deferred by Tauri. Only our prepared restart skips the
/// normal asynchronous drain gates. Ordinary exit stays blocked during install.
pub(crate) fn handle_event(app: &AppHandle, event: &tauri::RunEvent) -> bool {
    let phase = app
        .state::<AppUpdate>()
        .snapshot()
        .map(|s| s.phase)
        .unwrap_or("idle");
    match event {
        tauri::RunEvent::ExitRequested { code, api, .. }
            if phase == "restarting" && *code == Some(tauri::RESTART_EXIT_CODE) =>
        {
            true
        }
        tauri::RunEvent::ExitRequested { api, .. }
            if supervisor().is_updating() || phase == "installing" || phase == "restarting" =>
        {
            api.prevent_exit();
            true
        }
        _ => false,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_invalid_or_oversized_drafts() {
        assert!(validate_drafts(&BTreeMap::from([("arbitrary".into(), BTreeMap::new())])).is_err());
        assert!(validate_drafts(&BTreeMap::from([(
            "workbench".into(),
            BTreeMap::from([("s".into(), "x".repeat(2 * 1024 * 1024))])
        )]))
        .is_err());
        assert!(validate_drafts(&BTreeMap::from([(
            "workbench".into(),
            BTreeMap::from([("s".into(), "draft".into())])
        )]))
        .is_ok());
    }
    #[test]
    #[cfg(target_os = "macos")]
    fn rejects_dmg_and_unbundled_install() {
        assert!(check_install_location(std::path::Path::new(
            "/Volumes/Loom/Loom.app/Contents/MacOS/loom"
        ))
        .is_err());
        assert!(check_install_location(std::path::Path::new("/tmp/target/debug/loom")).is_err());
        assert!(check_install_location(std::path::Path::new(
            "/Applications/Loom.app/Contents/MacOS/loom"
        ))
        .is_ok());
    }
}
