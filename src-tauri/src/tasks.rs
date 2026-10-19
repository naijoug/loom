use crate::{
    agents, attachments, command_runner,
    context_builder::{self, ContextBuildOptions, ContextBuildOutput},
    implementation_review,
    models::{
        now_ms, CommandRun, CommandRunIntent, CommandRunStatus, CreateTaskInput, ErrorSummary,
        FeedbackInput, IdGenerator, LoopTraceEntry, PlanTodoItem, PlanTodoStatus, PlanningDecision,
        PlanningDecisionInput, Task, TaskEvent, TaskLifecycleInput, TaskStatus, UserFeedback,
    },
    plan_html, project_git, project_preferences, pty, run_recovery, storage, task_repository,
    task_state::{transition, TaskAction},
    task_summary,
};
use std::{fs, path::Path};
use tauri::{AppHandle, State};

mod lifecycle;
pub(crate) mod testing;

pub(crate) use lifecycle::ensure_task_active;
use lifecycle::{
    apply_block_task, apply_cancel_task, apply_complete_task, apply_complete_todo,
    apply_mark_ready_for_testing, apply_pause_task, apply_resume_task, apply_start_todo,
};
use testing::compact_summary_line;
#[cfg(test)]
pub(crate) use testing::generate_repair_context;

#[tauri::command]
pub fn list_tasks(
    project_path: String,
    recovery: State<'_, run_recovery::RunRecoveryRegistry>,
) -> Result<Vec<Task>, String> {
    let tasks_dir = storage::project_tasks_dir(Path::new(&project_path));

    if !tasks_dir.exists() {
        return Ok(Vec::new());
    }

    let should_reconcile = recovery.begin_project(Path::new(&project_path))?;
    let mut tasks = Vec::new();
    for entry in
        fs::read_dir(tasks_dir).map_err(|error| format!("failed to read tasks: {error}"))?
    {
        let path = entry
            .map_err(|error| format!("failed to read task entry: {error}"))?
            .path();
        if path.extension().and_then(|value| value.to_str()) == Some("json") {
            let task_id = path
                .file_stem()
                .and_then(|value| value.to_str())
                .ok_or_else(|| format!("invalid task filename '{}'", path.display()))?;
            let task = if should_reconcile {
                update_task(Path::new(&project_path), task_id, |task| {
                    run_recovery::reconcile_task(task);
                    Ok(())
                })?
                .0
            } else {
                task_repository::load(Path::new(&project_path), task_id)?
            };
            tasks.push(task);
        }
    }
    tasks.sort_by_key(|task: &Task| task.created_at_ms);

    Ok(tasks)
}

#[tauri::command]
pub fn create_task(
    app: AppHandle,
    ids: State<'_, IdGenerator>,
    input: CreateTaskInput,
) -> Result<Task, String> {
    let timestamp_ms = now_ms();
    let task_id = ids.next("task");
    let event_id = ids.next("event");
    let preferences =
        project_preferences::load_normalized_for_app(&app, Path::new(&input.project_path))?;
    let selected_planning_agent_ids = if input.selected_planning_agent_ids.is_empty() {
        preferences.planning_agent_ids
    } else {
        input.selected_planning_agent_ids
    };
    let primary_agent_id = input
        .primary_agent_id
        .or(preferences.implementation_agent_id);
    let task = Task {
        id: task_id.clone(),
        project_path: input.project_path,
        title: input.title,
        raw_requirement: input.raw_requirement.clone(),
        status: TaskStatus::DraftingRequirements,
        lifecycle: Default::default(),
        selected_planning_agent_ids,
        primary_agent_id,
        review_agent_ids: preferences.review_agent_ids,
        implementation_review_runs: Vec::new(),
        implementation_reviews: Vec::new(),
        implementation_review_decisions: Vec::new(),
        final_plan: None,
        final_plan_path: None,
        final_plan_html_path: None,
        discussion_summary: None,
        planning_runs: Vec::new(),
        agent_invocations: Vec::new(),
        plan_reviews: Vec::new(),
        planning_decisions: Vec::new(),
        plan_todos: Vec::new(),
        loop_trace: Vec::new(),
        events: vec![TaskEvent {
            id: event_id,
            task_id: task_id.clone(),
            timestamp_ms,
            actor: "user".to_string(),
            status: TaskStatus::DraftingRequirements,
            input_summary: Some(input.raw_requirement),
            output_summary: Some("Task created".to_string()),
            evidence_ref: None,
        }],
        command_runs: Vec::new(),
        feedback: Vec::new(),
        loop_compact_summary: None,
        repair_context_preview: None,
        git_baseline: None,
        summary: None,
        created_at_ms: timestamp_ms,
        updated_at_ms: timestamp_ms,
    };
    save_task(&task)?;

    Ok(task)
}

#[tauri::command]
pub fn record_planning_decision(
    ids: State<'_, IdGenerator>,
    input: PlanningDecisionInput,
) -> Result<Task, String> {
    update_task(Path::new(&input.project_path), &input.task_id, |task| {
        ensure_task_active(task)?;
        let timestamp_ms = now_ms();
        let decision = PlanningDecision {
            id: ids.next("decision"),
            task_id: task.id.clone(),
            title: input.title.clone(),
            content: input.content.clone(),
            status: "accepted".to_string(),
            created_at_ms: timestamp_ms,
        };

        task.planning_decisions.push(decision);
        task.final_plan = task
            .final_plan
            .clone()
            .map(|plan| render_plan_with_human_decisions(&plan, &task.planning_decisions));
        if let (Some(path), Some(plan)) = (&task.final_plan_path, &task.final_plan) {
            storage::atomic_write_text(Path::new(path), plan)
                .map_err(|error| format!("failed to write decision to plan: {error}"))?;
            task.final_plan_html_path = plan_html::write_task_plan_html(task)?;
        }
        task.events.push(TaskEvent {
            id: ids.next("event"),
            task_id: task.id.clone(),
            timestamp_ms,
            actor: "user".to_string(),
            status: task.status,
            input_summary: Some(input.title.clone()),
            output_summary: Some(input.content.clone()),
            evidence_ref: task.final_plan_path.clone(),
        });
        task.updated_at_ms = timestamp_ms;
        Ok(())
    })
    .map(|(task, ())| task)
}

fn render_plan_with_human_decisions(plan: &str, decisions: &[PlanningDecision]) -> String {
    let base = plan
        .split("\n## Human Decisions\n")
        .next()
        .unwrap_or(plan)
        .trim_end();
    let decision_section = if decisions.is_empty() {
        "- No human decisions captured yet.".to_string()
    } else {
        decisions
            .iter()
            .map(|decision| format!("- **{}**: {}", decision.title, decision.content))
            .collect::<Vec<_>>()
            .join("\n")
    };

    format!("{base}\n\n## Human Decisions\n\n{decision_section}\n")
}

#[tauri::command]
pub fn confirm_plan(
    ids: State<'_, IdGenerator>,
    project_path: String,
    task_id: String,
) -> Result<Task, String> {
    update_task(Path::new(&project_path), &task_id, |task| {
        ensure_task_active(task)?;
        let final_plan = task
            .final_plan
            .clone()
            .ok_or_else(|| "cannot confirm plan before a final plan exists".to_string())?;
        let plan_ref = task.final_plan_path.clone().unwrap_or_else(|| {
            storage::project_plans_dir(Path::new(&project_path))
                .join(format!("{}-final-plan.md", task.id))
                .display()
                .to_string()
        });
        let plan_todos = derive_plan_todos(&ids, &task.id, &plan_ref, &final_plan);

        if plan_todos.is_empty() {
            return Err(
                "cannot confirm plan because it has no implementation todo items".to_string(),
            );
        }

        task.status = transition(task.status, TaskAction::PlanConfirmed)?;
        task.plan_todos = plan_todos;
        task.updated_at_ms = now_ms();
        task.events.push(TaskEvent {
            id: ids.next("event"),
            task_id: task.id.clone(),
            timestamp_ms: task.updated_at_ms,
            actor: "user".to_string(),
            status: task.status,
            input_summary: Some("Plan confirmed".to_string()),
            output_summary: Some(
                "Implementation todo items generated from the final plan.".to_string(),
            ),
            evidence_ref: task.final_plan_path.clone(),
        });
        Ok(())
    })
    .map(|(task, ())| task)
}

#[tauri::command]
pub fn start_todo(
    ids: State<'_, IdGenerator>,
    project_path: String,
    task_id: String,
    todo_id: String,
    primary_agent_id: Option<String>,
    primary_agent_switch_reason: Option<String>,
) -> Result<Task, String> {
    update_task(Path::new(&project_path), &task_id, |task| {
        ensure_task_active(task)?;
        if task.git_baseline.is_none() {
            task.git_baseline = Some(project_git::capture_git_baseline(Path::new(&project_path)));
        }
        apply_start_todo(
            task,
            &todo_id,
            primary_agent_id,
            primary_agent_switch_reason,
            ids.next("event"),
        )
    })
    .map(|(task, ())| task)
}

#[tauri::command]
pub fn complete_todo(
    ids: State<'_, IdGenerator>,
    project_path: String,
    task_id: String,
    todo_id: String,
) -> Result<Task, String> {
    update_task(Path::new(&project_path), &task_id, |task| {
        ensure_task_active(task)?;
        apply_complete_todo(task, &todo_id, ids.next("event"))
    })
    .map(|(task, ())| task)
}

