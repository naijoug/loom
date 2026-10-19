//! Thin desktop host for the new core. Legacy Chat/Task is not involved.
mod connections;
mod host;
mod permissions;
mod runtime;
use host::Host;
use loom_core::{
    CoreReply, CoreRequest, HostCapabilities, Id, SessionKey, SessionView, WatchMessage,
};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc, Mutex,
    },
};
use tauri::{ipc::Channel, AppHandle, Manager, WebviewWindow};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

#[derive(Default)]
pub(crate) struct Workbench {
    host: Mutex<Option<Arc<Host>>>,
    exit_phase: AtomicU8,
}
fn authorize(window: &WebviewWindow) -> Result<(), String> {
    if window.label() != "main" {
        return Err("Workbench commands are restricted to the trusted main window".into());
    }
    Ok(())
}
impl Workbench {
    fn get(&self, app: &AppHandle) -> Result<Arc<Host>, String> {
        let mut slot = self
            .host
            .lock()
            .map_err(|_| "Workbench initialization unavailable")?;
        if let Some(host) = slot.as_ref() {
            return Ok(host.clone());
        }
        let default_root = app
            .path()
            .app_data_dir()
            .map_err(|e| e.to_string())?
            .join("workbench/v1");
        let root = if cfg!(debug_assertions) {
            std::env::var_os("LOOM_WORKBENCH_DATA_DIR")
                .map(PathBuf::from)
                .unwrap_or(default_root)
        } else {
            default_root
        };
        if !root.is_absolute() {
            return Err("Workbench data directory must be absolute".into());
        }
        let fixture =
            cfg!(debug_assertions) && std::env::var("LOOM_WORKBENCH_FIXTURE").as_deref() == Ok("1");
        let mut host = Host::open(&root, fixture).map_err(|e| e.to_string())?;
        if !fixture {
            Arc::get_mut(&mut host)
                .ok_or("Workbench initialization ownership lost")?
                .configure_sdk(runtime::SdkRuntime::development()?);
        }
        *slot = Some(host.clone());
        Ok(host)
    }
}
#[tauri::command]
pub(crate) async fn workbench_capabilities(
    app: AppHandle,
    window: WebviewWindow,
) -> Result<HostCapabilities, String> {
    authorize(&window)?;
    tauri::async_runtime::spawn_blocking(move || {
        let host = app.state::<Workbench>().get(&app)?;
        Ok(HostCapabilities {
            protocol_version: 1,
            runtime: host.runtime_name().into(),
            credential_store: host.connections.backend().into(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub(crate) async fn workbench_invoke(
    app: AppHandle,
    window: WebviewWindow,
    request: CoreRequest,
) -> Result<CoreReply, String> {
    authorize(&window)?;
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Workbench>().get(&app)?.invoke(request)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub(crate) async fn workbench_permissions(
    app: AppHandle,
    window: WebviewWindow,
    workspace_id: Id,
) -> Result<loom_core::PermissionSettings, String> {
    authorize(&window)?;
    tauri::async_runtime::spawn_blocking(move || {
        let host = app.state::<Workbench>().get(&app)?;
        Ok(loom_core::PermissionSettings {
            documents: host
                .core
                .permission_documents(&workspace_id)
                .map_err(|e| e.to_string())?,
            application_defaults: serde_json::to_value(
                loom_runtime::policy::PermissionConfig::bundled().map_err(|e| e.to_string())?,
            )
            .map_err(|_| "默认权限无法编码".to_string())?,
        })
    })
    .await
    .map_err(|_| "权限读取任务异常退出".to_string())?
}
#[tauri::command]
pub(crate) async fn workbench_save_permissions(
    app: AppHandle,
    window: WebviewWindow,
    request: loom_core::SavePermissionRequest,
) -> Result<loom_core::PermissionDocument, String> {
    authorize(&window)?;
    tauri::async_runtime::spawn_blocking(move || {
        let host = app.state::<Workbench>().get(&app)?;
        permissions::save(&host.core, request, host.root.clone())
    })
    .await
    .map_err(|_| "权限保存任务异常退出".to_string())?
}
#[tauri::command]
pub(crate) async fn workbench_save_connection(
    app: AppHandle,
    window: WebviewWindow,
    request: loom_core::SaveConnectionRequest,
) -> Result<loom_core::ModelConnection, String> {
    authorize(&window)?;
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Workbench>()
            .get(&app)?
            .connections
            .save(request)
    })
    .await
    .map_err(|_| "连接保存任务异常退出".to_string())?
}
#[tauri::command]
pub(crate) async fn workbench_watch(
    app: AppHandle,
    window: WebviewWindow,
    key: SessionKey,
    watch_id: Id,
    on_event: Channel<WatchMessage>,
) -> Result<SessionView, String> {
    authorize(&window)?;
    let label = window.label().to_owned();
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Workbench>()
            .get(&app)?
            .watch(label, watch_id, key, move |event| {
                on_event.send(event).map_err(|e| e.to_string())
            })
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub(crate) async fn workbench_unwatch(
    app: AppHandle,
    window: WebviewWindow,
    watch_id: Id,
) -> Result<(), String> {
    authorize(&window)?;
    let label = window.label().to_owned();
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Workbench>()
            .get(&app)?
            .unwatch(&label, &watch_id);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub(crate) async fn workbench_resume_queue(
    app: AppHandle,
    window: WebviewWindow,
    key: SessionKey,
) -> Result<(), String> {
    authorize(&window)?;
    tauri::async_runtime::spawn_blocking(move || {
        let _operation = crate::process_supervisor::supervisor().begin_operation()?;
        let host = app.state::<Workbench>().get(&app)?;
        if !host.runtime_available() {
            return Err("No runtime connection is configured".into());
        }
        host.schedule(key);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}
/// The new host drains before the existing exit gate. No legacy Task machinery
/// is reused; once this host is ready the existing gate can drain its own work.
pub(crate) fn handle_event(app: &AppHandle, event: &tauri::RunEvent) -> bool {
    let state = app.state::<Workbench>();
    let host = state.host.lock().ok().and_then(|host| host.clone());
    let Some(host) = host else { return false };
    if let tauri::RunEvent::WindowEvent {
        label,
        event: tauri::WindowEvent::Destroyed,
        ..
    } = event
    {
        host.close_window(label);
    }
    if let tauri::RunEvent::ExitRequested { code, api, .. } = event {
        if state.exit_phase.load(Ordering::Acquire) == 2 {
            return false;
        }
        api.prevent_exit();
        if state
            .exit_phase
            .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            // Close the common updater/execution gate before asynchronous drain.
            let _ = crate::process_supervisor::supervisor().begin_shutdown();
            host.begin_shutdown();
            let app = app.clone();
            let code = code.unwrap_or(0);
            tauri::async_runtime::spawn(async move {
                let result = tauri::async_runtime::spawn_blocking(move || {
                    host.drain(std::time::Duration::from_secs(6))
                })
                .await;
                match result {
                    Ok(Ok(())) => {
                        app.state::<Workbench>()
                            .exit_phase
                            .store(2, Ordering::Release);
                        app.exit(code);
                    }
                    error => {
                        app.state::<Workbench>()
                            .exit_phase
                            .store(0, Ordering::Release);
                        app.dialog()
                            .message(format!("新工作区仍未完成保存：{error:?}。请再次退出重试。"))
                            .title("Loom")
                            .kind(MessageDialogKind::Error)
                            .show(|_| {});
                    }
                }
            });
        }
        return true;
    }
    false
}

#[tauri::command]
pub(crate) async fn workbench_watch_workspace(
    app: AppHandle,
    window: WebviewWindow,
    workspace_id: Id,
    watch_id: Id,
    on_event: Channel<WatchMessage>,
) -> Result<(), String> {
    authorize(&window)?;
    let label = window.label().to_owned();
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Workbench>().get(&app)?.watch_workspace(
            label,
            watch_id,
            workspace_id,
            move |notice| on_event.send(notice).map_err(|e| e.to_string()),
        )
    })
    .await
    .map_err(|e| e.to_string())?
}
