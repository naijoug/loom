use crate::{
    agent_adapter::{
        self, AdapterInvocationRequest, PrepareAgentInvocationInput, PreparedAgentInvocation,
    },
    execution_policy::{self, ExecutionDecision, ExecutionRequest},
    migrations::{self, AGENT_STORE_SCHEMA_VERSION},
    models::{
        now_ms, AgentConfig, AgentConfigInput, AgentInvocation, IdGenerator, PlanReview,
        PlanningAgentLogEvent, PlanningAgentStatusEvent, PlanningDecision, PlanningDiscussionInput,
        PlanningRun, TaskEvent,
    },
    plan_html,
    process_supervisor::{self, ProcessKind, ProcessMetadata},
    session_capture::{
        find_session_id, resume_command_for_adapter, ADAPTER_CLAUDE_CODE, ADAPTER_CODEX, ADAPTER_GROK,
    },
    storage, tasks,
};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Mutex, OnceLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter, Runtime, State};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader},
    process::Command as TokioCommand,
    time::{interval, sleep},
};

mod artifacts;
mod config;
mod orchestrator;
mod prompts;
mod stream;

use artifacts::*;
use config::*;
use orchestrator::*;
use prompts::*;
use stream::*;

pub(crate) fn load_agents(app: &AppHandle) -> Result<Vec<AgentConfig>, String> {
    config::load_agents_inner(app)
}

pub(crate) fn redact_sensitive_text(input: &str) -> String {
    config::redact_sensitive_text_inner(input)
}

pub(crate) fn normalize_agent_stdout(output_mode: &str, stdout: &str) -> String {
    stream::normalize_agent_stdout_inner(output_mode, stdout)
}

const AGENTS_FILE: &str = "agents.json";
const ADAPTER_CLI: &str = "cli";
// Adapter types that used to ship as built-ins but are retired now (Amp needs
// paid credits for non-interactive use). Stored configs are dropped on load.
const RETIRED_ADAPTER_AMP: &str = "amp_cli";
const RETIRED_AGENT_AMP_ID: &str = "agent-amp";
const ADAPTER_DUMMY: &str = "dummy";

fn stopped_agent_tasks() -> &'static Mutex<HashSet<String>> {
    static TASKS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    TASKS.get_or_init(|| Mutex::new(HashSet::new()))
}

fn clear_task_stop(task_id: &str) -> Result<(), String> {
    stopped_agent_tasks()
        .lock()
        .map_err(|_| "agent task stop registry is unavailable".to_string())?
        .remove(task_id);
    Ok(())
}

fn task_stop_requested(task_id: &str) -> bool {
    stopped_agent_tasks()
        .lock()
        .map(|tasks| tasks.contains(task_id))
        .unwrap_or(true)
}

pub(crate) fn stop_task_runs(task_id: &str) -> Result<usize, String> {
    stopped_agent_tasks()
        .lock()
        .map_err(|_| "agent task stop registry is unavailable".to_string())?
        .insert(task_id.to_string());
    process_supervisor::supervisor()
        .stop_task_runs(task_id, &[ProcessKind::Agent], "task_lifecycle_changed")
        .map(|runs| runs.len())
}
// Real planning agents (claude/codex) routinely take 1-2 minutes on a real
// repository; a single observed run took ~72s. Keep a generous per-agent budget
// so genuine work is not killed mid-plan. Agents run sequentially, so total wall
// time is roughly this times the number of selected agents.
const PLANNING_TIMEOUT_MS: u64 = 240_000;
const SYNTHESIS_PROMPT_SUMMARY: &str = "Synthesize final plan";
// One automatic retry per agent phase keeps transient failures (timeouts,
// flaky exits) from sinking a whole planning round without letting a broken
// setup burn time in a loop.
const MAX_PLANNING_ATTEMPTS: u32 = 2;

const FAILURE_TIMEOUT: &str = "timeout";
const FAILURE_EMPTY_OUTPUT: &str = "empty_output";
const FAILURE_NONZERO_EXIT: &str = "nonzero_exit";
const FAILURE_NOT_RETRYABLE: &str = "not_retryable";

// Configuration-level errors that retrying cannot fix: the user has to change
// credentials, install the CLI, or pay for credits first.
const NOT_RETRYABLE_PATTERNS: &[&str] = &[
    "paid credits",
    "command not found",
    "no such file or directory",
    "not logged in",
    "login required",
    "please run /login",
    "authentication",
    "unauthorized",
    "invalid api key",
    "permission denied",
    "dedicated executable",
];

#[derive(Clone)]
struct PlanningPrompt {
    content: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CliOutputMode {
    Plain,
    ClaudeStreamJson,
    CodexJson,
}

struct CliProfile {
    adapter_type: String,
    command: String,
    args: Vec<String>,
    stdin_prompt: bool,
    output_mode: CliOutputMode,
}

struct PlanningInvocationResult {
    status: String,
    stdout: String,
    stderr: String,
    output_summary: String,
    evidence_ref: Option<String>,
    plan_path: Option<String>,
    exit_code: Option<i32>,
    timed_out: bool,
    attempt: u32,
    failure_kind: Option<String>,
    failure_detail: Option<String>,
    error_lines: Vec<String>,
    stderr_ref: Option<String>,
    session_id: Option<String>,
    resume_command: Option<String>,
    started_at_ms: u128,
    ended_at_ms: u128,
}

struct PlanReviewInvocationResult {
    status: String,
    raw_output: String,
    finding: String,
    severity: String,
    evidence_ref: Option<String>,
    stderr_ref: Option<String>,
    session_id: Option<String>,
    resume_command: Option<String>,
    started_at_ms: u128,
    ended_at_ms: u128,
}

#[derive(Clone)]
struct PlanningLogContext {
    task_id: String,
    planning_run_id: String,
    agent_id: String,
    agent_name: String,
    phase: String,
    attempt: u32,
}

trait PlanningEventEmitter: Clone + Send + Sync + 'static {
    fn emit_planning_agent_status(&self, event: PlanningAgentStatusEvent);
    fn emit_planning_agent_log(&self, event: PlanningAgentLogEvent);
}

impl<R: Runtime> PlanningEventEmitter for AppHandle<R> {
    fn emit_planning_agent_status(&self, event: PlanningAgentStatusEvent) {
        let _ = self.emit("loom://planning-agent-status", event);
    }

    fn emit_planning_agent_log(&self, event: PlanningAgentLogEvent) {
        let _ = self.emit("loom://planning-agent-log", event);
    }
}

#[tauri::command]
pub fn list_agents(app: AppHandle) -> Result<Vec<AgentConfig>, String> {
    load_agents(&app)
}

#[tauri::command]
pub fn prepare_agent_invocation(
    app: AppHandle,
    input: PrepareAgentInvocationInput,
) -> Result<PreparedAgentInvocation, String> {
    let task = tasks::load_task(Path::new(&input.project_path), &input.task_id)?;
    tasks::ensure_task_active(&task)?;
    let agent = load_agents(&app)?
        .into_iter()
        .find(|agent| agent.id == input.agent_id)
        .ok_or_else(|| format!("agent '{}' was not found", input.agent_id))?;
    let prepared = agent_adapter::prepare_invocation(
        &agent,
        &AdapterInvocationRequest {
            project_path: Path::new(&input.project_path),
            prompt: &input.prompt,
            prompt_file: None,
            stage: input.stage,
            resume_command: input.resume_command.as_deref(),
            embed_prompt: true,
        },
    )?;
    if prepared.stdin_prompt {
        return Err(format!(
            "agent '{}' requires stdin prompt transport, which is unavailable for interactive task runs",
            agent.name
        ));
    }
    Ok(prepared)
}

pub(crate) fn validate_implementation_agent(
    app: &AppHandle,
    agent_id: &str,
    program: &str,
) -> Result<AgentConfig, String> {
    let agent = load_agents(app)?
        .into_iter()
        .find(|agent| agent.id == agent_id)
        .ok_or_else(|| format!("implementation agent '{agent_id}' was not found"))?;
    validate_implementation_agent_config(&agent, program)?;
    Ok(agent)
}

fn validate_implementation_agent_config(agent: &AgentConfig, program: &str) -> Result<(), String> {
    if !agent.enabled {
        return Err(format!("implementation agent '{}' is disabled", agent.name));
    }
    if !agent.available {
        return Err(format!(
            "implementation agent '{}' command is unavailable",
            agent.name
        ));
    }
    if !agent.can_write_files {
        return Err(format!(
            "implementation agent '{}' is not allowed to write files",
            agent.name
        ));
    }
    if !agent.can_run_commands {
        return Err(format!(
            "implementation agent '{}' is not allowed to run commands",
            agent.name
        ));
    }
    if !agent
        .capabilities
        .iter()
        .any(|capability| capability == "implementation" || capability == "debugging")
    {
        return Err(format!(
            "agent '{}' has no implementation or debugging capability",
            agent.name
        ));
    }
    if agent.command.trim() != program.trim() {
        return Err(format!(
            "command '{}' does not match configured implementation agent '{}'",
            program, agent.name
        ));
    }
    Ok(())
}

