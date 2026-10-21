use super::{
    catalog::SkillDetail,
    error::{ErrorCode, Result, SkillsError},
    operations::{self, Operation, OperationRecord},
    repository::{LibraryView, Repository, SourceKind},
    sources,
};
use std::{path::PathBuf, sync::Arc};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

fn repository(app: &AppHandle, window: &WebviewWindow) -> Result<Repository> {
    if window.label() != "main" {
        return Err(SkillsError::new(
            ErrorCode::PermissionDenied,
            "Skills access requires the main window",
        ));
    }
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|_| SkillsError::invalid("Application data directory unavailable"))?;
    let data = std::env::var_os("LOOM_SKILLS_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| app_data.join("skills"));
    if !data.is_absolute() {
        return Err(SkillsError::invalid(
            "LOOM_SKILLS_DATA_DIR must be absolute",
        ));
    }
    Repository::open(&data, &app_data.join("skills-coordination"))
}

async fn blocking<T: Send + 'static>(
    repository: Repository,
    work: impl FnOnce(Repository) -> Result<T> + Send + 'static,
) -> Result<T> {
    tauri::async_runtime::spawn_blocking(move || work(repository))
        .await
        .map_err(|_| SkillsError::recovery("Skills worker interrupted"))?
}

#[tauri::command]
pub async fn skills_list_library(app: AppHandle, window: WebviewWindow) -> Result<LibraryView> {
    blocking(repository(&app, &window)?, |repo| repo.list()).await
}
#[tauri::command]
pub async fn skills_get_detail(
    app: AppHandle,
    window: WebviewWindow,
    skill_id: String,
) -> Result<SkillDetail> {
    blocking(repository(&app, &window)?, move |repo| {
        repo.detail(&skill_id)
    })
    .await
}
#[tauri::command]
pub async fn skills_add_local_source(
    app: AppHandle,
    window: WebviewWindow,
    path: String,
) -> Result<LibraryView> {
    blocking(repository(&app, &window)?, move |repo| {
        let _lease = repo.coordinator.enter()?;
        let _admission = crate::process_supervisor::supervisor()
            .begin_operation()
            .map_err(|e| SkillsError::new(ErrorCode::Updating, e))?;
        repo.add_local(&PathBuf::from(path))?;
        repo.list()
    })
    .await
}
#[tauri::command]
pub async fn skills_remove_source(
    app: AppHandle,
    window: WebviewWindow,
    source_id: String,
) -> Result<LibraryView> {
    blocking(repository(&app, &window)?, move |repo| {
        let _lease = repo.coordinator.enter()?;
        let _admission = crate::process_supervisor::supervisor()
            .begin_operation()
            .map_err(|e| SkillsError::new(ErrorCode::Updating, e))?;
        repo.remove_source(&source_id)?;
        repo.list()
    })
    .await
}

#[tauri::command]
pub async fn skills_import_repository(
    app: AppHandle,
    window: WebviewWindow,
    url: String,
) -> Result<OperationRecord> {
    let repo = repository(&app, &window)?;
    let source = sources::normalize_git_url(&url)?;
    let expected = repo
        .load()?
        .sources
        .iter()
        .find(|s| s.id == source.source_id)
        .and_then(|s| s.revision.clone());
    let op = Operation::begin(&repo, &source.source_id, "import")?;
    let record = op.record.clone();
    tauri::async_runtime::spawn(operations::import_git(
        repo,
        op,
        source,
        expected,
        move |event| {
            let _ = app.emit("loom://skills-operation", event);
        },
    ));
    Ok(record)
}
#[tauri::command]
pub async fn skills_refresh_source(
    app: AppHandle,
    window: WebviewWindow,
    source_id: String,
) -> Result<OperationRecord> {
    let repo = repository(&app, &window)?;
    let source = repo.get_source(&source_id)?;
    let op = Operation::begin(&repo, &source_id, "refresh")?;
    let record = op.record.clone();
    if source.kind == SourceKind::Git {
        let git = source
            .git
            .ok_or_else(|| SkillsError::recovery("Git source metadata missing"))?;
        tauri::async_runtime::spawn(operations::import_git(
            repo,
            op,
            git,
            source.revision,
            move |event| {
                let _ = app.emit("loom://skills-operation", event);
            },
        ));
    } else {
        let app = Arc::new(app);
        tauri::async_runtime::spawn_blocking(move || {
            let mut op = op;
            let result = repo
                .source_directory(&source)
                .and_then(|path| super::catalog::scan(&source.id, &path))
                .map(|_| ());
            let _ = op.finish(result);
            let _ = app.emit("loom://skills-operation", &op.record);
        });
    }
    Ok(record)
}
#[tauri::command]
pub async fn skills_list_operations(
    app: AppHandle,
    window: WebviewWindow,
) -> Result<Vec<OperationRecord>> {
    blocking(repository(&app, &window)?, |repo| operations::list(&repo)).await
}
#[tauri::command]
pub async fn skills_cancel_operation(
    app: AppHandle,
    window: WebviewWindow,
    operation_id: String,
) -> Result<()> {
    blocking(repository(&app, &window)?, move |repo| {
        operations::cancel(&repo, &operation_id)
    })
    .await
}
