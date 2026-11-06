//! Target IPC never delegates to legacy jobs. All entrypoints require main window.
use super::{
    commands::{blocking, store},
    target_models::*,
    target_runtime::Runtime,
    target_service::{self, Preflight},
    target_store::TargetStore,
};
use serde::Serialize;
#[cfg(test)]
use tauri::Manager;
use tauri::{AppHandle, Emitter, WebviewWindow};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetCapabilities {
    pub single_shot: bool,
    pub iteration: bool,
    pub experiments: bool,
}

/// Reads the single-shot switch. Iteration and experiments stay off until R6/E2.
pub fn target_capabilities() -> TargetCapabilities {
    TargetCapabilities {
        single_shot: cfg!(target_os = "macos")
            && std::env::var("LOOM_RESEARCH_TARGETS").as_deref() == Ok("1"),
        iteration: false,
        experiments: false,
    }
}

fn enabled<R: tauri::Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    #[cfg(test)]
    if app.try_state::<super::ipc_tests::Context>().is_some() {
        return Ok(());
    }
    let _ = app;
    if !cfg!(target_os = "macos") {
        return Err("unsupported_target_platform".into());
    }
    if std::env::var("LOOM_RESEARCH_TARGETS").as_deref() == Ok("1") {
        Ok(())
    } else {
        Err("research_targets_not_enabled".into())
    }
}
#[tauri::command]
pub async fn research_target_capabilities<R: tauri::Runtime>(
    window: WebviewWindow<R>,
) -> Result<TargetCapabilities, String> {
    if window.label() != "main" {
        return Err("Research access requires the main window".into());
    }
    Ok(target_capabilities())
}

#[tauri::command]
pub async fn research_preflight<R: tauri::Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    spec: TargetInput,
    research_id: Option<String>,
) -> Result<Preflight, String> {
    let store = store(&app, &window)?;
    enabled(&app)?;
    let _operation = crate::agent::begin_operation()?;
    if let Some(id) = &research_id {
        TargetStore::open(store.clone(), id)?;
    }
    let agent = ready_target_agent(&app, &spec.agent_id, spec.model.as_deref()).await?;
    crate::agent::verify_evidence_profile(&agent).await?;
    if !matches!(spec.target, Target::Github { .. }) {
        verify_tools(&store, &spec).await?;
    }
    blocking(move || target_service::preflight(&store, spec, research_id, &agent)).await
}
#[tauri::command]
pub async fn research_start_run<R: tauri::Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    preflight_id: String,
    request_id: String,
) -> Result<RunView, String> {
    let store = store(&app, &window)?;
    if let Some(view) = target_service::replay_start(&store, &preflight_id, &request_id)? {
        return Ok(view);
    }
    enabled(&app)?;
    let _operation = crate::agent::begin_operation()?;
    let preflight = target_service::read_preflight(&store, &preflight_id)?;
    let agent = ready_target_agent(
        &app,
        &preflight.input.agent_id,
        preflight.input.model.as_deref(),
    )
    .await?;

    let configured = agent.clone();
    let admitted =
        blocking(move || target_service::admit(store, &preflight_id, &request_id, &configured))
            .await?;
    let view = admitted.view;
    if let Some(writer) = admitted.writer {
        let runtime = Runtime::new(admitted.target, writer, view.spec.clone(), agent, None)?;
        launch(runtime, &app);
    }
    Ok(view)
}
#[tauri::command]
pub async fn research_get_run<R: tauri::Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    research_id: String,
    run_id: String,
) -> Result<RunView, String> {
    let store = store(&app, &window)?;
    blocking(move || TargetStore::open(store, &research_id)?.run(&run_id)).await
}
#[tauri::command]
pub async fn research_list_runs<R: tauri::Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    research_id: String,
) -> Result<Vec<RunView>, String> {
    let store = store(&app, &window)?;
    blocking(move || TargetStore::open(store, &research_id)?.list_runs()).await
}
#[tauri::command]
pub async fn research_cancel_run<R: tauri::Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    research_id: String,
    run_id: String,
) -> Result<bool, String> {
    let store = store(&app, &window)?;
    blocking(move || TargetStore::open(store, &research_id)?.cancel(&run_id)).await
}
#[tauri::command]
pub async fn research_read_report<R: tauri::Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    research_id: String,
    report_id: String,
) -> Result<ReportContent, String> {
    let store = store(&app, &window)?;
    blocking(move || TargetStore::open(store, &research_id)?.report(&report_id)).await
}
#[tauri::command]
pub async fn research_list_report_tree<R: tauri::Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    research_id: String,
    report_id: String,
    offset: usize,
    limit: usize,
) -> Result<Material, String> {
    let store = store(&app, &window)?;
    blocking(move || {
        let target = TargetStore::open(store, &research_id)?;
        let report = target.report(&report_id)?.report;
        let snapshot_id = report
            .default_source_snapshot_id
            .ok_or("report_has_no_source_snapshot")?;
        target.material(
            &report.run_id,
            &MaterialRequest::Tree {
                snapshot_id,
                offset,
                limit,
            },
        )
    })
    .await
}
#[tauri::command]
pub async fn research_read_report_source<R: tauri::Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    research_id: String,
    report_id: String,
    path: String,
    start_line: u32,
    end_line: u32,
) -> Result<Material, String> {
    let store = store(&app, &window)?;
    blocking(move || {
        TargetStore::open(store, &research_id)?
            .report_source(&report_id, &path, start_line, end_line)
    })
    .await
}
#[tauri::command]
pub async fn research_list_evidence<R: tauri::Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    research_id: String,
    report_id: String,
) -> Result<Vec<EvidenceRecord>, String> {
    let store = store(&app, &window)?;
    blocking(move || {
        let target = TargetStore::open(store, &research_id)?;
        target
            .report(&report_id)?
            .report
            .evidence_ids
            .iter()
            .map(|id| target.evidence(id).map(|(record, _)| record))
            .collect()
    })
    .await
}
#[tauri::command]
pub async fn research_read_evidence<R: tauri::Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    research_id: String,
    report_id: String,
    evidence_id: String,
    offset: usize,
    limit: usize,
) -> Result<Material, String> {
    let store = store(&app, &window)?;
    blocking(move || {
        let target = TargetStore::open(store, &research_id)?;
        let report = target.report(&report_id)?.report;
        if !report.evidence_ids.contains(&evidence_id) {
            return Err("report_evidence_scope_mismatch".into());
        }
        target.material(
            &report.run_id,
            &MaterialRequest::Evidence {
                evidence_id,
                offset,
                limit,
            },
        )
    })
    .await
}