#[tauri::command]
pub fn create_agent(
    app: AppHandle,
    ids: State<'_, IdGenerator>,
    input: AgentConfigInput,
) -> Result<AgentConfig, String> {
    let mut agents = load_agents(&app)?;
    let mut agent = AgentConfig {
        id: ids.next("agent"),
        name: input.name,
        command: input.command,
        args: input.args,
        working_directory_policy: input.working_directory_policy,
        capabilities: input.capabilities,
        adapter_type: input.adapter_type,
        can_write_files: input.can_write_files,
        can_run_commands: input.can_run_commands,
        enabled: input.enabled,
        available: false,
    };
    agent.available = command_available(&agent);
    agents.push(agent.clone());
    save_agents(&app, &agents)?;

    Ok(agent)
}

#[tauri::command]
pub fn update_agent(
    app: AppHandle,
    agent_id: String,
    input: AgentConfigInput,
) -> Result<Vec<AgentConfig>, String> {
    let mut agents = load_agents(&app)?;
    let mut updated = AgentConfig {
        id: agent_id.clone(),
        name: input.name,
        command: input.command,
        args: input.args,
        working_directory_policy: input.working_directory_policy,
        capabilities: input.capabilities,
        adapter_type: input.adapter_type,
        can_write_files: input.can_write_files,
        can_run_commands: input.can_run_commands,
        enabled: input.enabled,
        available: false,
    };
    updated.available = command_available(&updated);
    apply_agent_update(&mut agents, &agent_id, updated)?;
    save_agents(&app, &agents)?;

    Ok(agents)
}

#[tauri::command]
pub fn delete_agent(app: AppHandle, agent_id: String) -> Result<Vec<AgentConfig>, String> {
    let mut agents = load_agents(&app)?;
    delete_agent_config(&mut agents, &agent_id)?;
    save_agents(&app, &agents)?;

    Ok(agents)
}

#[tauri::command]
pub fn set_agent_enabled(
    app: AppHandle,
    agent_id: String,
    enabled: bool,
) -> Result<Vec<AgentConfig>, String> {
    let mut agents = load_agents(&app)?;

    if let Some(agent) = agents.iter_mut().find(|agent| agent.id == agent_id) {
        agent.enabled = enabled;
    }

    save_agents(&app, &agents)?;
    Ok(agents)
}

fn apply_agent_update(
    agents: &mut [AgentConfig],
    agent_id: &str,
    updated: AgentConfig,
) -> Result<(), String> {
    if is_default_agent_id(agent_id) {
        return Err("built-in Agent profiles can only be enabled or disabled".to_string());
    }

    let Some(agent) = agents.iter_mut().find(|agent| agent.id == agent_id) else {
        return Err("agent not found".to_string());
    };

    *agent = updated;
    Ok(())
}

fn delete_agent_config(agents: &mut Vec<AgentConfig>, agent_id: &str) -> Result<(), String> {
    if is_default_agent_id(agent_id) {
        return Err("built-in Agent profiles can only be disabled, not deleted".to_string());
    }

    let initial_len = agents.len();
    agents.retain(|agent| agent.id != agent_id);

    if agents.len() == initial_len {
        return Err("agent not found".to_string());
    }

    Ok(())
}

#[tauri::command]
pub async fn run_planning_discussion(
    app: AppHandle,
    ids: State<'_, IdGenerator>,
    input: PlanningDiscussionInput,
) -> Result<crate::models::Task, String> {
    let agents = load_agents(&app)?;
    let mut task = tasks::load_task(Path::new(&input.project_path), &input.task_id)?;
    tasks::ensure_task_active(&task)?;
    clear_task_stop(&task.id)?;
    let selected_agents = resolve_planning_agents(&agents, &input.agent_ids);

    if selected_agents.is_empty() {
        return Err("no enabled planning agents are available".to_string());
    }

    let started_at_ms = now_ms();
    let planning_run_id = ids.next("planning");
    let requirement = if input.requirement.trim().is_empty() {
        task.raw_requirement.clone()
    } else {
        input.requirement.trim().to_string()
    };
    let prompt_summary = format!("Planning discussion for task '{}'", task.title);
    let prompt = render_planning_prompt(&task.title, &input.project_path, &requirement);

    let project_path = PathBuf::from(&input.project_path);
    let mut planning_handles = Vec::new();
    for agent in selected_agents.iter().cloned() {
        let app = app.clone();
        let project_path = project_path.clone();
        let task_id = task.id.clone();
        let planning_run_id = planning_run_id.clone();
        let task_title = task.title.clone();
        let prompt = prompt.clone();
        let spawned_agent = agent.clone();
        let handle = tauri::async_runtime::spawn(async move {
            run_planning_agent_with_status(
                app,
                spawned_agent,
                project_path,
                task_id,
                planning_run_id,
                task_title,
                prompt,
                "planning",
                1,
                true,
            )
            .await
        });
        planning_handles.push((agent, handle));
    }

    let mut invocations = Vec::new();
    for (agent, handle) in planning_handles {
        let result = handle.await.unwrap_or_else(|error| {
            failed_planning_result(
                &agent,
                format!("planning worker failed to join: {error}"),
                None,
            )
        });
        invocations.push(agent_invocation_from_result(
            ids.inner(),
            &task.id,
            &planning_run_id,
            &agent,
            prompt_summary.clone(),
            result,
        ));
    }

    let planning_successful_invocations = invocations
        .iter()
        .filter(|invocation| invocation.status == "succeeded")
        .count();

    let mut discussion_summary = summarize_discussion(&selected_agents, &requirement, &invocations);
    let outcome = review_and_synthesize(
        app.clone(),
        ids.inner(),
        &project_path,
        &task.title,
        &task.id,
        &planning_run_id,
        &requirement,
        &selected_agents,
        &invocations,
        &discussion_summary,
    )
    .await?;
    discussion_summary.push_str(&outcome.source_note);
    let run_reviews = outcome.reviews;
    if let Some(synthesis_invocation) = outcome.synthesis_invocation {
        invocations.push(synthesis_invocation);
    }
    // Keep the review records and human decisions visible in the written plan
    // document, matching what a later "re-run reviews" would produce.
    let final_plan = if run_reviews.is_empty() && task.planning_decisions.is_empty() {
        outcome.final_plan
    } else {
        render_reviewed_final_plan(&outcome.final_plan, &run_reviews, &task.planning_decisions)
    };

    // On the first planning session, name the task from the final plan output
    // so the user never has to title it up front.
    if task.planning_runs.is_empty() && planning_successful_invocations > 0 {
        if let Some(title) = derive_plan_title_from_markdown(&final_plan) {
            task.title = title;
        }
    }

    // Later rounds of the same task overwrite the first round's plan file so
    // docs/plans/ holds one document per task instead of one per round.
    let plan_path = match task.final_plan_path.as_deref() {
        Some(existing) => PathBuf::from(existing),
        None => next_project_plan_path(Path::new(&input.project_path), &task.title)?,
    };
    fs::create_dir_all(
        plan_path
            .parent()
            .ok_or_else(|| "invalid plan path".to_string())?,
    )
    .map_err(|error| format!("failed to create plans directory: {error}"))?;
    storage::atomic_write_text(&plan_path, &final_plan)
        .map_err(|error| format!("failed to write final plan: {error}"))?;
    update_project_plans_index(
        Path::new(&input.project_path),
        &plan_path,
        "通过多 Agent 讨论生成最终实施计划，等待人工确认后进入实施。",
    )?;

    let finished_at_ms = now_ms();
    task.status = crate::task_state::transition(
        crate::task_state::transition(task.status, crate::task_state::TaskAction::PlanningStarted)?,
        crate::task_state::TaskAction::PlanningCompleted,
    )?;
    task.raw_requirement = requirement.clone();
    task.final_plan = Some(final_plan);
    task.final_plan_path = Some(plan_path.display().to_string());
    task.discussion_summary = Some(discussion_summary.clone());
    task.selected_planning_agent_ids = selected_agents
        .iter()
        .map(|agent| agent.id.clone())
        .collect();
    task.planning_runs.push(PlanningRun {
        id: planning_run_id.clone(),
        task_id: task.id.clone(),
        requirement,
        selected_agent_ids: task.selected_planning_agent_ids.clone(),
        status: if planning_successful_invocations > 0 {
            "succeeded".to_string()
        } else {
            "failed".to_string()
        },
        summary: discussion_summary,
        started_at_ms,
        ended_at_ms: Some(finished_at_ms),
    });
    task.agent_invocations.extend(invocations);
    task.plan_reviews
        .retain(|review| review.planning_run_id != planning_run_id);
    task.plan_reviews.extend(run_reviews);
    task.plan_todos = Vec::new();
    task.updated_at_ms = finished_at_ms;
    task.final_plan_html_path = plan_html::write_task_plan_html(&task)?;
    task.events.push(TaskEvent {
        id: ids.next("event"),
        task_id: task.id.clone(),
        timestamp_ms: finished_at_ms,
        actor: "agent".to_string(),
        status: task.status,
        input_summary: Some(prompt_summary),
        output_summary: Some(if planning_successful_invocations > 0 {
            "Planning discussion generated a final plan for review.".to_string()
        } else {
            "Planning discussion failed for all selected real agents.".to_string()
        }),
        evidence_ref: task.final_plan_path.clone(),
    });
    persist_planning_task(task, Path::new(&input.project_path))
}

