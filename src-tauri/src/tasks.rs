use crate::{
    models::{
        now_ms, CommandRun, CreateTaskInput, ErrorSummary, FeedbackInput, IdGenerator, Task,
        TaskEvent, UserFeedback,
    },
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
        selected_planning_agent_ids: Vec::new(),
        primary_agent_id: None,
        review_agent_ids: Vec::new(),
        final_plan: None,
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
    let context = format!(
        "Task: {}\n\nRequirement:\n{}\n\nFinal plan:\n{}\n\nLatest failure:\n{}\n\nLatest feedback:\n{}",
        task.title,
        task.raw_requirement,
        task.final_plan.clone().unwrap_or_else(|| "(none)".to_string()),
        latest_failed_run
            .and_then(|run| run.error_summary)
            .map(format_error_summary)
            .unwrap_or_else(|| "(none)".to_string()),
        latest_feedback.unwrap_or_else(|| "(none)".to_string())
    );
    task.repair_context_preview = Some(context);
    task.updated_at_ms = now_ms();
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

fn format_error_summary(summary: ErrorSummary) -> String {
    format!(
        "exitCode={:?}\nmatchedLines={}\nstderrTail={}",
        summary.exit_code,
        summary.matched_lines.join("\n"),
        summary.stderr_tail.join("\n")
    )
}
