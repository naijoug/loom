//! Tauri commands for Research. File IO runs on blocking threads; jobs run on
//! the async runtime and report progress through `loom://research-event`.
use super::{
    jobs::{Emit, Job},
    models::*,
    prompt,
    store::{self, Store},
    RESEARCH_EVENT,
};
use crate::{
    agent,
    models::{now_ms, AgentConfig},
    source_files,
};
use std::{path::Path, sync::Arc};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

const LANGUAGES: &[&str] = &["en", "zh-CN"];
const MAX_QUESTION_CHARS: usize = 8_000;

pub(super) fn store<R: tauri::Runtime>(
    app: &AppHandle<R>,
    window: &WebviewWindow<R>,
) -> Result<Store, String> {
    if window.label() != "main" {
        return Err("Research access requires the main window".into());
    }
    #[cfg(test)]
    if let Some(context) = app.try_state::<super::ipc_tests::Context>() {
        return Ok(Store::new(&context.root));
    }
    // The native acceptance profile must isolate Research as well as settings/agents.
    // Ordinary installations retain their established appData/research path.
    let app_data = if std::env::var_os("LOOM_PROFILE_DIR").is_some() {
        crate::storage::global_config_dir(app)?
    } else {
        app.path()
            .app_data_dir()
            .map_err(|_| "Application data directory unavailable".to_string())?
    };
    Ok(Store::new(&app_data))
}

pub(super) async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|_| "Research worker interrupted".to_string())?
}

fn emitter<R: tauri::Runtime>(app: &AppHandle<R>) -> Emit {
    let app = app.clone();
    Arc::new(move |event| {
        let _ = app.emit(RESEARCH_EVENT, event);
    })
}

fn question(text: &str) -> Result<String, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Enter a question".into());
    }
    if text.chars().count() > MAX_QUESTION_CHARS {
        return Err(format!(
            "Questions are limited to {MAX_QUESTION_CHARS} characters"
        ));
    }
    Ok(text.to_string())
}

/// Canonical path of a project the user opened in Loom; Research never reads
/// folders outside that list.
fn registered_project<R: tauri::Runtime>(
    app: &AppHandle<R>,
    path: &str,
) -> Result<(String, String), String> {
    let project = crate::projects::canonical_project_path(path)?;
    if !crate::storage::load_recent_projects(app)?
        .iter()
        .any(|item| Path::new(&item.path) == project)
    {
        return Err("Open this project in Loom before researching it".into());
    }
    let name = project
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| project.display().to_string());
    Ok((project.to_string_lossy().into_owned(), name))
}

/// Research runs only built-in adapters that are enabled, found on PATH and
/// actually start, so a broken install is reported before a question is recorded.
pub(super) async fn ready_agent<R: tauri::Runtime>(
    app: &AppHandle<R>,
    agent_id: &str,
    model: Option<&str>,
) -> Result<AgentConfig, String> {
    let agent = agent::load_agents(app)?
        .into_iter()
        .find(|agent| agent.id == agent_id)
        .ok_or("Agent not found")?;
    if !agent.enabled {
        return Err(format!("{} is disabled", agent.name));
    }
    if !agent.available {
        return Err(format!("{} was not found on this machine", agent.name));
    }
    if !agent::capabilities(&agent.adapter_type).model_selection {
        return Err(format!(
            "{} is not supported by Research; choose a built-in agent",
            agent.name
        ));
    }
    if let Some(model) = model {
        agent::validate_model_name(model)?;
    }
    agent::check_startable(&agent).await?;
    Ok(agent)
}

/// Re-checks what a follow-up depends on: the project is still registered and
/// the research's agent still starts.
async fn ready_for<R: tauri::Runtime>(
    app: &AppHandle<R>,
    research: &Research,
) -> Result<AgentConfig, String> {
    if let Some(path) = &research.project_path {
        registered_project(app, path)?;
    }
    ready_agent(app, &research.agent_id, research.model.as_deref()).await
}

