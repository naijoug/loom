use crate::models::{CommandRun, PlanTodoItem, Task, UserFeedback};
use serde::{Deserialize, Serialize};

const DEFAULT_MAX_PROMPT_CHARS: usize = 12_000;
const MIN_PROMPT_CHARS: usize = 1_500;
const PLAN_SECTION_CHARS: usize = 4_000;
const SUMMARY_SECTION_CHARS: usize = 1_500;
const FEEDBACK_LIMIT: usize = 3;
const RUN_LIMIT: usize = 3;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextBuildOptions {
    #[serde(default)]
    pub max_prompt_chars: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextBuildOutput {
    pub prompt: String,
    pub context_summary: String,
    pub compact_summary: String,
    pub included_sections: Vec<String>,
    pub truncated: bool,
}

pub fn build_implementation_context(
    task: &Task,
    todo_id: &str,
    options: ContextBuildOptions,
) -> Result<ContextBuildOutput, String> {
    let todo_index = task
        .plan_todos
        .iter()
        .position(|todo| todo.id == todo_id)
        .ok_or_else(|| "cannot build context because todo does not exist".to_string())?;
    let todo = &task.plan_todos[todo_index];
    let max_prompt_chars = options
        .max_prompt_chars
        .unwrap_or(DEFAULT_MAX_PROMPT_CHARS)
        .max(MIN_PROMPT_CHARS);
    let mut truncated = false;
    let mut included_sections = vec!["task".to_string(), "todo".to_string(), "rules".to_string()];

    let context_summary = format_context_summary(task, todo, todo_index);
    let compact_summary = format_compact_summary(task, todo);
    let mut prompt = [
        "# Loom Implementation Handoff",
        "",
        "You are implementing one selected todo from a confirmed Loom plan.",
        "",
        &format!("Project: {}", task.project_path),
        &format!("Task: {}", task.title),
        &format!("Todo {}: {}", todo_index + 1, todo.title),
        "",
        "Context summary:",
        &context_summary,
        "",
        "Execution rules:",
        "- Modify only the files needed for this todo.",
        "- Preserve unrelated user changes.",
        "- Run the smallest relevant verification before reporting completion.",
        "- Report changed files, verification evidence, blockers, and remaining risk.",
        "",
        "Current todo:",
        &format_todo(todo),
        "",
    ]
    .join("\n");

    let mut remaining = max_prompt_chars.saturating_sub(char_count(&prompt));
    let sections = [
        (
            "recent feedback",
            format_feedback_section(&task.feedback),
            SUMMARY_SECTION_CHARS,
        ),
        (
            "recent command evidence",
            format_command_runs_section(&task.command_runs),
            SUMMARY_SECTION_CHARS,
        ),
        (
            "loop compact summary",
            task.loop_compact_summary.clone().unwrap_or_default(),
            SUMMARY_SECTION_CHARS,
        ),
        (
            "discussion summary",
            task.discussion_summary.clone().unwrap_or_default(),
            SUMMARY_SECTION_CHARS,
        ),
        (
            "confirmed plan excerpt",
            task.final_plan.clone().unwrap_or_default(),
            PLAN_SECTION_CHARS,
        ),
    ];

    for (name, content, section_limit) in sections {
        if content.trim().is_empty() || remaining == 0 {
            continue;
        }
        let section = format_section(name, &content, section_limit, &mut truncated);
        if section.trim().is_empty() {
            continue;
        }
        let section_chars = char_count(&section);
        let to_append = if section_chars > remaining {
            truncated = true;
            take_chars(&section, remaining)
        } else {
            section
        };
        if !to_append.trim().is_empty() {
            prompt.push_str(&to_append);
            included_sections.push(name.to_string());
            remaining = max_prompt_chars.saturating_sub(char_count(&prompt));
        }
    }

    if char_count(&prompt) > max_prompt_chars {
        truncated = true;
        prompt = take_chars(&prompt, max_prompt_chars);
    }

    Ok(ContextBuildOutput {
        prompt,
        context_summary,
        compact_summary,
        included_sections,
        truncated,
    })
}

fn format_context_summary(task: &Task, todo: &PlanTodoItem, todo_index: usize) -> String {
    [
        format!("- Task status: {}", task.status),
        format!("- Todo position: {}", todo_index + 1),
        format!("- Todo status: {}", todo.status),
        format!(
            "- Plan reference: {}",
            todo.plan_ref
                .as_deref()
                .or(task.final_plan_path.as_deref())
                .unwrap_or("(none)")
        ),
        format!(
            "- Recent command runs available: {}",
            task.command_runs.len().min(RUN_LIMIT)
        ),
        format!(
            "- Recent feedback items available: {}",
            task.feedback.len().min(FEEDBACK_LIMIT)
        ),
    ]
    .join("\n")
}

fn format_compact_summary(task: &Task, todo: &PlanTodoItem) -> String {
    format!(
        "Task `{}` / todo `{}`. Status: {}. Latest failure: {}. Latest feedback: {}. Previous loop memory: {}.",
        task.title,
        todo.title,
        task.status,
        task.command_runs
            .iter()
            .rev()
            .find(|run| run.status == "failed"
                || run
                    .error_summary
                    .as_ref()
                    .is_some_and(|summary| summary.failed))
            .map(|run| run.command.as_str())
            .unwrap_or("none"),
        task.feedback
            .last()
            .map(|feedback| feedback.content.as_str())
            .unwrap_or("none"),
        task.loop_compact_summary
            .as_deref()
            .map(|summary| compact_line(summary, 400))
            .unwrap_or_else(|| "none".to_string())
    )
}

fn format_todo(todo: &PlanTodoItem) -> String {
    [
        format!("id: {}", todo.id),
        format!("title: {}", todo.title),
        format!("description: {}", todo.description),
        format!("status: {}", todo.status),
        format!("planRef: {}", todo.plan_ref.as_deref().unwrap_or("(none)")),
    ]
    .join("\n")
}

fn format_feedback_section(feedback: &[UserFeedback]) -> String {
    feedback
        .iter()
        .rev()
        .take(FEEDBACK_LIMIT)
        .map(|feedback| {
            format!(
                "- feedbackId={} run={} content={} reproduction={} expected={} quotedLog={} attachments={}",
                feedback.id,
                feedback.command_run_id.as_deref().unwrap_or("(none)"),
                feedback.content,
                feedback.reproduction_steps.as_deref().unwrap_or("(none)"),
                feedback.expected_behavior.as_deref().unwrap_or("(none)"),
                feedback.quoted_log.as_deref().unwrap_or("(none)"),
                feedback
                    .attachments
                    .iter()
                    .map(|attachment| attachment.stored_path.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_command_runs_section(runs: &[CommandRun]) -> String {
    runs.iter()
        .rev()
        .take(RUN_LIMIT)
        .map(format_command_run)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn format_command_run(run: &CommandRun) -> String {
    let summary = run
        .error_summary
        .as_ref()
        .and_then(|summary| {
            summary
                .matched_lines
                .first()
                .or_else(|| summary.stderr_tail.last())
                .cloned()
        })
        .unwrap_or_else(|| "(none)".to_string());

    format!(
        "runId={}\nintent={:?}\ncommand={}\nstatus={}\nexitCode={:?}\nstdoutLog={}\nstderrLog={}\nerror={}",
        run.id,
        run.intent,
        run.command,
        run.status,
        run.exit_code,
        run.stdout_log_ref.as_deref().unwrap_or("(none)"),
        run.stderr_log_ref.as_deref().unwrap_or("(none)"),
        summary
    )
}

fn format_section(name: &str, content: &str, section_limit: usize, truncated: &mut bool) -> String {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let body = if char_count(trimmed) > section_limit {
        *truncated = true;
        format!(
            "{}\n\n[truncated: original section exceeded {} chars]",
            take_chars(trimmed, section_limit),
            section_limit
        )
    } else {
        trimmed.to_string()
    };

    format!("## {name}\n\n{body}\n\n")
}

fn char_count(value: &str) -> usize {
    value.chars().count()
}

fn compact_line(value: &str, limit: usize) -> String {
    let compacted = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if char_count(&compacted) > limit {
        format!("{}...", take_chars(&compacted, limit.saturating_sub(3)))
    } else {
        compacted
    }
}

fn take_chars(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{CommandRunIntent, CommandRunStatus, ErrorSummary, PlanTodoStatus};

    fn task_fixture(final_plan: String) -> Task {
        Task {
            id: "task-1".to_string(),
            project_path: "/repo".to_string(),
            title: "Refactor loop orchestration".to_string(),
            raw_requirement: "Implement loop engine refactor".to_string(),
            status: crate::models::TaskStatus::Implementing,
            lifecycle: Default::default(),
            selected_planning_agent_ids: Vec::new(),
            primary_agent_id: None,
            review_agent_ids: Vec::new(),
            implementation_review_runs: Vec::new(),
            implementation_reviews: Vec::new(),
            implementation_review_decisions: Vec::new(),
            final_plan: Some(final_plan),
            final_plan_path: Some("/repo/docs/plans/loop.md".to_string()),
            final_plan_html_path: None,
            discussion_summary: Some("Planning agreed on intent-first command runs.".to_string()),
            planning_runs: Vec::new(),
            agent_invocations: Vec::new(),
            plan_reviews: Vec::new(),
            planning_decisions: Vec::new(),
            plan_todos: vec![PlanTodoItem {
                id: "todo-1".to_string(),
                task_id: "task-1".to_string(),
                title: "Split command run intent".to_string(),
                description: "Add intent metadata and preserve validation gate behavior."
                    .to_string(),
                status: PlanTodoStatus::Implementing,
                order: 0,
                plan_ref: Some("/repo/docs/plans/loop.md".to_string()),
            }],
            loop_trace: Vec::new(),
            events: Vec::new(),
            command_runs: vec![CommandRun {
                id: "run-1".to_string(),
                task_id: "task-1".to_string(),
                command: "pnpm test".to_string(),
                cwd: "/repo".to_string(),
                intent: CommandRunIntent::Validation,
                loop_id: Some("loop-1".to_string()),
                iteration: Some(1),
                attempt: Some(0),
                termination_reason: None,
                session_id: None,
                resume_command: None,
                started_at_ms: 1,
                ended_at_ms: Some(2),
                status: CommandRunStatus::Failed,
                exit_code: Some(1),
                stdout_log_ref: Some("/repo/.loom/logs/run-1.stdout.log".to_string()),
                stderr_log_ref: Some("/repo/.loom/logs/run-1.stderr.log".to_string()),
                error_summary: Some(ErrorSummary {
                    exit_code: Some(1),
                    stderr_tail: vec!["error: regression failed".to_string()],
                    matched_lines: vec!["error: regression failed".to_string()],
                    failed: true,
                    ..Default::default()
                }),
            }],
            feedback: vec![UserFeedback {
                id: "feedback-1".to_string(),
                task_id: "task-1".to_string(),
                command_run_id: Some("run-1".to_string()),
                content: "The validation command is failing after the intent split.".to_string(),
                reproduction_steps: None,
                expected_behavior: None,
                quoted_log: None,
                attachments: Vec::new(),
                timestamp_ms: 3,
            }],
            loop_compact_summary: None,
            repair_context_preview: None,
            git_baseline: None,
            summary: None,
            created_at_ms: 1,
            updated_at_ms: 3,
        }
    }

    #[test]
    fn builds_budgeted_implementation_context() {
        let mut task = task_fixture("# Plan\n\nImplement everything.".to_string());
        task.loop_compact_summary =
            Some("Previous loop: validation failed after the first repair.".to_string());

        let output = build_implementation_context(&task, "todo-1", ContextBuildOptions::default())
            .expect("context should build");

        assert!(output.prompt.contains("# Loom Implementation Handoff"));
        assert!(output.prompt.contains("Split command run intent"));
        assert!(output.prompt.contains("error: regression failed"));
        assert!(output.prompt.contains("The validation command is failing"));
        assert!(output
            .included_sections
            .contains(&"recent command evidence".to_string()));
        assert!(output
            .included_sections
            .contains(&"loop compact summary".to_string()));
        assert!(output.prompt.contains("Previous loop: validation failed"));
        assert!(output.compact_summary.contains("Previous loop memory"));
        assert!(output.compact_summary.contains("pnpm test"));
    }

    #[test]
    fn truncates_large_plan_to_prompt_budget() {
        let final_plan = format!("# Plan\n\n{}", "very long plan section\n".repeat(1_000));
        let task = task_fixture(final_plan);

        let output = build_implementation_context(
            &task,
            "todo-1",
            ContextBuildOptions {
                max_prompt_chars: Some(2_000),
            },
        )
        .expect("context should build");

        assert!(output.truncated);
        assert!(char_count(&output.prompt) <= 2_000);
        assert!(output.prompt.contains("Split command run intent"));
    }

    #[test]
    fn rejects_unknown_todo() {
        let task = task_fixture("# Plan".to_string());

        let error = build_implementation_context(&task, "missing", ContextBuildOptions::default())
            .unwrap_err();

        assert!(error.contains("todo does not exist"));
    }
}
