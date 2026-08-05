use super::*;

#[tauri::command]
pub fn append_feedback(ids: State<'_, IdGenerator>, input: FeedbackInput) -> Result<Task, String> {
    let task = load_task(Path::new(&input.project_path), &input.task_id)?;
    ensure_task_active(&task)?;
    if input.content.trim().is_empty()
        && input
            .reproduction_steps
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
        && input
            .expected_behavior
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
        && input
            .quoted_log
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
        && input.attachment_paths.is_empty()
    {
        return Err("feedback requires an issue, reproduction steps, expected behavior, log quote, or attachment".to_string());
    }
    if input
        .command_run_id
        .as_ref()
        .is_some_and(|run_id| task.command_runs.iter().all(|run| run.id != *run_id))
    {
        return Err("feedback commandRunId does not belong to this task".to_string());
    }
    let attachments = attachments::copy_feedback_attachments(
        Path::new(&input.project_path),
        &task.id,
        ids.inner(),
        &input.attachment_paths,
    )?;
    let feedback = UserFeedback {
        id: ids.next("feedback"),
        task_id: task.id.clone(),
        command_run_id: input.command_run_id.clone(),
        content: input.content.trim().to_string(),
        reproduction_steps: input
            .reproduction_steps
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        expected_behavior: input
            .expected_behavior
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        quoted_log: input
            .quoted_log
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        attachments,
        timestamp_ms: now_ms(),
    };
    let feedback_summary = format_feedback_for_repair(&feedback);
    let attachment_evidence = feedback
        .attachments
        .first()
        .map(|attachment| attachment.stored_path.clone());
    let command_run_id = input.command_run_id;
    let (task, ()) = update_task(
        Path::new(&input.project_path),
        &input.task_id,
        move |task| {
            ensure_task_active(task)?;
            if command_run_id
                .as_ref()
                .is_some_and(|run_id| task.command_runs.iter().all(|run| run.id != *run_id))
            {
                return Err("feedback commandRunId does not belong to this task".to_string());
            }
            task.feedback.push(feedback);
            let timestamp_ms = now_ms();
            task.events.push(TaskEvent {
                id: ids.next("event"),
                task_id: task.id.clone(),
                timestamp_ms,
                actor: "user".to_string(),
                status: task.status,
                input_summary: Some(compact_summary_line(&feedback_summary, 500)),
                output_summary: Some("Manual feedback captured".to_string()),
                evidence_ref: attachment_evidence.or(command_run_id),
            });
            task.updated_at_ms = timestamp_ms;
            Ok(())
        },
    )?;
    Ok(task)
}

#[tauri::command]
pub fn generate_repair_context(project_path: String, task_id: String) -> Result<Task, String> {
    let (task, ()) = update_task(Path::new(&project_path), &task_id, |task| {
        ensure_task_active(task)?;
        let latest_failed_run = task
            .command_runs
            .iter()
            .rev()
            .find(|run| run.status == "failed")
            .cloned();
        let latest_feedback = task.feedback.last().map(format_feedback_for_repair);
        let current_todo_context = format_current_todo_context(&task.plan_todos);
        let compact_summary = format_loop_compact_summary(
            task,
            latest_failed_run.as_ref(),
            latest_feedback.as_deref(),
            &current_todo_context,
        );
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
        let keep_implementation_stage = latest_failed_run.as_ref().is_some_and(|run| {
            run.intent == CommandRunIntent::Validation
                && run.loop_id.is_some()
                && matches!(task.status.as_str(), "implementing" | "reviewing")
        });
        task.repair_context_preview = Some(context);
        task.loop_compact_summary = Some(compact_summary);
        if !keep_implementation_stage {
            task.status = transition(task.status, TaskAction::RepairStarted)?;
        }
        task.updated_at_ms = now_ms();
        task.events.push(TaskEvent {
            id: format!("event-{}-repair-context", task.updated_at_ms),
            task_id: task.id.clone(),
            timestamp_ms: task.updated_at_ms,
            actor: "system".to_string(),
            status: task.status,
            input_summary: Some("Generated repair context for Agent handoff".to_string()),
            output_summary: Some(
                "Latest failure, user feedback, and current implementation scope were packaged for a scoped fix."
                    .to_string(),
            ),
            evidence_ref,
        });
        Ok(())
    })?;
    Ok(task)
}

fn format_feedback_for_repair(feedback: &UserFeedback) -> String {
    let attachments = feedback
        .attachments
        .iter()
        .map(|attachment| {
            format!(
                "- {} ({} bytes, {}) => {}",
                attachment.name,
                attachment.size_bytes,
                attachment.mime_type,
                attachment.stored_path
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    [
        (!feedback.content.is_empty()).then(|| format!("Issue:\n{}", feedback.content)),
        feedback
            .reproduction_steps
            .as_ref()
            .map(|value| format!("Reproduction steps:\n{value}")),
        feedback
            .expected_behavior
            .as_ref()
            .map(|value| format!("Expected behavior:\n{value}")),
        feedback
            .quoted_log
            .as_ref()
            .map(|value| format!("Quoted log:\n{value}")),
        (!attachments.is_empty()).then(|| format!("Attachments:\n{attachments}")),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join("\n\n")
}

fn format_loop_compact_summary(
    task: &Task,
    latest_failed_run: Option<&CommandRun>,
    latest_feedback: Option<&str>,
    current_todo_context: &str,
) -> String {
    let failure = latest_failed_run
        .map(|run| {
            format!(
                "run={} intent={:?} status={} exit={:?} command={}",
                run.id, run.intent, run.status, run.exit_code, run.command
            )
        })
        .unwrap_or_else(|| "none".to_string());

    format!(
        "Task status: {}. Current scope: {}. Latest failure: {}. Latest feedback: {}.",
        task.status,
        compact_summary_line(current_todo_context, 500),
        failure,
        latest_feedback
            .map(|feedback| compact_summary_line(feedback, 500))
            .unwrap_or_else(|| "none".to_string())
    )
}

pub(super) fn compact_summary_line(value: &str, limit: usize) -> String {
    let compacted = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if compacted.chars().count() > limit {
        format!(
            "{}...",
            compacted
                .chars()
                .take(limit.saturating_sub(3))
                .collect::<String>()
        )
    } else {
        compacted
    }
}