/// Claims the job slot, records `turn` as running via `change`, and spawns the job.
async fn start<R: tauri::Runtime>(
    app: &AppHandle<R>,
    store: Store,
    research_id: &str,
    agent: AgentConfig,
    change: impl FnOnce(&mut Research, &str) -> Result<u32, String> + Send + 'static,
) -> Result<Research, String> {
    let job_id = store::next_id("job");
    let cancel = store::claim_job(&store, research_id, &job_id)?;
    let updated = {
        let (store, id, job) = (store.clone(), research_id.to_string(), job_id.clone());
        blocking(move || store.update(&id, |research| change(research, &job))).await
    };
    let (research, turn) = match updated {
        Ok(value) => value,
        Err(error) => {
            store::release_job(&store, research_id, &job_id);
            return Err(error);
        }
    };
    let job = Job {
        store,
        research_id: research_id.into(),
        job_id,
        turn,
        cancel,
        agent,
        emit: emitter(app),
    };
    tauri::async_runtime::spawn(job.run());
    Ok(research)
}

fn running_turn(index: u32, question: String, job_id: &str) -> Turn {
    Turn {
        index,
        question,
        status: TurnStatus::Running,
        job_id: job_id.into(),
        started_at_ms: now_ms(),
        finished_at_ms: None,
        error: None,
        warnings: Vec::new(),
        refs: None,
        context: None,
    }
}

#[tauri::command]
pub async fn research_list<R: tauri::Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
) -> Result<ResearchList, String> {
    let store = store(&app, &window)?;
    blocking(move || Ok(store.list())).await
}

#[tauri::command]
pub async fn research_create<R: tauri::Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    input: CreateResearchInput,
) -> Result<Research, String> {
    let store = store(&app, &window)?;
    let question = question(&input.question)?;
    if !LANGUAGES.contains(&input.language.as_str()) {
        return Err("Unsupported language".into());
    }
    let model = input
        .model
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty());
    let project = input
        .project_path
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| registered_project(&app, p))
        .transpose()?;
    let agent = ready_agent(&app, &input.agent_id, model.as_deref()).await?;
    let now = now_ms();
    let research = Research {
        active_owner: None,
        workflow: Workflow::Legacy,
        target: None,
        schema_version: RESEARCH_SCHEMA_VERSION,
        research_id: store::next_id("research"),
        title: prompt::title(&question),
        agent_id: agent.id.clone(),
        agent_name: agent.name.clone(),
        adapter_type: agent.adapter_type.clone(),
        model,
        project_path: project.as_ref().map(|(path, _)| path.clone()),
        project_name: project.map(|(_, name)| name),
        language: input.language,
        prompt_version: prompt::PROMPT_VERSION,
        created_at_ms: now,
        updated_at_ms: now,
        turns: Vec::new(),
        resume_command: None,
        resume_agent_command: None,
    };
    {
        let (store, research) = (store.clone(), research.clone());
        blocking(move || store.create(&research)).await?;
    }
    start(
        &app,
        store,
        &research.research_id,
        agent,
        move |research, job| {
            research.turns.push(running_turn(1, question, job));
            Ok(1)
        },
    )
    .await
}

#[tauri::command]
pub async fn research_get<R: tauri::Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    research_id: String,
) -> Result<Research, String> {
    let store = store(&app, &window)?;
    blocking(move || store.load(&research_id)).await
}

/// Report of a finished turn with its citations re-checked against the current files.
#[tauri::command]
pub async fn research_read_turn<R: tauri::Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    research_id: String,
    turn: u32,
) -> Result<TurnContent, String> {
    let store = store(&app, &window)?;
    blocking(move || {
        let research = store.require_legacy(&research_id)?;
        let markdown = store
            .read_turn(&research_id, turn)?
            .ok_or("This question has no report yet")?;
        // A project that left Loom's list is no longer read; its citations stay unverified.
        let refs = match &research.project_path {
            Some(path) => match registered_project(&app, path) {
                Ok((path, _)) => source_files::page_refs(&markdown, Path::new(&path)),
                Err(_) => Vec::new(),
            },
            None => source_files::page_refs(&markdown, &store.workspace_path(&research_id)?),
        };
        Ok(TurnContent {
            index: turn,
            markdown,
            refs,
        })
    })
    .await
}

