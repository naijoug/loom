use super::*;

pub(super) fn apply_pause_task(
    task: &mut Task,
    event_id: String,
    reason: Option<String>,
) -> Result<(), String> {
    ensure_task_active(task)?;
    task.status = transition(task.status, TaskAction::Paused)?;
    task.lifecycle.paused = true;
    task.lifecycle.pause_reason = normalized_reason(reason, "Paused by user");
    let reason = task.lifecycle.pause_reason.clone();
    push_lifecycle_event(task, event_id, "Task paused", reason);
    Ok(())
}

pub(super) fn apply_resume_task(
    task: &mut Task,
    event_id: String,
    reason: Option<String>,
) -> Result<(), String> {
    let resumed_from = if task.lifecycle.paused {
        task.status = transition(
            task.status,
            TaskAction::Resumed {
                target: task.status,
            },
        )?;
        task.lifecycle.paused = false;
        task.lifecycle.pause_reason.take()
    } else if task.status == TaskStatus::Blocked {
        let target = task.lifecycle.resume_status.ok_or_else(|| {
            "blocked task has no recorded status to resume; choose a recovery action".to_string()
        })?;
        task.status = transition(task.status, TaskAction::Resumed { target })?;
        task.lifecycle.resume_status = None;
        task.lifecycle.blocked_reason.take()
    } else {
        return Err(format!(
            "task is neither paused nor blocked (status={})",
            task.status
        ));
    };
    push_lifecycle_event(
        task,
        event_id,
        "Task resumed",
        normalized_reason(reason, "Resumed by user").or(resumed_from),
    );
    Ok(())
}

pub(super) fn apply_block_task(
    task: &mut Task,
    event_id: String,
    reason: Option<String>,
) -> Result<(), String> {
    ensure_task_active(task)?;
    task.lifecycle.resume_status = Some(task.status);
    task.lifecycle.paused = false;
    task.lifecycle.pause_reason = None;
    task.lifecycle.blocked_reason = normalized_reason(reason, "Blocked by user");
    task.status = transition(task.status, TaskAction::Blocked)?;
    let reason = task.lifecycle.blocked_reason.clone();
    push_lifecycle_event(task, event_id, "Task blocked", reason);
    Ok(())
}

pub(super) fn apply_cancel_task(
    task: &mut Task,
    event_id: String,
    reason: Option<String>,
) -> Result<(), String> {
    if task.status.is_terminal() {
        return Err(format!(
            "cannot cancel terminal task status {}",
            task.status
        ));
    }
    task.status = transition(task.status, TaskAction::Cancelled)?;
    task.lifecycle.paused = false;
    task.lifecycle.resume_status = None;
    task.lifecycle.cancelled_reason = normalized_reason(reason, "Cancelled by user");
    let reason = task.lifecycle.cancelled_reason.clone();
    push_lifecycle_event(task, event_id, "Task cancelled", reason);
    Ok(())
}

fn normalized_reason(reason: Option<String>, fallback: &str) -> Option<String> {
    Some(
        reason
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| fallback.to_string()),
    )
}

fn push_lifecycle_event(task: &mut Task, event_id: String, action: &str, reason: Option<String>) {
    task.updated_at_ms = now_ms();
    task.events.push(TaskEvent {
        id: event_id,
        task_id: task.id.clone(),
        timestamp_ms: task.updated_at_ms,
        actor: "user".to_string(),
        status: task.status,
        input_summary: reason,
        output_summary: Some(action.to_string()),
        evidence_ref: None,
    });
}

pub(crate) fn ensure_task_active(task: &Task) -> Result<(), String> {
    if task.lifecycle.paused {
        return Err("task is paused; resume it before continuing".to_string());
    }
    if task.status == TaskStatus::Blocked {
        return Err("task is blocked; resume it before continuing".to_string());
    }
    if task.status.is_terminal() {
        return Err(format!("task status {} cannot be changed", task.status));
    }
    Ok(())
}