#[tauri::command]
pub fn switch_primary_agent(
    app: AppHandle,
    ids: State<'_, IdGenerator>,
    project_path: String,
    task_id: String,
    agent_id: String,
    reason: String,
) -> Result<Task, String> {
    let reason = reason.trim();
    if reason.len() < 5 {
        return Err("switching the primary Agent requires a specific reason".to_string());
    }
    let agent = agents::load_agents(&app)?
        .into_iter()
        .find(|agent| agent.id == agent_id)
        .ok_or_else(|| "the selected primary Agent was not found".to_string())?;
    agents::validate_implementation_agent(&app, &agent.id, &agent.command)?;
    update_task(Path::new(&project_path), &task_id, |task| {
        ensure_task_active(task)?;
        if task.primary_agent_id.as_deref() == Some(agent.id.as_str()) {
            return Err("the selected Agent is already the primary Agent".to_string());
        }
        let previous = task
            .primary_agent_id
            .replace(agent.id.clone())
            .unwrap_or_else(|| "none".to_string());
        let timestamp_ms = now_ms();
        task.updated_at_ms = timestamp_ms;
        task.events.push(TaskEvent {
            id: ids.next("event"),
            task_id: task.id.clone(),
            timestamp_ms,
            actor: "user".to_string(),
            status: task.status,
            input_summary: Some(format!(
                "Switched primary Agent from {previous} to {}",
                agent.id
            )),
            output_summary: Some(reason.to_string()),
            evidence_ref: None,
        });
        Ok(())
    })
    .map(|(task, ())| task)
}

#[tauri::command]
pub fn build_implementation_context(
    project_path: String,
    task_id: String,
    todo_id: String,
    options: Option<ContextBuildOptions>,
) -> Result<ContextBuildOutput, String> {
    let task = load_task(Path::new(&project_path), &task_id)?;
    context_builder::build_implementation_context(&task, &todo_id, options.unwrap_or_default())
}

#[tauri::command]
pub fn mark_ready_for_testing(
    ids: State<'_, IdGenerator>,
    project_path: String,
    task_id: String,
) -> Result<Task, String> {
    update_task(Path::new(&project_path), &task_id, |task| {
        ensure_task_active(task)?;
        apply_mark_ready_for_testing(task, ids.next("event"))
    })
    .map(|(task, ())| task)
}

#[tauri::command]
pub fn complete_task(
    ids: State<'_, IdGenerator>,
    project_path: String,
    task_id: String,
) -> Result<Task, String> {
    complete_task_inner(ids.inner(), project_path, task_id)
}

pub(crate) fn complete_task_inner(
    ids: &IdGenerator,
    project_path: String,
    task_id: String,
) -> Result<Task, String> {
    update_task(Path::new(&project_path), &task_id, |task| {
        ensure_task_active(task)?;
        apply_complete_task(task, ids.next("event"))?;
        let summary = task_summary::generate_and_persist(task)?;
        task.events.push(TaskEvent {
            id: ids.next("event"),
            task_id: task.id.clone(),
            timestamp_ms: summary.generated_at_ms,
            actor: "system".to_string(),
            status: task.status,
            input_summary: Some("Generated delivery summary".to_string()),
            output_summary: Some("JSON and Markdown delivery artifacts persisted".to_string()),
            evidence_ref: Some(summary.markdown_path.clone()),
        });
        task.summary = Some(summary);
        Ok(())
    })
    .map(|(task, ())| task)
}

#[tauri::command]
pub fn delete_task(project_path: String, task_id: String) -> Result<(), String> {
    let project = Path::new(&project_path);
    task_repository::delete(project, &task_id)?;

    // Best-effort cleanup of this task's planning evidence directory.
    let evidence_dir = storage::project_loom_dir(project)
        .join("planning")
        .join(&task_id);
    if evidence_dir.exists() {
        let _ = std::fs::remove_dir_all(&evidence_dir);
    }

    Ok(())
}

#[tauri::command]
pub async fn pause_task(
    ids: State<'_, IdGenerator>,
    commands: State<'_, command_runner::CommandRegistry>,
    ptys: State<'_, pty::PtyRegistry>,
    input: TaskLifecycleInput,
) -> Result<Task, String> {
    let project_path = Path::new(&input.project_path);
    let task = load_task(project_path, &input.task_id)?;
    ensure_task_active(&task)?;
    agents::stop_task_runs(&input.task_id)?;
    implementation_review::stop_task_runs(&input.task_id)?;
    command_runner::stop_task_runs(commands.inner(), &input.task_id, "task_paused").await?;
    pty::stop_task_runs(ptys.inner(), &input.task_id, "task_paused")?;

    update_task(project_path, &input.task_id, |task| {
        apply_pause_task(task, ids.next("event"), input.reason)
    })
    .map(|(task, ())| task)
}

#[tauri::command]
pub fn resume_task(ids: State<'_, IdGenerator>, input: TaskLifecycleInput) -> Result<Task, String> {
    update_task(Path::new(&input.project_path), &input.task_id, |task| {
        apply_resume_task(task, ids.next("event"), input.reason)
    })
    .map(|(task, ())| task)
}

#[tauri::command]
pub async fn block_task(
    ids: State<'_, IdGenerator>,
    commands: State<'_, command_runner::CommandRegistry>,
    ptys: State<'_, pty::PtyRegistry>,
    input: TaskLifecycleInput,
) -> Result<Task, String> {
    let project_path = Path::new(&input.project_path);
    let task = load_task(project_path, &input.task_id)?;
    ensure_task_active(&task)?;
    agents::stop_task_runs(&input.task_id)?;
    implementation_review::stop_task_runs(&input.task_id)?;
    command_runner::stop_task_runs(commands.inner(), &input.task_id, "task_blocked").await?;
    pty::stop_task_runs(ptys.inner(), &input.task_id, "task_blocked")?;

    update_task(project_path, &input.task_id, |task| {
        apply_block_task(task, ids.next("event"), input.reason)
    })
    .map(|(task, ())| task)
}

#[tauri::command]
pub async fn cancel_task(
    ids: State<'_, IdGenerator>,
    commands: State<'_, command_runner::CommandRegistry>,
    ptys: State<'_, pty::PtyRegistry>,
    input: TaskLifecycleInput,
) -> Result<Task, String> {
    let project_path = Path::new(&input.project_path);
    let task = load_task(project_path, &input.task_id)?;
    if task.status.is_terminal() {
        return Err(format!(
            "cannot cancel terminal task status {}",
            task.status
        ));
    }
    agents::stop_task_runs(&input.task_id)?;
    implementation_review::stop_task_runs(&input.task_id)?;
    command_runner::stop_task_runs(commands.inner(), &input.task_id, "task_cancelled").await?;
    pty::stop_task_runs(ptys.inner(), &input.task_id, "task_cancelled")?;

    update_task(project_path, &input.task_id, |task| {
        apply_cancel_task(task, ids.next("event"), input.reason)
    })
    .map(|(task, ())| task)
}

pub fn load_task(project_path: &Path, task_id: &str) -> Result<Task, String> {
    task_repository::load(project_path, task_id)
}

pub fn save_task(task: &Task) -> Result<(), String> {
    task_repository::save(task)
}

pub(crate) fn update_task<T>(
    project_path: &Path,
    task_id: &str,
    update: impl FnOnce(&mut Task) -> Result<T, String>,
) -> Result<(Task, T), String> {
    task_repository::update(project_path, task_id, update)
}

pub fn add_command_run(project_path: &Path, task_id: &str, run: CommandRun) -> Result<(), String> {
    update_task(project_path, task_id, move |task| {
        ensure_task_active(task)?;
        apply_command_start_status(task, &run)?;
        task.command_runs.push(run.clone());
        if run.loop_id.is_some() {
            task.loop_trace
                .push(command_loop_trace_entry(&task.id, &run, "command_started"));
        }
        let timestamp_ms = now_ms();
        task.events.push(TaskEvent {
            id: format!("event-{timestamp_ms}-command-start-{}", run.id),
            task_id: task.id.clone(),
            timestamp_ms,
            actor: "system".to_string(),
            status: task.status,
            input_summary: Some(run.command),
            output_summary: Some("Command started".to_string()),
            evidence_ref: Some(run.id),
        });
        task.updated_at_ms = timestamp_ms;
        Ok(())
    })
    .map(|_| ())
}

pub struct CommandRunCompletion {
    pub status: CommandRunStatus,
    pub exit_code: Option<i32>,
    pub error_summary: Option<ErrorSummary>,
    pub session_id: Option<String>,
    pub resume_command: Option<String>,
    pub termination_reason: Option<String>,
}

pub fn finish_command_run(
    project_path: &Path,
    task_id: &str,
    run_id: &str,
    completion: CommandRunCompletion,
) -> Result<(), String> {
    let run_id = run_id.to_string();
    update_task(project_path, task_id, move |task| {
        let mut command_text = None;
        let mut finished_run = None;
        if let Some(run) = task.command_runs.iter_mut().find(|run| run.id == run_id) {
            run.status = completion.status;
            run.exit_code = completion.exit_code;
            run.ended_at_ms = Some(now_ms());
            run.error_summary = completion.error_summary;
            run.session_id = completion.session_id;
            run.resume_command = completion.resume_command;
            if let Some(reason) = completion.termination_reason {
                run.termination_reason = Some(reason);
            }
            if completion.status == "cancelled" && run.termination_reason.is_none() {
                run.termination_reason = Some("cancelled".to_string());
            }
            command_text = Some(run.command.clone());
            finished_run = Some(run.clone());
        }
        if let Some(run) = finished_run.as_ref() {
            apply_command_finish_status(task, run, completion.status)?;
        }
        if let Some(run) = finished_run.as_ref().filter(|run| run.loop_id.is_some()) {
            task.loop_trace
                .push(command_loop_trace_entry(&task.id, run, "command_finished"));
        }
        let timestamp_ms = now_ms();
        task.events.push(TaskEvent {
            id: format!("event-{timestamp_ms}-command-end-{run_id}"),
            task_id: task.id.clone(),
            timestamp_ms,
            actor: "system".to_string(),
            status: task.status,
            input_summary: command_text,
            output_summary: Some(format!("Command {}", completion.status)),
            evidence_ref: Some(run_id),
        });
        task.updated_at_ms = timestamp_ms;
        Ok(())
    })
    .map(|_| ())
}

