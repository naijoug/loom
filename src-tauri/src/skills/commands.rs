use super::{
    catalog::SkillDetail,
    error::{ErrorCode, Result, SkillsError},
    installers::{
        self, InstallPlan, InstallReceipt, InstallRequest, InstallStatus, RecoveryItem,
        UninstallRequest,
    },
    operations::{self, Operation, OperationRecord},
    repository::{LibraryView, Repository, SourceKind},
    sources,
    targets::{InstallTarget, TargetContext},
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
    let home = app
        .path()
        .home_dir()
        .map_err(|_| SkillsError::invalid("User home unavailable"))?;
    super::runtime_paths::RuntimePaths::from_base(home, app_data, None)?.repository()
}

fn target_context(app: &AppHandle, project_path: Option<&str>) -> Result<TargetContext> {
    let home = app
        .path()
        .home_dir()
        .map_err(|_| SkillsError::invalid("User home unavailable"))?;
    let project = project_path
        .map(|path| crate::projects::canonical_project_path(path).map_err(SkillsError::invalid))
        .transpose()?;
    if let Some(project) = &project {
        let registered = crate::storage::load_recent_projects(app).map_err(SkillsError::invalid)?;
        if !registered
            .iter()
            .any(|entry| std::path::Path::new(&entry.path) == project)
        {
            return Err(SkillsError::new(
                ErrorCode::PermissionDenied,
                "Open this project in Loom before installing project skills",
            ));
        }
    }
    TargetContext::new(&home, project.as_deref())
}

