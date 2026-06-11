use crate::{
    models::{
        now_ms, CommandRun, CreateTaskInput, ErrorSummary, FeedbackInput, IdGenerator,
        PlanTodoItem, PlanningDecision, PlanningDecisionInput, Task, TaskEvent, UserFeedback,
    },
    plan_html,
    storage,
};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tauri::State;

#[tauri::command]
pub fn list_tasks(project_path: String) -> Result<Vec<Task>, String> {
    let tasks_dir = storage::project_tasks_dir(Path::new(&project_path));

    if !tasks_dir.exists() {
        return Ok(Vec::new());
    }

    let mut tasks = Vec::new();
    for entry in
        fs::read_dir(tasks_dir).map_err(|error| format!("failed to read tasks: {error}"))?
    {
        let path = entry
            .map_err(|error| format!("failed to read task entry: {error}"))?
            .path();
        if path.extension().and_then(|value| value.to_str()) == Some("json") {
            tasks.push(storage::read_json_file(&path)?);
        }
    }
    tasks.sort_by_key(|task: &Task| task.created_at_ms);

    Ok(tasks)
}

#[tauri::command]
pub fn create_task(ids: State<'_, IdGenerator>, input: CreateTaskInput) -> Result<Task, String> {
    let timestamp_ms = now_ms();
    let task_id = ids.next("task");
    let event_id = ids.next("event");
    let task = Task {
        id: task_id.clone(),
        project_path: input.project_path,
        title: input.title,
        raw_requirement: input.raw_requirement.clone(),
        status: "drafting_requirements".to_string(),
        selected_planning_agent_ids: input.selected_planning_agent_ids,
        primary_agent_id: input.primary_agent_id,
        review_agent_ids: Vec::new(),
        final_plan: None,
        final_plan_path: None,
        final_plan_html_path: None,
        discussion_summary: None,
        planning_runs: Vec::new(),
        agent_invocations: Vec::new(),
        plan_reviews: Vec::new(),
        planning_decisions: Vec::new(),
        plan_todos: Vec::new(),
        events: vec![TaskEvent {
            id: event_id,
            task_id: task_id.clone(),
            timestamp_ms,
            actor: "user".to_string(),
            status: "drafting_requirements".to_string(),
            input_summary: Some(input.raw_requirement),
            output_summary: Some("Task created".to_string()),
            evidence_ref: None,
        }],
        command_runs: Vec::new(),
        feedback: Vec::new(),
        repair_context_preview: None,
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
    let mut task = load_task(Path::new(&input.project_path), &input.task_id)?;
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
        fs::write(path, plan)
            .map_err(|error| format!("failed to write decision to plan: {error}"))?;
        task.final_plan_html_path = plan_html::write_task_plan_html(&task)?;
    }
    task.events.push(TaskEvent {
        id: ids.next("event"),
        task_id: task.id.clone(),
        timestamp_ms,
        actor: "user".to_string(),
        status: task.status.clone(),
        input_summary: Some(input.title),
        output_summary: Some(input.content),
        evidence_ref: task.final_plan_path.clone(),
    });
    task.updated_at_ms = timestamp_ms;
    save_task(&task)?;

    Ok(task)
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
    let mut task = load_task(Path::new(&project_path), &task_id)?;
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
        return Err("cannot confirm plan because it has no implementation todo items".to_string());
    }

    task.status = "ready_to_implement".to_string();
    task.plan_todos = plan_todos;
    task.updated_at_ms = now_ms();
    task.events.push(TaskEvent {
        id: ids.next("event"),
        task_id: task.id.clone(),
        timestamp_ms: task.updated_at_ms,
        actor: "user".to_string(),
        status: task.status.clone(),
        input_summary: Some("Plan confirmed".to_string()),
        output_summary: Some(
            "Implementation todo items generated from the final plan.".to_string(),
        ),
        evidence_ref: task.final_plan_path.clone(),
    });
    save_task(&task)?;

    Ok(task)
}