pub(super) fn apply_start_todo(
    task: &mut Task,
    todo_id: &str,
    primary_agent_id: Option<String>,
    primary_agent_switch_reason: Option<String>,
    event_id: String,
) -> Result<(), String> {
    let selected_title = task
        .plan_todos
        .iter()
        .find(|todo| todo.id == todo_id)
        .map(|todo| todo.title.clone())
        .ok_or_else(|| "cannot start todo because it does not exist".to_string())?;

    let previous_agent_id = task.primary_agent_id.clone();
    let switching_agent = previous_agent_id.is_some()
        && primary_agent_id.is_some()
        && previous_agent_id != primary_agent_id;
    let switch_reason = primary_agent_switch_reason
        .as_deref()
        .map(str::trim)
        .filter(|reason| !reason.is_empty());
    if switching_agent && switch_reason.is_none_or(|reason| reason.len() < 5) {
        return Err("switching the primary Agent requires a specific reason".to_string());
    }

    task.status = transition(task.status, TaskAction::TodoStarted)?;
    if let Some(primary_agent_id) = primary_agent_id {
        task.primary_agent_id = Some(primary_agent_id);
    }
    task.plan_todos = task
        .plan_todos
        .drain(..)
        .map(|todo| {
            if todo.id == todo_id {
                PlanTodoItem {
                    status: PlanTodoStatus::Implementing,
                    ..todo
                }
            } else if todo.status == "implementing" {
                PlanTodoItem {
                    status: PlanTodoStatus::Pending,
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
        status: task.status,
        input_summary: Some(format!("Started implementation todo: {selected_title}")),
        output_summary: Some(if switching_agent {
            format!(
                "Primary Agent switched from {} to {}: {}",
                previous_agent_id.as_deref().unwrap_or("none"),
                task.primary_agent_id.as_deref().unwrap_or("none"),
                switch_reason.unwrap_or("reason missing")
            )
        } else {
            "Implementation scope selected and ready for Agent handoff.".to_string()
        }),
        evidence_ref: task.final_plan_path.clone(),
    });

    Ok(())
}

pub(super) fn apply_complete_todo(
    task: &mut Task,
    todo_id: &str,
    event_id: String,
) -> Result<(), String> {
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
                    status: PlanTodoStatus::Done,
                    ..todo
                }
            } else {
                todo
            }
        })
        .collect();
    let all_done = task.plan_todos.iter().all(|todo| todo.status == "done");
    task.status = transition(task.status, TaskAction::TodoCompleted { all_done })?;
    task.updated_at_ms = now_ms();
    task.events.push(TaskEvent {
        id: event_id,
        task_id: task.id.clone(),
        timestamp_ms: task.updated_at_ms,
        actor: "user".to_string(),
        status: task.status,
        input_summary: Some(format!("Completed implementation todo: {selected_title}")),
        output_summary: Some("Todo marked done and ready for review evidence handoff.".to_string()),
        evidence_ref: task.final_plan_path.clone(),
    });

    Ok(())
}

pub(super) fn apply_mark_ready_for_testing(
    task: &mut Task,
    event_id: String,
) -> Result<(), String> {
    if task.status != "reviewing" {
        return Err(format!(
            "cannot mark task ready for testing because it is {} instead of reviewing",
            task.status
        ));
    }

    if task.plan_todos.iter().any(|todo| todo.status != "done") {
        return Err("cannot mark task ready for testing before all todos are done".to_string());
    }

    implementation_review::ensure_review_gate(task)?;

    task.status = transition(task.status, TaskAction::TestingRequested)?;
    task.updated_at_ms = now_ms();
    task.events.push(TaskEvent {
        id: event_id,
        task_id: task.id.clone(),
        timestamp_ms: task.updated_at_ms,
        actor: "user".to_string(),
        status: task.status,
        input_summary: Some("Marked task ready for testing".to_string()),
        output_summary: Some(
            "Implementation review completed; task is ready for debug validation.".to_string(),
        ),
        evidence_ref: task.final_plan_path.clone(),
    });

    Ok(())
}

pub(super) fn apply_complete_task(task: &mut Task, event_id: String) -> Result<(), String> {
    if task.status != "verifying" {
        return Err(format!(
            "cannot complete task because it is {} instead of verifying",
            task.status
        ));
    }

    let latest_validation = task
        .command_runs
        .iter()
        .filter(|run| {
            matches!(
                run.intent,
                CommandRunIntent::Validation | CommandRunIntent::Legacy
            )
        })
        .max_by_key(|run| run.started_at_ms)
        .ok_or_else(|| "cannot complete task without validation evidence".to_string())?;
    if latest_validation.status != CommandRunStatus::Succeeded {
        return Err(format!(
            "cannot complete task because latest validation '{}' is {}",
            latest_validation.command, latest_validation.status
        ));
    }
    let latest_successful_run_id = Some(latest_validation.id.clone());

    task.status = transition(task.status, TaskAction::Accepted)?;
    task.updated_at_ms = now_ms();
    task.events.push(TaskEvent {
        id: event_id,
        task_id: task.id.clone(),
        timestamp_ms: task.updated_at_ms,
        actor: "user".to_string(),
        status: task.status,
        input_summary: Some("Accepted verification and completed task".to_string()),
        output_summary: Some(
            "Task completed after human acceptance of verification evidence.".to_string(),
        ),
        evidence_ref: latest_successful_run_id,
    });

    Ok(())
}
