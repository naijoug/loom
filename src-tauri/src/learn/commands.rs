//! Tauri commands for Learn. File IO runs on blocking threads; jobs run on the
//! async runtime and report progress through `loom://learn-event`.
use super::{
    discover::{self, Discover},
    fetch, github,
    jobs::{self, Emit, Job},
    models::*,
    store::{self, Store},
    LEARN_EVENT,
};
use crate::{
    agent,
    models::{now_ms, AgentConfig},
    source_files as source,
};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

const LANGUAGES: &[&str] = &["en", "zh-CN"];
const DEPTHS: &[&str] = &["quick", "standard", "deep"];

fn store(app: &AppHandle, window: &WebviewWindow) -> Result<Store, String> {
    if window.label() != "main" {
        return Err("Learn access requires the main window".into());
    }
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|_| "Application data directory unavailable".to_string())?;
    Ok(Store::new(&app_data))
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|_| "Learn worker interrupted".to_string())?
}

fn emitter(app: &AppHandle) -> Emit {
    let app = app.clone();
    Arc::new(move |event| {
        let _ = app.emit(LEARN_EVENT, event);
    })
}

/// Learn runs only built-in adapters that are enabled and found on PATH.
fn resolve_agent(
    app: &AppHandle,
    agent_id: &str,
    model: Option<&str>,
) -> Result<AgentConfig, String> {
    let agent = crate::agent::load_agents(app)?
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
            "{} is not supported by Learn; choose a built-in agent",
            agent.name
        ));
    }
    if let Some(model) = model {
        agent::validate_model_name(model)?;
    }
    Ok(agent)
}

/// Resolves the agent and checks that its CLI actually starts, so a broken install
/// is reported before any clone or job begins.
async fn ready_agent(
    app: &AppHandle,
    agent_id: &str,
    model: Option<&str>,
) -> Result<AgentConfig, String> {
    let agent = resolve_agent(app, agent_id, model)?;
    crate::agent::check_startable(&agent).await?;
    Ok(agent)
}

/// `agent` is the already checked agent when the caller has one; otherwise it is
/// resolved and checked here if any planned stage needs it.
async fn start(
    app: &AppHandle,
    store: Store,
    study: &Study,
    stage: &str,
    agent: Option<AgentConfig>,
) -> Result<String, String> {
    let (stages, auto_level1) = jobs::plan(study, stage)?;
    let needs_agent = stages
        .iter()
        .any(|s| !matches!(s.as_str(), "fetch" | "inventory"));
    let agent = match agent {
        Some(agent) => Some(agent),
        None if needs_agent => {
            Some(ready_agent(app, &study.agent_id, study.model.as_deref()).await?)
        }
        None => None,
    };
    let job_id = store::next_id("job");
    let cancel = store::claim_job(&study.study_id, &job_id)?;
    let job = Job {
        store,
        study_id: study.study_id.clone(),
        job_id: job_id.clone(),
        cancel,
        agent,
        emit: emitter(app),
    };
    tauri::async_runtime::spawn(job.run(stages, auto_level1));
    Ok(job_id)
}

#[tauri::command]
pub async fn learn_list_studies(
    app: AppHandle,
    window: WebviewWindow,
) -> Result<StudyList, String> {
    let store = store(&app, &window)?;
    blocking(move || Ok(store.list())).await
}

#[tauri::command]
pub async fn learn_create_study(
    app: AppHandle,
    window: WebviewWindow,
    input: CreateStudyInput,
) -> Result<StudyDetail, String> {
    let store = store(&app, &window)?;
    let repo = fetch::parse_github_url(&input.url)?;
    let git_ref = input
        .git_ref
        .map(|r| r.trim().to_string())
        .filter(|r| !r.is_empty());
    if let Some(git_ref) = &git_ref {
        fetch::validate_ref(git_ref)?;
    }
    let model = input
        .model
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty());
    if !LANGUAGES.contains(&input.language.as_str()) {
        return Err("Unsupported language".into());
    }
    if !DEPTHS.contains(&input.depth.as_str()) {
        return Err("Unsupported depth".into());
    }
    let agent = ready_agent(&app, &input.agent_id, model.as_deref()).await?;
    let now = now_ms();
    let study = Study {
        schema_version: STUDY_SCHEMA_VERSION,
        study_id: store::next_id("study"),
        url: repo.url.clone(),
        owner: repo.owner.clone(),
        repo: repo.repo.clone(),
        git_ref,
        commit: None,
        agent_id: agent.id.clone(),
        agent_name: agent.name.clone(),
        adapter_type: agent.adapter_type.clone(),
        model,
        language: input.language,
        depth: input.depth,
        prompt_version: super::generate::PROMPT_VERSION,
        created_at_ms: now,
        updated_at_ms: now,
        stages: ["fetch", "inventory", "architecture"]
            .into_iter()
            .map(StageState::pending)
            .collect(),
        pages: Vec::new(),
        coverage: None,
        checkout_fingerprint: None,
    };
    let created = {
        let (store, study) = (store.clone(), study.clone());
        blocking(move || store.create(&study)).await
    };
    created?;
    start(&app, store, &study, "fetch", Some(agent)).await?;
    Ok(StudyDetail {
        study,
        architecture: None,
    })
}