#[tauri::command]
pub fn start_todo(
    ids: State<'_, IdGenerator>,
    project_path: String,
    task_id: String,
    todo_id: String,
    primary_agent_id: Option<String>,
) -> Result<Task, String> {
    let mut task = load_task(Path::new(&project_path), &task_id)?;
    apply_start_todo(&mut task, &todo_id, primary_agent_id, ids.next("event"))?;
    save_task(&task)?;

    Ok(task)
}

#[tauri::command]
pub fn complete_todo(
    ids: State<'_, IdGenerator>,
    project_path: String,
    task_id: String,
    todo_id: String,
) -> Result<Task, String> {
    let mut task = load_task(Path::new(&project_path), &task_id)?;
    apply_complete_todo(&mut task, &todo_id, ids.next("event"))?;
    save_task(&task)?;

    Ok(task)
}

#[tauri::command]
pub fn mark_ready_for_testing(
    ids: State<'_, IdGenerator>,
    project_path: String,
    task_id: String,
) -> Result<Task, String> {
    let mut task = load_task(Path::new(&project_path), &task_id)?;
    apply_mark_ready_for_testing(&mut task, ids.next("event"))?;
    save_task(&task)?;

    Ok(task)
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
    let mut task = load_task(Path::new(&project_path), &task_id)?;
    apply_complete_task(&mut task, ids.next("event"))?;
    save_task(&task)?;

    Ok(task)
}

#[tauri::command]
pub fn delete_task(project_path: String, task_id: String) -> Result<(), String> {
    let project = Path::new(&project_path);
    let path = task_path(project, &task_id);

    if path.exists() {
        std::fs::remove_file(&path).map_err(|error| format!("failed to delete task: {error}"))?;
    }

    // Best-effort cleanup of this task's planning evidence directory.
    let evidence_dir = storage::project_loom_dir(project)
        .join("planning")
        .join(&task_id);
    if evidence_dir.exists() {
        let _ = std::fs::remove_dir_all(&evidence_dir);
    }

    Ok(())
}

fn apply_start_todo(
    task: &mut Task,
    todo_id: &str,
    primary_agent_id: Option<String>,
    event_id: String,
) -> Result<(), String> {
    let selected_title = task
        .plan_todos
        .iter()
        .find(|todo| todo.id == todo_id)
        .map(|todo| todo.title.clone())
        .ok_or_else(|| "cannot start todo because it does not exist".to_string())?;

    task.status = "implementing".to_string();
    if primary_agent_id.is_some() {
        task.primary_agent_id = primary_agent_id;
    }
    task.plan_todos = task
        .plan_todos
        .drain(..)
        .map(|todo| {
            if todo.id == todo_id {
                PlanTodoItem {
                    status: "implementing".to_string(),
                    ..todo
                }
            } else if todo.status == "implementing" {
                PlanTodoItem {
                    status: "pending".to_string(),
                    ..todo
                }
            } else {
                todo
            }
        })
        .collect();
    task.updated_at_ms = now_ms();
    task.events.push(TaskEvent {
        id: event_id,
        task_id: task.id.clone(),
        timestamp_ms: task.updated_at_ms,
        actor: "user".to_string(),
        status: task.status.clone(),
        input_summary: Some(format!("Started implementation todo: {selected_title}")),
        output_summary: Some(
            "Implementation scope selected and ready for Agent handoff.".to_string(),
        ),
        evidence_ref: task.final_plan_path.clone(),
    });

    Ok(())
}

fn apply_complete_todo(task: &mut Task, todo_id: &str, event_id: String) -> Result<(), String> {
    let selected_todo = task
        .plan_todos
        .iter()
        .find(|todo| todo.id == todo_id)
        .ok_or_else(|| "cannot complete todo because it does not exist".to_string())?;
    if selected_todo.status != "implementing" {
        return Err(format!(
            "cannot complete todo because it is {} instead of implementing",
            selected_todo.status
        ));
    }
    let selected_title = selected_todo.title.clone();

    task.plan_todos = task
        .plan_todos
        .drain(..)
        .map(|todo| {
            if todo.id == todo_id {
                PlanTodoItem {
                    status: "done".to_string(),
                    ..todo
                }
            } else {
                todo
            }
        })
        .collect();
    task.status = if task.plan_todos.iter().all(|todo| todo.status == "done") {
        "reviewing".to_string()
    } else {
        "implementing".to_string()
    };
    task.updated_at_ms = now_ms();
    task.events.push(TaskEvent {
        id: event_id,
        task_id: task.id.clone(),
        timestamp_ms: task.updated_at_ms,
        actor: "user".to_string(),
        status: task.status.clone(),
        input_summary: Some(format!("Completed implementation todo: {selected_title}")),
        output_summary: Some("Todo marked done and ready for review evidence handoff.".to_string()),
        evidence_ref: task.final_plan_path.clone(),
    });

    Ok(())
}