fn command_loop_trace_entry(task_id: &str, run: &CommandRun, entry_type: &str) -> LoopTraceEntry {
    let timestamp_ms = now_ms();
    let fingerprint = run.error_summary.as_ref().and_then(error_fingerprint);
    LoopTraceEntry {
        id: format!("trace-{timestamp_ms}-{entry_type}-{}", run.id),
        task_id: task_id.to_string(),
        loop_id: run.loop_id.clone().unwrap_or_default(),
        stage: command_trace_stage(&run.intent).to_string(),
        entry_type: entry_type.to_string(),
        iteration: run.iteration,
        attempt: run.attempt,
        context_summary: format!(
            "loop={} iteration={} attempt={}",
            run.loop_id.as_deref().unwrap_or("(none)"),
            run.iteration
                .map(|value| value.to_string())
                .unwrap_or_else(|| "(none)".to_string()),
            run.attempt
                .map(|value| value.to_string())
                .unwrap_or_else(|| "(none)".to_string())
        ),
        action_summary: run.command.clone(),
        verification_summary: command_trace_verification(run),
        command_run_id: Some(run.id.clone()),
        fingerprint,
        termination_reason: run
            .termination_reason
            .clone()
            .or_else(|| run.ended_at_ms.map(|_| run.status.to_string())),
        token_usage: None,
        timestamp_ms,
    }
}

fn command_trace_stage(intent: &CommandRunIntent) -> &'static str {
    match intent {
        CommandRunIntent::AgentAction => "implement",
        CommandRunIntent::Validation => "testing",
        CommandRunIntent::Preview => "preview",
        CommandRunIntent::LoopStep => "loop",
        CommandRunIntent::Legacy => "legacy",
    }
}

fn command_trace_verification(run: &CommandRun) -> String {
    match run.status.as_str() {
        "running" => "Command started; awaiting process exit.".to_string(),
        status => format!(
            "Command {status}; exitCode={}; error={}",
            run.exit_code
                .map(|value| value.to_string())
                .unwrap_or_else(|| "(none)".to_string()),
            run.error_summary
                .as_ref()
                .and_then(|summary| summary
                    .matched_lines
                    .first()
                    .or_else(|| summary.stderr_tail.last()))
                .map(String::as_str)
                .unwrap_or("(none)")
        ),
    }
}

fn error_fingerprint(summary: &ErrorSummary) -> Option<String> {
    let evidence = if summary.matched_lines.is_empty() {
        &summary.stderr_tail
    } else {
        &summary.matched_lines
    };
    let compacted = evidence
        .iter()
        .map(|line| line.trim().to_ascii_lowercase())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    if compacted.is_empty() {
        None
    } else {
        Some(compact_summary_line(&compacted, 600))
    }
}

fn is_implementation_loop_validation(task: &Task, run: &CommandRun) -> bool {
    run.intent == CommandRunIntent::Validation
        && run.loop_id.is_some()
        && matches!(task.status.as_str(), "implementing" | "reviewing")
}

fn apply_command_start_status(task: &mut Task, run: &CommandRun) -> Result<(), String> {
    if is_implementation_loop_validation(task, run) {
        return Ok(());
    }

    if matches!(
        &run.intent,
        CommandRunIntent::Validation | CommandRunIntent::Legacy
    ) {
        task.status = transition(task.status, TaskAction::ValidationStarted)?;
    }

    Ok(())
}

fn apply_command_finish_status(
    task: &mut Task,
    run: &CommandRun,
    status: CommandRunStatus,
) -> Result<(), String> {
    if is_implementation_loop_validation(task, run) {
        if status == "succeeded" {
            task.status = transition(task.status, TaskAction::ValidationPassed)?;
        }
        return Ok(());
    }

    if matches!(
        &run.intent,
        CommandRunIntent::Validation | CommandRunIntent::Legacy
    ) {
        task.status = if status == "succeeded" {
            transition(task.status, TaskAction::ValidationPassed)?
        } else {
            transition(task.status, TaskAction::ValidationFailed)?
        };
    }

    Ok(())
}

fn format_failed_run_context(run: &CommandRun) -> String {
    let summary = run
        .error_summary
        .clone()
        .map(format_error_summary)
        .unwrap_or_else(|| "(none)".to_string());
    format!(
        "runId={}\ncommand={}\nstatus={}\nexitCode={:?}\nstdoutLog={}\nstderrLog={}\n{}",
        run.id,
        run.command,
        run.status,
        run.exit_code,
        run.stdout_log_ref.as_deref().unwrap_or("(none)"),
        run.stderr_log_ref.as_deref().unwrap_or("(none)"),
        summary
    )
}

fn format_current_todo_context(todos: &[PlanTodoItem]) -> String {
    if let Some(todo) = todos.iter().find(|todo| todo.status == "implementing") {
        return format!(
            "id={}\ntitle={}\nstatus={}\nplanRef={}",
            todo.id,
            todo.title,
            todo.status,
            todo.plan_ref.as_deref().unwrap_or("(none)")
        );
    }

    if let Some(todo) = todos.iter().find(|todo| todo.status == "pending") {
        return format!(
            "No todo is currently marked implementing. Next pending todo:\nid={}\ntitle={}\nstatus={}\nplanRef={}",
            todo.id,
            todo.title,
            todo.status,
            todo.plan_ref.as_deref().unwrap_or("(none)")
        );
    }

    "(none)".to_string()
}

fn format_error_summary(summary: ErrorSummary) -> String {
    format!(
        "exitCode={:?}\nmatchedLines={}\nstderrTail={}",
        summary.exit_code,
        summary.matched_lines.join("\n"),
        summary.stderr_tail.join("\n")
    )
}

fn derive_plan_todos(
    ids: &State<'_, IdGenerator>,
    task_id: &str,
    plan_ref: &str,
    final_plan: &str,
) -> Vec<PlanTodoItem> {
    implementation_todo_lines(final_plan)
        .into_iter()
        .take(12)
        .enumerate()
        .map(|(index, title)| PlanTodoItem {
            id: ids.next("todo"),
            task_id: task_id.to_string(),
            description: format!("From final plan: {title}"),
            title,
            status: PlanTodoStatus::Pending,
            order: index as u32,
            plan_ref: Some(plan_ref.to_string()),
        })
        .collect()
}

fn implementation_todo_lines(final_plan: &str) -> Vec<String> {
    let mut in_section = false;
    let mut section_level: Option<usize> = None;
    let mut ignored_child_level: Option<usize> = None;
    let mut todos = Vec::new();

    for line in final_plan.lines() {
        let trimmed = line.trim();
        if let Some(level) = markdown_heading_level(trimmed) {
            if in_section && section_level.is_some_and(|section_level| level > section_level) {
                if ignored_child_level.is_some_and(|ignored_level| level <= ignored_level) {
                    ignored_child_level = None;
                }
                if ignored_child_level.is_none() && is_non_todo_child_heading(trimmed) {
                    ignored_child_level = Some(level);
                }
                continue;
            }

            in_section = is_implementation_todo_heading(trimmed);
            section_level = in_section.then_some(level);
            ignored_child_level = None;
            continue;
        }

        if !in_section || ignored_child_level.is_some() {
            continue;
        }

        if let Some(todo) = strip_todo_marker(trimmed) {
            if !todo.is_empty()
                && !todo
                    .to_lowercase()
                    .starts_with("no implementation todo items")
            {
                todos.push(todo.to_string());
            }
        }
    }

    todos
}

fn markdown_heading_level(line: &str) -> Option<usize> {
    let marker_count = line.chars().take_while(|char| *char == '#').count();
    if (2..=6).contains(&marker_count) && line.as_bytes().get(marker_count) == Some(&b' ') {
        Some(marker_count)
    } else {
        None
    }
}

fn is_implementation_todo_heading(line: &str) -> bool {
    let heading = normalize_plan_heading(line);

    let normalized_heading = heading
        .split(['/', '／', '-', '—', '（', '(', '：', ':'])
        .map(str::trim)
        .find(|part| !part.is_empty())
        .unwrap_or(heading.as_str());

    is_known_implementation_todo_heading(normalized_heading)
        || (heading.contains("implementation")
            && (heading.contains("todo")
                || heading.contains("task")
                || heading.contains("checklist")
                || heading.contains("plan")
                || heading.contains("milestone")))
        || (heading.contains("任务")
            && (heading.contains("实施")
                || heading.contains("实现")
                || heading.contains("拆解")
                || heading.contains("具体")))
        || (heading.contains("里程碑") && (heading.contains("实施") || heading.contains("实现")))
}

