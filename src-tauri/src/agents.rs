use crate::{
    models::{
        now_ms, AgentConfig, AgentConfigInput, AgentInvocation, IdGenerator, PlanReview,
        PlanningAgentLogEvent, PlanningAgentStatusEvent, PlanningDecision, PlanningDiscussionInput,
        PlanningRun, TaskEvent,
    },
    plan_html, storage, tasks,
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter, Runtime, State};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader},
    process::Command as TokioCommand,
    time::{interval, sleep},
};

const AGENTS_FILE: &str = "agents.json";
const ADAPTER_CODEX: &str = "codex_cli";
const ADAPTER_CLAUDE_CODE: &str = "claude_code_cli";
const ADAPTER_CLI: &str = "cli";
// Adapter types that used to ship as built-ins but are retired now (Amp needs
// paid credits for non-interactive use). Stored configs are dropped on load.
const RETIRED_ADAPTER_AMP: &str = "amp_cli";
const RETIRED_AGENT_AMP_ID: &str = "agent-amp";
const ADAPTER_DUMMY: &str = "dummy";
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
    fs::write(&plan_path, &final_plan)
        .map_err(|error| format!("failed to write final plan: {error}"))?;
    update_project_plans_index(
        Path::new(&input.project_path),
        &plan_path,
        "通过多 Agent 讨论生成最终实施计划，等待人工确认后进入实施。",
    )?;

    let finished_at_ms = now_ms();
    task.status = "plan_review".to_string();
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
        status: task.status.clone(),
        input_summary: Some(prompt_summary),
        output_summary: Some(if planning_successful_invocations > 0 {
            "Planning discussion generated a final plan for review.".to_string()
        } else {
            "Planning discussion failed for all selected real agents.".to_string()
        }),
        evidence_ref: task.final_plan_path.clone(),
    });
    tasks::save_task(&task)?;

    Ok(task)
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
        fs::write(path, plan).map_err(|error| format!("failed to write reviewed plan: {error}"))?;
        task.final_plan_html_path = plan_html::write_task_plan_html(&task)?;
    }
    task.status = "plan_review".to_string();
    task.updated_at_ms = now_ms();
    task.events.push(TaskEvent {
        id: ids.next("event"),
        task_id: task.id.clone(),
        timestamp_ms: task.updated_at_ms,
        actor: "agent".to_string(),
        status: task.status.clone(),
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
    tasks::save_task(&task)?;

    Ok(task)
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
    fs::write(&plan_path, &final_plan)
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
    task.status = "plan_review".to_string();
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
        status: task.status.clone(),
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
    tasks::save_task(&task)?;

    Ok(task)
}

/// Classify why an invocation result is unusable. Returns `None` for a usable
/// success. An exit-0 run with empty stdout is unusable for planning, so it is
/// classified (and later downgraded to failed) rather than silently accepted.
fn classify_failure(result: &PlanningInvocationResult) -> Option<String> {
    if result.status == "succeeded" && !result.stdout.trim().is_empty() {
        return None;
    }

    let haystack = format!("{}\n{}", result.stderr, result.output_summary).to_lowercase();
    if NOT_RETRYABLE_PATTERNS
        .iter()
        .any(|pattern| haystack.contains(pattern))
    {
        return Some(FAILURE_NOT_RETRYABLE.to_string());
    }
    if result.timed_out {
        return Some(FAILURE_TIMEOUT.to_string());
    }
    if result.stdout.trim().is_empty() && result.status == "succeeded" {
        return Some(FAILURE_EMPTY_OUTPUT.to_string());
    }
    Some(FAILURE_NONZERO_EXIT.to_string())
}

fn failure_detail_for_result(kind: &str, result: &PlanningInvocationResult) -> String {
    match kind {
        FAILURE_TIMEOUT => format!("Timed out after {}s", PLANNING_TIMEOUT_MS / 1000),
        FAILURE_EMPTY_OUTPUT => "Agent completed but produced no output".to_string(),
        FAILURE_NOT_RETRYABLE => extract_error_lines(&result.stderr)
            .into_iter()
            .next()
            .unwrap_or_else(|| "Configuration error; fix the agent setup, then retry".to_string()),
        _ => match result.exit_code {
            Some(code) => format!("Exited with code {code}"),
            None => "Agent process failed before an exit code was available".to_string(),
        },
    }
}

fn extract_error_lines(stderr: &str) -> Vec<String> {
    stderr
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| {
            let lower = line.to_ascii_lowercase();
            !lower.starts_with("hook:")
                && (lower.starts_with("error:")
                    || lower.starts_with("fatal:")
                    || lower.starts_with("panic")
                    || lower.contains("panic"))
        })
        .take(5)
        .map(str::to_string)
        .collect()
}

