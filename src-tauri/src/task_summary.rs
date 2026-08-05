use crate::{
    agents,
    models::{
        now_ms, CommandRunIntent, IdGenerator, Task, TaskEvent, TaskStatus, TaskSummary,
        TaskSummaryDecision, TaskSummaryReview, TaskSummaryValidation,
    },
    project_git, storage, tasks,
};
use serde::Deserialize;
use std::{fs, path::Path};
use tauri::State;

fn clean(value: &str) -> String {
    agents::redact_sensitive_text(value)
}

fn markdown_inline(value: &str) -> String {
    clean(value).replace('`', "'").replace('\n', " ")
}

fn build_summary(task: &Task) -> TaskSummary {
    let (mut changed_files, git_failure) = project_git::collect_changed_files(task);
    for file in &mut changed_files {
        file.path = clean(&file.path);
        file.status = clean(&file.status);
    }
    let total_additions = changed_files.iter().filter_map(|file| file.additions).sum();
    let total_deletions = changed_files.iter().filter_map(|file| file.deletions).sum();

    let mut decisions = task
        .planning_decisions
        .iter()
        .map(|decision| TaskSummaryDecision {
            kind: "planning".to_string(),
            title: clean(&decision.title),
            detail: clean(&decision.content),
        })
        .collect::<Vec<_>>();
    for decision in &task.implementation_review_decisions {
        let finding_title = task
            .implementation_reviews
            .iter()
            .flat_map(|review| &review.findings)
            .find(|finding| finding.id == decision.finding_id)
            .map(|finding| finding.title.as_str())
            .unwrap_or("Implementation Review finding");
        decisions.push(TaskSummaryDecision {
            kind: "implementation_review".to_string(),
            title: clean(finding_title),
            detail: clean(&format!("{}: {}", decision.decision, decision.reason)),
        });
    }

    let reviews = task
        .implementation_reviews
        .iter()
        .map(|review| TaskSummaryReview {
            reviewer: clean(&review.reviewer_agent_name),
            status: clean(&review.status),
            summary: clean(&review.summary),
            finding_count: review.findings.len(),
            evidence_ref: review.evidence_ref.as_deref().map(clean),
        })
        .collect::<Vec<_>>();
    let validation_evidence = task
        .command_runs
        .iter()
        .filter(|run| {
            matches!(
                run.intent,
                CommandRunIntent::Validation | CommandRunIntent::Legacy
            )
        })
        .rev()
        .take(20)
        .map(|run| TaskSummaryValidation {
            run_id: run.id.clone(),
            command: clean(&run.command),
            status: run.status,
            exit_code: run.exit_code,
            stdout_log_ref: run.stdout_log_ref.as_deref().map(clean),
            stderr_log_ref: run.stderr_log_ref.as_deref().map(clean),
            started_at_ms: run.started_at_ms,
            ended_at_ms: run.ended_at_ms,
        })
        .collect::<Vec<_>>();

    let mut remaining_risks = Vec::new();
    let mut recommendations = Vec::new();
    for finding in task
        .implementation_reviews
        .iter()
        .flat_map(|review| &review.findings)
    {
        let item = clean(&format!(
            "{}: {} — {}",
            finding.severity, finding.title, finding.detail
        ));
        if matches!(finding.severity.as_str(), "blocker" | "risk")
            && !matches!(finding.status.as_str(), "dismissed")
        {
            remaining_risks.push(item);
        } else if finding.severity == "suggestion" {
            recommendations.push(item);
        }
    }
    if let Some(error) = git_failure {
        remaining_risks.push(clean(&format!(
            "Git attribution unavailable; changed files use evidence fallback: {error}"
        )));
    }
    if changed_files
        .iter()
        .any(|file| file.attribution == "pre_existing")
    {
        remaining_risks.push(
            "The worktree contained pre-existing changes at implementation start; those files are labeled pre_existing."
                .to_string(),
        );
    }
    if remaining_risks.is_empty() {
        remaining_risks.push("No recorded remaining risks.".to_string());
    }
    if recommendations.is_empty() {
        recommendations.push(
            "Keep the persisted validation commands green after follow-up changes.".to_string(),
        );
    }

    let summary_dir = storage::project_tasks_dir(Path::new(&task.project_path)).join(&task.id);
    TaskSummary {
        task_id: task.id.clone(),
        title: clean(&task.title),
        requirement: clean(&task.raw_requirement),
        completed_todos: task
            .plan_todos
            .iter()
            .filter(|todo| todo.status == "done")
            .map(|todo| clean(&todo.title))
            .collect(),
        changed_files,
        total_additions,
        total_deletions,
        decisions,
        reviews,
        validation_evidence,
        remaining_risks,
        recommendations,
        generated_at_ms: now_ms(),
        json_path: summary_dir.join("summary.json").display().to_string(),
        markdown_path: summary_dir.join("summary.md").display().to_string(),
    }
}