fn normalize_plan_heading(line: &str) -> String {
    let heading = line
        .trim_start_matches('#')
        .trim()
        .trim_end_matches(':')
        .trim();
    let heading = heading
        .trim_matches(|char| matches!(char, '*' | '_' | '`'))
        .trim_start_matches(|char: char| {
            !char.is_alphanumeric() && !matches!(char, '\u{4e00}'..='\u{9fff}')
        })
        .trim()
        .trim_matches(|char| matches!(char, '*' | '_' | '`'));

    heading.to_lowercase()
}

fn is_known_implementation_todo_heading(heading: &str) -> bool {
    matches!(
        heading,
        "implementation todo"
            | "implementation todos"
            | "implementation tasks"
            | "tasks"
            | "todo"
            | "todos"
            | "具体任务"
            | "实施任务"
            | "任务拆解"
            | "实现任务"
            | "implementation task breakdown"
            | "implementation steps"
            | "implementation step"
            | "implementation plan"
            | "implementation milestone"
            | "implementation milestones"
            | "milestone"
            | "milestones"
            | "action items"
            | "action item"
            | "next steps"
            | "next step"
            | "实施步骤"
            | "执行步骤"
            | "实施里程碑"
            | "实现里程碑"
            | "里程碑"
            | "行动项"
            | "下一步"
    )
}

fn is_non_todo_child_heading(line: &str) -> bool {
    matches!(
        normalize_plan_heading(line).as_str(),
        "note" | "notes" | "备注" | "说明" | "acceptance criteria" | "验收标准" | "验证策略"
    )
}

fn strip_todo_marker(line: &str) -> Option<&str> {
    let bullet = line
        .strip_prefix("- ")
        .or_else(|| line.strip_prefix("* "))
        .or_else(|| line.strip_prefix("+ "));
    if let Some(value) = bullet {
        return Some(strip_checkbox_marker(value.trim()));
    }

    for separator in [". ", ") ", "、", "．"] {
        if let Some((number, rest)) = line.split_once(separator) {
            if number.chars().all(|char| char.is_ascii_digit()) {
                return Some(strip_checkbox_marker(rest.trim()));
            }
        }
    }

    None
}