fn apply_mark_ready_for_testing(task: &mut Task, event_id: String) -> Result<(), String> {
    if task.status != "reviewing" {
        return Err(format!(
            "cannot mark task ready for testing because it is {} instead of reviewing",
            task.status
        ));
    }

    if task.plan_todos.iter().any(|todo| todo.status != "done") {
        return Err("cannot mark task ready for testing before all todos are done".to_string());
    }

    task.status = "debugging".to_string();
    task.updated_at_ms = now_ms();
    task.events.push(TaskEvent {
        id: event_id,
        task_id: task.id.clone(),
        timestamp_ms: task.updated_at_ms,
        actor: "user".to_string(),
        status: task.status.clone(),
        input_summary: Some("Marked task ready for testing".to_string()),
        output_summary: Some(
            "Implementation review completed; task is ready for debug validation.".to_string(),
        ),
        evidence_ref: task.final_plan_path.clone(),
    });

    Ok(())
}

fn apply_complete_task(task: &mut Task, event_id: String) -> Result<(), String> {
    if task.status != "verifying" {
        return Err(format!(
            "cannot complete task because it is {} instead of verifying",
            task.status
        ));
    }

    let latest_successful_run_id = task
        .command_runs
        .iter()
        .rev()
        .find(|run| run.status == "succeeded")
        .map(|run| run.id.clone());

    task.status = "completed".to_string();
    task.updated_at_ms = now_ms();
    task.events.push(TaskEvent {
        id: event_id,
        task_id: task.id.clone(),
        timestamp_ms: task.updated_at_ms,
        actor: "user".to_string(),
        status: task.status.clone(),
        input_summary: Some("Accepted verification and completed task".to_string()),
        output_summary: Some(
            "Task completed after human acceptance of verification evidence.".to_string(),
        ),
        evidence_ref: latest_successful_run_id,
    });

    Ok(())
}

#[tauri::command]
pub fn append_feedback(ids: State<'_, IdGenerator>, input: FeedbackInput) -> Result<Task, String> {
    let mut task = load_task(Path::new(&input.project_path), &input.task_id)?;
    let feedback = UserFeedback {
        id: ids.next("feedback"),
        task_id: task.id.clone(),
        command_run_id: input.command_run_id.clone(),
        content: input.content.clone(),
        timestamp_ms: now_ms(),
    };
    task.feedback.push(feedback);
    task.events.push(TaskEvent {
        id: ids.next("event"),
        task_id: task.id.clone(),
        timestamp_ms: now_ms(),
        actor: "user".to_string(),
        status: task.status.clone(),
        input_summary: Some(input.content),
        output_summary: Some("Manual feedback captured".to_string()),
        evidence_ref: input.command_run_id,
    });
    task.updated_at_ms = now_ms();
    save_task(&task)?;

    Ok(task)
}