fn resume_command_for_profile(profile: &CliProfile, session_id: &str) -> Option<String> {
    match profile.adapter_type.as_str() {
        ADAPTER_CLAUDE_CODE => Some(format!("{} --resume {}", profile.command, session_id)),
        ADAPTER_CODEX => Some(format!("{} resume {}", profile.command, session_id)),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn planning_status_event(
    agent: &AgentConfig,
    task_id: &str,
    planning_run_id: &str,
    phase: &str,
    status: &str,
    attempt: u32,
    started_at_ms: u128,
    ended_at_ms: Option<u128>,
) -> PlanningAgentStatusEvent {
    PlanningAgentStatusEvent {
        task_id: task_id.to_string(),
        planning_run_id: planning_run_id.to_string(),
        agent_id: agent.id.clone(),
        agent_name: agent.name.clone(),
        phase: phase.to_string(),
        status: status.to_string(),
        attempt,
        started_at_ms,
        ended_at_ms,
        elapsed_ms: ended_at_ms.map(|ended| ended.saturating_sub(started_at_ms)),
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_planning_agent_with_status<E: PlanningEventEmitter>(
    emitter: E,
    agent: AgentConfig,
    project_path: PathBuf,
    task_id: String,
    planning_run_id: String,
    task_title: String,
    prompt: PlanningPrompt,
    phase: &'static str,
    start_attempt: u32,
    auto_retry: bool,
) -> PlanningInvocationResult {
    let max_attempt = if auto_retry {
        start_attempt + MAX_PLANNING_ATTEMPTS - 1
    } else {
        start_attempt
    };
    let mut attempt = start_attempt;

    loop {
        let started_at_ms = now_ms();
        emitter.emit_planning_agent_status(planning_status_event(
            &agent,
            &task_id,
            &planning_run_id,
            phase,
            "running",
            attempt,
            started_at_ms,
            None,
        ));

        let mut result = if phase == "synthesis" {
            run_synthesis_agent(
                &agent,
                &project_path,
                &task_title,
                &task_id,
                &planning_run_id,
                &prompt,
                attempt,
                emitter.clone(),
            )
            .await
        } else {
            run_planning_agent(
                &agent,
                &project_path,
                &task_title,
                &task_id,
                &planning_run_id,
                &prompt,
                attempt,
                emitter.clone(),
            )
            .await
        }
        .unwrap_or_else(|error| failed_planning_result(&agent, error, Some(started_at_ms)));
        result.attempt = attempt;

        let Some(kind) = classify_failure(&result) else {
            emitter.emit_planning_agent_status(planning_status_event(
                &agent,
                &task_id,
                &planning_run_id,
                phase,
                &result.status,
                attempt,
                result.started_at_ms,
                Some(result.ended_at_ms),
            ));
            return result;
        };

        if result.status == "succeeded" {
            // Exit 0 with no stdout is not a usable plan; surface it as a failure.
            result.status = "failed".to_string();
            result.output_summary = format!("{} produced no output.", agent.name);
        }
        result.failure_kind = Some(kind.clone());
        result.error_lines = extract_error_lines(&result.stderr);
        result.failure_detail = Some(failure_detail_for_result(&kind, &result));

        if kind != FAILURE_NOT_RETRYABLE && attempt < max_attempt {
            emitter.emit_planning_agent_status(planning_status_event(
                &agent,
                &task_id,
                &planning_run_id,
                phase,
                "retrying",
                attempt,
                result.started_at_ms,
                Some(result.ended_at_ms),
            ));
            attempt += 1;
            continue;
        }

        emitter.emit_planning_agent_status(planning_status_event(
            &agent,
            &task_id,
            &planning_run_id,
            phase,
            &result.status,
            attempt,
            result.started_at_ms,
            Some(result.ended_at_ms),
        ));
        return result;
    }
}

fn agent_invocation_from_result(
    ids: &IdGenerator,
    task_id: &str,
    planning_run_id: &str,
    agent: &AgentConfig,
    prompt_summary: String,
    result: PlanningInvocationResult,
) -> AgentInvocation {
    AgentInvocation {
        id: ids.next("invoke"),
        planning_run_id: planning_run_id.to_string(),
        task_id: task_id.to_string(),
        agent_id: agent.id.clone(),
        agent_name: agent.name.clone(),
        status: result.status,
        prompt_summary,
        raw_output: result.stdout,
        output_summary: result.output_summary,
        evidence_ref: result.evidence_ref,
        plan_path: result.plan_path,
        stderr_tail: stderr_tail(&result.stderr),
        exit_code: result.exit_code,
        timed_out: result.timed_out,
        attempt: result.attempt,
        failure_kind: result.failure_kind,
        failure_detail: result.failure_detail,
        error_lines: result.error_lines,
        stderr_ref: result.stderr_ref,
        session_id: result.session_id,
        resume_command: result.resume_command,
        started_at_ms: result.started_at_ms,
        ended_at_ms: Some(result.ended_at_ms),
    }
}

fn failed_planning_result(
    agent: &AgentConfig,
    error: String,
    started_at_ms: Option<u128>,
) -> PlanningInvocationResult {
    let started_at_ms = started_at_ms.unwrap_or_else(now_ms);
    PlanningInvocationResult {
        status: "failed".to_string(),
        stdout: String::new(),
        stderr: redact_sensitive_text(&error),
        output_summary: format!("{} failed before producing output.", agent.name),
        evidence_ref: None,
        plan_path: None,
        exit_code: None,
        timed_out: false,
        attempt: 1,
        failure_kind: None,
        failure_detail: None,
        error_lines: extract_error_lines(&error),
        stderr_ref: None,
        session_id: None,
        resume_command: None,
        started_at_ms,
        ended_at_ms: now_ms(),
    }
}

fn successful_plan_invocations(invocations: &[AgentInvocation]) -> Vec<AgentInvocation> {
    invocations
        .iter()
        .filter(|invocation| {
            invocation.status == "succeeded" && !invocation.raw_output.trim().is_empty()
        })
        .cloned()
        .collect()
}

/// The effective drafting set for a planning run: the latest invocation per
/// agent (retries supersede earlier attempts), excluding synthesis records.
fn latest_drafting_invocations(
    invocations: &[AgentInvocation],
    planning_run_id: &str,
) -> Vec<AgentInvocation> {
    let mut by_agent: Vec<AgentInvocation> = Vec::new();
    for invocation in invocations.iter().filter(|invocation| {
        invocation.planning_run_id == planning_run_id
            && invocation.prompt_summary != SYNTHESIS_PROMPT_SUMMARY
    }) {
        if let Some(existing) = by_agent
            .iter_mut()
            .find(|existing| existing.agent_id == invocation.agent_id)
        {
            *existing = invocation.clone();
        } else {
            by_agent.push(invocation.clone());
        }
    }
    by_agent
}

/// Run every reviewer-vs-target pair in parallel. A failed review becomes a
/// failed `PlanReview` record instead of aborting the round.
#[allow(clippy::too_many_arguments)]
async fn run_cross_reviews<E: PlanningEventEmitter>(
    emitter: E,
    ids: &IdGenerator,
    project_path: &Path,
    task_title: &str,
    task_id: &str,
    planning_run_id: &str,
    agents: &[AgentConfig],
    candidates: &[AgentInvocation],
) -> Vec<PlanReview> {
    let mut handles = Vec::new();
    for reviewer in agents.iter().filter(|agent| {
        candidates
            .iter()
            .any(|candidate| candidate.agent_id == agent.id)
    }) {
        for target in candidates
            .iter()
            .filter(|candidate| candidate.agent_id != reviewer.id)
        {
            let emitter = emitter.clone();
            let reviewer = reviewer.clone();
            let target = target.clone();
            let project_path = project_path.to_path_buf();
            let task_title = task_title.to_string();
            let task_id = task_id.to_string();
            let planning_run_id = planning_run_id.to_string();
            handles.push(tauri::async_runtime::spawn(async move {
                // Progress events identify the pair, not just the reviewer, so
                // one reviewer covering several targets stays distinguishable.
                let pair = AgentConfig {
                    id: format!("{}->{}", reviewer.id, target.agent_id),
                    name: format!("{} → {}", reviewer.name, target.agent_name),
                    ..reviewer.clone()
                };
                let started_at_ms = now_ms();
                emitter.emit_planning_agent_status(planning_status_event(
                    &pair,
                    &task_id,
                    &planning_run_id,
                    "review",
                    "running",
                    1,
                    started_at_ms,
                    None,
                ));
                let result = run_plan_review_agent(
                    &reviewer,
                    &project_path,
                    &task_title,
                    &task_id,
                    &planning_run_id,
                    &target,
                    &pair,
                    emitter.clone(),
                )
                .await
                .unwrap_or_else(|error| PlanReviewInvocationResult {
                    status: "failed".to_string(),
                    finding: format!(
                        "{} failed to review {}: {error}",
                        reviewer.name, target.agent_name
                    ),
                    severity: "blocker".to_string(),
                    raw_output: redact_sensitive_text(&error),
                    evidence_ref: None,
                    stderr_ref: None,
                    session_id: None,
                    resume_command: None,
                    started_at_ms,
                    ended_at_ms: now_ms(),
                });
                emitter.emit_planning_agent_status(planning_status_event(
                    &pair,
                    &task_id,
                    &planning_run_id,
                    "review",
                    &result.status,
                    1,
                    result.started_at_ms,
                    Some(result.ended_at_ms),
                ));
                (reviewer, target, result)
            }));
        }
    }

    let mut reviews = Vec::new();
    for handle in handles {
        let Ok((reviewer, target, result)) = handle.await else {
            continue;
        };
        reviews.push(PlanReview {
            id: ids.next("review"),
            planning_run_id: planning_run_id.to_string(),
            task_id: task_id.to_string(),
            reviewer_agent_id: reviewer.id,
            reviewer_agent_name: reviewer.name,
            target_agent_id: target.agent_id,
            target_agent_name: target.agent_name,
            status: result.status,
            finding: result.finding,
            severity: result.severity,
            accepted: false,
            raw_output: result.raw_output,
            evidence_ref: result.evidence_ref,
            stderr_ref: result.stderr_ref,
            session_id: result.session_id,
            resume_command: result.resume_command,
            started_at_ms: result.started_at_ms,
            ended_at_ms: Some(result.ended_at_ms),
        });
    }
    reviews
}

struct PlanningSynthesisOutcome {
    final_plan: String,
    reviews: Vec<PlanReview>,
    synthesis_invocation: Option<AgentInvocation>,
    /// Sentence appended to the discussion summary describing where the final
    /// plan came from (synthesis / direct adoption / fallback).
    source_note: String,
}

/// Shared tail of the planning pipeline: cross-review the candidates, then
/// synthesize (or adopt / fall back to) the final plan. Used by the initial
/// discussion and by per-agent retries.
#[allow(clippy::too_many_arguments)]
async fn review_and_synthesize<E: PlanningEventEmitter>(
    emitter: E,
    ids: &IdGenerator,
    project_path: &Path,
    task_title: &str,
    task_id: &str,
    planning_run_id: &str,
    requirement: &str,
    selected_agents: &[AgentConfig],
    drafting_invocations: &[AgentInvocation],
    discussion_summary: &str,
) -> Result<PlanningSynthesisOutcome, String> {
    let candidates = successful_plan_invocations(drafting_invocations);

    if candidates.len() >= 2 {
        let reviews = run_cross_reviews(
            emitter.clone(),
            ids,
            project_path,
            task_title,
            task_id,
            planning_run_id,
            selected_agents,
            &candidates,
        )
        .await;
        let synthesis_agent = choose_synthesis_agent(selected_agents, &candidates)
            .ok_or_else(|| "failed to select a synthesis agent".to_string())?;
        let synthesis_prompt = render_synthesis_prompt(
            task_title,
            &project_path.display().to_string(),
            requirement,
            &candidates,
            &reviews,
        );
        let synthesis_result = run_planning_agent_with_status(
            emitter,
            synthesis_agent.clone(),
            project_path.to_path_buf(),
            task_id.to_string(),
            planning_run_id.to_string(),
            task_title.to_string(),
            synthesis_prompt,
            "synthesis",
            1,
            true,
        )
        .await;
        let synthesis_succeeded = synthesis_result.status == "succeeded";
        let synthesis_invocation = agent_invocation_from_result(
            ids,
            task_id,
            planning_run_id,
            &synthesis_agent,
            SYNTHESIS_PROMPT_SUMMARY.to_string(),
            synthesis_result,
        );

        if synthesis_succeeded {
            let successful_reviews = reviews
                .iter()
                .filter(|review| review.status == "succeeded")
                .count();
            Ok(PlanningSynthesisOutcome {
                final_plan: synthesis_invocation.raw_output.clone(),
                source_note: format!(
                    ". Final plan source: synthesized by {} from {} candidate plans and {} cross-review findings.",
                    synthesis_agent.name,
                    candidates.len(),
                    successful_reviews
                ),
                reviews,
                synthesis_invocation: Some(synthesis_invocation),
            })
        } else {
            let source_note =
                ". Final plan source: synthesis failed; used deterministic fallback from candidate plans."
                    .to_string();
            let mut all_invocations = drafting_invocations.to_vec();
            all_invocations.push(synthesis_invocation.clone());
            Ok(PlanningSynthesisOutcome {
                final_plan: render_final_plan(
                    task_title,
                    requirement,
                    &format!("{discussion_summary}{source_note}"),
                    &all_invocations,
                ),
                source_note,
                reviews,
                synthesis_invocation: Some(synthesis_invocation),
            })
        }
    } else if candidates.len() == 1 {
        let candidate = &candidates[0];
        Ok(PlanningSynthesisOutcome {
            final_plan: candidate.raw_output.clone(),
            source_note: format!(
                ". Final plan source: directly adopted {} candidate plan.",
                candidate.agent_name
            ),
            reviews: Vec::new(),
            synthesis_invocation: None,
        })
    } else {
        let source_note =
            ". Final plan source: deterministic fallback because all selected agents failed."
                .to_string();
        Ok(PlanningSynthesisOutcome {
            final_plan: render_final_plan(
                task_title,
                requirement,
                &format!("{discussion_summary}{source_note}"),
                drafting_invocations,
            ),
            source_note,
            reviews: Vec::new(),
            synthesis_invocation: None,
        })
    }
}

fn choose_synthesis_agent(
    agents: &[AgentConfig],
    candidates: &[AgentInvocation],
) -> Option<AgentConfig> {
    let successful_agent_ids = candidates
        .iter()
        .map(|candidate| candidate.agent_id.as_str())
        .collect::<Vec<_>>();

    agents
        .iter()
        .find(|agent| {
            successful_agent_ids.contains(&agent.id.as_str())
                && effective_adapter_type(agent) == ADAPTER_CLAUDE_CODE
        })
        .or_else(|| {
            agents
                .iter()
                .find(|agent| successful_agent_ids.contains(&agent.id.as_str()))
        })
        .cloned()
}

#[allow(clippy::too_many_arguments)]
async fn run_plan_review_agent<E: PlanningEventEmitter>(
    reviewer: &AgentConfig,
    project_path: &Path,
    task_title: &str,
    task_id: &str,
    planning_run_id: &str,
    target: &AgentInvocation,
    log_agent: &AgentConfig,
    emitter: E,
) -> Result<PlanReviewInvocationResult, String> {
    let evidence_dir =
        planning_evidence_dir(project_path, task_id, planning_run_id).join("reviews");
    fs::create_dir_all(&evidence_dir)
        .map_err(|error| format!("failed to create review evidence directory: {error}"))?;

    let review_id = format!("{}-reviews-{}", reviewer.id, target.agent_id);
    let prompt_path = evidence_dir.join(format!("{review_id}.prompt.md"));
    let stdout_path = evidence_dir.join(format!("{review_id}.stdout.md"));
    let stderr_path = evidence_dir.join(format!("{review_id}.stderr.log"));
    let prompt = render_plan_review_prompt(reviewer, target);
    let redacted_prompt = redact_sensitive_text(&prompt.content);
    fs::write(&prompt_path, &redacted_prompt)
        .map_err(|error| format!("failed to write review prompt: {error}"))?;

    if effective_adapter_type(reviewer) == ADAPTER_DUMMY {
        let started_at_ms = now_ms();
        let output = deterministic_plan_review_output(reviewer, target);
        fs::write(&stdout_path, &output)
            .map_err(|error| format!("failed to write dummy review output: {error}"))?;
        fs::write(&stderr_path, "")
            .map_err(|error| format!("failed to write dummy review stderr: {error}"))?;

        return Ok(PlanReviewInvocationResult {
            status: "succeeded".to_string(),
            finding: summarize_review_output(&output),
            severity: infer_review_severity(&output),
            raw_output: output,
            evidence_ref: Some(stdout_path.display().to_string()),
            stderr_ref: Some(stderr_path.display().to_string()),
            session_id: None,
            resume_command: None,
            started_at_ms,
            ended_at_ms: now_ms(),
        });
    }

    let profile = build_cli_profile(reviewer, project_path, &prompt_path, Some(task_title))?;
    let log_context = PlanningLogContext {
        task_id: task_id.to_string(),
        planning_run_id: planning_run_id.to_string(),
        agent_id: log_agent.id.clone(),
        agent_name: log_agent.name.clone(),
        phase: "review".to_string(),
        attempt: 1,
    };
    match run_cli_profile(
        &profile,
        project_path,
        &redacted_prompt,
        emitter,
        Some(log_context),
    )
    .await
    {
        Ok(result) => {
            fs::write(&stdout_path, &result.stdout)
                .map_err(|error| format!("failed to write review stdout: {error}"))?;
            fs::write(&stderr_path, &result.stderr)
                .map_err(|error| format!("failed to write review stderr: {error}"))?;
            Ok(PlanReviewInvocationResult {
                status: result.status,
                finding: summarize_review_output(&result.stdout),
                severity: infer_review_severity(&result.stdout),
                raw_output: result.stdout,
                evidence_ref: Some(stdout_path.display().to_string()),
                stderr_ref: Some(stderr_path.display().to_string()),
                session_id: result.session_id,
                resume_command: result.resume_command,
                started_at_ms: result.started_at_ms,
                ended_at_ms: result.ended_at_ms,
            })
        }
        Err(error) => {
            let started_at_ms = now_ms();
            let stderr = redact_sensitive_text(&error);
            fs::write(&stdout_path, "")
                .map_err(|error| format!("failed to write empty review stdout: {error}"))?;
            fs::write(&stderr_path, &stderr)
                .map_err(|error| format!("failed to write review stderr: {error}"))?;
            Ok(PlanReviewInvocationResult {
                status: "failed".to_string(),
                finding: format!("{} failed to review {}.", reviewer.name, target.agent_name),
                severity: "blocker".to_string(),
                raw_output: stderr,
                evidence_ref: Some(stdout_path.display().to_string()),
                stderr_ref: Some(stderr_path.display().to_string()),
                session_id: None,
                resume_command: None,
                started_at_ms,
                ended_at_ms: now_ms(),
            })
        }
    }
}

/// Evidence file suffix that keeps every attempt's artifacts on disk: the
/// first attempt keeps the historical names, retries get `.attempt-N`.
fn attempt_suffix(attempt: u32) -> String {
    if attempt > 1 {
        format!(".attempt-{attempt}")
    } else {
        String::new()
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_planning_agent<E: PlanningEventEmitter>(
    agent: &AgentConfig,
    project_path: &Path,
    task_title: &str,
    task_id: &str,
    planning_run_id: &str,
    prompt: &PlanningPrompt,
    attempt: u32,
    emitter: E,
) -> Result<PlanningInvocationResult, String> {
    let evidence_dir = planning_evidence_dir(project_path, task_id, planning_run_id);
    fs::create_dir_all(&evidence_dir)
        .map_err(|error| format!("failed to create planning evidence directory: {error}"))?;

    let suffix = attempt_suffix(attempt);
    let prompt_path = evidence_dir.join(format!("{}{suffix}.prompt.md", agent.id));
    let stdout_path = evidence_dir.join(format!("{}{suffix}.stdout.md", agent.id));
    let plan_path = evidence_dir.join(format!("{}{suffix}.plan.md", agent.id));
    let stderr_path = evidence_dir.join(format!("{}{suffix}.stderr.log", agent.id));
    let redacted_prompt = redact_sensitive_text(&prompt.content);
    fs::write(&prompt_path, &redacted_prompt)
        .map_err(|error| format!("failed to write planning prompt: {error}"))?;

    if effective_adapter_type(agent) == ADAPTER_DUMMY {
        let started_at_ms = now_ms();
        let output = deterministic_planning_output(agent, prompt);
        fs::write(&stdout_path, &output)
            .map_err(|error| format!("failed to write dummy planning output: {error}"))?;
        fs::write(
            &plan_path,
            render_candidate_plan_document(agent, prompt, &output, started_at_ms),
        )
        .map_err(|error| format!("failed to write dummy candidate plan: {error}"))?;
        fs::write(&stderr_path, "")
            .map_err(|error| format!("failed to write dummy planning stderr: {error}"))?;

        return Ok(PlanningInvocationResult {
            status: "succeeded".to_string(),
            stdout: output.clone(),
            stderr: String::new(),
            output_summary: summarize_agent_output(agent, &output),
            evidence_ref: Some(stdout_path.display().to_string()),
            plan_path: Some(plan_path.display().to_string()),
            exit_code: Some(0),
            timed_out: false,
            attempt: 1,
            failure_kind: None,
            failure_detail: None,
            error_lines: Vec::new(),
            stderr_ref: Some(stderr_path.display().to_string()),
            session_id: None,
            resume_command: None,
            started_at_ms,
            ended_at_ms: now_ms(),
        });
    }

    let profile = build_cli_profile(agent, project_path, &prompt_path, Some(task_title))?;
    let log_context = PlanningLogContext {
        task_id: task_id.to_string(),
        planning_run_id: planning_run_id.to_string(),
        agent_id: agent.id.clone(),
        agent_name: agent.name.clone(),
        phase: "planning".to_string(),
        attempt,
    };
    let result = run_cli_profile(
        &profile,
        project_path,
        &redacted_prompt,
        emitter,
        Some(log_context),
    )
    .await;

    match result {
        Ok(mut result) => {
            fs::write(&stdout_path, &result.stdout)
                .map_err(|error| format!("failed to write planning stdout: {error}"))?;
            fs::write(
                &plan_path,
                render_candidate_plan_document(agent, prompt, &result.stdout, result.started_at_ms),
            )
            .map_err(|error| format!("failed to write candidate plan: {error}"))?;
            fs::write(&stderr_path, &result.stderr)
                .map_err(|error| format!("failed to write planning stderr: {error}"))?;
            result.evidence_ref = Some(stdout_path.display().to_string());
            result.plan_path = Some(plan_path.display().to_string());
            result.stderr_ref = Some(stderr_path.display().to_string());
            Ok(result)
        }
        Err(error) => {
            let started_at_ms = now_ms();
            let stderr = redact_sensitive_text(&error);
            fs::write(&stdout_path, "")
                .map_err(|error| format!("failed to write empty planning stdout: {error}"))?;
            fs::write(&stderr_path, &stderr)
                .map_err(|error| format!("failed to write planning stderr: {error}"))?;
            Ok(PlanningInvocationResult {
                status: "failed".to_string(),
                stdout: String::new(),
                stderr: stderr.clone(),
                output_summary: format!("{} failed before producing output.", agent.name),
                evidence_ref: Some(stdout_path.display().to_string()),
                plan_path: None,
                exit_code: None,
                timed_out: false,
                attempt: 1,
                failure_kind: None,
                failure_detail: None,
                error_lines: Vec::new(),
                stderr_ref: Some(stderr_path.display().to_string()),
                session_id: None,
                resume_command: None,
                started_at_ms,
                ended_at_ms: now_ms(),
            })
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_synthesis_agent<E: PlanningEventEmitter>(
    agent: &AgentConfig,
    project_path: &Path,
    task_title: &str,
    task_id: &str,
    planning_run_id: &str,
    prompt: &PlanningPrompt,
    attempt: u32,
    emitter: E,
) -> Result<PlanningInvocationResult, String> {
    let evidence_dir = planning_evidence_dir(project_path, task_id, planning_run_id);
    fs::create_dir_all(&evidence_dir)
        .map_err(|error| format!("failed to create synthesis evidence directory: {error}"))?;

    let suffix = attempt_suffix(attempt);
    let prompt_path = evidence_dir.join(format!("synthesis{suffix}.prompt.md"));
    let stdout_path = evidence_dir.join(format!("synthesis{suffix}.stdout.md"));
    let stderr_path = evidence_dir.join(format!("synthesis{suffix}.stderr.log"));
    let redacted_prompt = redact_sensitive_text(&prompt.content);
    fs::write(&prompt_path, &redacted_prompt)
        .map_err(|error| format!("failed to write synthesis prompt: {error}"))?;

    if effective_adapter_type(agent) == ADAPTER_DUMMY {
        let started_at_ms = now_ms();
        let output = deterministic_synthesis_output(prompt);
        fs::write(&stdout_path, &output)
            .map_err(|error| format!("failed to write dummy synthesis output: {error}"))?;
        fs::write(&stderr_path, "")
            .map_err(|error| format!("failed to write dummy synthesis stderr: {error}"))?;

        return Ok(PlanningInvocationResult {
            status: "succeeded".to_string(),
            stdout: output.clone(),
            stderr: String::new(),
            output_summary: "Synthesized final plan from candidate plans.".to_string(),
            evidence_ref: Some(stdout_path.display().to_string()),
            plan_path: None,
            exit_code: Some(0),
            timed_out: false,
            attempt: 1,
            failure_kind: None,
            failure_detail: None,
            error_lines: Vec::new(),
            stderr_ref: Some(stderr_path.display().to_string()),
            session_id: None,
            resume_command: None,
            started_at_ms,
            ended_at_ms: now_ms(),
        });
    }

    let profile = build_cli_profile(agent, project_path, &prompt_path, Some(task_title))?;
    let log_context = PlanningLogContext {
        task_id: task_id.to_string(),
        planning_run_id: planning_run_id.to_string(),
        agent_id: agent.id.clone(),
        agent_name: agent.name.clone(),
        phase: "synthesis".to_string(),
        attempt,
    };
    let result = run_cli_profile(
        &profile,
        project_path,
        &redacted_prompt,
        emitter,
        Some(log_context),
    )
    .await;

    match result {
        Ok(mut result) => {
            fs::write(&stdout_path, &result.stdout)
                .map_err(|error| format!("failed to write synthesis stdout: {error}"))?;
            fs::write(&stderr_path, &result.stderr)
                .map_err(|error| format!("failed to write synthesis stderr: {error}"))?;
            result.evidence_ref = Some(stdout_path.display().to_string());
            result.plan_path = None;
            result.stderr_ref = Some(stderr_path.display().to_string());
            Ok(result)
        }
        Err(error) => {
            let started_at_ms = now_ms();
            let stderr = redact_sensitive_text(&error);
            fs::write(&stdout_path, "")
                .map_err(|error| format!("failed to write empty synthesis stdout: {error}"))?;
            fs::write(&stderr_path, &stderr)
                .map_err(|error| format!("failed to write synthesis stderr: {error}"))?;
            Ok(PlanningInvocationResult {
                status: "failed".to_string(),
                stdout: String::new(),
                stderr: stderr.clone(),
                output_summary: format!("{} failed to synthesize the final plan.", agent.name),
                evidence_ref: Some(stdout_path.display().to_string()),
                plan_path: None,
                exit_code: None,
                timed_out: false,
                attempt: 1,
                failure_kind: None,
                failure_detail: None,
                error_lines: Vec::new(),
                stderr_ref: Some(stderr_path.display().to_string()),
                session_id: None,
                resume_command: None,
                started_at_ms,
                ended_at_ms: now_ms(),
            })
        }
    }
}

struct ParsedCliStdout {
    stdout: Option<String>,
    session_id: Option<String>,
}

async fn read_planning_stream<E, Reader>(
    emitter: E,
    context: Option<PlanningLogContext>,
    output_mode: CliOutputMode,
    stream: &'static str,
    reader: Reader,
) -> String
where
    E: PlanningEventEmitter,
    Reader: AsyncRead + Unpin + Send + 'static,
{
    let mut lines = BufReader::new(reader).lines();
    let mut raw_lines = Vec::new();
    let mut pending_log_lines = Vec::new();
    let mut ticker = interval(Duration::from_millis(100));

    loop {
        tokio::select! {
            line = lines.next_line() => {
                match line {
                    Ok(Some(line)) => {
                        raw_lines.push(line.clone());
                        if context.is_some() {
                            pending_log_lines.extend(log_lines_for_stream(output_mode, stream, &line));
                            if pending_log_lines.len() >= 32 {
                                flush_planning_log(&emitter, context.as_ref(), stream, &mut pending_log_lines);
                            }
                        }
                    }
                    Ok(None) => break,
                    Err(error) => {
                        if context.is_some() {
                            pending_log_lines.push(format!("failed to read {stream}: {error}"));
                        }
                        break;
                    }
                }
            }
            _ = ticker.tick() => {
                flush_planning_log(&emitter, context.as_ref(), stream, &mut pending_log_lines);
            }
        }
    }

    flush_planning_log(&emitter, context.as_ref(), stream, &mut pending_log_lines);
    raw_lines.join("\n")
}

fn flush_planning_log<E: PlanningEventEmitter>(
    emitter: &E,
    context: Option<&PlanningLogContext>,
    stream: &str,
    pending_log_lines: &mut Vec<String>,
) {
    if pending_log_lines.is_empty() {
        return;
    }
    let Some(context) = context else {
        pending_log_lines.clear();
        return;
    };
    let lines = std::mem::take(pending_log_lines)
        .into_iter()
        .flat_map(|line| {
            redact_sensitive_text(&line)
                .lines()
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>();
    if lines.is_empty() {
        return;
    }

    emitter.emit_planning_agent_log(PlanningAgentLogEvent {
        task_id: context.task_id.clone(),
        planning_run_id: context.planning_run_id.clone(),
        agent_id: context.agent_id.clone(),
        agent_name: context.agent_name.clone(),
        phase: context.phase.clone(),
        attempt: context.attempt,
        stream: stream.to_string(),
        lines,
        timestamp_ms: now_ms(),
    });
}

fn log_lines_for_stream(output_mode: CliOutputMode, stream: &str, line: &str) -> Vec<String> {
    if stream != "stdout" {
        return vec![line.to_string()];
    }

    match output_mode {
        CliOutputMode::Plain => vec![line.to_string()],
        CliOutputMode::ClaudeStreamJson => serde_json::from_str::<serde_json::Value>(line)
            .map(|value| claude_log_lines(&value))
            .unwrap_or_else(|_| vec![line.to_string()]),
        CliOutputMode::CodexJson => serde_json::from_str::<serde_json::Value>(line)
            .map(|value| codex_log_lines(&value))
            .unwrap_or_else(|_| vec![line.to_string()]),
    }
}

fn parse_cli_stdout(output_mode: CliOutputMode, stdout: &str) -> ParsedCliStdout {
    match output_mode {
        CliOutputMode::Plain => ParsedCliStdout {
            stdout: None,
            session_id: None,
        },
        CliOutputMode::ClaudeStreamJson => parse_claude_stream(stdout),
        CliOutputMode::CodexJson => parse_codex_stream(stdout),
    }
}

fn parse_claude_stream(stdout: &str) -> ParsedCliStdout {
    let mut session_id = None;
    let mut result_stdout = None;
    let mut assistant_chunks = Vec::new();

    for line in stdout.lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            assistant_chunks.push(line.to_string());
            continue;
        };
        if session_id.is_none() {
            session_id = find_session_id(&value);
        }

        match event_type(&value).as_deref() {
            Some("result") => {
                if let Some(text) = string_field(&value, "result") {
                    if !text.trim().is_empty() {
                        result_stdout = Some(text.to_string());
                    }
                }
            }
            Some("assistant") => {
                assistant_chunks.extend(assistant_text_chunks(&value));
            }
            _ => {}
        }
    }

    ParsedCliStdout {
        stdout: result_stdout.or_else(|| join_chunks(assistant_chunks)),
        session_id,
    }
}

fn parse_codex_stream(stdout: &str) -> ParsedCliStdout {
    let mut session_id = None;
    let mut last_agent_message = None;
    let mut fallback_chunks = Vec::new();

    for line in stdout.lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            fallback_chunks.push(line.to_string());
            continue;
        };
        if session_id.is_none() {
            session_id = find_session_id(&value);
        }

        let event = event_type(&value).unwrap_or_default();
        if is_agent_message_event(&event, &value) {
            let text = assistant_text_chunks(&value).join("\n");
            if !text.trim().is_empty() {
                last_agent_message = Some(text);
            }
        } else {
            fallback_chunks.extend(codex_log_lines(&value));
        }
    }

    ParsedCliStdout {
        stdout: last_agent_message.or_else(|| join_chunks(fallback_chunks)),
        session_id,
    }
}

fn claude_log_lines(value: &serde_json::Value) -> Vec<String> {
    match event_type(value).as_deref() {
        Some("system") => find_session_id(value)
            .map(|id| vec![format!("Session started: {id}")])
            .unwrap_or_default(),
        Some("assistant") => assistant_text_lines(value),
        Some("stream_event") => string_field_deep(value, &["text"])
            .map(split_log_text)
            .unwrap_or_default(),
        Some("result") => {
            if bool_field(value, "is_error") == Some(true) {
                string_field(value, "result")
                    .map(split_log_text)
                    .unwrap_or_default()
            } else {
                Vec::new()
            }
        }
        Some(kind) => vec![format!("[{kind}]")],
        None => Vec::new(),
    }
}

fn codex_log_lines(value: &serde_json::Value) -> Vec<String> {
    let event = event_type(value).unwrap_or_default();
    if event == "thread.started" {
        return find_session_id(value)
            .map(|id| vec![format!("Session started: {id}")])
            .unwrap_or_default();
    }
    if is_agent_message_event(&event, value) {
        return assistant_text_lines(value);
    }
    if event.contains("exec_command") || event.contains("command") {
        if let Some(command) = string_field_deep(value, &["command", "cmd"]) {
            return vec![format!("$ {command}")];
        }
        if let Some(output) = string_field_deep(value, &["output", "text", "delta"]) {
            return split_log_text(output);
        }
    }
    if event.is_empty() {
        Vec::new()
    } else {
        vec![format!("[{event}]")]
    }
}

fn event_type(value: &serde_json::Value) -> Option<String> {
    string_field(value, "type")
        .or_else(|| value.get("msg").and_then(|msg| string_field(msg, "type")))
        .or_else(|| {
            value
                .get("event")
                .and_then(|event| string_field(event, "type"))
        })
        .or_else(|| string_field(value, "event"))
        .map(str::to_string)
}

fn find_session_id(value: &serde_json::Value) -> Option<String> {
    string_field_deep(
        value,
        &[
            "session_id",
            "sessionId",
            "thread_id",
            "threadId",
            "conversation_id",
        ],
    )
    .map(str::to_string)
    .or_else(|| {
        let event = event_type(value)?;
        if event == "thread.started" {
            string_field_deep(value, &["id"]).map(str::to_string)
        } else {
            None
        }
    })
}

fn is_agent_message_event(event: &str, value: &serde_json::Value) -> bool {
    event.contains("agent_message")
        || event.contains("assistant")
        || contains_type_value(value, &["agent_message", "assistant"])
        || string_field_deep(value, &["role"]) == Some("assistant")
}

fn contains_type_value(value: &serde_json::Value, expected: &[&str]) -> bool {
    match value {
        serde_json::Value::Object(map) => {
            map.get("type")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|value| expected.contains(&value))
                || map
                    .values()
                    .any(|child| contains_type_value(child, expected))
        }
        serde_json::Value::Array(items) => items
            .iter()
            .any(|child| contains_type_value(child, expected)),
        _ => false,
    }
}

fn assistant_text_lines(value: &serde_json::Value) -> Vec<String> {
    assistant_text_chunks(value)
        .into_iter()
        .flat_map(|text| split_log_text(&text))
        .collect()
}

fn assistant_text_chunks(value: &serde_json::Value) -> Vec<String> {
    let mut output = Vec::new();
    collect_text_values(value, &mut output);
    output
        .into_iter()
        .filter(|text| !text.trim().is_empty())
        .collect()
}

fn collect_text_values(value: &serde_json::Value, output: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                if matches!(
                    key.as_str(),
                    "text" | "message" | "content" | "delta" | "result"
                ) {
                    if let Some(text) = child.as_str() {
                        if !text.trim().is_empty() {
                            output.push(text.to_string());
                        }
                    }
                }
                collect_text_values(child, output);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_text_values(item, output);
            }
        }
        _ => {}
    }
}

fn string_field<'a>(value: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(serde_json::Value::as_str)
}

fn bool_field(value: &serde_json::Value, key: &str) -> Option<bool> {
    value.get(key).and_then(serde_json::Value::as_bool)
}

fn string_field_deep<'a>(value: &'a serde_json::Value, keys: &[&str]) -> Option<&'a str> {
    match value {
        serde_json::Value::Object(map) => {
            for key in keys {
                if let Some(text) = map.get(*key).and_then(serde_json::Value::as_str) {
                    return Some(text);
                }
            }
            map.values()
                .find_map(|child| string_field_deep(child, keys))
        }
        serde_json::Value::Array(items) => items
            .iter()
            .find_map(|child| string_field_deep(child, keys)),
        _ => None,
    }
}