pub(crate) fn update_lease(app: &AppHandle) -> Result<super::coordination::LifecycleLease> {
    let root = app
        .path()
        .app_data_dir()
        .map_err(|_| SkillsError::invalid("Application data directory unavailable"))?;
    super::coordination::Coordinator::open(&root.join("skills-coordination"))?.try_update()
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
    source_mode: Option<String>,
) -> Result<OperationRecord> {
    let repo = repository(&app, &window)?;
    let source = sources::normalize_git_url(&url)?;
    let api = match source_mode.as_deref().unwrap_or("git") {
        "git" => false,
        "github-api" => true,
        _ => return Err(SkillsError::invalid("Unsupported repository source mode")),
    };
    let id = if api {
        super::github_api::source_id(&source)?
    } else {
        source.source_id.clone()
    };
    let expected = repo
        .load()?
        .sources
        .iter()
        .find(|s| s.id == id)
        .and_then(|s| s.revision.clone());
    let client = if api {
        Some(super::github_api::Client::from_environment()?)
    } else {
        None
    };
    let op = Operation::begin(&repo, &id, "import")?;
    let record = op.record.clone();
    if let Some(client) = client {
        tauri::async_runtime::spawn(super::github_api::import(
            repo,
            op,
            source,
            expected,
            client,
            move |event| {
                let _ = app.emit("loom://skills-operation", event);
            },
        ));
    } else {
        tauri::async_runtime::spawn(operations::import_git(
            repo,
            op,
            source,
            expected,
            move |event| {
                let _ = app.emit("loom://skills-operation", event);
            },
        ));
    }
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
    let client = if source.kind == SourceKind::GithubApi {
        Some(super::github_api::Client::from_environment()?)
    } else {
        None
    };
    let op = Operation::begin(&repo, &source_id, "refresh")?;
    let record = op.record.clone();
    if let Some(client) = client {
        let git = source
            .git
            .ok_or_else(|| SkillsError::recovery("GitHub API source metadata missing"))?;
        tauri::async_runtime::spawn(super::github_api::import(
            repo,
            op,
            git,
            source.revision,
            client,
            move |event| {
                let _ = app.emit("loom://skills-operation", event);
            },
        ));
    } else if source.kind == SourceKind::Git {
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

#[tauri::command]
pub async fn skills_list_install_targets(
    app: AppHandle,
    window: WebviewWindow,
    project_path: Option<String>,
) -> Result<Vec<InstallTarget>> {
    let repo = repository(&app, &window)?;
    let context = target_context(&app, project_path.as_deref())?;
    blocking(repo, move |_| context.targets()).await
}
#[tauri::command]
pub async fn skills_get_install_status(
    app: AppHandle,
    window: WebviewWindow,
    skill_id: String,
    project_path: Option<String>,
) -> Result<Vec<InstallStatus>> {
    let repo = repository(&app, &window)?;
    let context = target_context(&app, project_path.as_deref())?;
    blocking(repo, move |repo| {
        installers::statuses(&repo, &context, &skill_id)
    })
    .await
}
#[tauri::command]
pub async fn skills_list_installations(
    app: AppHandle,
    window: WebviewWindow,
    project_path: Option<String>,
) -> Result<Vec<InstallStatus>> {
    let repo = repository(&app, &window)?;
    let context = target_context(&app, project_path.as_deref())?;
    blocking(repo, move |repo| installers::inventory(&repo, &context)).await
}
#[tauri::command]
pub async fn skills_plan_install(
    app: AppHandle,
    window: WebviewWindow,
    input: InstallRequest,
) -> Result<InstallPlan> {
    let repo = repository(&app, &window)?;
    let context = target_context(&app, input.project_path.as_deref())?;
    blocking(repo, move |repo| {
        installers::plan_install(&repo, &context, &input)
    })
    .await
}
#[tauri::command]
pub async fn skills_plan_uninstall(
    app: AppHandle,
    window: WebviewWindow,
    input: UninstallRequest,
) -> Result<InstallPlan> {
    let repo = repository(&app, &window)?;
    let context = target_context(&app, input.project_path.as_deref())?;
    blocking(repo, move |repo| {
        installers::plan_uninstall(&repo, &context, &input)
    })
    .await
}
#[tauri::command]
pub async fn skills_apply_install(
    app: AppHandle,
    window: WebviewWindow,
    plan_id: String,
    project_path: Option<String>,
) -> Result<InstallReceipt> {
    let repo = repository(&app, &window)?;
    let context = target_context(&app, project_path.as_deref())?;
    blocking(repo, move |repo| {
        let plan = installers::load_plan(&repo, &plan_id)?;
        let source_id = plan
            .source
            .as_ref()
            .map(|s| s.source_id.as_str())
            .unwrap_or("installed");
        let mut operation = Operation::begin(
            &repo,
            source_id,
            if plan.action == installers::PlanAction::Install {
                "install"
            } else {
                "uninstall"
            },
        )?;
        operation.stage("applying")?;
        let _ = app.emit("loom://skills-operation", &operation.record);
        let result = installers::apply(&repo, &context, &plan_id, &mut operation);
        let terminal = match &result {
            Ok(receipt)
                if receipt.items.iter().all(|i| {
                    matches!(i.status.as_str(), "installed" | "uninstalled" | "skipped")
                }) =>
            {
                Ok(())
            }
            Ok(receipt)
                if receipt.items.iter().all(|item| {
                    matches!(
                        item.status.as_str(),
                        "installed" | "uninstalled" | "skipped" | "cancelled"
                    )
                }) =>
            {
                Err(SkillsError::new(
                    ErrorCode::Cancelled,
                    "Installation cancelled at a safe checkpoint",
                ))
            }
            Ok(_) => Err(SkillsError::new(
                ErrorCode::Conflict,
                "Some installation actions did not complete; inspect their results",
            )),
            Err(error) => Err(error.clone()),
        };
        operation.finish(terminal)?;
        let _ = app.emit("loom://skills-operation", &operation.record);
        result
    })
    .await
}
#[tauri::command]
pub async fn skills_list_pending(
    app: AppHandle,
    window: WebviewWindow,
    project_path: Option<String>,
) -> Result<Vec<RecoveryItem>> {
    let repo = repository(&app, &window)?;
    let context = target_context(&app, project_path.as_deref())?;
    blocking(repo, move |repo| {
        installers::recovery_items(&repo, &context)
    })
    .await
}
#[tauri::command]
pub async fn skills_recover_installation(
    app: AppHandle,
    window: WebviewWindow,
    operation_id: String,
    project_path: Option<String>,
) -> Result<String> {
    let repo = repository(&app, &window)?;
    let context = target_context(&app, project_path.as_deref())?;
    blocking(repo, move |repo| {
        let mut operation = Operation::begin(&repo, "installed", "recover")?;
        let _ = app.emit("loom://skills-operation", &operation.record);
        let result = installers::recover_one(&repo, &context, &operation_id);
        operation.finish(result.as_ref().map(|_| ()).map_err(Clone::clone))?;
        let _ = app.emit("loom://skills-operation", &operation.record);
        result.map(|outcome| {
            if outcome == super::transactions::Outcome::Committed {
                "committed"
            } else {
                "rolled_back"
            }
            .into()
        })
    })
    .await
}

#[tauri::command]
pub async fn skills_list_translation_providers(
    app: AppHandle,
    window: WebviewWindow,
) -> Result<super::translation::config::ProviderList> {
    blocking(repository(&app, &window)?, |repo| {
        super::translation::config::list(
            &repo,
            &super::translation::config::RuntimeConfig::capture()?,
        )
    })
    .await
}
#[tauri::command]
pub async fn skills_save_translation_config(
    app: AppHandle,
    window: WebviewWindow,
    input: super::translation::config::ConfigInput,
) -> Result<super::translation::config::ProviderList> {
    blocking(repository(&app, &window)?, move |repo| {
        super::translation::config::save(
            &repo,
            input,
            &super::translation::config::RuntimeConfig::capture()?,
        )
    })
    .await
}

#[tauri::command]
pub async fn skills_translate(
    app: AppHandle,
    window: WebviewWindow,
    input: super::translation::TranslationInput,
) -> Result<super::translation::TranslationSummary> {
    let prepared = blocking(repository(&app, &window)?, move |repo| {
        super::translation::prepare(
            repo,
            input,
            &super::translation::config::RuntimeConfig::capture()?,
        )
    })
    .await?;
    let summary = prepared.summary();
    tauri::async_runtime::spawn(super::translation::run(prepared, move |event| {
        let _ = app.emit("loom://skills-operation", event);
    }));
    Ok(summary)
}
#[tauri::command]
pub async fn skills_translation_history(
    app: AppHandle,
    window: WebviewWindow,
    skill_id: String,
) -> Result<Vec<super::translation::TranslationSummary>> {
    blocking(repository(&app, &window)?, move |repo| {
        super::translation::history(&repo, &skill_id)
    })
    .await
}
#[tauri::command]
pub async fn skills_translation_result(
    app: AppHandle,
    window: WebviewWindow,
    operation_id: String,
) -> Result<super::translation::TranslationResult> {
    blocking(repository(&app, &window)?, move |repo| {
        super::translation::result(&repo, &operation_id)
    })
    .await
}

/// Deliberately bypass repository()/target_context(): those initialize directories.
#[tauri::command]
pub async fn skills_plan_migration(
    app: AppHandle,
    window: WebviewWindow,
    input: super::migration::MigrationInput,
) -> Result<super::migration::MigrationPreview> {
    if window.label() != "main" {
        return Err(SkillsError::new(
            ErrorCode::PermissionDenied,
            "Skills preview requires the main window",
        ));
    }
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|_| SkillsError::invalid("Application data unavailable"))?;
    let home = app
        .path()
        .home_dir()
        .map_err(|_| SkillsError::invalid("Home unavailable"))?;
    let paths = super::runtime_paths::RuntimePaths::from_base(home, app_data.clone(), None)?;
    let default = app_data
        .parent()
        .ok_or_else(|| SkillsError::invalid("Application data parent unavailable"))?
        .join("com.naijoug.skills-manager");
    tauri::async_runtime::spawn_blocking(move || {
        let project = input
            .project_path
            .as_deref()
            .map(crate::projects::canonical_project_path)
            .transpose()
            .map_err(SkillsError::invalid)?;
        if let Some(project) = &project {
            let config = std::env::var_os("LOOM_PROFILE_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| app_data.join("loom"));
            if !config.is_absolute() {
                return Err(SkillsError::invalid("Profile path must be absolute"));
            }
            let path = super::files::resource_path(&config.join("recent-projects.json"))?;
            let entries: Vec<crate::models::ProjectSummary> = super::files::read_json(&path)?;
            if !entries
                .iter()
                .any(|p| std::path::Path::new(&p.path) == project)
            {
                return Err(SkillsError::new(
                    ErrorCode::PermissionDenied,
                    "Open the project in Loom before previewing its legacy installations",
                ));
            }
        }
        let targets = TargetContext::new(&paths.home, project.as_deref())?;
        super::migration::preview(&default, &paths.library, &targets, &input)
    })
    .await
    .map_err(|_| SkillsError::recovery("Migration preview worker interrupted"))?
}
