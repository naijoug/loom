use super::{
    jobs::{Emit, Pipeline},
    models::*,
    prompt,
    store::{self, Store},
    DISCUSS_EVENT,
};
use crate::{agent, models::now_ms, source_files};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};
fn store(app: &AppHandle, window: &WebviewWindow) -> Result<Store, String> {
    if window.label() != "main" {
        return Err("Discuss access requires the main window".into());
    }
    Ok(Store::new(
        &app.path().app_data_dir().map_err(|e| e.to_string())?,
    ))
}
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|_| "Discussion worker interrupted".to_string())?
}
fn emitter(app: &AppHandle) -> Emit {
    let app = app.clone();
    Arc::new(move |event| {
        let _ = app.emit(DISCUSS_EVENT, event);
    })
}
fn project(app: &AppHandle, path: &str) -> Result<(String, String), String> {
    let p = crate::projects::canonical_project_path(path)?;
    if !crate::storage::load_recent_projects(app)?
        .iter()
        .any(|r| Path::new(&r.path) == p)
    {
        return Err("Open this project in Loom before discussing it".into());
    }
    Ok((
        p.to_string_lossy().into_owned(),
        p.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
    ))
}
pub(super) fn validate(input: &mut CreateDiscussionInput) -> Result<(), String> {
    input.topic = input.topic.trim().to_string();
    if input.topic.is_empty() || input.topic.chars().count() > 8000 {
        return Err("Enter a topic of 1–8000 characters".into());
    }
    if !(2..=4).contains(&input.participants.len()) {
        return Err("Choose 2–4 participants".into());
    }
    if input.confirmed_calls != input.participants.len() * 2 + 1 {
        return Err("Confirm the expected number of Agent calls before starting".into());
    }
    if !["en", "zh-CN"].contains(&input.language.as_str()) {
        return Err("Unsupported language".into());
    }
    let mut choices = HashSet::new();
    for p in &mut input.participants {
        p.model = p
            .model
            .take()
            .map(|m| m.trim().to_string())
            .filter(|m| !m.is_empty());
        if let Some(model) = &p.model {
            agent::validate_model_name(model)?;
        }
        if !choices.insert((p.agent_id.clone(), p.model.clone())) {
            return Err("Choose a different Agent or model for each participant".into());
        }
    }
    Ok(())
}
fn supported(adapter: &str) -> bool {
    matches!(
        adapter,
        agent::ADAPTER_CODEX
            | agent::ADAPTER_CLAUDE
            | agent::ADAPTER_OPENCODE
            | agent::ADAPTER_GROK
    )
}
async fn start(
    app: &AppHandle,
    store: Store,
    id: String,
    retry: Option<String>,
) -> Result<Discussion, String> {
    let agents = agent::load_agents(app)?
        .into_iter()
        .map(|a| (a.id.clone(), a))
        .collect::<HashMap<_, _>>();
    let (s, key, target) = (store.clone(), id.clone(), retry.clone());
    let (d, lease) = blocking(move || {
        s.load(&key)?;
        s.begin(&key, target.as_deref())
    })
    .await?;
    tauri::async_runtime::spawn(
        Pipeline {
            store,
            id,
            agents,
            emit: emitter(app),
        }
        .run(lease, retry),
    );
    Ok(d)
}
#[tauri::command]
pub async fn discuss_list(app: AppHandle, window: WebviewWindow) -> Result<DiscussionList, String> {
    let s = store(&app, &window)?;
    blocking(move || Ok(s.list())).await
}
#[tauri::command]
pub async fn discuss_get(
    app: AppHandle,
    window: WebviewWindow,
    discussion_id: String,
) -> Result<Discussion, String> {
    let s = store(&app, &window)?;
    blocking(move || s.load(&discussion_id)).await
}
#[tauri::command]
pub async fn discuss_create(
    app: AppHandle,
    window: WebviewWindow,
    mut input: CreateDiscussionInput,
) -> Result<Discussion, String> {
    let s = store(&app, &window)?;
    validate(&mut input)?;
    let agents = agent::load_agents(&app)?;
    let participants = input
        .participants
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let a = agents
                .iter()
                .find(|a| a.id == p.agent_id)
                .ok_or("Agent not found")?;
            if !supported(&a.adapter_type) {
                return Err("Discuss requires built-in CLI adapters".into());
            }
            // Availability and startup checks run per call, so a broken participant cannot discard its peers.
            Ok(Participant {
                id: format!("p{}", i + 1),
                agent_id: a.id.clone(),
                name: a.name.clone(),
                adapter_type: a.adapter_type.clone(),
                model: p.model.clone(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let project = if let Some(path) = input.project_path.filter(|p| !p.trim().is_empty()) {
        let app = app.clone();
        Some(blocking(move || project(&app, path.trim())).await?)
    } else {
        None
    };
    let now = now_ms();
    let id = store::next_id("discussion");
    let d = Discussion {
        schema_version: SCHEMA_VERSION,
        discussion_id: id.clone(),
        title: input.topic.chars().take(100).collect(),
        topic: input.topic,
        language: input.language,
        project_path: project.as_ref().map(|p| p.0.clone()),
        project_name: project.map(|p| p.1),
        tasks: tasks(&participants),
        participants,
        status: "interrupted".into(),
        active_run_id: None,
        created_at_ms: now,
        updated_at_ms: now,
        prompt_version: prompt::VERSION,
    };
    let copy = s.clone();
    blocking(move || copy.create(&d)).await?;
    start(&app, s, id, None).await
}
#[tauri::command]
pub async fn discuss_retry(
    app: AppHandle,
    window: WebviewWindow,
    discussion_id: String,
    task_id: String,
    confirmed_calls: usize,
) -> Result<Discussion, String> {
    let s = store(&app, &window)?;
    if confirmed_calls != 1 {
        return Err("Confirm one Agent call before retrying".into());
    }
    let (copy, id, app_copy) = (s.clone(), discussion_id.clone(), app.clone());
    blocking(move || {
        let d = copy.load(&id)?;
        if let Some(p) = d.project_path {
            project(&app_copy, &p)?;
        }
        Ok(())
    })
    .await?;
    start(&app, s, discussion_id, Some(task_id)).await
}
#[tauri::command]
pub async fn discuss_cancel(
    app: AppHandle,
    window: WebviewWindow,
    discussion_id: String,
) -> Result<bool, String> {
    let s = store(&app, &window)?;
    blocking(move || {
        s.load(&discussion_id)?;
        Ok(store::cancel(&discussion_id))
    })
    .await
}
fn source_root(app: &AppHandle, s: &Store, d: &Discussion) -> Result<PathBuf, String> {
    match &d.project_path {
        Some(p) => Ok(project(app, p)?.0.into()),
        None => s.workspace(&d.discussion_id),
    }
}
#[tauri::command]
pub async fn discuss_read_report(
    app: AppHandle,
    window: WebviewWindow,
    discussion_id: String,
    task_id: String,
) -> Result<Report, String> {
    let s = store(&app, &window)?;
    blocking(move || {
        let d = s.load(&discussion_id)?;
        let t = d.task(&task_id)?;
        let markdown = s
            .read_report(&discussion_id, t)?
            .ok_or("This call has no report yet")?;
        let refs = source_root(&app, &s, &d)
            .map(|p| source_files::page_refs(&markdown, &p))
            .unwrap_or_default();
        Ok(Report {
            task_id,
            markdown,
            refs,
        })
    })
    .await
}
#[tauri::command]
pub async fn discuss_read_source(
    app: AppHandle,
    window: WebviewWindow,
    discussion_id: String,
    path: String,
    start_line: Option<u32>,
    end_line: Option<u32>,
) -> Result<SourceText, String> {
    let s = store(&app, &window)?;
    blocking(move || {
        let d = s.load(&discussion_id)?;
        source_files::read_source(&source_root(&app, &s, &d)?, &path, start_line, end_line)
    })
    .await
}
#[tauri::command]
pub async fn discuss_read_job_logs(
    app: AppHandle,
    window: WebviewWindow,
    discussion_id: String,
    job_id: String,
    stream: String,
    offset: Option<u64>,
) -> Result<LogChunk, String> {
    let s = store(&app, &window)?;
    blocking(move || s.read_log(&discussion_id, &job_id, &stream, offset.unwrap_or(0))).await
}
#[tauri::command]
pub async fn discuss_delete(
    app: AppHandle,
    window: WebviewWindow,
    discussion_id: String,
) -> Result<(), String> {
    let s = store(&app, &window)?;
    let id = discussion_id.clone();
    blocking(move || s.delete(&id)).await?;
    emitter(&app)(DiscussEvent {
        discussion_id,
        task_id: None,
        status: "deleted".into(),
    });
    Ok(())
}
#[tauri::command]
pub async fn discuss_list_tree(
    app: AppHandle,
    window: WebviewWindow,
    discussion_id: String,
    dir: Option<String>,
) -> Result<Vec<TreeEntry>, String> {
    let s = store(&app, &window)?;
    blocking(move || {
        let d = s.load(&discussion_id)?;
        source_files::list_tree(&source_root(&app, &s, &d)?, dir.as_deref().unwrap_or(""))
    })
    .await
}