#[tauri::command]
pub async fn run_plan_reviews(
    app: AppHandle,
    ids: State<'_, IdGenerator>,
    project_path: String,
    task_id: String,
) -> Result<crate::models::Task, String> {
    let agents = load_agents(&app)?;
    let mut task = tasks::load_task(Path::new(&project_path), &task_id)?;
    tasks::ensure_task_active(&task)?;
    clear_task_stop(&task.id)?;
    let planning_run = task
        .planning_runs
        .last()
        .cloned()
        .ok_or_else(|| "cannot run plan reviews before a planning run exists".to_string())?;
    let selected_agents = resolve_planning_agents(&agents, &planning_run.selected_agent_ids);
    let candidates = successful_plan_invocations(&latest_drafting_invocations(
        &task.agent_invocations,
        &planning_run.id,
    ));

    if candidates.len() < 2 {
        return Err("plan review requires at least two successful agent proposals".to_string());
    }

    let reviews = run_cross_reviews(
        app.clone(),
        ids.inner(),
        Path::new(&project_path),
        &task.title,
        &task.id,
        &planning_run.id,
        &selected_agents,
        &candidates,
    )
    .await;

    task.plan_reviews
        .retain(|review| review.planning_run_id != planning_run.id);
    task.plan_reviews.extend(reviews);
    task.final_plan = task.final_plan.clone().map(|plan| {
        render_reviewed_final_plan(&plan, &task.plan_reviews, &task.planning_decisions)
    });
    if let (Some(path), Some(plan)) = (&task.final_plan_path, &task.final_plan) {
        storage::atomic_write_text(Path::new(path), plan)
            .map_err(|error| format!("failed to write reviewed plan: {error}"))?;
        task.final_plan_html_path = plan_html::write_task_plan_html(&task)?;
    }
    task.status = crate::task_state::transition(
        task.status,
        crate::task_state::TaskAction::PlanningCompleted,
    )?;
    task.updated_at_ms = now_ms();
    task.events.push(TaskEvent {
        id: ids.next("event"),
        task_id: task.id.clone(),
        timestamp_ms: task.updated_at_ms,
        actor: "agent".to_string(),
        status: task.status,
        input_summary: Some("Ran agent-to-agent plan reviews".to_string()),
        output_summary: Some(format!(
            "{} mutual review records captured.",
            task.plan_reviews
                .iter()
                .filter(|review| review.planning_run_id == planning_run.id)
                .count()
        )),
        evidence_ref: task.final_plan_path.clone(),
    });
    persist_planning_task(task, Path::new(&project_path))
}

/// Re-run a single failed drafting agent inside the latest planning run, then
/// replay cross-review and synthesis on the updated candidate set.
#[tauri::command]
pub async fn retry_planning_agent(
    app: AppHandle,
    ids: State<'_, IdGenerator>,
    project_path: String,
    task_id: String,
    agent_id: String,
) -> Result<crate::models::Task, String> {
    let agents = load_agents(&app)?;
    let mut task = tasks::load_task(Path::new(&project_path), &task_id)?;
    tasks::ensure_task_active(&task)?;
    clear_task_stop(&task.id)?;
    let planning_run = task
        .planning_runs
        .last()
        .cloned()
        .ok_or_else(|| "cannot retry before a planning run exists".to_string())?;
    if planning_run.ended_at_ms.is_none() {
        return Err("the latest planning run is still in progress".to_string());
    }

    let agent = resolve_planning_agents(&agents, std::slice::from_ref(&agent_id))
        .into_iter()
        .next()
        .ok_or_else(|| "agent is not available for planning".to_string())?;
    let previous = latest_drafting_invocations(&task.agent_invocations, &planning_run.id)
        .into_iter()
        .find(|invocation| invocation.agent_id == agent.id)
        .ok_or_else(|| "agent was not part of the latest planning run".to_string())?;
    if previous.status == "succeeded" {
        return Err("only failed agents can be retried".to_string());
    }

    let prompt = render_planning_prompt(&task.title, &project_path, &planning_run.requirement);
    let prompt_summary = format!("Planning discussion for task '{}'", task.title);
    let result = run_planning_agent_with_status(
        app.clone(),
        agent.clone(),
        PathBuf::from(&project_path),
        task.id.clone(),
        planning_run.id.clone(),
        task.title.clone(),
        prompt,
        "planning",
        previous.attempt + 1,
        false,
    )
    .await;
    task.agent_invocations.push(agent_invocation_from_result(
        ids.inner(),
        &task.id,
        &planning_run.id,
        &agent,
        prompt_summary.clone(),
        result,
    ));

    let selected_agents = resolve_planning_agents(&agents, &planning_run.selected_agent_ids);
    let drafting = latest_drafting_invocations(&task.agent_invocations, &planning_run.id);
    let mut discussion_summary =
        summarize_discussion(&selected_agents, &planning_run.requirement, &drafting);
    let outcome = review_and_synthesize(
        app.clone(),
        ids.inner(),
        Path::new(&project_path),
        &task.title,
        &task.id,
        &planning_run.id,
        &planning_run.requirement,
        &selected_agents,
        &drafting,
        &discussion_summary,
    )
    .await?;
    discussion_summary.push_str(&outcome.source_note);
    let run_reviews = outcome.reviews;
    if let Some(synthesis_invocation) = outcome.synthesis_invocation {
        task.agent_invocations.push(synthesis_invocation);
    }
    let final_plan = if run_reviews.is_empty() && task.planning_decisions.is_empty() {
        outcome.final_plan
    } else {
        render_reviewed_final_plan(&outcome.final_plan, &run_reviews, &task.planning_decisions)
    };

    let plan_path = match task.final_plan_path.as_deref() {
        Some(existing) => PathBuf::from(existing),
        None => next_project_plan_path(Path::new(&project_path), &task.title)?,
    };
    fs::create_dir_all(
        plan_path
            .parent()
            .ok_or_else(|| "invalid plan path".to_string())?,
    )
    .map_err(|error| format!("failed to create plans directory: {error}"))?;
    storage::atomic_write_text(&plan_path, &final_plan)
        .map_err(|error| format!("failed to write final plan: {error}"))?;
    update_project_plans_index(
        Path::new(&project_path),
        &plan_path,
        "通过多 Agent 讨论生成最终实施计划，等待人工确认后进入实施。",
    )?;

    let finished_at_ms = now_ms();
    let drafting_succeeded = drafting
        .iter()
        .any(|invocation| invocation.status == "succeeded");
    task.status = crate::task_state::transition(
        task.status,
        crate::task_state::TaskAction::PlanningCompleted,
    )?;
    task.final_plan = Some(final_plan);
    task.final_plan_path = Some(plan_path.display().to_string());
    task.discussion_summary = Some(discussion_summary.clone());
    if let Some(run) = task.planning_runs.last_mut() {
        run.status = if drafting_succeeded {
            "succeeded".to_string()
        } else {
            "failed".to_string()
        };
        run.summary = discussion_summary;
        run.ended_at_ms = Some(finished_at_ms);
    }
    task.plan_reviews
        .retain(|review| review.planning_run_id != planning_run.id);
    task.plan_reviews.extend(run_reviews);
    task.plan_todos = Vec::new();
    task.updated_at_ms = finished_at_ms;
    task.final_plan_html_path = plan_html::write_task_plan_html(&task)?;
    task.events.push(TaskEvent {
        id: ids.next("event"),
        task_id: task.id.clone(),
        timestamp_ms: finished_at_ms,
        actor: "agent".to_string(),
        status: task.status,
        input_summary: Some(format!("Retried planning agent {}", agent.name)),
        output_summary: Some(format!(
            "Retry {} the planning round for {}.",
            if drafting_succeeded {
                "recovered"
            } else {
                "did not recover"
            },
            agent.name
        )),
        evidence_ref: task.final_plan_path.clone(),
    });
    persist_planning_task(task, Path::new(&project_path))
}

fn persist_planning_task(
    mut candidate: crate::models::Task,
    project_path: &Path,
) -> Result<crate::models::Task, String> {
    let task_id = candidate.id.clone();
    tasks::update_task(project_path, &task_id, move |latest| {
        merge_concurrent_task_state(&mut candidate, latest);
        *latest = candidate;
        Ok(())
    })
    .map(|(task, ())| task)
}

fn merge_concurrent_task_state(task: &mut crate::models::Task, latest: &crate::models::Task) {
    task.lifecycle = latest.lifecycle.clone();
    if latest.lifecycle.paused
        || matches!(
            latest.status,
            crate::models::TaskStatus::Blocked | crate::models::TaskStatus::Cancelled
        )
    {
        task.status = latest.status;
    }

    for event in latest.events.iter().cloned() {
        if task.events.iter().all(|current| current.id != event.id) {
            task.events.push(event);
        }
    }
    for run in latest.command_runs.iter().cloned() {
        if let Some(current) = task
            .command_runs
            .iter_mut()
            .find(|current| current.id == run.id)
        {
            *current = run;
        } else {
            task.command_runs.push(run);
        }
    }
    for feedback in latest.feedback.iter().cloned() {
        if task
            .feedback
            .iter()
            .all(|current| current.id != feedback.id)
        {
            task.feedback.push(feedback);
        }
    }
    for decision in latest.planning_decisions.iter().cloned() {
        if task
            .planning_decisions
            .iter()
            .all(|current| current.id != decision.id)
        {
            task.planning_decisions.push(decision);
        }
    }
    task.updated_at_ms = task.updated_at_ms.max(latest.updated_at_ms);
}

/// Classify why an invocation result is unusable. Returns `None` for a usable
/// success. An exit-0 run with empty stdout is unusable for planning, so it is
/// classified (and later downgraded to failed) rather than silently accepted.
#[cfg(test)]
mod tests {
    use super::*;