fn strip_checkbox_marker(line: &str) -> &str {
    ["[ ] ", "[x] ", "[X] "]
        .iter()
        .find_map(|marker| line.strip_prefix(marker))
        .unwrap_or(line)
        .trim()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_implementation_todos_from_final_plan() {
        let plan = "# Plan\n\n## Implementation Todo\n\n1. Wire planning review\n2. Persist todos\n\n## Acceptance Criteria\n\n- Done";

        assert_eq!(
            implementation_todo_lines(plan),
            vec![
                "Wire planning review".to_string(),
                "Persist todos".to_string()
            ]
        );
    }

    #[test]
    fn extracts_implementation_todos_from_markdown_checklists_and_parentheses() {
        let plan = "# Plan\n\n## Implementation Todo\n\n- [ ] Review selected Agent evidence\n2) [x] Confirm handoff todos\n* Persist review decision\n\n## Acceptance Criteria\n\n- Done";

        assert_eq!(
            implementation_todo_lines(plan),
            vec![
                "Review selected Agent evidence".to_string(),
                "Confirm handoff todos".to_string(),
                "Persist review decision".to_string()
            ]
        );
    }

    #[test]
    fn extracts_implementation_todos_from_agent_bullet_variants() {
        let plan = "# Plan\n\n## Implementation Todo\n\n+ Normalize plus bullets\n1、支持中文编号\n2．Handle fullwidth ordered lists\n\n## Acceptance Criteria\n\n- Done";

        assert_eq!(
            implementation_todo_lines(plan),
            vec![
                "Normalize plus bullets".to_string(),
                "支持中文编号".to_string(),
                "Handle fullwidth ordered lists".to_string()
            ]
        );
    }

    #[test]
    fn extracts_implementation_todos_from_chinese_task_sections() {
        let plan = "# 计划\n\n## 具体任务\n\n- [ ] 生成最终计划\n2) 调用主 Agent 实施\n\n## 验证策略\n\n- cargo test";

        assert_eq!(
            implementation_todo_lines(plan),
            vec!["生成最终计划".to_string(), "调用主 Agent 实施".to_string()]
        );
    }

    #[test]
    fn extracts_implementation_todos_from_subheadings() {
        let plan = "# Plan\n\n### Implementation Tasks\n\n- Run Agent review\n- Verify evidence\n\n#### Notes\n\n- This note is not an implementation todo";

        assert_eq!(
            implementation_todo_lines(plan),
            vec![
                "Run Agent review".to_string(),
                "Verify evidence".to_string()
            ]
        );
    }

    #[test]
    fn extracts_implementation_todos_from_bilingual_and_descriptive_headings() {
        let bilingual_plan = "# 计划\n\n## 具体任务（Implementation Todo）\n\n- 记录 Review 证据\n- 运行最小验证\n\n## 风险\n\n- 无";
        let descriptive_plan = "# Plan\n\n## Implementation Checklist - handoff\n\n- Capture selected Agent evidence\n- Confirm validation command\n\n## Notes\n\n- not a todo";

        assert_eq!(
            implementation_todo_lines(bilingual_plan),
            vec!["记录 Review 证据".to_string(), "运行最小验证".to_string()]
        );
        assert_eq!(
            implementation_todo_lines(descriptive_plan),
            vec![
                "Capture selected Agent evidence".to_string(),
                "Confirm validation command".to_string()
            ]
        );
    }

    #[test]
    fn keeps_collecting_todos_inside_child_headings() {
        let plan = "# Plan\n\n## Implementation Todo\n\n### Backend\n\n- Persist review evidence\n\n### Frontend\n\n- Show review handoff state\n\n## Acceptance Criteria\n\n- Not a todo";

        assert_eq!(
            implementation_todo_lines(plan),
            vec![
                "Persist review evidence".to_string(),
                "Show review handoff state".to_string()
            ]
        );
    }

    #[test]
    fn keeps_collecting_todos_inside_child_headings_for_subsection_todo_sections() {
        let plan = "# Plan\n\n### Implementation Tasks\n\n#### Backend\n\n- Persist command evidence\n\n#### Frontend\n\n- Display repair handoff\n\n### Acceptance Criteria\n\n- Not a todo";

        assert_eq!(
            implementation_todo_lines(plan),
            vec![
                "Persist command evidence".to_string(),
                "Display repair handoff".to_string()
            ]
        );
    }

    #[test]
    fn extracts_implementation_todos_from_formatted_headings() {
        let emoji_plan = "# Plan\n\n## ✅ **Implementation Todo**\n\n- Capture review evidence\n- Run validation\n\n## Notes\n\n- not a todo";
        let backtick_plan = "# Plan\n\n## `Implementation Tasks`: \n\n- Confirm handoff\n\n## Acceptance Criteria\n\n- not a todo";

        assert_eq!(
            implementation_todo_lines(emoji_plan),
            vec![
                "Capture review evidence".to_string(),
                "Run validation".to_string()
            ]
        );
        assert_eq!(
            implementation_todo_lines(backtick_plan),
            vec!["Confirm handoff".to_string()]
        );
    }

    #[test]
    fn extracts_implementation_todos_from_steps_and_action_items_headings() {
        let steps_plan = "# Plan\n\n## Implementation Steps\n\n1. Select primary Agent\n2. Run scoped verification\n\n## Notes\n\n- not a todo";
        let action_items_plan = "# 计划\n\n## 行动项\n\n- 保存 Review 结果\n- 记录验收证据\n\n## 验证策略\n\n- cargo test";

        assert_eq!(
            implementation_todo_lines(steps_plan),
            vec![
                "Select primary Agent".to_string(),
                "Run scoped verification".to_string()
            ]
        );
        assert_eq!(
            implementation_todo_lines(action_items_plan),
            vec!["保存 Review 结果".to_string(), "记录验收证据".to_string()]
        );
    }

    #[test]
    fn extracts_implementation_todos_from_plan_and_milestone_headings() {
        let implementation_plan = "# Plan\n\n## Implementation Plan\n\n- Persist selected todo evidence\n- Request review handoff\n\n## Acceptance Criteria\n\n- not a todo";
        let implementation_milestones = "# 计划\n\n## 实施里程碑\n\n1. 生成任务拆解\n2. 运行最小验证\n\n## 验证策略\n\n- cargo test";
        let standalone_milestones = "# Plan\n\n## Milestones\n\n- Build review handoff\n- Verify debug loop\n\n## Risks\n\n- Keep scope small";
        let chinese_standalone_milestones =
            "# 计划\n\n## 里程碑\n\n- 串联 Agent 编排\n- 保存验收证据\n\n## 风险\n\n- 避免过度实现";

        assert_eq!(
            implementation_todo_lines(implementation_plan),
            vec![
                "Persist selected todo evidence".to_string(),
                "Request review handoff".to_string()
            ]
        );
        assert_eq!(
            implementation_todo_lines(implementation_milestones),
            vec!["生成任务拆解".to_string(), "运行最小验证".to_string()]
        );
        assert_eq!(
            implementation_todo_lines(standalone_milestones),
            vec![
                "Build review handoff".to_string(),
                "Verify debug loop".to_string()
            ]
        );
        assert_eq!(
            implementation_todo_lines(chinese_standalone_milestones),
            vec!["串联 Agent 编排".to_string(), "保存验收证据".to_string()]
        );
    }

    #[test]
    fn start_todo_persists_scope_and_resets_previous_implementing_item() {
        let mut task = Task {
            id: "task-1".to_string(),
            project_path: "/repo".to_string(),
            title: "Ship scoped implementation".to_string(),
            raw_requirement: "Implement one todo at a time".to_string(),
            status: TaskStatus::ReadyToImplement,
            lifecycle: Default::default(),
            selected_planning_agent_ids: Vec::new(),
            primary_agent_id: None,
            review_agent_ids: Vec::new(),
            implementation_review_runs: Vec::new(),
            implementation_reviews: Vec::new(),
            implementation_review_decisions: Vec::new(),
            final_plan: Some("# Plan".to_string()),
            final_plan_path: Some("/repo/docs/plans/plan.md".to_string()),
            final_plan_html_path: None,
            discussion_summary: None,
            planning_runs: Vec::new(),
            agent_invocations: Vec::new(),
            plan_reviews: Vec::new(),
            planning_decisions: Vec::new(),
            plan_todos: vec![
                PlanTodoItem {
                    id: "todo-1".to_string(),
                    task_id: "task-1".to_string(),
                    title: "Old scope".to_string(),
                    description: "Old scope".to_string(),
                    status: PlanTodoStatus::Implementing,
                    order: 0,
                    plan_ref: None,
                },
                PlanTodoItem {
                    id: "todo-2".to_string(),
                    task_id: "task-1".to_string(),
                    title: "New scope".to_string(),
                    description: "New scope".to_string(),
                    status: PlanTodoStatus::Pending,
                    order: 1,
                    plan_ref: None,
                },
            ],
            loop_trace: Vec::new(),
            events: Vec::new(),
            command_runs: Vec::new(),
            feedback: Vec::new(),
            loop_compact_summary: None,
            repair_context_preview: None,
            git_baseline: None,
            summary: None,
            created_at_ms: 1,
            updated_at_ms: 1,
        };

        apply_start_todo(
            &mut task,
            "todo-2",
            Some("agent-claude".to_string()),
            None,
            "event-1".to_string(),
        )
        .unwrap();

        assert_eq!(task.status, "implementing");
        assert_eq!(task.primary_agent_id.as_deref(), Some("agent-claude"));
        assert_eq!(task.plan_todos[0].status, "pending");
        assert_eq!(task.plan_todos[1].status, "implementing");
        assert_eq!(task.events.len(), 1);
        assert_eq!(task.events[0].status, "implementing");
        assert_eq!(
            task.events[0].evidence_ref.as_deref(),
            Some("/repo/docs/plans/plan.md")
        );
        assert!(task.events[0]
            .input_summary
            .as_deref()
            .unwrap_or_default()
            .contains("New scope"));
    }

    #[test]
    fn complete_todo_marks_item_done_and_moves_all_done_tasks_to_review() {
        let mut task = Task {
            id: "task-1".to_string(),
            project_path: "/repo".to_string(),
            title: "Ship scoped implementation".to_string(),
            raw_requirement: "Implement one todo at a time".to_string(),
            status: TaskStatus::Implementing,
            lifecycle: Default::default(),
            selected_planning_agent_ids: Vec::new(),
            primary_agent_id: None,
            review_agent_ids: Vec::new(),
            implementation_review_runs: Vec::new(),
            implementation_reviews: Vec::new(),
            implementation_review_decisions: Vec::new(),
            final_plan: Some("# Plan".to_string()),
            final_plan_path: Some("/repo/docs/plans/plan.md".to_string()),
            final_plan_html_path: None,
            discussion_summary: None,
            planning_runs: Vec::new(),
            agent_invocations: Vec::new(),
            plan_reviews: Vec::new(),
            planning_decisions: Vec::new(),
            plan_todos: vec![PlanTodoItem {
                id: "todo-1".to_string(),
                task_id: "task-1".to_string(),
                title: "Verified scope".to_string(),
                description: "Verified scope".to_string(),
                status: PlanTodoStatus::Implementing,
                order: 0,
                plan_ref: None,
            }],
            loop_trace: Vec::new(),
            events: Vec::new(),
            command_runs: Vec::new(),
            feedback: Vec::new(),
            loop_compact_summary: None,
            repair_context_preview: None,
            git_baseline: None,
            summary: None,
            created_at_ms: 1,
            updated_at_ms: 1,
        };

        apply_complete_todo(&mut task, "todo-1", "event-1".to_string()).unwrap();

        assert_eq!(task.status, "reviewing");
        assert_eq!(task.plan_todos[0].status, "done");
        assert_eq!(task.events.len(), 1);
        assert_eq!(task.events[0].status, "reviewing");
        assert!(task.events[0]
            .input_summary
            .as_deref()
            .unwrap_or_default()
            .contains("Verified scope"));
    }

    #[test]
    fn complete_todo_rejects_pending_scope_without_mutating_task() {
        let mut task = Task {
            id: "task-1".to_string(),
            project_path: "/repo".to_string(),
            title: "Ship scoped implementation".to_string(),
            raw_requirement: "Implement one todo at a time".to_string(),
            status: TaskStatus::ReadyToImplement,
            lifecycle: Default::default(),
            selected_planning_agent_ids: Vec::new(),
            primary_agent_id: None,
            review_agent_ids: Vec::new(),
            implementation_review_runs: Vec::new(),
            implementation_reviews: Vec::new(),
            implementation_review_decisions: Vec::new(),
            final_plan: Some("# Plan".to_string()),
            final_plan_path: Some("/repo/docs/plans/plan.md".to_string()),
            final_plan_html_path: None,
            discussion_summary: None,
            planning_runs: Vec::new(),
            agent_invocations: Vec::new(),
            plan_reviews: Vec::new(),
            planning_decisions: Vec::new(),
            plan_todos: vec![PlanTodoItem {
                id: "todo-1".to_string(),
                task_id: "task-1".to_string(),
                title: "Pending scope".to_string(),
                description: "Pending scope".to_string(),
                status: PlanTodoStatus::Pending,
                order: 0,
                plan_ref: None,
            }],
            loop_trace: Vec::new(),
            events: Vec::new(),
            command_runs: Vec::new(),
            feedback: Vec::new(),
            loop_compact_summary: None,
            repair_context_preview: None,
            git_baseline: None,
            summary: None,
            created_at_ms: 1,
            updated_at_ms: 1,
        };

        let error = apply_complete_todo(&mut task, "todo-1", "event-1".to_string()).unwrap_err();

        assert!(error.contains("instead of implementing"));
        assert_eq!(task.status, "ready_to_implement");
        assert_eq!(task.plan_todos[0].status, "pending");
        assert!(task.events.is_empty());
    }

    fn transition_task_fixture(
        status: &str,
        todo_statuses: &[&str],
        command_runs: Vec<CommandRun>,
    ) -> Task {
        Task {
            id: "task-1".to_string(),
            project_path: "/repo".to_string(),
            title: "Ship validation loop".to_string(),
            raw_requirement: "Complete implementation and validation".to_string(),
            status: status.parse().expect("known task status fixture"),
            lifecycle: Default::default(),
            selected_planning_agent_ids: Vec::new(),
            primary_agent_id: None,
            review_agent_ids: Vec::new(),
            implementation_review_runs: Vec::new(),
            implementation_reviews: Vec::new(),
            implementation_review_decisions: Vec::new(),
            final_plan: Some("# Plan".to_string()),
            final_plan_path: Some("/repo/docs/plans/plan.md".to_string()),
            final_plan_html_path: None,
            discussion_summary: None,
            planning_runs: Vec::new(),
            agent_invocations: Vec::new(),
            plan_reviews: Vec::new(),
            planning_decisions: Vec::new(),
            plan_todos: todo_statuses
                .iter()
                .enumerate()
                .map(|(index, todo_status)| PlanTodoItem {
                    id: format!("todo-{index}"),
                    task_id: "task-1".to_string(),
                    title: format!("Todo {index}"),
                    description: format!("Todo {index}"),
                    status: todo_status.parse().expect("known todo status fixture"),
                    order: index as u32,
                    plan_ref: None,
                })
                .collect(),
            loop_trace: Vec::new(),
            events: Vec::new(),
            command_runs,
            feedback: Vec::new(),
            loop_compact_summary: None,
            repair_context_preview: None,
            git_baseline: None,
            summary: None,
            created_at_ms: 1,
            updated_at_ms: 1,
        }
    }

    fn transition_command_run(intent: CommandRunIntent, loop_id: Option<String>) -> CommandRun {
        CommandRun {
            id: "run-1".to_string(),
            task_id: "task-1".to_string(),
            command: "pnpm test".to_string(),
            cwd: "/repo".to_string(),
            intent,
            loop_id,
            iteration: None,
            attempt: None,
            termination_reason: None,
            session_id: None,
            resume_command: None,
            started_at_ms: 1,
            ended_at_ms: None,
            status: CommandRunStatus::Running,
            exit_code: None,
            stdout_log_ref: None,
            stderr_log_ref: None,
            error_summary: None,
        }
    }

    fn attach_passing_implementation_review(task: &mut Task) {
        task.primary_agent_id = Some("builder".to_string());
        task.implementation_review_runs
            .push(crate::models::ImplementationReviewRun {
                id: "review-run-1".to_string(),
                task_id: task.id.clone(),
                reviewer_agent_ids: vec!["reviewer".to_string()],
                status: "succeeded".to_string(),
                context_ref: "/repo/.loom/review-context.md".to_string(),
                review_ids: vec!["review-1".to_string()],
                started_at_ms: 1,
                ended_at_ms: Some(2),
            });
        task.implementation_reviews
            .push(crate::models::ImplementationReview {
                id: "review-1".to_string(),
                run_id: "review-run-1".to_string(),
                task_id: task.id.clone(),
                reviewer_agent_id: "reviewer".to_string(),
                reviewer_agent_name: "Reviewer".to_string(),
                status: "succeeded".to_string(),
                summary: "Looks good".to_string(),
                raw_output: "{}".to_string(),
                evidence_ref: None,
                stderr_ref: None,
                exit_code: Some(0),
                failure_detail: None,
                findings: Vec::new(),
                started_at_ms: 1,
                ended_at_ms: Some(2),
            });
    }

    #[test]
    fn command_run_old_json_defaults_to_legacy_intent() {
        let run: CommandRun = serde_json::from_value(serde_json::json!({
            "id": "run-1",
            "taskId": "task-1",
            "command": "pnpm test",
            "cwd": "/repo",
            "startedAtMs": 1,
            "endedAtMs": null,
            "status": "succeeded",
            "exitCode": 0,
            "stdoutLogRef": null,
            "stderrLogRef": null,
            "errorSummary": null
        }))
        .expect("old command run json should deserialize");

        assert_eq!(run.intent, CommandRunIntent::Legacy);
        assert_eq!(run.loop_id, None);
        assert_eq!(run.iteration, None);
        assert_eq!(run.attempt, None);
        assert_eq!(run.termination_reason, None);
    }

    #[test]
    fn switching_primary_agent_requires_and_records_a_reason() {
        let mut task = transition_task_fixture("ready_to_implement", &["pending"], Vec::new());
        task.primary_agent_id = Some("agent-codex".to_string());

        let error = apply_start_todo(
            &mut task,
            "todo-0",
            Some("agent-claude".to_string()),
            None,
            "event-1".to_string(),
        )
        .unwrap_err();
        assert!(error.contains("requires a specific reason"));

        apply_start_todo(
            &mut task,
            "todo-0",
            Some("agent-claude".to_string()),
            Some("Codex is unavailable for this framework".to_string()),
            "event-2".to_string(),
        )
        .expect("switch with reason");
        assert_eq!(task.primary_agent_id.as_deref(), Some("agent-claude"));
        assert!(task.events[0]
            .output_summary
            .as_deref()
            .unwrap_or_default()
            .contains("Codex is unavailable"));
    }

    #[test]
    fn command_run_intent_policy_keeps_agent_and_preview_from_advancing_task() {
        let mut task = transition_task_fixture("implementing", &["implementing"], Vec::new());
        let agent_run = transition_command_run(CommandRunIntent::AgentAction, None);

        apply_command_start_status(&mut task, &agent_run).expect("agent start remains in stage");
        assert_eq!(task.status, "implementing");
        apply_command_finish_status(&mut task, &agent_run, CommandRunStatus::Succeeded)
            .expect("agent finish remains in stage");
        assert_eq!(task.status, "implementing");

        let mut preview_task = transition_task_fixture("debugging", &["done"], Vec::new());
        let preview_run = transition_command_run(CommandRunIntent::Preview, None);
        apply_command_start_status(&mut preview_task, &preview_run)
            .expect("preview start remains in stage");
        assert_eq!(preview_task.status, "debugging");
        apply_command_finish_status(&mut preview_task, &preview_run, CommandRunStatus::Failed)
            .expect("preview finish remains in stage");
        assert_eq!(preview_task.status, "debugging");
    }

    #[test]
    fn command_run_intent_policy_preserves_validation_acceptance_flow() {
        let mut task = transition_task_fixture("reviewing", &["done"], Vec::new());
        let run = transition_command_run(CommandRunIntent::Validation, None);

        apply_command_start_status(&mut task, &run).expect("validation starts");
        assert_eq!(task.status, "debugging");
        apply_command_finish_status(&mut task, &run, CommandRunStatus::Succeeded)
            .expect("validation passes");
        assert_eq!(task.status, "verifying");
        apply_command_finish_status(&mut task, &run, CommandRunStatus::Failed)
            .expect("validation fails");
        assert_eq!(task.status, "debugging");
    }

    #[test]
    fn pause_and_resume_preserve_the_workflow_stage_and_gate_mutations() {
        let mut task = transition_task_fixture("implementing", &["implementing"], Vec::new());

        apply_pause_task(
            &mut task,
            "event-pause".to_string(),
            Some("Waiting for product input".to_string()),
        )
        .expect("pause task");
        assert_eq!(task.status, "implementing");
        assert!(task.lifecycle.paused);
        assert!(ensure_task_active(&task).unwrap_err().contains("paused"));

        apply_resume_task(&mut task, "event-resume".to_string(), None).expect("resume task");
        assert_eq!(task.status, "implementing");
        assert!(!task.lifecycle.paused);
        ensure_task_active(&task).expect("resumed task is active");
        assert_eq!(task.events.len(), 2);
    }

    #[test]
    fn blocked_task_resumes_to_its_exact_previous_status() {
        let mut task = transition_task_fixture("fixing", &["done"], Vec::new());

        apply_block_task(
            &mut task,
            "event-block".to_string(),
            Some("External service unavailable".to_string()),
        )
        .expect("block task");
        assert_eq!(task.status, "blocked");
        assert_eq!(task.lifecycle.resume_status, Some(TaskStatus::Fixing));

        apply_resume_task(&mut task, "event-resume".to_string(), None).expect("resume task");
        assert_eq!(task.status, "fixing");
        assert_eq!(task.lifecycle.resume_status, None);
    }

    #[test]
    fn cancelling_a_task_is_terminal_and_clears_resume_state() {
        let mut task = transition_task_fixture("debugging", &["done"], Vec::new());
        task.lifecycle.paused = true;
        task.lifecycle.resume_status = Some(TaskStatus::Implementing);

        apply_cancel_task(
            &mut task,
            "event-cancel".to_string(),
            Some("Scope withdrawn".to_string()),
        )
        .expect("cancel task");
        assert_eq!(task.status, "cancelled");
        assert!(!task.lifecycle.paused);
        assert_eq!(task.lifecycle.resume_status, None);
        assert!(ensure_task_active(&task)
            .unwrap_err()
            .contains("cannot be changed"));
    }

    #[test]
    fn loop_bound_validation_stays_in_implementation_until_it_passes() {
        let mut task = transition_task_fixture("reviewing", &["done"], Vec::new());
        let run = transition_command_run(
            CommandRunIntent::Validation,
            Some("loop-task-1-todo-1".to_string()),
        );

        apply_command_start_status(&mut task, &run).expect("loop validation starts");
        assert_eq!(task.status, "reviewing");
        apply_command_finish_status(&mut task, &run, CommandRunStatus::Failed)
            .expect("loop validation failure keeps implementation stage");
        assert_eq!(task.status, "reviewing");
        apply_command_finish_status(&mut task, &run, CommandRunStatus::Succeeded)
            .expect("loop validation success advances to verifying");
        assert_eq!(task.status, "verifying");
    }

    #[test]
    fn repair_context_keeps_loop_bound_implementation_stage() {
        let root = std::env::temp_dir().join(format!("loom-loop-repair-context-{}", now_ms()));
        std::fs::create_dir_all(&root).expect("test project should be created");
        let mut failed_run = transition_command_run(
            CommandRunIntent::Validation,
            Some("loop-task-1-todo-0".to_string()),
        );
        failed_run.id = "run-loop-failed".to_string();
        failed_run.status = CommandRunStatus::Failed;
        failed_run.exit_code = Some(43);
        failed_run.error_summary = Some(ErrorSummary {
            exit_code: Some(43),
            stderr_tail: vec!["sentinel missing".to_string()],
            matched_lines: vec!["sentinel missing".to_string()],
            failed: true,
            ..Default::default()
        });
        let mut task = transition_task_fixture("reviewing", &["done"], vec![failed_run]);
        task.project_path = root.display().to_string();
        save_task(&task).expect("task should be saved");

        let updated = generate_repair_context(root.display().to_string(), task.id.clone())
            .expect("repair context should be generated");

        assert_eq!(updated.status, "reviewing");
        assert!(updated.repair_context_preview.is_some());
        assert_eq!(updated.events.last().unwrap().status, "reviewing");

        let persisted = load_task(&root, &task.id).expect("task should persist");
        assert_eq!(persisted.status, "reviewing");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn loop_bound_command_runs_append_trace_entries() {
        let root = std::env::temp_dir().join(format!("loom-loop-trace-{}", now_ms()));
        std::fs::create_dir_all(&root).expect("test project should be created");
        let mut task = transition_task_fixture("reviewing", &["done"], Vec::new());
        task.id = "task-trace".to_string();
        task.project_path = root.display().to_string();
        save_task(&task).expect("task should be saved");

        let run = CommandRun {
            id: "run-trace".to_string(),
            task_id: task.id.clone(),
            command: "pnpm test".to_string(),
            cwd: root.display().to_string(),
            intent: CommandRunIntent::Validation,
            loop_id: Some("loop-1".to_string()),
            iteration: Some(2),
            attempt: Some(1),
            termination_reason: None,
            session_id: None,
            resume_command: None,
            started_at_ms: 1,
            ended_at_ms: None,
            status: CommandRunStatus::Running,
            exit_code: None,
            stdout_log_ref: None,
            stderr_log_ref: None,
            error_summary: None,
        };

        add_command_run(&root, &task.id, run).expect("command run should start");
        let started = load_task(&root, &task.id).expect("task should reload");
        assert_eq!(started.loop_trace.len(), 1);
        assert_eq!(started.loop_trace[0].entry_type, "command_started");
        assert_eq!(started.loop_trace[0].stage, "testing");
        assert_eq!(started.loop_trace[0].iteration, Some(2));

        finish_command_run(
            &root,
            &task.id,
            "run-trace",
            CommandRunCompletion {
                status: CommandRunStatus::Failed,
                exit_code: Some(1),
                error_summary: Some(ErrorSummary {
                    exit_code: Some(1),
                    stderr_tail: vec!["error: failed".to_string()],
                    matched_lines: vec!["error: failed".to_string()],
                    failed: true,
                    ..Default::default()
                }),
                session_id: None,
                resume_command: None,
                termination_reason: None,
            },
        )
        .expect("command run should finish");
        let finished = load_task(&root, &task.id).expect("task should reload");
        assert_eq!(finished.loop_trace.len(), 2);
        assert_eq!(finished.loop_trace[1].entry_type, "command_finished");
        assert_eq!(
            finished.loop_trace[1].termination_reason.as_deref(),
            Some("failed")
        );
        assert_eq!(
            finished.loop_trace[1].fingerprint.as_deref(),
            Some("error: failed")
        );

        std::fs::remove_dir_all(root).expect("test project should be cleaned up");
    }

    #[test]
    fn mark_ready_for_testing_moves_reviewing_task_to_debugging() {
        let mut task = transition_task_fixture("reviewing", &["done", "done"], Vec::new());
        attach_passing_implementation_review(&mut task);

        apply_mark_ready_for_testing(&mut task, "event-1".to_string()).unwrap();

        assert_eq!(task.status, "debugging");
        assert_eq!(task.events.len(), 1);
        assert_eq!(task.events[0].actor, "user");
        assert_eq!(task.events[0].status, "debugging");
        assert_eq!(
            task.events[0].evidence_ref.as_deref(),
            Some("/repo/docs/plans/plan.md")
        );
    }

    #[test]
    fn mark_ready_for_testing_rejects_wrong_task_status() {
        let mut task = transition_task_fixture("implementing", &["done"], Vec::new());

        let error = apply_mark_ready_for_testing(&mut task, "event-1".to_string()).unwrap_err();

        assert!(error.contains("instead of reviewing"));
        assert_eq!(task.status, "implementing");
        assert!(task.events.is_empty());
    }

    #[test]
    fn mark_ready_for_testing_rejects_unfinished_todos() {
        let mut task = transition_task_fixture("reviewing", &["done", "pending"], Vec::new());

        let error = apply_mark_ready_for_testing(&mut task, "event-1".to_string()).unwrap_err();

        assert!(error.contains("before all todos are done"));
        assert_eq!(task.status, "reviewing");
        assert!(task.events.is_empty());
    }

    #[test]
    fn mark_ready_for_testing_rejects_missing_independent_review() {
        let mut task = transition_task_fixture("reviewing", &["done"], Vec::new());
        task.primary_agent_id = Some("builder".to_string());

        let error = apply_mark_ready_for_testing(&mut task, "event-1".to_string()).unwrap_err();

        assert!(error.contains("independent implementation Review"));
        assert_eq!(task.status, "reviewing");
    }

    #[test]
    fn mark_ready_for_testing_rejects_an_open_review_blocker() {
        let mut task = transition_task_fixture("reviewing", &["done"], Vec::new());
        attach_passing_implementation_review(&mut task);
        task.implementation_reviews[0]
            .findings
            .push(crate::models::ImplementationReviewFinding {
                id: "finding-1".to_string(),
                review_id: "review-1".to_string(),
                severity: "blocker".to_string(),
                title: "Broken behavior".to_string(),
                detail: "The requirement is not implemented.".to_string(),
                file: Some("src/lib.rs".to_string()),
                line: Some(10),
                status: "open".to_string(),
                created_at_ms: 2,
            });

        let error = apply_mark_ready_for_testing(&mut task, "event-1".to_string()).unwrap_err();

        assert!(error.contains("Broken behavior"));
        assert_eq!(task.status, "reviewing");
    }

    #[test]
    fn complete_task_moves_verifying_task_to_completed_with_evidence() {
        let successful_run = CommandRun {
            id: "run-1".to_string(),
            task_id: "task-1".to_string(),
            command: "pnpm build".to_string(),
            cwd: "/repo".to_string(),
            intent: CommandRunIntent::Validation,
            loop_id: None,
            iteration: None,
            attempt: None,
            termination_reason: None,
            session_id: None,
            resume_command: None,
            started_at_ms: 1,
            ended_at_ms: Some(2),
            status: CommandRunStatus::Succeeded,
            exit_code: Some(0),
            stdout_log_ref: None,
            stderr_log_ref: None,
            error_summary: None,
        };
        let mut task = transition_task_fixture("verifying", &["done"], vec![successful_run]);

        apply_complete_task(&mut task, "event-1".to_string()).unwrap();

        assert_eq!(task.status, "completed");
        assert_eq!(task.events.len(), 1);
        assert_eq!(task.events[0].actor, "user");
        assert_eq!(task.events[0].status, "completed");
        assert_eq!(task.events[0].evidence_ref.as_deref(), Some("run-1"));
    }

    #[test]
    fn complete_task_rejects_agent_actions_and_stale_validation_success() {
        let mut agent_action = transition_command_run(CommandRunIntent::AgentAction, None);
        agent_action.status = CommandRunStatus::Succeeded;
        let mut task = transition_task_fixture("verifying", &["done"], vec![agent_action]);
        assert!(apply_complete_task(&mut task, "event-agent".to_string())
            .unwrap_err()
            .contains("validation evidence"));

        let mut passed = transition_command_run(CommandRunIntent::Validation, None);
        passed.id = "run-pass".to_string();
        passed.status = CommandRunStatus::Succeeded;
        passed.started_at_ms = 10;
        let mut failed = transition_command_run(CommandRunIntent::Validation, None);
        failed.id = "run-fail".to_string();
        failed.status = CommandRunStatus::Failed;
        failed.started_at_ms = 20;
        let mut task = transition_task_fixture("verifying", &["done"], vec![passed, failed]);
        let error = apply_complete_task(&mut task, "event-stale".to_string()).unwrap_err();
        assert!(error.contains("latest validation"));
        assert_eq!(task.status, "verifying");
    }

    #[test]
    fn task_storage_rejects_path_traversal_ids() {
        let root = std::env::temp_dir().join(format!("loom-task-id-{}", now_ms()));
        std::fs::create_dir_all(&root).unwrap();
        assert!(load_task(&root, "../escape")
            .err()
            .expect("path traversal should fail")
            .contains("invalid task id"));
        assert!(
            delete_task(root.display().to_string(), "../escape".to_string())
                .unwrap_err()
                .contains("invalid task id")
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn legacy_task_file_is_migrated_on_load() {
        let root = std::env::temp_dir().join(format!("loom-task-legacy-{}", now_ms()));
        std::fs::create_dir_all(storage::project_tasks_dir(&root)).unwrap();
        let mut task = transition_task_fixture("debugging", &["done"], Vec::new());
        task.id = "task-legacy".to_string();
        task.project_path = root.display().to_string();
        let path = task_repository::task_path(&root, &task.id);
        storage::atomic_write_json(&path, &task).unwrap();

        let loaded = load_task(&root, &task.id).expect("legacy task should migrate");
        let stored: serde_json::Value = storage::read_json_file(&path).unwrap();

        assert_eq!(loaded.id, task.id);
        assert_eq!(
            stored["schemaVersion"],
            crate::migrations::TASK_SCHEMA_VERSION
        );
        assert_eq!(stored["data"]["id"], "task-legacy");
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn concurrent_command_updates_preserve_every_run_and_event() {
        let root = std::env::temp_dir().join(format!("loom-task-concurrency-{}", now_ms()));
        std::fs::create_dir_all(&root).unwrap();
        let mut task = transition_task_fixture("debugging", &["done"], Vec::new());
        task.id = "task-concurrent".to_string();
        task.project_path = root.display().to_string();
        save_task(&task).unwrap();

        let starts = (0..8)
            .map(|index| {
                let root = root.clone();
                std::thread::spawn(move || {
                    let mut run = transition_command_run(CommandRunIntent::AgentAction, None);
                    run.id = format!("run-concurrent-{index}");
                    run.task_id = "task-concurrent".to_string();
                    add_command_run(&root, "task-concurrent", run).unwrap();
                })
            })
            .collect::<Vec<_>>();
        for thread in starts {
            thread.join().unwrap();
        }

        let finishes = (0..8)
            .map(|index| {
                let root = root.clone();
                std::thread::spawn(move || {
                    finish_command_run(
                        &root,
                        "task-concurrent",
                        &format!("run-concurrent-{index}"),
                        CommandRunCompletion {
                            status: CommandRunStatus::Succeeded,
                            exit_code: Some(0),
                            error_summary: None,
                            session_id: None,
                            resume_command: None,
                            termination_reason: None,
                        },
                    )
                    .unwrap();
                })
            })
            .collect::<Vec<_>>();
        for thread in finishes {
            thread.join().unwrap();
        }

        let persisted = load_task(&root, "task-concurrent").unwrap();
        assert_eq!(persisted.command_runs.len(), 8);
        assert!(persisted
            .command_runs
            .iter()
            .all(|run| run.status == CommandRunStatus::Succeeded));
        assert_eq!(
            persisted
                .events
                .iter()
                .filter(|event| event
                    .evidence_ref
                    .as_deref()
                    .is_some_and(|value| value.starts_with("run-concurrent-")))
                .count(),
            16
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn concurrent_lifecycle_and_run_completion_do_not_lose_scalar_or_evidence_updates() {
        let root = std::env::temp_dir().join(format!("loom-task-lifecycle-race-{}", now_ms()));
        std::fs::create_dir_all(&root).unwrap();
        let mut run = transition_command_run(CommandRunIntent::Validation, None);
        run.id = "run-lifecycle-race".to_string();
        run.task_id = "task-lifecycle-race".to_string();
        let mut task = transition_task_fixture("debugging", &["done"], vec![run]);
        task.id = "task-lifecycle-race".to_string();
        task.project_path = root.display().to_string();
        save_task(&task).unwrap();

        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let pause_root = root.clone();
        let pause_barrier = barrier.clone();
        let pause = std::thread::spawn(move || {
            pause_barrier.wait();
            update_task(&pause_root, "task-lifecycle-race", |task| {
                apply_pause_task(
                    task,
                    "event-lifecycle-race-pause".to_string(),
                    Some("pause during completion".to_string()),
                )
            })
            .unwrap();
        });
        let finish_root = root.clone();
        let finish_barrier = barrier.clone();
        let finish = std::thread::spawn(move || {
            finish_barrier.wait();
            finish_command_run(
                &finish_root,
                "task-lifecycle-race",
                "run-lifecycle-race",
                CommandRunCompletion {
                    status: CommandRunStatus::Succeeded,
                    exit_code: Some(0),
                    error_summary: None,
                    session_id: None,
                    resume_command: None,
                    termination_reason: None,
                },
            )
            .unwrap();
        });
        barrier.wait();
        pause.join().unwrap();
        finish.join().unwrap();

        let persisted = load_task(&root, "task-lifecycle-race").unwrap();
        assert!(persisted.lifecycle.paused);
        assert_eq!(persisted.status, TaskStatus::Verifying);
        assert_eq!(
            persisted.command_runs[0].status,
            CommandRunStatus::Succeeded
        );
        assert!(persisted
            .events
            .iter()
            .any(|event| event.id == "event-lifecycle-race-pause"));
        assert!(persisted.events.iter().any(|event| {
            event.evidence_ref.as_deref() == Some("run-lifecycle-race")
                && event
                    .output_summary
                    .as_deref()
                    .is_some_and(|summary| summary.contains("succeeded"))
        }));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn append_only_merge_finishes_only_the_matching_command_run() {
        let mut first = transition_command_run(CommandRunIntent::AgentAction, None);
        first.id = "run-first".to_string();
        let mut second = transition_command_run(CommandRunIntent::AgentAction, None);
        second.id = "run-second".to_string();
        let mut target =
            transition_task_fixture("debugging", &["done"], vec![first.clone(), second.clone()]);
        let mut persisted = target.clone();
        persisted.command_runs[1].status = CommandRunStatus::Succeeded;
        persisted.command_runs[1].exit_code = Some(0);

        task_repository::merge_append_only_task_state(&mut target, persisted);

        assert_eq!(target.command_runs[0].status, CommandRunStatus::Running);
        assert_eq!(target.command_runs[1].status, CommandRunStatus::Succeeded);
        assert_eq!(target.command_runs[1].exit_code, Some(0));
    }

    #[test]
    fn delete_task_removes_file_and_is_idempotent() {
        let root = std::env::temp_dir().join(format!("loom-delete-task-{}", now_ms()));
        std::fs::create_dir_all(&root).unwrap();
        let mut task = transition_task_fixture("reviewing", &["done"], Vec::new());
        task.id = "task-del".to_string();
        task.project_path = root.display().to_string();
        save_task(&task).unwrap();
        let path = task_repository::task_path(&root, &task.id);
        assert!(path.exists());

        delete_task(root.display().to_string(), task.id.clone()).unwrap();
        assert!(!path.exists());

        // Deleting an already-removed task is a no-op success.
        delete_task(root.display().to_string(), task.id.clone()).unwrap();

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn complete_task_rejects_non_verifying_task() {
        let mut task = transition_task_fixture("debugging", &["done"], Vec::new());

        let error = apply_complete_task(&mut task, "event-1".to_string()).unwrap_err();

        assert!(error.contains("instead of verifying"));
        assert_eq!(task.status, "debugging");
        assert!(task.events.is_empty());
    }

    #[test]
    fn start_todo_rejects_unknown_todo_without_mutating_task() {
        let mut task = Task {
            id: "task-1".to_string(),
            project_path: "/repo".to_string(),
            title: "Ship scoped implementation".to_string(),
            raw_requirement: "Implement one todo at a time".to_string(),
            status: TaskStatus::ReadyToImplement,
            lifecycle: Default::default(),
            selected_planning_agent_ids: Vec::new(),
            primary_agent_id: None,
            review_agent_ids: Vec::new(),
            implementation_review_runs: Vec::new(),
            implementation_reviews: Vec::new(),
            implementation_review_decisions: Vec::new(),
            final_plan: None,
            final_plan_path: None,
            final_plan_html_path: None,
            discussion_summary: None,
            planning_runs: Vec::new(),
            agent_invocations: Vec::new(),
            plan_reviews: Vec::new(),
            planning_decisions: Vec::new(),
            plan_todos: vec![PlanTodoItem {
                id: "todo-1".to_string(),
                task_id: "task-1".to_string(),
                title: "Known scope".to_string(),
                description: "Known scope".to_string(),
                status: PlanTodoStatus::Pending,
                order: 0,
                plan_ref: None,
            }],
            loop_trace: Vec::new(),
            events: Vec::new(),
            command_runs: Vec::new(),
            feedback: Vec::new(),
            loop_compact_summary: None,
            repair_context_preview: None,
            git_baseline: None,
            summary: None,
            created_at_ms: 1,
            updated_at_ms: 1,
        };

        let error =
            apply_start_todo(&mut task, "missing", None, None, "event-1".to_string()).unwrap_err();

        assert!(error.contains("does not exist"));
        assert_eq!(task.status, "ready_to_implement");
        assert_eq!(task.plan_todos[0].status, "pending");
        assert!(task.events.is_empty());
    }

    #[test]
    fn formats_current_todo_context_for_repair_handoff() {
        let todos = vec![
            PlanTodoItem {
                id: "todo-1".to_string(),
                task_id: "task-1".to_string(),
                title: "Build unrelated screen".to_string(),
                description: "Unrelated".to_string(),
                status: PlanTodoStatus::Pending,
                order: 0,
                plan_ref: Some("/repo/docs/plans/plan.md".to_string()),
            },
            PlanTodoItem {
                id: "todo-2".to_string(),
                task_id: "task-1".to_string(),
                title: "Fix failing debug validation".to_string(),
                description: "Scoped fix".to_string(),
                status: PlanTodoStatus::Implementing,
                order: 1,
                plan_ref: Some("/repo/docs/plans/plan.md".to_string()),
            },
        ];

        let context = format_current_todo_context(&todos);

        assert!(context.contains("id=todo-2"));
        assert!(context.contains("title=Fix failing debug validation"));
        assert!(context.contains("status=implementing"));
        assert!(context.contains("planRef=/repo/docs/plans/plan.md"));
        assert!(!context.contains("Build unrelated screen"));
    }

    #[test]
    fn formats_pending_todo_context_when_no_scope_is_started() {
        let todos = vec![PlanTodoItem {
            id: "todo-1".to_string(),
            task_id: "task-1".to_string(),
            title: "Start scoped implementation".to_string(),
            description: "Pending scope".to_string(),
            status: PlanTodoStatus::Pending,
            order: 0,
            plan_ref: None,
        }];

        let context = format_current_todo_context(&todos);

        assert!(context.contains("No todo is currently marked implementing"));
        assert!(context.contains("id=todo-1"));
        assert!(context.contains("planRef=(none)"));
    }

    #[test]
    fn formats_failed_run_context_with_replay_evidence() {
        let context = format_failed_run_context(&CommandRun {
            id: "run-1".to_string(),
            task_id: "task-1".to_string(),
            command: "pnpm build".to_string(),
            cwd: "/repo".to_string(),
            intent: CommandRunIntent::Validation,
            loop_id: None,
            iteration: None,
            attempt: None,
            termination_reason: None,
            session_id: None,
            resume_command: None,
            started_at_ms: 1,
            ended_at_ms: Some(2),
            status: CommandRunStatus::Failed,
            exit_code: Some(1),
            stdout_log_ref: Some("/repo/.loom/logs/task-1/run-1.stdout.log".to_string()),
            stderr_log_ref: Some("/repo/.loom/logs/task-1/run-1.stderr.log".to_string()),
            error_summary: Some(ErrorSummary {
                exit_code: Some(1),
                stderr_tail: vec!["error: build failed".to_string()],
                matched_lines: vec!["error: build failed".to_string()],
                failed: true,
                ..Default::default()
            }),
        });

        assert!(context.contains("runId=run-1"));
        assert!(context.contains("command=pnpm build"));
        assert!(context.contains("stdoutLog=/repo/.loom/logs/task-1/run-1.stdout.log"));
        assert!(context.contains("stderrLog=/repo/.loom/logs/task-1/run-1.stderr.log"));
        assert!(context.contains("matchedLines=error: build failed"));
    }

    #[test]
    fn ignores_failed_plan_without_todos() {
        let plan = "# Plan\n\n## Implementation Todo\n\nNo implementation todo items were generated because all selected planning agents failed.";

        assert!(implementation_todo_lines(plan).is_empty());
    }
}