/// Asks a follow-up question. One question runs at a time per research.
#[tauri::command]
pub async fn research_ask<R: tauri::Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    research_id: String,
    question: String,
) -> Result<Research, String> {
    let store = store(&app, &window)?;
    let question = self::question(&question)?;
    let research = {
        let (store, id) = (store.clone(), research_id.clone());
        blocking(move || store.load(&id)).await?
    };
    if research.workflow != Workflow::Legacy {
        return Err("workflow_mismatch".into());
    }
    let agent = ready_for(&app, &research).await?;
    start(&app, store, &research_id, agent, move |research, job| {
        if research.running_turn().is_some() {
            return Err("This research is already answering a question".into());
        }
        let index = research.turns.len() as u32 + 1;
        research.turns.push(running_turn(index, question, job));
        Ok(index)
    })
    .await
}

/// Re-runs the last question when it failed, was stopped or was interrupted.
#[tauri::command]
pub async fn research_retry<R: tauri::Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    research_id: String,
) -> Result<Research, String> {
    let store = store(&app, &window)?;
    let research = {
        let (store, id) = (store.clone(), research_id.clone());
        blocking(move || store.load(&id)).await?
    };
    if research.workflow != Workflow::Legacy {
        return Err("workflow_mismatch".into());
    }
    let agent = ready_for(&app, &research).await?;
    start(&app, store, &research_id, agent, move |research, job| {
        let last = research.turns.last_mut().ok_or("Nothing to retry")?;
        if matches!(last.status, TurnStatus::Running | TurnStatus::Done) {
            return Err("Only a failed or stopped question can be retried".into());
        }
        *last = running_turn(last.index, last.question.clone(), job);
        Ok(last.index)
    })
    .await
}

#[tauri::command]
pub async fn research_cancel<R: tauri::Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    research_id: String,
) -> Result<bool, String> {
    let store = store(&app, &window)?;
    blocking(move || store.cancel_legacy(&research_id)).await
}

#[tauri::command]
pub async fn research_read_job_logs<R: tauri::Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    research_id: String,
    job_id: String,
    stream: String,
    offset: Option<u64>,
) -> Result<JobLogChunk, String> {
    let store = store(&app, &window)?;
    blocking(move || store.read_log(&research_id, &job_id, &stream, offset.unwrap_or(0), None))
        .await
}

/// Root a research's citations resolve against: its project while it is still
/// registered, else its workspace.
fn source_root<R: tauri::Runtime>(
    app: &AppHandle<R>,
    store: &Store,
    research_id: &str,
) -> Result<std::path::PathBuf, String> {
    match store.require_legacy(research_id)?.project_path {
        Some(path) => Ok(registered_project(app, &path)?.0.into()),
        None => store.workspace_path(research_id),
    }
}

#[tauri::command]
pub async fn research_list_tree<R: tauri::Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    research_id: String,
    dir: Option<String>,
) -> Result<Vec<TreeEntry>, String> {
    let store = store(&app, &window)?;
    blocking(move || {
        source_files::list_tree(
            &source_root(&app, &store, &research_id)?,
            dir.as_deref().unwrap_or(""),
        )
    })
    .await
}

#[tauri::command]
pub async fn research_read_source<R: tauri::Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    research_id: String,
    path: String,
    start_line: Option<u32>,
    end_line: Option<u32>,
) -> Result<SourceText, String> {
    let store = store(&app, &window)?;
    blocking(move || {
        source_files::read_source(
            &source_root(&app, &store, &research_id)?,
            &path,
            start_line,
            end_line,
        )
    })
    .await
}

#[tauri::command]
pub async fn research_delete<R: tauri::Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    research_id: String,
) -> Result<(), String> {
    let store = store(&app, &window)?;
    let id = research_id.clone();
    blocking(move || store.delete(&id)).await?;
    let _ = app.emit(
        RESEARCH_EVENT,
        ResearchEvent {
            research_id,
            job_id: None,
            turn: None,
            status: "deleted".into(),
            error: None,
        },
    );
    Ok(())
}