fn split_log_text(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim_end)
        .filter(|line| !line.trim().is_empty())
        .map(str::to_string)
        .collect()
}

fn join_chunks(chunks: Vec<String>) -> Option<String> {
    let text = chunks
        .into_iter()
        .filter(|chunk| !chunk.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    if text.trim().is_empty() {
        None
    } else {
        Some(text)
    }
}

async fn run_cli_profile<E: PlanningEventEmitter>(
    profile: &CliProfile,
    project_path: &Path,
    prompt: &str,
    emitter: E,
    log_context: Option<PlanningLogContext>,
) -> Result<PlanningInvocationResult, String> {
    let started_at_ms = now_ms();
    let mut command = TokioCommand::new(&profile.command);
    command
        .args(&profile.args)
        .current_dir(project_path)
        .stdin(if profile.stdin_prompt {
            std::process::Stdio::piped()
        } else {
            std::process::Stdio::null()
        })
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = command
        .spawn()
        .map_err(|error| format!("failed to start {}: {error}", profile.command))?;

    if profile.stdin_prompt {
        if let Some(mut stdin) = child.stdin.take() {
            let prompt = prompt.to_string();
            tauri::async_runtime::spawn(async move {
                let _ = stdin.write_all(prompt.as_bytes()).await;
                let _ = stdin.shutdown().await;
            });
        }
    }

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "failed to capture agent stdout".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "failed to capture agent stderr".to_string())?;
    let stdout_task = tauri::async_runtime::spawn(read_planning_stream(
        emitter.clone(),
        log_context.clone(),
        profile.output_mode,
        "stdout",
        stdout,
    ));
    let stderr_task = tauri::async_runtime::spawn(read_planning_stream(
        emitter,
        log_context,
        profile.output_mode,
        "stderr",
        stderr,
    ));

    let timeout = sleep(Duration::from_millis(PLANNING_TIMEOUT_MS));
    tokio::pin!(timeout);
    let (status, timed_out) = tokio::select! {
        status = child.wait() => {
            (status.map_err(|error| format!("failed to wait for {}: {error}", profile.command))?, false)
        }
        _ = &mut timeout => {
            let _ = child.start_kill();
            let status = child
                .wait()
                .await
                .map_err(|error| format!("failed to kill timed out {}: {error}", profile.command))?;
            (status, true)
        }
    };

    let raw_stdout = stdout_task
        .await
        .map_err(|error| format!("failed to join stdout reader: {error}"))?;
    let raw_stderr = stderr_task
        .await
        .map_err(|error| format!("failed to join stderr reader: {error}"))?;
    let parsed = parse_cli_stdout(profile.output_mode, &raw_stdout);
    let stdout = redact_sensitive_text(&parsed.stdout.unwrap_or(raw_stdout));
    let stderr = redact_sensitive_text(&raw_stderr);
    let session_id = parsed.session_id;
    let resume_command = session_id
        .as_deref()
        .and_then(|session_id| resume_command_for_profile(profile, session_id));
    let error_lines = extract_error_lines(&stderr);
    let exit_code = status.code();
    let stderr_only_error = status.success()
        && stdout.trim().is_empty()
        && stderr
            .lines()
            .map(str::trim_start)
            .any(|line| line.starts_with("Error:") || line.starts_with("error:"));
    let status_text = if status.success() && !timed_out && !stderr_only_error {
        "succeeded"
    } else {
        "failed"
    }
    .to_string();
    let output_summary = if status_text == "succeeded" {
        summarize_cli_output(&stdout)
    } else if timed_out {
        format!(
            "{} timed out after {} ms.",
            profile.command, PLANNING_TIMEOUT_MS
        )
    } else if stderr_only_error {
        format!(
            "{} reported an error without stdout. {}",
            profile.command,
            stderr_tail(&stderr).join(" ")
        )
    } else {
        format!(
            "{} exited with status {:?}. {}",
            profile.command,
            exit_code,
            stderr_tail(&stderr).join(" ")
        )
    };

    Ok(PlanningInvocationResult {
        status: status_text,
        stdout,
        stderr,
        output_summary,
        evidence_ref: None,
        plan_path: None,
        exit_code,
        timed_out,
        attempt: 1,
        failure_kind: None,
        failure_detail: None,
        error_lines,
        stderr_ref: None,
        session_id,
        resume_command,
        started_at_ms,
        ended_at_ms: now_ms(),
    })
}

