use crate::{
    models::{
        now_ms, CommandRun, CreateTaskInput, ErrorSummary, FeedbackInput, IdGenerator,
        PlanTodoItem, Task, TaskEvent, UserFeedback,
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
        final_plan_path: None,
        discussion_summary: None,
        planning_runs: Vec::new(),
        agent_invocations: Vec::new(),
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
        "Task: {}\n\nRequirement:\n{}\n\nFinal plan:\n{}\n\nLatest failure:\n{}\n\nLatest feedback:\n{}\n\nRepair handoff checklist:\n- Reproduce or explain the failure using the command and log references above.\n- Apply the smallest safe fix for the current task.\n- Re-run the most relevant validation command and attach the result.",
        task.title,
        task.raw_requirement,
        task.final_plan.clone().unwrap_or_else(|| "(none)".to_string()),
        latest_failed_run
            .as_ref()
            .map(format_failed_run_context)
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
    let mut todos = Vec::new();

    for line in final_plan.lines() {
        let trimmed = line.trim();
        if let Some(level) = markdown_heading_level(trimmed) {
            if in_section
                && section_level
                    .is_some_and(|section_level| level > section_level && section_level <= 2)
            {
                continue;
            }

            in_section = is_implementation_todo_heading(trimmed);
            section_level = in_section.then_some(level);
            continue;
        }

        if !in_section {
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
                || heading.contains("checklist")))
        || (heading.contains("任务")
            && (heading.contains("实施")
                || heading.contains("实现")
                || heading.contains("拆解")
                || heading.contains("具体")))
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
    )
}

fn strip_todo_marker(line: &str) -> Option<&str> {
    let bullet = line.strip_prefix("- ").or_else(|| line.strip_prefix("* "));
    if let Some(value) = bullet {
        return Some(strip_checkbox_marker(value.trim()));
    }

    let (number, rest) = line.split_once(". ").or_else(|| line.split_once(") "))?;
    if number.chars().all(|char| char.is_ascii_digit()) {
        Some(strip_checkbox_marker(rest.trim()))
    } else {
        None
    }
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