fn render_markdown(summary: &TaskSummary) -> String {
    let todos = summary
        .completed_todos
        .iter()
        .map(|todo| format!("- [x] {}", markdown_inline(todo)))
        .collect::<Vec<_>>()
        .join("\n");
    let files = summary
        .changed_files
        .iter()
        .map(|file| {
            format!(
                "- `{}` — {} — +{}/-{} — {}",
                markdown_inline(&file.path),
                markdown_inline(&file.status),
                file.additions
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "?".to_string()),
                file.deletions
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "?".to_string()),
                markdown_inline(&file.attribution)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let decisions = summary
        .decisions
        .iter()
        .map(|decision| {
            format!(
                "- **{}** ({}): {}",
                markdown_inline(&decision.title),
                markdown_inline(&decision.kind),
                markdown_inline(&decision.detail)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let reviews = summary
        .reviews
        .iter()
        .map(|review| {
            format!(
                "- **{}** — {} — {} finding(s): {}",
                markdown_inline(&review.reviewer),
                markdown_inline(&review.status),
                review.finding_count,
                markdown_inline(&review.summary)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let validations = summary
        .validation_evidence
        .iter()
        .map(|run| {
            format!(
                "- `{}` — {} — exit {:?} — stdout: `{}` — stderr: `{}`",
                markdown_inline(&run.command),
                run.status,
                run.exit_code,
                markdown_inline(run.stdout_log_ref.as_deref().unwrap_or("none")),
                markdown_inline(run.stderr_log_ref.as_deref().unwrap_or("none"))
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let risks = summary
        .remaining_risks
        .iter()
        .map(|risk| format!("- {}", markdown_inline(risk)))
        .collect::<Vec<_>>()
        .join("\n");
    let recommendations = summary
        .recommendations
        .iter()
        .map(|item| format!("- {}", markdown_inline(item)))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "# {}\n\nGenerated: {}\n\n## Requirement\n\n{}\n\n## Completed Todos\n\n{}\n\n## Changed Files\n\nTotal: +{} / -{}\n\n{}\n\n## Key Decisions\n\n{}\n\n## Implementation Reviews\n\n{}\n\n## Validation Evidence\n\n{}\n\n## Remaining Risks\n\n{}\n\n## Recommendations\n\n{}\n",
        markdown_inline(&summary.title),
        summary.generated_at_ms,
        clean(&summary.requirement),
        if todos.is_empty() { "- None" } else { &todos },
        summary.total_additions,
        summary.total_deletions,
        if files.is_empty() { "- None captured" } else { &files },
        if decisions.is_empty() { "- None" } else { &decisions },
        if reviews.is_empty() { "- None" } else { &reviews },
        if validations.is_empty() { "- None" } else { &validations },
        risks,
        recommendations,
    )
}

pub fn generate_and_persist(task: &Task) -> Result<TaskSummary, String> {
    let summary = build_summary(task);
    let markdown = render_markdown(&summary);
    storage::atomic_write_json(Path::new(&summary.json_path), &summary)?;
    storage::atomic_write_text(Path::new(&summary.markdown_path), &markdown)?;
    Ok(summary)
}

#[tauri::command]
pub fn regenerate_task_summary(
    ids: State<'_, IdGenerator>,
    project_path: String,
    task_id: String,
) -> Result<Task, String> {
    let mut task = tasks::load_task(Path::new(&project_path), &task_id)?;
    if task.status != TaskStatus::Completed {
        return Err("task summary can only be regenerated for a completed task".to_string());
    }
    let summary = generate_and_persist(&task)?;
    task.updated_at_ms = now_ms();
    task.events.push(TaskEvent {
        id: ids.next("event"),
        task_id: task.id.clone(),
        timestamp_ms: task.updated_at_ms,
        actor: "user".to_string(),
        status: task.status,
        input_summary: Some("Regenerated delivery summary".to_string()),
        output_summary: Some("JSON and Markdown delivery artifacts updated".to_string()),
        evidence_ref: Some(summary.markdown_path.clone()),
    });
    task.summary = Some(summary);
    tasks::save_task(&task)?;
    Ok(task)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportTaskSummaryInput {
    pub project_path: String,
    pub task_id: String,
    pub target_path: String,
    pub format: String,
}

#[tauri::command]
pub fn export_task_summary(input: ExportTaskSummaryInput) -> Result<String, String> {
    let task = tasks::load_task(Path::new(&input.project_path), &input.task_id)?;
    let summary = task
        .summary
        .ok_or_else(|| "task has no persisted delivery summary".to_string())?;
    let source = match input.format.as_str() {
        "markdown" => summary.markdown_path,
        "json" => summary.json_path,
        _ => return Err("summary export format must be 'markdown' or 'json'".to_string()),
    };
    let content = fs::read_to_string(&source)
        .map_err(|error| format!("failed to read summary artifact: {error}"))?;
    let target = Path::new(&input.target_path);
    if target.file_name().is_none() {
        return Err("summary export target must be a file".to_string());
    }
    storage::atomic_write_text(target, &content)?;
    Ok(target.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_renderer_does_not_expose_token_values() {
        let summary = TaskSummary {
            task_id: "task-1".to_string(),
            title: "Delivery".to_string(),
            requirement: "Authorization: Bearer secret-value".to_string(),
            completed_todos: Vec::new(),
            changed_files: Vec::new(),
            total_additions: 0,
            total_deletions: 0,
            decisions: Vec::new(),
            reviews: Vec::new(),
            validation_evidence: Vec::new(),
            remaining_risks: vec!["No recorded remaining risks.".to_string()],
            recommendations: vec!["Keep checks green.".to_string()],
            generated_at_ms: 1,
            json_path: "/tmp/summary.json".to_string(),
            markdown_path: "/tmp/summary.md".to_string(),
        };
        let markdown = render_markdown(&summary);
        assert!(!markdown.contains("secret-value"));
        assert!(markdown.contains("[REDACTED]"));
    }
}