fn resolve_planning_agents(agents: &[AgentConfig], requested_ids: &[String]) -> Vec<AgentConfig> {
    if !requested_ids.is_empty() {
        return requested_ids
            .iter()
            .filter_map(|id| agents.iter().find(|agent| &agent.id == id))
            .filter(|agent| agent.enabled && agent.available && has_planning_capability(agent))
            .cloned()
            .collect();
    }

    agents
        .iter()
        .filter(|agent| agent.enabled && agent.available && has_planning_capability(agent))
        .cloned()
        .collect()
}

fn has_planning_capability(agent: &AgentConfig) -> bool {
    agent
        .capabilities
        .iter()
        .any(|capability| capability == "planning")
}

fn render_planning_prompt(
    task_title: &str,
    project_path: &str,
    requirement: &str,
) -> PlanningPrompt {
    PlanningPrompt {
        content: format!(
            "# Loom Planning Request\n\n## Task\n\n{task_title}\n\n## Project\n\n{project_path}\n\n## Requirement\n\n{requirement}\n\n## Operating Constraints\n\n- Planning stage only: do not modify files.\n- First inspect the existing project code and tests before proposing work.\n- Cite concrete repository file paths and symbols when describing current state or file impact.\n- Do not give generic advice. Produce a plan that another engineer can execute independently.\n- Call out assumptions and blockers explicitly.\n\n## Required Output Template\n\nReturn Markdown with these exact top-level sections:\n\n# <concise plan title>\n\n## Goal\n\nState the intended outcome in one or two paragraphs.\n\n## Non-goals\n\nList work that is intentionally excluded.\n\n## Current State\n\nSummarize the relevant existing code, including concrete file paths and important symbols.\n\n## Technical Approach\n\nDescribe the implementation strategy and important tradeoffs.\n\n## File Impact\n\nUse a table with columns: File / Symbol, Change, Reason.\n\n## Milestones\n\nUse a table with columns: Step, Task, Files / Symbols, Dependencies, Verification.\n\n## Risks\n\nUse bullets or a table. Mark severe items with `blocker` or `risk` text.\n\n## Verification Strategy\n\nList the exact checks, tests, builds, and manual validation that should prove the plan.\n\n## Implementation Todo\n\nProvide numbered implementation tasks. Each item must be specific and actionable.\n"
        ),
    }
}