    fn test_agent(command: &str, adapter_type: &str, args: Vec<String>) -> AgentConfig {
        AgentConfig {
            id: format!("agent-{command}"),
            name: command.to_string(),
            command: command.to_string(),
            args,
            working_directory_policy: "project_root".to_string(),
            capabilities: vec!["planning".to_string()],
            adapter_type: adapter_type.to_string(),
            can_write_files: false,
            can_run_commands: false,
            enabled: true,
            available: true,
        }
    }

    #[test]
    fn legacy_agent_store_is_migrated_on_load() {
        let root = std::env::temp_dir().join(format!("loom-agents-legacy-{}", now_ms()));
        fs::create_dir_all(&root).expect("test root");
        let path = root.join(AGENTS_FILE);
        let agents = vec![test_agent("fixture", ADAPTER_CLI, Vec::new())];
        storage::atomic_write_json(&path, &agents).expect("legacy agent store");

        let loaded = load_agent_store(&path).expect("legacy agent store should migrate");
        let stored: serde_json::Value = storage::read_json_file(&path).expect("migrated store");

        assert_eq!(loaded.len(), 1);
        assert_eq!(stored["schemaVersion"], AGENT_STORE_SCHEMA_VERSION);
        assert_eq!(stored["data"][0]["command"], "fixture");
        fs::remove_dir_all(root).ok();
    }

    fn test_invocation(agent_name: &str, status: &str, raw_output: &str) -> AgentInvocation {
        AgentInvocation {
            id: format!("invocation-{agent_name}"),
            planning_run_id: "planning-run-test".to_string(),
            task_id: "task-test".to_string(),
            agent_id: format!("agent-{agent_name}"),
            agent_name: agent_name.to_string(),
            status: status.to_string(),
            prompt_summary: "test prompt".to_string(),
            raw_output: raw_output.to_string(),
            output_summary: "test summary".to_string(),
            evidence_ref: None,
            plan_path: None,
            stderr_tail: Vec::new(),
            exit_code: Some(0),
            timed_out: false,
            attempt: 1,
            failure_kind: None,
            failure_detail: None,
            error_lines: Vec::new(),
            stderr_ref: None,
            session_id: None,
            resume_command: None,
            started_at_ms: 1,
            ended_at_ms: Some(2),
        }
    }

    fn test_review(reviewer: &str, target: &str, severity: &str, finding: &str) -> PlanReview {
        PlanReview {
            id: format!("review-{reviewer}-{target}"),
            planning_run_id: "planning-run-test".to_string(),
            task_id: "task-test".to_string(),
            reviewer_agent_id: format!("agent-{reviewer}"),
            reviewer_agent_name: reviewer.to_string(),
            target_agent_id: format!("agent-{target}"),
            target_agent_name: target.to_string(),
            status: "succeeded".to_string(),
            finding: finding.to_string(),
            severity: severity.to_string(),
            accepted: false,
            raw_output: finding.to_string(),
            evidence_ref: None,
            stderr_ref: None,
            session_id: None,
            resume_command: None,
            started_at_ms: 1,
            ended_at_ms: Some(2),
        }
    }

    fn test_result(status: &str, stdout: &str, stderr: &str) -> PlanningInvocationResult {
        PlanningInvocationResult {
            status: status.to_string(),
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
            output_summary: String::new(),
            evidence_ref: None,
            plan_path: None,
            exit_code: Some(if status == "succeeded" { 0 } else { 1 }),
            timed_out: false,
            attempt: 1,
            failure_kind: None,
            failure_detail: None,
            error_lines: extract_error_lines(stderr),
            stderr_ref: None,
            session_id: None,
            resume_command: None,
            started_at_ms: 1,
            ended_at_ms: 2,
        }
    }

    #[derive(Clone)]
    struct NoopEmitter;

    impl PlanningEventEmitter for NoopEmitter {
        fn emit_planning_agent_status(&self, _event: PlanningAgentStatusEvent) {}
        fn emit_planning_agent_log(&self, _event: PlanningAgentLogEvent) {}
    }

    #[derive(Clone, Default)]
    struct RecordingEmitter {
        events: std::sync::Arc<std::sync::Mutex<Vec<PlanningAgentStatusEvent>>>,
        logs: std::sync::Arc<std::sync::Mutex<Vec<PlanningAgentLogEvent>>>,
    }

    impl PlanningEventEmitter for RecordingEmitter {
        fn emit_planning_agent_status(&self, event: PlanningAgentStatusEvent) {
            self.events.lock().expect("event lock").push(event);
        }

        fn emit_planning_agent_log(&self, event: PlanningAgentLogEvent) {
            self.logs.lock().expect("log lock").push(event);
        }
    }

    #[test]
    fn renders_prompt_with_requirement_and_project_path() {
        let prompt = render_planning_prompt("Add adapter", "/tmp/project", "Use real CLIs");

        assert!(prompt.content.contains("Add adapter"));
        assert!(prompt.content.contains("/tmp/project"));
        assert!(prompt.content.contains("Use real CLIs"));
        assert!(prompt.content.contains("do not modify files"));
        assert!(prompt.content.contains("## Current State"));
        assert!(prompt.content.contains("## File Impact"));
        assert!(prompt.content.contains("## Implementation Todo"));
        assert!(prompt
            .content
            .contains("Cite concrete repository file paths"));
    }

    #[test]
    fn synthesis_prompt_includes_cross_review_findings() {
        let candidates = vec![test_invocation(
            "claude",
            "succeeded",
            "## Goal\n\nShip candidate\n\n## Implementation Todo\n\n1. Wire synthesis",
        )];
        let reviews = vec![test_review(
            "codex",
            "claude",
            "risk",
            "Migration order has a hidden dependency",
        )];

        let prompt =
            render_synthesis_prompt("Plan", "/repo", "Need final plan", &candidates, &reviews);

        assert!(prompt.content.contains("Candidate 1 — claude"));
        assert!(prompt.content.contains("Wire synthesis"));
        assert!(prompt.content.contains("## Cross-Review Findings"));
        assert!(prompt.content.contains("codex → claude"));
        assert!(prompt
            .content
            .contains("Migration order has a hidden dependency"));
        assert!(prompt
            .content
            .contains("Return only the final Markdown plan"));
    }

    #[test]
    fn synthesis_prompt_omits_review_section_without_findings() {
        let candidates = vec![test_invocation("claude", "succeeded", "## Goal\n\nShip")];
        let mut failed_review = test_review("codex", "claude", "blocker", "review crashed");
        failed_review.status = "failed".to_string();

        let prompt = render_synthesis_prompt(
            "Plan",
            "/repo",
            "Need final plan",
            &candidates,
            &[failed_review],
        );

        assert!(!prompt.content.contains("## Cross-Review Findings"));
        assert!(!prompt.content.contains("review crashed"));
    }

    #[test]
    fn candidate_plan_document_includes_frontmatter() {
        let agent = test_agent("codex", ADAPTER_CODEX, Vec::new());
        let prompt = render_planning_prompt("Plan", "/repo", "Implement candidate plan output");
        let document = render_candidate_plan_document(&agent, &prompt, "# Candidate", 42);

        assert!(document.starts_with("---\nagent: \"codex\""));
        assert!(document.contains("agentId: \"agent-codex\""));
        assert!(document.contains("generatedAtMs: 42"));
        assert!(document.contains("requirementSummary: \"Implement candidate plan output\""));
        assert!(document.contains("# Candidate"));
    }

    #[test]
    fn final_plan_uses_successful_agent_implementation_todos() {
        let invocations = vec![
            test_invocation(
                "codex",
                "succeeded",
                "# Plan\n\n## Implementation Tasks\n\n1. Add task bridge\n- [ ] Persist review evidence\n\n## Risks\n\n- keep small",
            ),
            test_invocation(
                "claude",
                "failed",
                "## Implementation Todo\n\n1. Should be ignored because invocation failed",
            ),
        ];

        let final_plan = render_final_plan("Task Bridge", "Ship bridge", "summary", &invocations);

        assert!(final_plan.contains("1. Add task bridge"));
        assert!(final_plan.contains("2. Persist review evidence"));
        assert!(!final_plan.contains("Should be ignored"));
        assert!(!final_plan.contains("Review successful Agent output"));
    }

    #[test]
    fn final_plan_deduplicates_agent_todos() {
        let invocations = vec![
            test_invocation(
                "codex",
                "succeeded",
                "## Implementation Todo\n\n- Add task bridge\n- Add validation",
            ),
            test_invocation(
                "claude",
                "succeeded",
                "## Implementation Todo\n\n1. add task bridge\n2. Persist evidence",
            ),
        ];

        assert_eq!(
            collect_implementation_todos_from_invocations(&invocations),
            vec![
                "Add task bridge".to_string(),
                "Add validation".to_string(),
                "Persist evidence".to_string()
            ]
        );
    }

    #[test]
    fn final_plan_parses_chinese_agent_todo_sections() {
        let invocations = vec![test_invocation(
            "hermes",
            "succeeded",
            "# 方案\n\n## 实施任务\n\n1、抽取任务桥接边界\n• [ ] 记录 Review 证据\n\n## 风险\n\n- 保持切片可回滚",
        )];

        let final_plan = render_final_plan("任务桥接", "支持中文计划", "summary", &invocations);

        assert!(final_plan.contains("1. 抽取任务桥接边界"));
        assert!(final_plan.contains("2. 记录 Review 证据"));
        assert!(!final_plan.contains("Review successful Agent output"));
    }