#[tauri::command]
pub fn generate_repair_context(project_path: String, task_id: String) -> Result<Task, String> {
    let mut task = load_task(Path::new(&project_path), &task_id)?;
    let latest_failed_run = task
        .command_runs
        .iter()
        .rev()
        .find(|run| run.status == "failed")
        .cloned();
    let latest_feedback = task
        .feedback
        .last()
        .map(|feedback| feedback.content.clone());
    let current_todo_context = format_current_todo_context(&task.plan_todos);
    let context = format!(
        "Task: {}\n\nRequirement:\n{}\n\nFinal plan:\n{}\n\nCurrent implementation scope:\n{}\n\nLatest failure:\n{}\n\nLatest feedback:\n{}\n\nRepair handoff checklist:\n- Reproduce or explain the failure using the command and log references above.\n- Keep the fix scoped to the current implementation todo unless the evidence proves a wider issue.\n- Re-run the most relevant validation command and attach the result.",
        task.title,
        task.raw_requirement,
        task.final_plan.clone().unwrap_or_else(|| "(none)".to_string()),
        current_todo_context,
        latest_failed_run
            .as_ref()
            .map(format_failed_run_context)
            .unwrap_or_else(|| "(none)".to_string()),
        latest_feedback.unwrap_or_else(|| "(none)".to_string())
    );
    let evidence_ref = latest_failed_run
        .as_ref()
        .map(|run| run.id.clone())
        .or_else(|| task.feedback.last().map(|feedback| feedback.id.clone()));
    task.repair_context_preview = Some(context);
    task.status = "fixing".to_string();
    task.updated_at_ms = now_ms();
    task.events.push(TaskEvent {
        id: format!("event-{}-repair-context", task.updated_at_ms),
        task_id: task.id.clone(),
        timestamp_ms: task.updated_at_ms,
        actor: "system".to_string(),
        status: task.status.clone(),
        input_summary: Some("Generated repair context for Agent handoff".to_string()),
        output_summary: Some(
            "Latest failure, user feedback, and current implementation scope were packaged for a scoped fix."
                .to_string(),
        ),
        evidence_ref,
    });
    save_task(&task)?;

    Ok(task)
}

pub fn load_task(project_path: &Path, task_id: &str) -> Result<Task, String> {
    storage::read_json_file(&task_path(project_path, task_id))
}

pub fn save_task(task: &Task) -> Result<(), String> {
    let path = task_path(Path::new(&task.project_path), &task.id);
    storage::atomic_write_json(&path, task)
}

pub fn add_command_run(project_path: &Path, task_id: &str, run: CommandRun) -> Result<(), String> {
    let mut task = load_task(project_path, task_id)?;
    task.status = "debugging".to_string();
    task.command_runs.push(run.clone());
    task.events.push(TaskEvent {
        id: format!("event-{}-command-start", now_ms()),
        task_id: task.id.clone(),
        timestamp_ms: now_ms(),
        actor: "system".to_string(),
        status: task.status.clone(),
        input_summary: Some(run.command),
        output_summary: Some("Command started".to_string()),
        evidence_ref: Some(run.id),
    });
    task.updated_at_ms = now_ms();
    save_task(&task)
}

pub fn finish_command_run(
    project_path: &Path,
    task_id: &str,
    run_id: &str,
    status: &str,
    exit_code: Option<i32>,
    error_summary: Option<ErrorSummary>,
) -> Result<(), String> {
    let mut task = load_task(project_path, task_id)?;
    let mut command_text = None;
    if let Some(run) = task.command_runs.iter_mut().find(|run| run.id == run_id) {
        run.status = status.to_string();
        run.exit_code = exit_code;
        run.ended_at_ms = Some(now_ms());
        run.error_summary = error_summary;
        command_text = Some(run.command.clone());
    }
    task.status = if status == "succeeded" {
        "verifying".to_string()
    } else {
        "debugging".to_string()
    };
    task.events.push(TaskEvent {
        id: format!("event-{}-command-end", now_ms()),
        task_id: task.id.clone(),
        timestamp_ms: now_ms(),
        actor: "system".to_string(),
        status: task.status.clone(),
        input_summary: command_text,
        output_summary: Some(format!("Command {status}")),
        evidence_ref: Some(run_id.to_string()),
    });
    task.updated_at_ms = now_ms();
    save_task(&task)
}