fn render_cross_review_findings(reviews: &[PlanReview]) -> String {
    reviews
        .iter()
        .filter(|review| review.status == "succeeded")
        .map(|review| {
            format!(
                "- **{} → {}** [{}]: {}",
                review.reviewer_agent_name,
                review.target_agent_name,
                review.severity,
                review.finding
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_synthesis_prompt(
    task_title: &str,
    project_path: &str,
    requirement: &str,
    candidates: &[AgentInvocation],
    reviews: &[PlanReview],
) -> PlanningPrompt {
    let candidate_text = candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| {
            format!(
                "## Candidate {} — {}\n\nEvidence: {}\nCandidate plan path: {}\n\n{}",
                index + 1,
                candidate.agent_name,
                candidate
                    .evidence_ref
                    .as_deref()
                    .unwrap_or("(no stdout evidence)"),
                candidate
                    .plan_path
                    .as_deref()
                    .unwrap_or("(no candidate plan file)"),
                candidate.raw_output.trim()
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n---\n\n");

    let findings = render_cross_review_findings(reviews);
    let review_section = if findings.is_empty() {
        String::new()
    } else {
        format!("\n\n## Cross-Review Findings\n\n{findings}")
    };

    PlanningPrompt {
        content: format!(
            "# Loom Final Plan Synthesis Request\n\n## Task\n\n{task_title}\n\n## Project\n\n{project_path}\n\n## Requirement\n\n{requirement}\n\n## Candidate Plans\n\n{candidate_text}{review_section}\n\n## Instructions\n\n- Synthesize the candidate plans into one final implementation plan.\n- If cross-review findings are listed above, resolve every blocker and risk item explicitly in the final plan.\n- Resolve contradictions by choosing the approach best supported by concrete repository evidence.\n- Preserve concrete file paths, symbols, risks, and verification steps.\n- If a candidate is generic, discard the generic parts instead of copying them.\n- Return only the final Markdown plan.\n\n## Required Output Template\n\nUse these exact top-level sections:\n\n# <concise final plan title>\n\n## Goal\n\n## Non-goals\n\n## Current State\n\n## Technical Approach\n\n## File Impact\n\n## Milestones\n\n## Risks\n\n## Verification Strategy\n\n## Implementation Todo\n"
        ),
    }
}

fn render_plan_review_prompt(reviewer: &AgentConfig, target: &AgentInvocation) -> PlanningPrompt {
    PlanningPrompt {
        content: format!(
            "# Loom Plan Review Request\n\n## Reviewer\n\n{}\n\n## Plan Under Review\n\nAgent: {}\nStatus: {}\nEvidence: {}\n\n## Target Plan Output\n\n{}\n\n## Review Instructions\n\n- Review this plan as another planning Agent, not as the implementer.\n- Identify concrete gaps, contradictions, risks, and test weaknesses.\n- Call out points you agree with.\n- Keep the review scoped to planning; do not modify files.\n\n## Expected Output\n\nUse these sections:\n\n1. Agreement\n2. Concerns\n3. Missing details\n4. Suggested changes\n5. Severity: info|risk|blocker\n",
            reviewer.name,
            target.agent_name,
            target.status,
            target.evidence_ref.clone().unwrap_or_else(|| "(none)".to_string()),
            target.raw_output
        ),
    }
}

fn deterministic_planning_output(agent: &AgentConfig, prompt: &PlanningPrompt) -> String {
    format!(
        "# Dummy Planning Candidate\n\n## Goal\n\nCapture planning evidence for {}.\n\n## Non-goals\n\n- Do not treat dummy output as real Agent reasoning.\n\n## Current State\n\n- `src-tauri/src/agents.rs` runs planning agents and persists evidence.\n- `src-tauri/src/tasks.rs` derives todos from the final plan.\n\n## Technical Approach\n\n- Capture the raw requirement and selected planning agents.\n- Persist each agent discussion output with an evidence reference.\n- Generate a final Markdown plan and derive implementation todo items.\n\n## File Impact\n\n| File / Symbol | Change | Reason |\n|---|---|---|\n| `src-tauri/src/agents.rs` | Persist planning evidence | Keep planning auditable |\n\n## Milestones\n\n| Step | Task | Files / Symbols | Dependencies | Verification |\n|---|---|---|---|---|\n| 1 | Persist Agent output | `run_planning_agent` | None | Unit test evidence path |\n\n## Risks\n\n- risk: Keep dummy output clearly marked as test-only.\n\n## Verification Strategy\n\n- Run Rust unit tests for planning helpers.\n\n## Implementation Todo\n\n1. Capture the raw requirement and selected planning agents.\n2. Persist each agent discussion output with an evidence reference.\n3. Generate a final Markdown plan and derive implementation todo items.\n\nPrompt excerpt:\n{}",
        agent.name,
        prompt.content.lines().take(12).collect::<Vec<_>>().join("\n")
    )
}

fn deterministic_synthesis_output(prompt: &PlanningPrompt) -> String {
    format!(
        "# Synthesized Dummy Final Plan\n\n## Goal\n\nProduce a deterministic final plan from dummy candidate plans.\n\n## Non-goals\n\n- Do not execute implementation work during planning.\n\n## Current State\n\n- `src-tauri/src/agents.rs` owns planning orchestration.\n- `.loom/planning/` stores prompt and stdout evidence.\n\n## Technical Approach\n\n- Combine successful candidate plans into a single final Markdown document.\n- Preserve concrete file paths and verification steps from candidates.\n\n## File Impact\n\n| File / Symbol | Change | Reason |\n|---|---|---|\n| `src-tauri/src/agents.rs` | Select or synthesize final plan | Avoid generic concatenated output |\n\n## Milestones\n\n| Step | Task | Files / Symbols | Dependencies | Verification |\n|---|---|---|---|---|\n| 1 | Synthesize candidates | `render_synthesis_prompt` | Candidate plans | Unit test synthesis prompt |\n\n## Risks\n\n- risk: Dummy synthesis is only for test coverage.\n\n## Verification Strategy\n\n- Run `cargo test` for planning helpers.\n\n## Implementation Todo\n\n1. Select successful candidate plans.\n2. Synthesize one final Markdown plan.\n3. Persist the final plan and planning evidence.\n\nPrompt excerpt:\n{}",
        prompt.content.lines().take(16).collect::<Vec<_>>().join("\n")
    )
}

fn render_candidate_plan_document(
    agent: &AgentConfig,
    prompt: &PlanningPrompt,
    output: &str,
    generated_at_ms: u128,
) -> String {
    format!(
        "---\nagent: \"{}\"\nagentId: \"{}\"\ngeneratedAtMs: {}\nrequirementSummary: \"{}\"\n---\n\n{}{}\n",
        yaml_escape(&agent.name),
        yaml_escape(&agent.id),
        generated_at_ms,
        yaml_escape(&extract_requirement_summary(&prompt.content)),
        output.trim(),
        if output.trim().is_empty() { "" } else { "\n" }
    )
}

fn extract_requirement_summary(prompt: &str) -> String {
    let Some((_, after_heading)) = prompt.split_once("\n## Requirement\n\n") else {
        return "(unknown)".to_string();
    };
    let requirement = after_heading
        .split("\n## ")
        .next()
        .unwrap_or(after_heading)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if requirement.chars().count() <= 160 {
        requirement
    } else {
        format!("{}…", requirement.chars().take(160).collect::<String>())
    }
}

fn yaml_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn deterministic_plan_review_output(reviewer: &AgentConfig, target: &AgentInvocation) -> String {
    format!(
        "Reviewer: {}\nTarget: {}\n\nAgreement:\n- The target plan gives a usable implementation direction.\n\nConcerns:\n- Confirm that scope stays tied to the planning MVP before implementation work begins.\n\nMissing details:\n- Add explicit verification steps and human decision points.\n\nSuggested changes:\n- Split final plan generation from mutual review evidence.\n\nSeverity: risk\n",
        reviewer.name, target.agent_name
    )
}

fn build_cli_profile(
    agent: &AgentConfig,
    project_path: &Path,
    prompt_path: &Path,
    session_title: Option<&str>,
) -> Result<CliProfile, String> {
    let adapter_type = effective_adapter_type(agent);
    let command = agent.command.trim().to_string();
    if command.is_empty() {
        return Err(format!("agent '{}' has no command", agent.name));
    }

    let mut args = if agent.args.is_empty() {
        default_profile_args(&adapter_type, project_path)
    } else {
        agent.args.clone()
    };
    if adapter_type == ADAPTER_CLAUDE_CODE && !args.iter().any(|arg| arg == "--name" || arg == "-n")
    {
        if let Some(title) = session_title {
            args.push("--name".to_string());
            args.push(loom_session_name(title));
        }
    }
    let had_prompt_file = args.iter().any(|arg| arg.contains("{promptFile}"));
    args = replace_arg_placeholders(args, project_path, prompt_path);
    let stdin_prompt = !had_prompt_file;
    let output_mode = cli_output_mode(&adapter_type, &args);

    Ok(CliProfile {
        adapter_type,
        command,
        args,
        stdin_prompt,
        output_mode,
    })
}

fn default_profile_args(adapter_type: &str, project_path: &Path) -> Vec<String> {
    match adapter_type {
        ADAPTER_CODEX => vec![
            "exec".to_string(),
            "--json".to_string(),
            "--cd".to_string(),
            project_path.display().to_string(),
            "--sandbox".to_string(),
            "read-only".to_string(),
            "-".to_string(),
        ],
        // Planning is read-only by intent (enforced by the prompt + headless `-p`,
        // which denies edit tools). We deliberately avoid `--permission-mode plan`:
        // in plan mode Claude hands the real plan to its ExitPlanMode tool and
        // `--output-format text` only prints a terse confirmation, so the captured
        // stdout would be an almost-empty plan document.
        ADAPTER_CLAUDE_CODE => vec![
            "-p".to_string(),
            "--verbose".to_string(),
            "--output-format".to_string(),
            "stream-json".to_string(),
            "--include-partial-messages".to_string(),
        ],
        _ => Vec::new(),
    }
}

fn cli_output_mode(adapter_type: &str, args: &[String]) -> CliOutputMode {
    match adapter_type {
        ADAPTER_CLAUDE_CODE
            if args
                .windows(2)
                .any(|pair| pair[0] == "--output-format" && pair[1] == "stream-json") =>
        {
            CliOutputMode::ClaudeStreamJson
        }
        ADAPTER_CODEX if args.iter().any(|arg| arg == "--json") => CliOutputMode::CodexJson,
        _ => CliOutputMode::Plain,
    }
}

fn loom_session_name(title: &str) -> String {
    let mut name = format!("Loom · {}", title.trim());
    if name.chars().count() > 40 {
        name = name.chars().take(39).collect::<String>();
        name.push('…');
    }
    name
}

fn replace_arg_placeholders(
    args: Vec<String>,
    project_path: &Path,
    prompt_path: &Path,
) -> Vec<String> {
    let project_path = project_path.display().to_string();
    let prompt_path = prompt_path.display().to_string();
    args.into_iter()
        .map(|arg| {
            arg.replace("{projectPath}", &project_path)
                .replace("{promptFile}", &prompt_path)
        })
        .collect()
}

fn effective_adapter_type(agent: &AgentConfig) -> String {
    match agent.adapter_type.as_str() {
        ADAPTER_CLI => match agent.command.as_str() {
            "codex" => ADAPTER_CODEX.to_string(),
            "claude" => ADAPTER_CLAUDE_CODE.to_string(),
            _ => ADAPTER_CLI.to_string(),
        },
        value => value.to_string(),
    }
}

fn summarize_agent_output(agent: &AgentConfig, output: &str) -> String {
    let first_plan_line = output
        .lines()
        .find(|line| line.trim_start().starts_with("- "))
        .unwrap_or("Produced a planning recommendation.");
    format!(
        "{}: {}",
        agent.name,
        first_plan_line.trim_start_matches("- ")
    )
}

fn summarize_cli_output(output: &str) -> String {
    output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.chars().take(220).collect())
        .unwrap_or_else(|| "Agent completed without text output.".to_string())
}

fn summarize_review_output(output: &str) -> String {
    output
        .lines()
        .map(str::trim)
        .find(|line| {
            !line.is_empty()
                && !line.ends_with(':')
                && !line.to_lowercase().starts_with("severity:")
        })
        .map(|line| line.trim_start_matches("- ").chars().take(180).collect())
        .unwrap_or_else(|| "Review completed without a concise finding.".to_string())
}

fn infer_review_severity(output: &str) -> String {
    let lower = output.to_lowercase();
    if lower.contains("severity: blocker") || lower.contains("blocker") || lower.contains("阻塞")
    {
        "blocker".to_string()
    } else if lower.contains("severity: risk")
        || lower.contains("risk")
        || lower.contains("concern")
        || lower.contains("风险")
    {
        "risk".to_string()
    } else {
        "info".to_string()
    }
}

fn summarize_discussion(
    agents: &[AgentConfig],
    requirement: &str,
    invocations: &[AgentInvocation],
) -> String {
    let names = agents
        .iter()
        .map(|agent| agent.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let succeeded = invocations
        .iter()
        .filter(|invocation| invocation.status == "succeeded")
        .map(|invocation| invocation.agent_name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let failed = invocations
        .iter()
        .filter(|invocation| invocation.status != "succeeded")
        .map(|invocation| invocation.agent_name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "Planning agents requested: {names}. Successful agents: {}. Failed agents: {}. Requirement: {requirement}",
        if succeeded.is_empty() { "(none)" } else { &succeeded },
        if failed.is_empty() { "(none)" } else { &failed },
    )
}

/// Derive a concise task title from the first successful agent's plan output,
/// preferring the stated goal. Falls back to the first meaningful content line.
#[cfg(test)]
fn derive_plan_title(invocations: &[AgentInvocation]) -> Option<String> {
    let output = invocations
        .iter()
        .find(|invocation| {
            invocation.status == "succeeded" && !invocation.raw_output.trim().is_empty()
        })
        .map(|invocation| invocation.raw_output.as_str())?;

    derive_plan_title_from_markdown(output)
}

fn derive_plan_title_from_markdown(markdown: &str) -> Option<String> {
    let lines: Vec<&str> = markdown.lines().collect();

    let is_content_line = |line: &str| {
        let trimmed = line.trim();
        !trimmed.is_empty() && !trimmed.starts_with('#') && !trimmed.starts_with("---")
    };

    let goal_text = lines.iter().enumerate().find_map(|(index, line)| {
        let lower = line.to_lowercase();
        if !(lower.contains("goal") || lower.contains("目标")) {
            return None;
        }

        // Inline form: "**Goal:** Add X" / "Goal: Add X".
        if let Some((_, after)) = line.split_once(':') {
            let after = clean_title(after);
            if !after.is_empty() {
                return Some(after);
            }
        }

        // Heading form: take the next content line.
        lines[index + 1..]
            .iter()
            .find(|candidate| is_content_line(candidate))
            .map(|candidate| candidate.trim().to_string())
    });

    let raw_title = goal_text.or_else(|| {
        lines
            .iter()
            .find(|line| is_content_line(line))
            .map(|line| line.trim().to_string())
    })?;

    let title = clean_title(&raw_title);
    if title.is_empty() {
        None
    } else {
        Some(title)
    }
}

/// Strip Markdown decoration and leading list markers, then cap the length.
fn clean_title(text: &str) -> String {
    let stripped = text.replace("**", "").replace(['`', '#'], "");
    let stripped = stripped.trim();
    let stripped = stripped.trim_start_matches(|c: char| {
        c.is_ascii_digit() || matches!(c, '.' | ')' | '-' | '*' | '、' | '：' | ':' | ' ')
    });
    let cleaned = stripped.trim();

    if cleaned.chars().count() <= 64 {
        return cleaned.to_string();
    }

    let truncated: String = cleaned.chars().take(64).collect();
    match truncated.rsplit_once(' ') {
        Some((head, _)) if head.chars().count() > 24 => format!("{head}…"),
        _ => format!("{truncated}…"),
    }
}

fn render_final_plan(
    task_title: &str,
    requirement: &str,
    discussion_summary: &str,
    invocations: &[AgentInvocation],
) -> String {
    let has_successful_agent = invocations
        .iter()
        .any(|invocation| invocation.status == "succeeded");
    let agent_notes = invocations
        .iter()
        .map(|invocation| {
            format!(
                "- **{}** ({}) : {}",
                invocation.agent_name, invocation.status, invocation.output_summary
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let agent_proposals = invocations
        .iter()
        .filter(|invocation| {
            invocation.status == "succeeded" && !invocation.raw_output.trim().is_empty()
        })
        .map(|invocation| {
            format!(
                "### {}\n\n{}",
                invocation.agent_name,
                invocation.raw_output.trim()
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    let agent_proposals = if agent_proposals.is_empty() {
        "No successful agent produced a detailed proposal.".to_string()
    } else {
        agent_proposals
    };
    let agent_todos = collect_implementation_todos_from_invocations(invocations);
    let implementation_todo = if !agent_todos.is_empty() {
        agent_todos
            .iter()
            .enumerate()
            .map(|(index, todo)| format!("{}. {}", index + 1, todo))
            .collect::<Vec<_>>()
            .join("\n")
    } else if has_successful_agent {
        "1. Review successful Agent output and select the implementation slice.\n2. Persist planning runs and Agent invocation evidence.\n3. Generate the final plan document and todo list.\n4. Add the implementation handoff view."
            .to_string()
    } else {
        "No implementation todo items were generated because all selected planning agents failed."
            .to_string()
    };
    let acceptance_criteria = if has_successful_agent {
        "- The planning discussion is visible in the right-side conversation stream.\n- The final plan is written to `docs/plans/` in the selected project.\n- The user can confirm the plan and move the task to `ready_to_implement` with actionable todo items."
    } else {
        "- Failed Agent diagnostics are visible in the right-side conversation stream.\n- The final plan is written to `docs/plans/` in the selected project for troubleshooting.\n- The task remains in plan review until a successful planning run is available."
    };

    format!(
        "# {task_title} — Final Plan\n\n## Requirement\n\n{requirement}\n\n## Discussion Summary\n\n{discussion_summary}\n\n## Agent Notes\n\n{agent_notes}\n\n## Agent Proposals\n\n{agent_proposals}\n\n## Implementation Todo\n\n{implementation_todo}\n\n## Acceptance Criteria\n\n{acceptance_criteria}\n"
    )
}

fn render_reviewed_final_plan(
    plan: &str,
    reviews: &[PlanReview],
    decisions: &[PlanningDecision],
) -> String {
    let base = plan
        .split("\n## Mutual Plan Reviews\n")
        .next()
        .unwrap_or(plan)
        .trim_end();
    let review_section = if reviews.is_empty() {
        "- No mutual review records captured yet.".to_string()
    } else {
        reviews
            .iter()
            .map(|review| {
                format!(
                    "- **{} → {}** [{} / {}]: {}",
                    review.reviewer_agent_name,
                    review.target_agent_name,
                    review.status,
                    review.severity,
                    review.finding
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let decision_section = if decisions.is_empty() {
        "- No human decisions captured yet.".to_string()
    } else {
        decisions
            .iter()
            .map(|decision| format!("- **{}**: {}", decision.title, decision.content))
            .collect::<Vec<_>>()
            .join("\n")
    };

    format!(
        "{base}\n\n## Mutual Plan Reviews\n\n{review_section}\n\n## Human Decisions\n\n{decision_section}\n"
    )
}

fn collect_implementation_todos_from_invocations(invocations: &[AgentInvocation]) -> Vec<String> {
    let mut todos = Vec::new();

    for invocation in invocations
        .iter()
        .filter(|invocation| invocation.status == "succeeded")
    {
        for todo in implementation_todos_from_agent_output(&invocation.raw_output) {
            let duplicate = todos
                .iter()
                .any(|existing: &String| existing.eq_ignore_ascii_case(&todo));
            if !duplicate {
                todos.push(todo);
            }
            if todos.len() >= 8 {
                return todos;
            }
        }
    }

    todos
}

fn implementation_todos_from_agent_output(output: &str) -> Vec<String> {
    let mut in_section = false;
    let mut todos = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            in_section = is_implementation_todo_heading(trimmed);
            continue;
        }

        if !in_section {
            continue;
        }

        if let Some(todo) = strip_agent_todo_marker(trimmed) {
            if !todo.is_empty() && !todo.to_lowercase().starts_with("no implementation todo") {
                todos.push(todo.to_string());
            }
        }
    }

    todos
}

fn is_implementation_todo_heading(line: &str) -> bool {
    let heading = line.trim_start_matches('#').trim().to_lowercase();
    matches!(
        heading.as_str(),
        "implementation todo"
            | "implementation todos"
            | "implementation tasks"
            | "implementation task list"
            | "implementation plan"
            | "实施任务"
            | "实施待办"
            | "实现任务"
            | "实现待办"
            | "开发任务"
            | "任务清单"
    )
}

fn strip_agent_todo_marker(line: &str) -> Option<&str> {
    let bullet = line
        .strip_prefix("- ")
        .or_else(|| line.strip_prefix("* "))
        .or_else(|| line.strip_prefix("• "));
    if let Some(value) = bullet {
        return Some(strip_agent_checkbox_marker(value.trim()));
    }

    for marker in [". ", ") ", "、"] {
        if let Some((number, rest)) = line.split_once(marker) {
            if number.chars().all(|char| char.is_ascii_digit()) {
                return Some(strip_agent_checkbox_marker(rest.trim()));
            }
        }
    }

    None
}

fn strip_agent_checkbox_marker(line: &str) -> &str {
    ["[ ] ", "[x] ", "[X] "]
        .iter()
        .find_map(|marker| line.strip_prefix(marker))
        .unwrap_or(line)
        .trim()
}

fn planning_evidence_dir(project_path: &Path, task_id: &str, planning_run_id: &str) -> PathBuf {
    storage::project_loom_dir(project_path)
        .join("planning")
        .join(task_id)
        .join(planning_run_id)
}

fn next_project_plan_path(project_path: &Path, task_title: &str) -> Result<PathBuf, String> {
    next_project_plan_path_for_stamp(
        project_path,
        &local_date_string(),
        &local_time_string(),
        task_title,
    )
}

fn update_project_plans_index(
    project_path: &Path,
    plan_path: &Path,
    summary: &str,
) -> Result<(), String> {
    let date = plan_path
        .parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("invalid plan date path: {}", plan_path.display()))?;
    let file_name = plan_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("invalid plan file name: {}", plan_path.display()))?;
    let index_path = project_path.join("docs").join("PLANS.md");
    let existing = if index_path.exists() {
        fs::read_to_string(&index_path)
            .map_err(|error| format!("failed to read plans index: {error}"))?
    } else {
        "# 计划文档索引\n".to_string()
    };
    let updated = render_updated_plans_index(&existing, date, file_name, summary);

    if let Some(parent) = index_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create plans index directory: {error}"))?;
    }
    fs::write(&index_path, updated).map_err(|error| format!("failed to write plans index: {error}"))
}

fn render_updated_plans_index(
    existing: &str,
    date: &str,
    file_name: &str,
    summary: &str,
) -> String {
    let entry = format!("- `{file_name}`\n  > {}", summary.trim());
    let mut lines: Vec<String> = existing.lines().map(str::to_string).collect();
    if lines.is_empty() {
        lines.push("# 计划文档索引".to_string());
    }

    let heading = format!("## {date}");
    if let Some(index) = lines.iter().position(|line| line.trim() == heading) {
        if date_section_contains_plan(&lines, index, file_name) {
            return ensure_trailing_newline(&update_date_section_plan_summary(
                &lines, index, file_name, summary,
            ));
        }

        let insert_at = date_section_plan_insert_index(&lines, index, file_name);
        lines.insert(insert_at, entry);
        return ensure_trailing_newline(&lines.join("\n"));
    }

    let mut insert_at = plan_date_insert_index(&lines, date);
    if insert_at == lines.len() && lines.last().is_some_and(|line| !line.trim().is_empty()) {
        lines.push(String::new());
        insert_at += 1;
    }
    lines.insert(insert_at, format!("## {date}"));
    lines.insert(insert_at + 1, String::new());
    lines.insert(insert_at + 2, entry);
    lines.insert(insert_at + 3, String::new());

    ensure_trailing_newline(&lines.join("\n"))
}

fn date_section_contains_plan(lines: &[String], heading_index: usize, file_name: &str) -> bool {
    lines
        .iter()
        .skip(heading_index + 1)
        .take_while(|line| !line.trim_start().starts_with("## "))
        .any(|line| plan_entry_file_name(line).is_some_and(|entry| entry == file_name))
}

fn update_date_section_plan_summary(
    lines: &[String],
    heading_index: usize,
    file_name: &str,
    summary: &str,
) -> String {
    let mut updated = lines.to_vec();
    let section_end = lines
        .iter()
        .enumerate()
        .skip(heading_index + 1)
        .find_map(|(index, line)| line.trim_start().starts_with("## ").then_some(index))
        .unwrap_or(lines.len());

    if let Some(entry_index) = lines
        .iter()
        .enumerate()
        .skip(heading_index + 1)
        .take(section_end.saturating_sub(heading_index + 1))
        .find_map(|(index, line)| {
            plan_entry_file_name(line)
                .is_some_and(|entry| entry == file_name)
                .then_some(index)
        })
    {
        let summary_line = format!("  > {}", summary.trim());
        if updated
            .get(entry_index + 1)
            .is_some_and(|line| line.trim_start().starts_with("> "))
        {
            updated[entry_index + 1] = summary_line;
        } else {
            updated.insert(entry_index + 1, summary_line);
        }
    }

    updated.join("\n")
}

fn date_section_plan_insert_index(
    lines: &[String],
    heading_index: usize,
    file_name: &str,
) -> usize {
    let section_end = lines
        .iter()
        .enumerate()
        .skip(heading_index + 1)
        .find_map(|(index, line)| line.trim_start().starts_with("## ").then_some(index))
        .unwrap_or(lines.len());

    for (index, line) in lines
        .iter()
        .enumerate()
        .skip(heading_index + 1)
        .take(section_end.saturating_sub(heading_index + 1))
    {
        if let Some(existing_file_name) = plan_entry_file_name(line) {
            if file_name > existing_file_name {
                return index;
            }
        }
    }

    section_end
}

fn plan_entry_file_name(line: &str) -> Option<&str> {
    let value = line.trim().strip_prefix("- ")?.trim();
    value
        .strip_prefix('`')
        .and_then(|rest| rest.split_once('`').map(|(file_name, _)| file_name))
        .or_else(|| value.split_whitespace().next())
}

fn plan_date_insert_index(lines: &[String], date: &str) -> usize {
    lines
        .iter()
        .enumerate()
        .skip(1)
        .find_map(|(index, line)| {
            plan_heading_date(line)
                .filter(|heading_date| date > *heading_date)
                .map(|_| index)
        })
        .unwrap_or(lines.len())
}

fn plan_heading_date(line: &str) -> Option<&str> {
    let value = line.trim().strip_prefix("## ")?;
    (value.len() == 10
        && value
            .chars()
            .all(|char| char.is_ascii_digit() || char == '-'))
    .then_some(value)
}

fn ensure_trailing_newline(content: &str) -> String {
    let mut content = content.to_string();
    if !content.ends_with('\n') {
        content.push('\n');
    }
    content
}

fn next_project_plan_path_for_stamp(
    project_path: &Path,
    local_date: &str,
    local_time: &str,
    task_title: &str,
) -> Result<PathBuf, String> {
    let plans_dir = storage::project_plans_dir(project_path).join(local_date);
    let base_name = format!("{}-{}", local_time, slugify_plan_title(task_title));
    let mut candidate = plans_dir.join(format!("{base_name}.md"));
    let mut suffix = 2;

    while candidate.exists() {
        candidate = plans_dir.join(format!("{base_name}-{suffix}.md"));
        suffix += 1;
    }

    Ok(candidate)
}

fn slugify_plan_title(title: &str) -> String {
    let mut slug = String::new();
    let mut previous_dash = false;

    for character in title.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            slug.push(character);
            previous_dash = false;
        } else if !previous_dash && !slug.is_empty() {
            slug.push('-');
            previous_dash = true;
        }

        if slug.len() >= 64 {
            break;
        }
    }

    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "plan".to_string()
    } else {
        slug
    }
}

fn local_date_string() -> String {
    platform_local_date().unwrap_or_else(|| utc_date_string(SystemTime::now()))
}

fn local_time_string() -> String {
    platform_local_time().unwrap_or_else(|| utc_time_string(SystemTime::now()))
}

#[cfg(not(windows))]
fn platform_local_date() -> Option<String> {
    Command::new("date")
        .arg("+%Y-%m-%d")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| value.len() == 10)
}

#[cfg(windows)]
fn platform_local_date() -> Option<String> {
    Command::new("powershell")
        .args(["-NoProfile", "-Command", "Get-Date -Format yyyy-MM-dd"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| value.len() == 10)
}

#[cfg(not(windows))]
fn platform_local_time() -> Option<String> {
    Command::new("date")
        .arg("+%H:%M")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| value.len() == 5)
}

#[cfg(windows)]
fn platform_local_time() -> Option<String> {
    Command::new("powershell")
        .args(["-NoProfile", "-Command", "Get-Date -Format HH:mm"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| value.len() == 5)
}

fn utc_date_string(time: SystemTime) -> String {
    let seconds = time
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default();
    let days = seconds.div_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

fn utc_time_string(time: SystemTime) -> String {
    let seconds = time
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default();
    let seconds_in_day = seconds.rem_euclid(86_400);
    let hour = seconds_in_day.div_euclid(3_600);
    let minute = seconds_in_day.rem_euclid(3_600).div_euclid(60);
    format!("{hour:02}:{minute:02}")
}

// Howard Hinnant's civil-from-days algorithm, using days since Unix epoch.
fn civil_from_days(days_since_epoch: i64) -> (i32, u32, u32) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 }.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096).div_euclid(365);
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2).div_euclid(153);
    let day = doy - (153 * mp + 2).div_euclid(5) + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += if month <= 2 { 1 } else { 0 };

    (year as i32, month as u32, day as u32)
}

fn load_agents(app: &AppHandle) -> Result<Vec<AgentConfig>, String> {
    let path = agents_path(app)?;

    if !path.exists() {
        let agents = discover_default_agents();
        save_agents(app, &agents)?;
        return Ok(agents);
    }

    let mut agents: Vec<AgentConfig> = storage::read_json_file(&path)?;
    merge_missing_default_agents(&mut agents);
    for agent in &mut agents {
        agent.available = command_available(agent);
    }
    save_agents(app, &agents)?;

    Ok(agents)
}

fn save_agents(app: &AppHandle, agents: &[AgentConfig]) -> Result<(), String> {
    storage::atomic_write_json(&agents_path(app)?, &agents)
}

fn agents_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(storage::global_config_dir(app)?.join(AGENTS_FILE))
}

fn discover_default_agents() -> Vec<AgentConfig> {
    let mut agents = [
        ("agent-codex", "Codex", "codex", ADAPTER_CODEX, true, true),
        (
            "agent-claude",
            "Claude Code",
            "claude",
            ADAPTER_CLAUDE_CODE,
            true,
            true,
        ),
    ]
    .into_iter()
    .map(
        |(id, name, command, adapter_type, can_write_files, can_run_commands)| {
            let mut agent = AgentConfig {
                id: id.to_string(),
                name: name.to_string(),
                command: command.to_string(),
                args: Vec::new(),
                working_directory_policy: "project_root".to_string(),
                capabilities: vec![
                    "planning".to_string(),
                    "implementation".to_string(),
                    "review".to_string(),
                ],
                adapter_type: adapter_type.to_string(),
                can_write_files,
                can_run_commands,
                enabled: true,
                available: false,
            };
            agent.available = command_available(&agent);
            agent
        },
    )
    .collect::<Vec<_>>();

    agents.push(AgentConfig {
        id: "agent-dummy".to_string(),
        name: "Dummy Agent (test)".to_string(),
        command: "dummy".to_string(),
        args: Vec::new(),
        working_directory_policy: "project_root".to_string(),
        capabilities: vec!["planning".to_string()],
        adapter_type: ADAPTER_DUMMY.to_string(),
        can_write_files: false,
        can_run_commands: false,
        enabled: false,
        available: true,
    });

    agents
}

fn merge_missing_default_agents(agents: &mut Vec<AgentConfig>) {
    // Drop retired built-ins (and any custom config on the same adapter) from
    // previously saved files so they stop showing up in the UI.
    agents.retain(|agent| {
        agent.id != RETIRED_AGENT_AMP_ID && agent.adapter_type != RETIRED_ADAPTER_AMP
    });

    for default_agent in discover_default_agents() {
        if let Some(existing) = agents.iter_mut().find(|agent| agent.id == default_agent.id) {
            let preserve_enabled = existing.adapter_type != ADAPTER_DUMMY;
            existing.name = default_agent.name.clone();
            existing.command = default_agent.command.clone();
            existing.args = default_agent.args.clone();
            existing.working_directory_policy = default_agent.working_directory_policy.clone();
            existing.capabilities = default_agent.capabilities.clone();
            existing.adapter_type = default_agent.adapter_type.clone();
            existing.can_write_files = default_agent.can_write_files;
            existing.can_run_commands = default_agent.can_run_commands;
            existing.available = default_agent.available;
            if !preserve_enabled {
                existing.enabled = default_agent.enabled;
            }
            continue;
        }

        let already_present = agents.iter().any(|agent| {
            agent.command == default_agent.command
                && effective_adapter_type(agent) == default_agent.adapter_type
        });

        if !already_present {
            agents.push(default_agent);
        }
    }
}

fn is_default_agent_id(agent_id: &str) -> bool {
    matches!(agent_id, "agent-codex" | "agent-claude" | "agent-dummy")
}

fn command_available(agent: &AgentConfig) -> bool {
    if agent.adapter_type == ADAPTER_DUMMY {
        return true;
    }

    if agent.command.trim().is_empty() {
        return false;
    }

    #[cfg(windows)]
    let available = Command::new("where").arg(&agent.command).output();

    #[cfg(not(windows))]
    let available = Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {}", shell_escape(&agent.command)))
        .output();

    available
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn stderr_tail(stderr: &str) -> Vec<String> {
    stderr
        .lines()
        .rev()
        .take(20)
        .map(str::to_string)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

pub(crate) fn redact_sensitive_text(input: &str) -> String {
    let mut output = input
        .lines()
        .map(redact_sensitive_line)
        .collect::<Vec<_>>()
        .join("\n");

    if input.ends_with('\n') {
        output.push('\n');
    }

    redact_bearer_tokens(&output)
}

fn redact_sensitive_line(line: &str) -> String {
    let lower = line.to_ascii_lowercase();
    let sensitive_keys = [
        "api_key",
        "apikey",
        "access_token",
        "auth_token",
        "token",
        "secret",
        "password",
        "passwd",
    ];

    if !sensitive_keys.iter().any(|key| lower.contains(key)) {
        return line.to_string();
    }

    match line
        .char_indices()
        .find(|(_, character)| matches!(character, '=' | ':'))
    {
        Some((index, separator)) => format!("{}{} [REDACTED]", &line[..index], separator),
        None => "[REDACTED sensitive line]".to_string(),
    }
}

fn redact_bearer_tokens(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut remaining = input;

    while let Some(index) = remaining.to_ascii_lowercase().find("bearer ") {
        output.push_str(&remaining[..index]);
        output.push_str(&remaining[index..index + 7]);
        output.push_str("[REDACTED]");

        let token_start = index + 7;
        let token_end = remaining[token_start..]
            .char_indices()
            .find(|(_, character)| character.is_whitespace())
            .map(|(offset, _)| token_start + offset)
            .unwrap_or(remaining.len());
        remaining = &remaining[token_end..];
    }

    output.push_str(remaining);
    output
}

#[cfg(not(windows))]
fn shell_escape(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

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
        let args = default_profile_args(ADAPTER_CLAUDE_CODE, Path::new("/tmp/project"));

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

        let profile = build_cli_profile(&agent, project_path, prompt_path, None)
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
        let agent = test_agent(
            "sh",
            ADAPTER_CLI,
            vec!["-c".to_string(), "exit 9".to_string()],
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
            "sh",
            ADAPTER_CLI,
            vec![
                "-c".to_string(),
                "printf 'Error: requires paid credits\\n' >&2; exit 1".to_string(),
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