    #[test]
    fn final_plan_embeds_full_successful_agent_proposals() {
        let invocations = vec![
            test_invocation(
                "claude",
                "succeeded",
                "## Goal\n\nFocus the planning composer on Cmd+K.\n\n## Verification plan\n\nManual focus check.",
            ),
            test_invocation("codex", "failed", "boom: codex could not run"),
        ];

        let final_plan = render_final_plan("Planning", "Add a shortcut", "summary", &invocations);

        assert!(final_plan.contains("## Agent Proposals"));
        assert!(final_plan.contains("### claude"));
        assert!(final_plan.contains("Focus the planning composer on Cmd+K."));
        assert!(final_plan.contains("Manual focus check."));
        // Failed agents do not contribute proposal bodies.
        assert!(!final_plan.contains("boom: codex could not run"));
    }

    #[tokio::test]
    #[ignore = "requires a real local `claude` CLI with credentials; run explicitly"]
    async fn real_claude_planning_agent_produces_usable_plan_document() {
        let root = std::env::temp_dir().join(format!("loom-real-plan-{}", now_ms()));
        fs::create_dir_all(&root).expect("create temp project");
        let agent = test_agent("claude", ADAPTER_CLAUDE_CODE, Vec::new());
        let prompt = render_planning_prompt(
            "Add a Cmd+K shortcut to focus the planning composer",
            &root.display().to_string(),
            "When the user presses Cmd+K in the planning room, focus the message textarea. Planning only.",
        );

        let result = run_planning_agent(
            &agent,
            &root,
            "Add a Cmd+K shortcut to focus the planning composer",
            "task-real",
            "planning-real",
            &prompt,
            1,
            NoopEmitter,
        )
        .await
        .expect("planning agent should run");

        assert_eq!(result.status, "succeeded", "stderr: {}", result.stderr);
        assert!(
            result.stdout.trim().len() > 200,
            "expected a substantial plan, got: {}",
            result.stdout
        );

        let invocation = test_invocation("claude", &result.status, &result.stdout);
        let plan = render_final_plan(
            "Add a Cmd+K shortcut to focus the planning composer",
            "Focus the composer on Cmd+K",
            "summary",
            std::slice::from_ref(&invocation),
        );
        assert!(plan.contains("## Agent Proposals"));
        assert!(plan.contains(result.stdout.trim()));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn derive_plan_title_uses_goal_heading_from_first_successful_agent() {
        let invocations = vec![
            test_invocation("codex", "failed", "boom"),
            test_invocation(
                "claude",
                "succeeded",
                "## 1. Goal\n\nFocus the planning composer when the user presses Cmd+K.\n\n## 2. Approach\n\n- step",
            ),
        ];

        let title = derive_plan_title(&invocations).expect("should derive a title");

        assert_eq!(
            title,
            "Focus the planning composer when the user presses Cmd+K."
        );
    }

    #[test]
    fn derive_plan_title_handles_inline_goal_and_strips_markdown() {
        let invocations = vec![test_invocation(
            "codex",
            "succeeded",
            "1. **Goal:** Add a `Cmd+K` shortcut\n\n2. Approach",
        )];

        let title = derive_plan_title(&invocations).expect("should derive a title");

        assert_eq!(title, "Add a Cmd+K shortcut");
    }

    #[test]
    fn derive_plan_title_falls_back_to_first_content_line() {
        let invocations = vec![test_invocation(
            "claude",
            "succeeded",
            "# Plan\n\nRework the log search pipeline for streaming.\n",
        )];

        let title = derive_plan_title(&invocations).expect("should derive a title");

        assert_eq!(title, "Rework the log search pipeline for streaming.");
    }

    #[test]
    fn derive_plan_title_returns_none_without_successful_output() {
        let invocations = vec![test_invocation("codex", "failed", "boom: could not run")];

        assert!(derive_plan_title(&invocations).is_none());
    }

    #[test]
    fn claude_planning_profile_avoids_plan_permission_mode() {
        // Plan mode routes the real plan to ExitPlanMode and prints only a terse
        // confirmation, so it must not be used for captured planning output.
        let profile = build_cli_profile(
            &test_agent("claude", ADAPTER_CLAUDE_CODE, Vec::new()),
            Path::new("/tmp/project"),
            Path::new("/tmp/project/prompt.md"),
            None,
            agent_adapter::AgentStage::Planning,
        )
        .expect("Claude planning profile");
        let args = profile.args;

        assert!(args.contains(&"-p".to_string()));
        assert!(args.contains(&"stream-json".to_string()));
        assert!(args.contains(&"--include-partial-messages".to_string()));
        assert!(!args.iter().any(|arg| arg == "plan"));
        assert!(!args.iter().any(|arg| arg == "--permission-mode"));
    }

    #[test]
    fn builds_codex_and_claude_profiles() {
        let project_path = Path::new("/tmp/project");
        let prompt_path = Path::new("/tmp/project/.loom/planning/prompt.md");

        let codex = build_cli_profile(
            &test_agent("codex", ADAPTER_CODEX, Vec::new()),
            project_path,
            prompt_path,
            Some("Implement live planning output"),
            agent_adapter::AgentStage::Planning,
        )
        .expect("codex profile should build");
        assert_eq!(codex.command, "codex");
        assert!(codex.args.contains(&"exec".to_string()));
        assert!(codex.args.contains(&"--json".to_string()));
        assert!(codex.args.contains(&"read-only".to_string()));
        assert_eq!(codex.output_mode, CliOutputMode::CodexJson);
        assert!(!codex
            .args
            .iter()
            .any(|arg| arg.contains("ask-for-approval")));
        assert!(codex.stdin_prompt);

        let claude = build_cli_profile(
            &test_agent("claude", ADAPTER_CLAUDE_CODE, Vec::new()),
            project_path,
            prompt_path,
            Some("Implement live planning output"),
            agent_adapter::AgentStage::Planning,
        )
        .expect("claude profile should build");
        assert_eq!(claude.command, "claude");
        assert!(claude.args.contains(&"-p".to_string()));
        assert!(claude.args.contains(&"--verbose".to_string()));
        assert!(claude.args.contains(&"stream-json".to_string()));
        assert!(claude.args.contains(&"--name".to_string()));
        assert!(claude.args.iter().any(|arg| arg.starts_with("Loom · ")));
        assert_eq!(claude.output_mode, CliOutputMode::ClaudeStreamJson);
        assert!(!claude.args.iter().any(|arg| arg.contains("claude-code")));
    }

    #[test]
    fn load_drops_retired_amp_agents() {
        let mut agents = vec![
            test_agent("codex", ADAPTER_CODEX, Vec::new()),
            AgentConfig {
                id: RETIRED_AGENT_AMP_ID.to_string(),
                name: "Amp".to_string(),
                command: "amp".to_string(),
                args: Vec::new(),
                working_directory_policy: "project_root".to_string(),
                capabilities: vec!["planning".to_string()],
                adapter_type: RETIRED_ADAPTER_AMP.to_string(),
                can_write_files: false,
                can_run_commands: false,
                enabled: true,
                available: false,
            },
            AgentConfig {
                id: "agent-custom-amp".to_string(),
                name: "My Amp".to_string(),
                command: "amp".to_string(),
                args: vec!["-x".to_string()],
                working_directory_policy: "project_root".to_string(),
                capabilities: vec!["planning".to_string()],
                adapter_type: RETIRED_ADAPTER_AMP.to_string(),
                can_write_files: false,
                can_run_commands: false,
                enabled: true,
                available: false,
            },
        ];

        merge_missing_default_agents(&mut agents);

        assert!(agents
            .iter()
            .all(|agent| agent.adapter_type != RETIRED_ADAPTER_AMP));
        assert!(agents.iter().all(|agent| agent.id != RETIRED_AGENT_AMP_ID));
        // The remaining defaults are still synced in.
        assert!(agents.iter().any(|agent| agent.id == "agent-claude"));
    }

    #[test]
    fn syncs_default_dummy_to_disabled_test_fixture() {
        let mut agents = vec![AgentConfig {
            id: "agent-dummy".to_string(),
            name: "Dummy Planner".to_string(),
            command: "dummy".to_string(),
            args: Vec::new(),
            working_directory_policy: "project_root".to_string(),
            capabilities: vec!["planning".to_string(), "review".to_string()],
            adapter_type: ADAPTER_DUMMY.to_string(),
            can_write_files: false,
            can_run_commands: false,
            enabled: true,
            available: true,
        }];

        merge_missing_default_agents(&mut agents);

        let dummy = agents
            .iter()
            .find(|agent| agent.id == "agent-dummy")
            .expect("dummy fixture should remain present");
        assert_eq!(dummy.name, "Dummy Agent (test)");
        assert!(!dummy.enabled);
        assert_eq!(dummy.capabilities, vec!["planning".to_string()]);
    }

    #[test]
    fn updates_custom_agent_config() {
        let mut agents = vec![AgentConfig {
            id: "agent-custom".to_string(),
            name: "Custom".to_string(),
            command: "custom".to_string(),
            args: Vec::new(),
            working_directory_policy: "project_root".to_string(),
            capabilities: vec!["planning".to_string()],
            adapter_type: ADAPTER_CLI.to_string(),
            can_write_files: false,
            can_run_commands: false,
            enabled: true,
            available: false,
        }];
        let updated = AgentConfig {
            id: "agent-custom".to_string(),
            name: "Custom Writer".to_string(),
            command: "custom-agent".to_string(),
            args: vec!["--json".to_string()],
            working_directory_policy: "custom".to_string(),
            capabilities: vec!["implementation".to_string(), "review".to_string()],
            adapter_type: ADAPTER_CLI.to_string(),
            can_write_files: true,
            can_run_commands: true,
            enabled: false,
            available: true,
        };

        apply_agent_update(&mut agents, "agent-custom", updated).unwrap();

        assert_eq!(agents[0].name, "Custom Writer");
        assert_eq!(agents[0].args, vec!["--json".to_string()]);
        assert_eq!(
            agents[0].capabilities,
            vec!["implementation".to_string(), "review".to_string()]
        );
        assert!(agents[0].can_write_files);
        assert!(!agents[0].enabled);
    }

    #[test]
    fn rejects_edit_and_delete_for_builtin_agents() {
        let mut agents = discover_default_agents();
        let codex = agents
            .iter()
            .find(|agent| agent.id == "agent-codex")
            .cloned()
            .expect("default codex profile should exist");

        assert_eq!(
            apply_agent_update(&mut agents, "agent-codex", codex).unwrap_err(),
            "built-in Agent profiles can only be enabled or disabled"
        );
        assert_eq!(
            delete_agent_config(&mut agents, "agent-codex").unwrap_err(),
            "built-in Agent profiles can only be disabled, not deleted"
        );
        assert!(agents.iter().any(|agent| agent.id == "agent-codex"));
    }

    #[test]
    fn implementation_agent_requires_capability_permissions_and_matching_command() {
        let mut agent = test_agent("codex", ADAPTER_CODEX, Vec::new());
        agent.capabilities.push("implementation".to_string());

        assert!(validate_implementation_agent_config(&agent, "codex")
            .unwrap_err()
            .contains("write files"));
        agent.can_write_files = true;
        assert!(validate_implementation_agent_config(&agent, "codex")
            .unwrap_err()
            .contains("run commands"));
        agent.can_run_commands = true;
        validate_implementation_agent_config(&agent, "codex")
            .expect("fully permitted implementation agent");
        assert!(validate_implementation_agent_config(&agent, "claude")
            .unwrap_err()
            .contains("does not match"));
    }

    #[test]
    fn task_stop_marker_prevents_agent_retries_until_an_explicit_new_run() {
        let task_id = format!("task-stop-marker-{}", now_ms());
        clear_task_stop(&task_id).expect("clear initial marker");
        assert!(!task_stop_requested(&task_id));

        assert_eq!(stop_task_runs(&task_id).expect("mark task stopped"), 0);
        assert!(task_stop_requested(&task_id));

        clear_task_stop(&task_id).expect("new run clears marker");
        assert!(!task_stop_requested(&task_id));
    }

    #[test]
    fn deletes_custom_agent_config() {
        let mut agents = vec![
            test_agent("codex", ADAPTER_CODEX, Vec::new()),
            AgentConfig {
                id: "agent-custom".to_string(),
                name: "Custom".to_string(),
                command: "custom".to_string(),
                args: Vec::new(),
                working_directory_policy: "project_root".to_string(),
                capabilities: vec!["planning".to_string()],
                adapter_type: ADAPTER_CLI.to_string(),
                can_write_files: false,
                can_run_commands: false,
                enabled: true,
                available: false,
            },
        ];

        delete_agent_config(&mut agents, "agent-custom").unwrap();

        assert_eq!(agents.len(), 1);
        assert!(agents.iter().all(|agent| agent.id != "agent-custom"));
    }

    #[test]
    fn requested_agents_do_not_fall_back_to_every_enabled_agent() {
        let agents = vec![
            test_agent("codex", ADAPTER_CODEX, Vec::new()),
            AgentConfig {
                id: "agent-disabled".to_string(),
                name: "Disabled Claude".to_string(),
                command: "claude".to_string(),
                args: Vec::new(),
                working_directory_policy: "project_root".to_string(),
                capabilities: vec!["planning".to_string()],
                adapter_type: ADAPTER_CLAUDE_CODE.to_string(),
                can_write_files: false,
                can_run_commands: false,
                enabled: false,
                available: true,
            },
        ];

        let selected = resolve_planning_agents(&agents, &["agent-disabled".to_string()]);

        assert!(selected.is_empty());
    }

    #[test]
    fn empty_agent_request_uses_enabled_planning_agents() {
        let agents = vec![test_agent("codex", ADAPTER_CODEX, Vec::new())];

        let selected = resolve_planning_agents(&agents, &[]);

        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].id, "agent-codex");
    }

    #[test]
    fn slugifies_plan_title_for_project_docs_plans_filename() {
        assert_eq!(
            slugify_plan_title("Real CLI Agent Adapter!"),
            "real-cli-agent-adapter"
        );
        assert_eq!(slugify_plan_title("计划功能"), "plan");
    }

    #[test]
    fn fallback_utc_date_and_time_format_plan_stamp_parts() {
        assert_eq!(utc_date_string(UNIX_EPOCH), "1970-01-01");
        assert_eq!(utc_time_string(UNIX_EPOCH), "00:00");
        assert_eq!(
            utc_date_string(UNIX_EPOCH + Duration::from_secs(86_400 * 20_000)),
            "2024-10-04"
        );
        assert_eq!(
            utc_time_string(UNIX_EPOCH + Duration::from_secs(23 * 3_600 + 59 * 60)),
            "23:59"
        );
    }

    #[test]
    fn next_project_plan_path_uses_date_directory_time_and_slug() {
        let root = std::env::temp_dir().join(format!("loom-plan-path-test-{}", now_ms()));
        let path = next_project_plan_path_for_stamp(
            &root,
            "2026-05-12",
            "04:00",
            "Real CLI Agent Adapter",
        )
        .expect("plan path should be generated");

        assert_eq!(
            path.parent(),
            Some(root.join("docs").join("plans").join("2026-05-12")).as_deref()
        );
        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("04:00-real-cli-agent-adapter.md")
        );
    }

    #[test]
    fn next_project_plan_path_suffixes_colliding_same_minute_plan() {
        let root = std::env::temp_dir().join(format!("loom-plan-path-collision-test-{}", now_ms()));
        let plans_dir = root.join("docs").join("plans").join("2026-05-12");
        fs::create_dir_all(&plans_dir).expect("test dir should be created");
        fs::write(
            plans_dir.join("04:00-real-cli-agent-adapter.md"),
            "existing",
        )
        .expect("collision file should be written");

        let path = next_project_plan_path_for_stamp(
            &root,
            "2026-05-12",
            "04:00",
            "Real CLI Agent Adapter",
        )
        .expect("plan path should be generated");

        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("04:00-real-cli-agent-adapter-2.md")
        );

        fs::remove_dir_all(root).expect("test dir should be removed");
    }

    #[test]
    fn plans_index_inserts_newest_plan_under_existing_date() {
        let existing = "# 计划文档索引\n\n## 2026-05-12\n\n- `04:00-old.md`\n  > 旧计划。\n\n## 2026-05-11\n\n- `17:52-real-cli-agent-adapter.md`\n  > 已实施真实 CLI adapter。\n";

        let updated = render_updated_plans_index(
            existing,
            "2026-05-12",
            "05:10-plan-index-sync.md",
            "自动维护计划索引。",
        );

        assert!(updated.contains(
            "## 2026-05-12\n\n- `05:10-plan-index-sync.md`\n  > 自动维护计划索引。\n- `04:00-old.md`"
        ));
    }

    #[test]
    fn plans_index_keeps_existing_date_entries_in_time_desc_order() {
        let existing = "# 计划文档索引\n\n## 2026-05-12\n\n- `08:00-newer.md`\n  > 较新的计划。\n- `04:00-older.md`\n  > 较旧的计划。\n";

        let updated = render_updated_plans_index(
            existing,
            "2026-05-12",
            "06:30-middle.md",
            "中间时间应插入到对应位置。",
        );

        assert!(updated.contains(
            "- `08:00-newer.md`\n  > 较新的计划。\n- `06:30-middle.md`\n  > 中间时间应插入到对应位置。\n- `04:00-older.md`"
        ));
    }

    #[test]
    fn plans_index_adds_missing_date_near_top() {
        let existing = "# 计划文档索引\n\n## 2026-05-11\n\n- `17:52-real-cli-agent-adapter.md`\n  > 已实施真实 CLI adapter。\n";

        let updated = render_updated_plans_index(
            existing,
            "2026-05-12",
            "05:10-plan-index-sync.md",
            "自动维护计划索引。",
        );

        assert!(
            updated.starts_with("# 计划文档索引\n\n## 2026-05-12\n\n- `05:10-plan-index-sync.md`")
        );
        assert!(updated.contains("\n## 2026-05-11\n"));
    }

    #[test]
    fn plans_index_updates_duplicate_plan_file_summary_in_same_date_section() {
        let existing =
            "# 计划文档索引\n\n## 2026-05-12\n\n- `05:10-plan-index-sync.md`\n  > 原摘要。\n";

        let updated = render_updated_plans_index(
            existing,
            "2026-05-12",
            "05:10-plan-index-sync.md",
            "新摘要应覆盖旧摘要，而不是重复写入。",
        );

        assert_eq!(updated.matches("05:10-plan-index-sync.md").count(), 1);
        assert!(updated
            .contains("- `05:10-plan-index-sync.md`\n  > 新摘要应覆盖旧摘要，而不是重复写入。"));
        assert!(!updated.contains("原摘要。"));
    }

    #[test]
    fn plans_index_allows_same_file_name_on_different_dates() {
        let existing = "# 计划文档索引\n\n## 2026-05-12\n\n- `05:10-plan-index-sync.md`\n  > 今天的计划。\n\n## 2026-05-11\n\n- `17:52-real-cli-agent-adapter.md`\n  > 昨天的计划。\n";

        let updated = render_updated_plans_index(
            existing,
            "2026-05-11",
            "05:10-plan-index-sync.md",
            "允许不同日期下同名计划文件。",
        );

        assert_eq!(updated.matches("05:10-plan-index-sync.md").count(), 2);
        assert!(updated.contains(
            "## 2026-05-11\n\n- `17:52-real-cli-agent-adapter.md`\n  > 昨天的计划。\n- `05:10-plan-index-sync.md`\n  > 允许不同日期下同名计划文件。"
        ));
    }

    #[test]
    fn plans_index_inserts_missing_older_date_after_newer_dates() {
        let existing = "# 计划文档索引\n\n## 2026-05-12\n\n- `05:10-new.md`\n  > 新计划。\n\n## 2026-05-10\n\n- `23:00-old.md`\n  > 旧计划。\n";

        let updated = render_updated_plans_index(
            existing,
            "2026-05-11",
            "04:00-middle.md",
            "中间日期应保持日期倒序。",
        );

        assert!(updated.contains(
            "## 2026-05-12\n\n- `05:10-new.md`\n  > 新计划。\n\n## 2026-05-11\n\n- `04:00-middle.md`\n  > 中间日期应保持日期倒序。\n\n## 2026-05-10"
        ));
    }

    #[test]
    fn replaces_prompt_and_project_placeholders() {
        let project_path = Path::new("/tmp/project");
        let prompt_path = Path::new("/tmp/project/.loom/prompt.md");
        let agent = test_agent(
            "agent-cli",
            ADAPTER_CLI,
            vec![
                "--project".to_string(),
                "{projectPath}".to_string(),
                "--prompt".to_string(),
                "{promptFile}".to_string(),
            ],
        );

        let profile = build_cli_profile(
            &agent,
            project_path,
            prompt_path,
            None,
            agent_adapter::AgentStage::Planning,
        )
        .expect("custom cli profile should build");

        assert!(profile.args.contains(&"/tmp/project".to_string()));
        assert!(profile
            .args
            .contains(&"/tmp/project/.loom/prompt.md".to_string()));
        assert!(!profile.stdin_prompt);
    }

    #[tokio::test]
    async fn runs_cli_profile_with_stdin_and_captures_stdout() {
        let profile = CliProfile {
            adapter_type: ADAPTER_CLI.to_string(),
            command: "sh".to_string(),
            args: vec!["-c".to_string(), "cat".to_string()],
            stdin_prompt: true,
            output_mode: CliOutputMode::Plain,
        };

        let result = run_cli_profile(
            &profile,
            Path::new("."),
            "hello from prompt",
            NoopEmitter,
            None,
        )
        .await
        .expect("shell profile should run");

        assert_eq!(result.status, "succeeded");
        assert_eq!(result.exit_code, Some(0));
        assert!(result.stdout.contains("hello from prompt"));
    }

    #[test]
    fn parses_claude_stream_json_result_and_session() {
        let stream = r##"{"type":"system","subtype":"init","session_id":"claude-session-1"}"##
            .to_string()
            + "\n"
            + r##"{"type":"assistant","message":{"content":[{"type":"text","text":"Drafting..."}]}}"##
            + "\n"
            + r##"{"type":"result","subtype":"success","result":"# Final Plan\n\nShip live output.","session_id":"claude-session-1"}"##;

        let parsed = parse_claude_stream(&stream);

        assert_eq!(parsed.session_id.as_deref(), Some("claude-session-1"));
        assert_eq!(
            parsed.stdout.as_deref(),
            Some("# Final Plan\n\nShip live output.")
        );
    }

    #[test]
    fn parses_codex_json_result_and_session() {
        let stream = r##"{"type":"thread.started","thread_id":"codex-thread-1"}"##.to_string()
            + "\n"
            + r##"{"type":"agent_message","message":"# Codex Plan\n\nUse JSONL."}"##;

        let parsed = parse_codex_stream(&stream);

        assert_eq!(parsed.session_id.as_deref(), Some("codex-thread-1"));
        assert_eq!(parsed.stdout.as_deref(), Some("# Codex Plan\n\nUse JSONL."));
    }

    #[test]
    fn parses_codex_item_completed_agent_message() {
        let stream = r##"{"type":"thread.started","thread_id":"019eb60b-a94f"}"##.to_string()
            + "\n"
            + r##"{"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":"LOOM_SMOKE_OK"}}"##;

        let parsed = parse_codex_stream(&stream);

        assert_eq!(parsed.session_id.as_deref(), Some("019eb60b-a94f"));
        assert_eq!(parsed.stdout.as_deref(), Some("LOOM_SMOKE_OK"));
    }

    #[tokio::test]
    async fn cli_profile_emits_batched_planning_logs() {
        let profile = CliProfile {
            adapter_type: ADAPTER_CLI.to_string(),
            command: "sh".to_string(),
            args: vec![
                "-c".to_string(),
                "printf 'line one\\nline two\\n'; printf 'fatal: nope\\n' >&2".to_string(),
            ],
            stdin_prompt: false,
            output_mode: CliOutputMode::Plain,
        };
        let emitter = RecordingEmitter::default();
        let context = PlanningLogContext {
            task_id: "task-1".to_string(),
            planning_run_id: "planning-1".to_string(),
            agent_id: "agent-codex".to_string(),
            agent_name: "Codex".to_string(),
            phase: "planning".to_string(),
            attempt: 2,
        };

        let result = run_cli_profile(&profile, Path::new("."), "", emitter.clone(), Some(context))
            .await
            .expect("shell profile should run");

        assert_eq!(result.status, "succeeded");
        let logs = emitter.logs.lock().expect("log lock");
        let lines = logs
            .iter()
            .flat_map(|event| event.lines.iter().map(String::as_str))
            .collect::<Vec<_>>();
        assert!(lines.contains(&"line one"));
        assert!(lines.contains(&"line two"));
        assert!(lines.contains(&"fatal: nope"));
        assert!(logs.iter().any(|event| event.attempt == 2));
    }

    #[test]
    fn redacts_sensitive_tokens_from_planning_logs() {
        let bearer_line = format!("Authorization: {} {}", "Bearer", "samplecredential");
        let input = format!("api_key=sk-live-123\n{bearer_line}\nkeep this line");
        let redacted = redact_sensitive_text(&input);

        assert!(redacted.contains("api_key= [REDACTED]"));
        assert!(redacted.contains("Bearer [REDACTED]"));
        assert!(redacted.contains("keep this line"));
        assert!(!redacted.contains("sk-live-123"));
        assert!(!redacted.contains("samplecredential"));
    }

    #[tokio::test]
    async fn cli_profile_redacts_stdout_and_stderr_before_storage() {
        let profile = CliProfile {
            adapter_type: ADAPTER_CLI.to_string(),
            command: "sh".to_string(),
            args: vec![
                "-c".to_string(),
                "printf 'token=secret-stdout\\n'; printf 'password=hunter2\\n' >&2".to_string(),
            ],
            stdin_prompt: false,
            output_mode: CliOutputMode::Plain,
        };

        let result = run_cli_profile(&profile, Path::new("."), "", NoopEmitter, None)
            .await
            .expect("shell profile should run");

        assert!(!result.stdout.contains("secret-stdout"));
        assert!(!result.stderr.contains("hunter2"));
        assert!(result.stdout.contains("token= [REDACTED]"));
        assert!(result.stderr.contains("password= [REDACTED]"));
    }

    #[tokio::test]
    async fn failed_cli_profile_preserves_stderr_tail() {
        let profile = CliProfile {
            adapter_type: ADAPTER_CLI.to_string(),
            command: "sh".to_string(),
            args: vec![
                "-c".to_string(),
                "printf 'fatal: nope\\n' >&2; exit 7".to_string(),
            ],
            stdin_prompt: false,
            output_mode: CliOutputMode::Plain,
        };

        let result = run_cli_profile(&profile, Path::new("."), "", NoopEmitter, None)
            .await
            .expect("failing shell profile should still return result");

        assert_eq!(result.status, "failed");
        assert_eq!(result.exit_code, Some(7));
        assert_eq!(stderr_tail(&result.stderr), vec!["fatal: nope".to_string()]);
    }

    #[tokio::test]
    async fn stderr_only_error_is_failed_even_with_zero_exit() {
        let profile = CliProfile {
            adapter_type: ADAPTER_CLI.to_string(),
            command: "sh".to_string(),
            args: vec![
                "-c".to_string(),
                "printf 'Error: paid credits required\\n' >&2".to_string(),
            ],
            stdin_prompt: false,
            output_mode: CliOutputMode::Plain,
        };

        let result = run_cli_profile(&profile, Path::new("."), "", NoopEmitter, None)
            .await
            .expect("stderr-only error profile should return result");

        assert_eq!(result.status, "failed");
        assert_eq!(result.exit_code, Some(0));
        assert!(result.output_summary.contains("reported an error"));
    }

    #[test]
    fn classify_failure_returns_none_for_usable_success() {
        let result = test_result("succeeded", "# Plan body", "");

        assert_eq!(classify_failure(&result), None);
    }

    #[test]
    fn classify_failure_detects_not_retryable_config_errors() {
        let credits = test_result(
            "failed",
            "",
            "Error: Execute mode requires paid credits and cannot run in non-interactive contexts.",
        );
        let missing_cli = test_result(
            "failed",
            "",
            "failed to start codex: No such file or directory (os error 2)",
        );

        assert_eq!(
            classify_failure(&credits),
            Some(FAILURE_NOT_RETRYABLE.to_string())
        );
        assert_eq!(
            classify_failure(&missing_cli),
            Some(FAILURE_NOT_RETRYABLE.to_string())
        );
    }

    #[test]
    fn classify_failure_detects_timeout_and_empty_output() {
        let mut timed_out = test_result("failed", "", "");
        timed_out.timed_out = true;
        let empty = test_result("succeeded", "   ", "");
        let crashed = test_result("failed", "", "fatal: nope");

        assert_eq!(
            classify_failure(&timed_out),
            Some(FAILURE_TIMEOUT.to_string())
        );
        assert_eq!(
            classify_failure(&empty),
            Some(FAILURE_EMPTY_OUTPUT.to_string())
        );
        assert_eq!(
            classify_failure(&crashed),
            Some(FAILURE_NONZERO_EXIT.to_string())
        );
    }

    #[test]
    fn error_lines_ignore_hook_noise_and_keep_key_failures() {
        let stderr =
            "hook: Stop\nsome progress\nError: paid credits required\nfatal: nope\npanic in worker";

        assert_eq!(
            extract_error_lines(stderr),
            vec![
                "Error: paid credits required".to_string(),
                "fatal: nope".to_string(),
                "panic in worker".to_string(),
            ]
        );
    }

    #[test]
    fn attempt_suffix_only_marks_retries() {
        assert_eq!(attempt_suffix(1), "");
        assert_eq!(attempt_suffix(2), ".attempt-2");
    }

    #[test]
    fn latest_drafting_invocations_keeps_last_attempt_per_agent() {
        let mut first = test_invocation("codex", "failed", "");
        first.attempt = 1;
        let mut retried = test_invocation("codex", "succeeded", "# Plan");
        retried.attempt = 2;
        let claude = test_invocation("claude", "succeeded", "# Other plan");
        let mut synthesis = test_invocation("claude", "succeeded", "# Final");
        synthesis.prompt_summary = SYNTHESIS_PROMPT_SUMMARY.to_string();
        let invocations = vec![first, claude.clone(), retried.clone(), synthesis];

        let drafting = latest_drafting_invocations(&invocations, "planning-run-test");

        assert_eq!(drafting.len(), 2);
        let codex = drafting
            .iter()
            .find(|invocation| invocation.agent_name == "codex")
            .expect("codex entry");
        assert_eq!(codex.attempt, 2);
        assert_eq!(codex.status, "succeeded");
        assert!(drafting
            .iter()
            .all(|invocation| invocation.prompt_summary != SYNTHESIS_PROMPT_SUMMARY));
    }

    fn temp_project(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("loom-{label}-{}", now_ms()));
        fs::create_dir_all(&root).expect("create temp project");
        root
    }

    #[tokio::test]
    async fn cross_reviews_cover_every_pair_and_run_in_parallel_workers() {
        let root = temp_project("cross-reviews");
        let agents = vec![
            test_agent("alpha", ADAPTER_DUMMY, Vec::new()),
            test_agent("beta", ADAPTER_DUMMY, Vec::new()),
        ];
        let candidates = vec![
            test_invocation("alpha", "succeeded", "# Plan A"),
            test_invocation("beta", "succeeded", "# Plan B"),
        ];
        let ids = IdGenerator::default();
        let emitter = RecordingEmitter::default();

        let reviews = run_cross_reviews(
            emitter.clone(),
            &ids,
            &root,
            "Task",
            "task-test",
            "planning-run-test",
            &agents,
            &candidates,
        )
        .await;

        assert_eq!(reviews.len(), 2);
        assert!(reviews.iter().all(|review| review.status == "succeeded"));
        assert!(reviews
            .iter()
            .any(|review| review.reviewer_agent_name == "alpha"
                && review.target_agent_name == "beta"));
        assert!(reviews
            .iter()
            .any(|review| review.reviewer_agent_name == "beta"
                && review.target_agent_name == "alpha"));
        let events = emitter.events.lock().expect("events");
        assert!(events.iter().all(|event| event.phase == "review"));
        assert_eq!(
            events
                .iter()
                .filter(|event| event.status == "succeeded")
                .count(),
            2
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn review_and_synthesize_runs_reviews_before_synthesis() {
        let root = temp_project("synthesize");
        let agents = vec![
            test_agent("alpha", ADAPTER_DUMMY, Vec::new()),
            test_agent("beta", ADAPTER_DUMMY, Vec::new()),
        ];
        let drafting = vec![
            test_invocation("alpha", "succeeded", "# Plan A"),
            test_invocation("beta", "succeeded", "# Plan B"),
        ];
        let ids = IdGenerator::default();

        let outcome = review_and_synthesize(
            NoopEmitter,
            &ids,
            &root,
            "Task",
            "task-test",
            "planning-run-test",
            "Requirement",
            &agents,
            &drafting,
            "summary",
        )
        .await
        .expect("pipeline should complete");

        assert_eq!(outcome.reviews.len(), 2);
        assert!(outcome.synthesis_invocation.is_some());
        assert!(outcome.source_note.contains("synthesized by alpha"));
        assert!(outcome.source_note.contains("2 cross-review findings"));
        assert!(!outcome.final_plan.trim().is_empty());

        let _ = fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn review_and_synthesize_adopts_single_candidate_without_reviews() {
        let root = temp_project("adopt");
        let agents = vec![
            test_agent("alpha", ADAPTER_DUMMY, Vec::new()),
            test_agent("beta", ADAPTER_DUMMY, Vec::new()),
        ];
        let drafting = vec![
            test_invocation("alpha", "succeeded", "# Only plan"),
            test_invocation("beta", "failed", ""),
        ];
        let ids = IdGenerator::default();

        let outcome = review_and_synthesize(
            NoopEmitter,
            &ids,
            &root,
            "Task",
            "task-test",
            "planning-run-test",
            "Requirement",
            &agents,
            &drafting,
            "summary",
        )
        .await
        .expect("pipeline should complete");

        assert!(outcome.reviews.is_empty());
        assert!(outcome.synthesis_invocation.is_none());
        assert_eq!(outcome.final_plan, "# Only plan");
        assert!(outcome.source_note.contains("directly adopted alpha"));

        let _ = fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn review_and_synthesize_falls_back_when_all_agents_failed() {
        let root = temp_project("fallback");
        let agents = vec![test_agent("alpha", ADAPTER_DUMMY, Vec::new())];
        let drafting = vec![test_invocation("alpha", "failed", "")];
        let ids = IdGenerator::default();

        let outcome = review_and_synthesize(
            NoopEmitter,
            &ids,
            &root,
            "Task",
            "task-test",
            "planning-run-test",
            "Requirement",
            &agents,
            &drafting,
            "summary",
        )
        .await
        .expect("pipeline should complete");

        assert!(outcome.reviews.is_empty());
        assert!(outcome.synthesis_invocation.is_none());
        assert!(outcome.source_note.contains("deterministic fallback"));
        assert!(outcome.final_plan.contains("Final Plan"));

        let _ = fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn planning_wrapper_retries_retryable_failures_once() {
        let root = temp_project("retry");
        // A cli agent whose command always exits non-zero: retryable failure.
        let agent = test_agent("false", ADAPTER_CLI, Vec::new());
        let emitter = RecordingEmitter::default();
        let prompt = render_planning_prompt("Task", &root.display().to_string(), "Requirement");

        let result = run_planning_agent_with_status(
            emitter.clone(),
            agent,
            root.clone(),
            "task-test".to_string(),
            "planning-run-test".to_string(),
            "Task".to_string(),
            prompt,
            "planning",
            1,
            true,
        )
        .await;

        assert_eq!(result.status, "failed");
        assert_eq!(result.attempt, 2);
        assert_eq!(result.failure_kind, Some(FAILURE_NONZERO_EXIT.to_string()));
        let events = emitter.events.lock().expect("events");
        assert_eq!(
            events
                .iter()
                .filter(|event| event.status == "retrying")
                .count(),
            1
        );
        assert!(events
            .iter()
            .any(|event| event.status == "failed" && event.attempt == 2));

        let _ = fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn planning_wrapper_does_not_retry_config_errors() {
        let root = temp_project("no-retry");
        let agent = test_agent(
            "node",
            ADAPTER_CLI,
            vec![
                "-e".to_string(),
                "console.error('Error: requires paid credits'); process.exit(1)".to_string(),
            ],
        );
        let emitter = RecordingEmitter::default();
        let prompt = render_planning_prompt("Task", &root.display().to_string(), "Requirement");

        let result = run_planning_agent_with_status(
            emitter.clone(),
            agent,
            root.clone(),
            "task-test".to_string(),
            "planning-run-test".to_string(),
            "Task".to_string(),
            prompt,
            "planning",
            1,
            true,
        )
        .await;

        assert_eq!(result.status, "failed");
        assert_eq!(result.attempt, 1);
        assert_eq!(result.failure_kind, Some(FAILURE_NOT_RETRYABLE.to_string()));
        let events = emitter.events.lock().expect("events");
        assert!(events.iter().all(|event| event.status != "retrying"));

        let _ = fs::remove_dir_all(&root);
    }
}