fn task_path(project_path: &Path, task_id: &str) -> PathBuf {
    storage::project_tasks_dir(project_path).join(format!("{task_id}.json"))
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
            status: "pending".to_string(),
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
            status: "ready_to_implement".to_string(),
            selected_planning_agent_ids: Vec::new(),
            primary_agent_id: None,
            review_agent_ids: Vec::new(),
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
                    status: "implementing".to_string(),
                    order: 0,
                    plan_ref: None,
                },
                PlanTodoItem {
                    id: "todo-2".to_string(),
                    task_id: "task-1".to_string(),
                    title: "New scope".to_string(),
                    description: "New scope".to_string(),
                    status: "pending".to_string(),
                    order: 1,
                    plan_ref: None,
                },
            ],
            events: Vec::new(),
            command_runs: Vec::new(),
            feedback: Vec::new(),
            repair_context_preview: None,
            created_at_ms: 1,
            updated_at_ms: 1,
        };

        apply_start_todo(
            &mut task,
            "todo-2",
            Some("agent-claude".to_string()),
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
            status: "implementing".to_string(),
            selected_planning_agent_ids: Vec::new(),
            primary_agent_id: None,
            review_agent_ids: Vec::new(),
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
                status: "implementing".to_string(),
                order: 0,
                plan_ref: None,
            }],
            events: Vec::new(),
            command_runs: Vec::new(),
            feedback: Vec::new(),
            repair_context_preview: None,
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
            status: "ready_to_implement".to_string(),
            selected_planning_agent_ids: Vec::new(),
            primary_agent_id: None,
            review_agent_ids: Vec::new(),
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
                status: "pending".to_string(),
                order: 0,
                plan_ref: None,
            }],
            events: Vec::new(),
            command_runs: Vec::new(),
            feedback: Vec::new(),
            repair_context_preview: None,
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
            status: status.to_string(),
            selected_planning_agent_ids: Vec::new(),
            primary_agent_id: None,
            review_agent_ids: Vec::new(),
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
                    status: (*todo_status).to_string(),
                    order: index as u32,
                    plan_ref: None,
                })
                .collect(),
            events: Vec::new(),
            command_runs,
            feedback: Vec::new(),
            repair_context_preview: None,
            created_at_ms: 1,
            updated_at_ms: 1,
        }
    }

    #[test]
    fn mark_ready_for_testing_moves_reviewing_task_to_debugging() {
        let mut task = transition_task_fixture("reviewing", &["done", "done"], Vec::new());

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
    fn complete_task_moves_verifying_task_to_completed_with_evidence() {
        let successful_run = CommandRun {
            id: "run-1".to_string(),
            task_id: "task-1".to_string(),
            command: "pnpm build".to_string(),
            cwd: "/repo".to_string(),
            started_at_ms: 1,
            ended_at_ms: Some(2),
            status: "succeeded".to_string(),
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
    fn delete_task_removes_file_and_is_idempotent() {
        let root = std::env::temp_dir().join(format!("loom-delete-task-{}", now_ms()));
        std::fs::create_dir_all(&root).unwrap();
        let mut task = transition_task_fixture("reviewing", &["done"], Vec::new());
        task.id = "task-del".to_string();
        task.project_path = root.display().to_string();
        save_task(&task).unwrap();
        let path = task_path(&root, &task.id);
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
            status: "ready_to_implement".to_string(),
            selected_planning_agent_ids: Vec::new(),
            primary_agent_id: None,
            review_agent_ids: Vec::new(),
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
                status: "pending".to_string(),
                order: 0,
                plan_ref: None,
            }],
            events: Vec::new(),
            command_runs: Vec::new(),
            feedback: Vec::new(),
            repair_context_preview: None,
            created_at_ms: 1,
            updated_at_ms: 1,
        };

        let error =
            apply_start_todo(&mut task, "missing", None, "event-1".to_string()).unwrap_err();

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
                status: "pending".to_string(),
                order: 0,
                plan_ref: Some("/repo/docs/plans/plan.md".to_string()),
            },
            PlanTodoItem {
                id: "todo-2".to_string(),
                task_id: "task-1".to_string(),
                title: "Fix failing debug validation".to_string(),
                description: "Scoped fix".to_string(),
                status: "implementing".to_string(),
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
            status: "pending".to_string(),
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
            started_at_ms: 1,
            ended_at_ms: Some(2),
            status: "failed".to_string(),
            exit_code: Some(1),
            stdout_log_ref: Some("/repo/.loom/logs/task-1/run-1.stdout.log".to_string()),
            stderr_log_ref: Some("/repo/.loom/logs/task-1/run-1.stderr.log".to_string()),
            error_summary: Some(ErrorSummary {
                exit_code: Some(1),
                stderr_tail: vec!["error: build failed".to_string()],
                matched_lines: vec!["error: build failed".to_string()],
                failed: true,
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