#[tauri::command]
pub async fn learn_get_study(
    app: AppHandle,
    window: WebviewWindow,
    study_id: String,
) -> Result<StudyDetail, String> {
    let store = store(&app, &window)?;
    blocking(move || {
        Ok(StudyDetail {
            study: store.load(&study_id)?,
            architecture: store.architecture(&study_id)?,
        })
    })
    .await
}

#[tauri::command]
pub async fn learn_read_page(
    app: AppHandle,
    window: WebviewWindow,
    study_id: String,
    page_id: String,
) -> Result<PageContent, String> {
    let store = store(&app, &window)?;
    blocking(move || {
        let markdown = store
            .read_page(&study_id, &page_id)?
            .ok_or("This page has not been generated yet")?;
        let refs = super::generate::page_refs(&markdown, &store.checkout(&study_id)?);
        Ok(PageContent {
            page_id,
            markdown,
            refs,
        })
    })
    .await
}

/// Starts `fetch`, `inventory`, `architecture`, `module:<id>`, `glossary` or
/// `all` (every missing page). One job per study at a time.
#[tauri::command]
pub async fn learn_generate(
    app: AppHandle,
    window: WebviewWindow,
    study_id: String,
    stage: String,
) -> Result<String, String> {
    let store = store(&app, &window)?;
    let study = {
        let (store, study_id) = (store.clone(), study_id.clone());
        blocking(move || store.load(&study_id)).await?
    };
    start(&app, store, &study, &stage, None).await
}

#[tauri::command]
pub async fn learn_cancel_job(
    app: AppHandle,
    window: WebviewWindow,
    study_id: String,
) -> Result<bool, String> {
    store(&app, &window)?;
    Ok(store::running_job(&study_id)
        .map(|job| job.cancel.store(true, std::sync::atomic::Ordering::Relaxed))
        .is_some())
}

#[tauri::command]
pub async fn learn_read_job_logs(
    app: AppHandle,
    window: WebviewWindow,
    study_id: String,
    job_id: String,
    stream: String,
    offset: Option<u64>,
) -> Result<JobLogChunk, String> {
    let store = store(&app, &window)?;
    blocking(move || store.read_log(&study_id, &job_id, &stream, offset.unwrap_or(0), None)).await
}

#[tauri::command]
pub async fn learn_list_tree(
    app: AppHandle,
    window: WebviewWindow,
    study_id: String,
    dir: Option<String>,
) -> Result<Vec<TreeEntry>, String> {
    let store = store(&app, &window)?;
    blocking(move || source::list_tree(&store.checkout(&study_id)?, dir.as_deref().unwrap_or("")))
        .await
}

#[tauri::command]
pub async fn learn_read_source(
    app: AppHandle,
    window: WebviewWindow,
    study_id: String,
    path: String,
    start_line: Option<u32>,
    end_line: Option<u32>,
) -> Result<SourceText, String> {
    let store = store(&app, &window)?;
    blocking(move || source::read_source(&store.checkout(&study_id)?, &path, start_line, end_line))
        .await
}

#[tauri::command]
pub async fn learn_delete_study(
    app: AppHandle,
    window: WebviewWindow,
    study_id: String,
) -> Result<(), String> {
    let store = store(&app, &window)?;
    let id = study_id.clone();
    blocking(move || store.delete(&id)).await?;
    let _ = app.emit(
        LEARN_EVENT,
        LearnEvent {
            study_id,
            job_id: None,
            stage: "study".into(),
            status: "deleted".into(),
            error: None,
        },
    );
    Ok(())
}

/// Rising GitHub repositories: created within the period, sorted by stars.
#[tauri::command]
pub async fn learn_github_trending(
    app: AppHandle,
    window: WebviewWindow,
    period: String,
    language: Option<String>,
    refresh: Option<bool>,
) -> Result<TrendingResult, String> {
    store(&app, &window)?;
    if !github::PERIODS.contains(&period.as_str()) {
        return Err("Unsupported period".into());
    }
    let language = language
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty());
    let (query, fetched_at_ms, cached, repos) =
        github::trending(&period, language.as_deref(), refresh.unwrap_or(false)).await?;
    Ok(TrendingResult {
        repos,
        query,
        fetched_at_ms,
        cached,
    })
}

/// One discover-chat turn: a tool-less agent call, then GitHub lookups.
/// Built-in CLIs and direct-API agents are accepted.
#[tauri::command]
pub async fn learn_discover_send(
    app: AppHandle,
    window: WebviewWindow,
    input: DiscoverInput,
) -> Result<DiscoverReply, String> {
    store(&app, &window)?;
    discover::validate(&input)?;
    let model = input
        .model
        .as_deref()
        .map(str::trim)
        .filter(|m| !m.is_empty())
        .map(str::to_string);
    let agent = ready_agent(&app, &input.agent_id, model.as_deref()).await?;
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|_| "Application data directory unavailable".to_string())?;
    Discover::new(&app_data)
        .send(&agent, &input, model.as_deref())
        .await
}

#[tauri::command]
pub async fn learn_discover_cancel(
    app: AppHandle,
    window: WebviewWindow,
    run_id: String,
) -> Result<bool, String> {
    store(&app, &window)?;
    Ok(discover::cancel(&run_id))
}