async fn continue_action<R: tauri::Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    research_id: String,
    action: target_service::RunAction,
    request_id: String,
) -> Result<RunView, String> {
    let store = store(&app, &window)?;
    if let Some(view) = target_service::replay_action(&store, &research_id, &action, &request_id)? {
        return Ok(view);
    }
    enabled(&app)?;
    let _operation = crate::agent::begin_operation()?;
    let target = TargetStore::open(store.clone(), &research_id)?;
    let run_id = match &action {
        target_service::RunAction::Followup {
            parent_report_id, ..
        } => target.report(parent_report_id)?.report.run_id,
        target_service::RunAction::Retry { run_id }
        | target_service::RunAction::Resume { run_id } => run_id.clone(),
    };
    let spec = target.run(&run_id)?.spec;
    let agent = ready_target_agent(&app, &spec.input.agent_id, spec.input.model.as_deref()).await?;

    let configured = agent.clone();
    let admitted = blocking(move || {
        target_service::continue_run(store, &research_id, action, &request_id, &configured)
    })
    .await?;
    let view = admitted.view;
    if let Some(writer) = admitted.writer {
        let runtime = Runtime::new(admitted.target, writer, view.spec.clone(), agent, None)?;
        launch(runtime, &app);
    }
    Ok(view)
}
#[tauri::command]
pub async fn research_ask_run<R: tauri::Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    research_id: String,
    parent_report_id: String,
    question: String,
    request_id: String,
) -> Result<RunView, String> {
    continue_action(
        app,
        window,
        research_id,
        target_service::RunAction::Followup {
            parent_report_id,
            question,
        },
        request_id,
    )
    .await
}
#[tauri::command]
pub async fn research_retry_run<R: tauri::Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    research_id: String,
    run_id: String,
    request_id: String,
) -> Result<RunView, String> {
    continue_action(
        app,
        window,
        research_id,
        target_service::RunAction::Retry { run_id },
        request_id,
    )
    .await
}
#[tauri::command]
pub async fn research_resume_run<R: tauri::Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    research_id: String,
    run_id: String,
    request_id: String,
) -> Result<RunView, String> {
    continue_action(
        app,
        window,
        research_id,
        target_service::RunAction::Resume { run_id },
        request_id,
    )
    .await
}

async fn verify_tools(store: &super::store::Store, input: &TargetInput) -> Result<(), String> {
    let tools = input.tools.as_ref().ok_or("rea_runtime_required")?;
    let cwd = super::files::path(&store.root, ".tool-checks")?;
    super::files::directory(&cwd)?;
    let id = super::store::next_id("tool-check");
    tools
        .verify_runtime(
            &cwd,
            &id,
            crate::agent::ProcessOwner::Feature {
                feature: "research".into(),
                scope_id: "preflight".into(),
                run_id: id.clone(),
            },
        )
        .await?;
    Ok(())
}

fn launch<R: tauri::Runtime>(runtime: Runtime, app: &AppHandle<R>) {
    let app = app.clone();
    let runtime = runtime.with_emitter(std::sync::Arc::new(move |event| {
        let _ = app.emit(super::RESEARCH_EVENT, event);
    }));
    tauri::async_runtime::spawn(async move {
        if let Err(error) = runtime.execute().await {
            eprintln!(
                "Research target persistence failed: {}",
                crate::agent::redact_sensitive_text(&error)
            );
        }
    });
}

#[tauri::command]
pub async fn research_export<R: tauri::Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    research_id: String,
    input: ExportInput,
) -> Result<ExportResult, String> {
    let store = store(&app, &window)?;
    blocking(move || {
        TargetStore::open(store, &research_id)?
            .writer(None)?
            .export_report(&input)
    })
    .await
}

async fn ready_target_agent<R: tauri::Runtime>(
    app: &AppHandle<R>,
    agent_id: &str,
    model: Option<&str>,
) -> Result<crate::models::AgentConfig, String> {
    if let Some(model) = model {
        crate::agent::validate_model_name(model)?;
    }
    #[cfg(test)]
    if let Some(context) = app.try_state::<super::ipc_tests::Context>() {
        return context
            .agents
            .iter()
            .find(|a| a.id == agent_id && a.enabled && a.available)
            .cloned()
            .ok_or_else(|| "target_agent_unavailable".into());
    }
    crate::agent::load_agents(app)?
        .into_iter()
        .find(|a| a.id == agent_id && a.enabled && a.available)
        .ok_or_else(|| "target_agent_unavailable".into())
}

#[cfg(test)]
mod tests {
    use super::target_capabilities;

    #[test]
    fn capability_response_keeps_iteration_and_experiments_off() {
        let caps = target_capabilities();
        assert!(!caps.iteration && !caps.experiments);
        if !cfg!(target_os = "macos")
            || std::env::var("LOOM_RESEARCH_TARGETS").as_deref() != Ok("1")
        {
            assert!(!caps.single_shot);
        }
    }
}
