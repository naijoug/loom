use crate::{
    models::{now_ms, CommandRunStatus, LoopTraceEntry, Task, TaskEvent},
    process_supervisor::{self, ProcessKind},
};
use std::{collections::HashSet, fs, path::Path, sync::Mutex};

#[derive(Default)]
pub struct RunRecoveryRegistry {
    reconciled_projects: Mutex<HashSet<String>>,
}

impl RunRecoveryRegistry {
    pub fn begin_project(&self, project_path: &Path) -> Result<bool, String> {
        let canonical = fs::canonicalize(project_path).map_err(|error| {
            format!(
                "failed to normalize project path '{}' for run recovery: {error}",
                project_path.display()
            )
        })?;
        let mut projects = self
            .reconciled_projects
            .lock()
            .map_err(|_| "run recovery registry is unavailable".to_string())?;
        Ok(projects.insert(canonical.display().to_string()))
    }
}

pub fn reconcile_task(task: &mut Task) -> usize {
    let timestamp_ms = now_ms();
    let active_processes = process_supervisor::supervisor().task_runs(
        &task.id,
        &[
            ProcessKind::Command,
            ProcessKind::ImplementationReview,
            ProcessKind::Pty,
        ],
    );
    let active_command_run_ids = active_processes
        .iter()
        .filter(|process| matches!(process.kind, ProcessKind::Command | ProcessKind::Pty))
        .map(|process| process.run_id.as_str())
        .collect::<HashSet<_>>();
    let has_active_review = active_processes
        .iter()
        .any(|process| process.kind == ProcessKind::ImplementationReview);
    let interrupted_runs = task
        .command_runs
        .iter_mut()
        .filter(|run| {
            run.status == CommandRunStatus::Running
                && !active_command_run_ids.contains(run.id.as_str())
        })
        .map(|run| {
            run.status = CommandRunStatus::Interrupted;
            run.ended_at_ms = Some(timestamp_ms);
            run.termination_reason = Some("app_restarted".to_string());
            run.clone()
        })
        .collect::<Vec<_>>();

    for run in &interrupted_runs {
        task.events.push(TaskEvent {
            id: format!("event-{timestamp_ms}-recovery-{}", run.id),
            task_id: task.id.clone(),
            timestamp_ms,
            actor: "system".to_string(),
            status: task.status,
            input_summary: Some(run.command.clone()),
            output_summary: Some(if run.session_id.is_some() {
                "Run was interrupted by an application restart; its native Agent session can be resumed explicitly."
                    .to_string()
            } else {
                "Run was interrupted by an application restart and can be retried safely."
                    .to_string()
            }),
            evidence_ref: Some(run.id.clone()),
        });
        if let Some(loop_id) = &run.loop_id {
            task.loop_trace.push(LoopTraceEntry {
                id: format!("trace-{timestamp_ms}-recovery-{}", run.id),
                task_id: task.id.clone(),
                loop_id: loop_id.clone(),
                stage: "recovery".to_string(),
                entry_type: "run_interrupted".to_string(),
                iteration: run.iteration,
                attempt: run.attempt,
                context_summary: "Application restarted while the command was running.".to_string(),
                action_summary: run.command.clone(),
                verification_summary:
                    "The orphaned run was marked interrupted; no success or failure was inferred."
                        .to_string(),
                command_run_id: Some(run.id.clone()),
                fingerprint: None,
                termination_reason: Some("app_restarted".to_string()),
                token_usage: None,
                timestamp_ms,
            });
        }
    }

    let interrupted_review_run_ids = task
        .implementation_review_runs
        .iter_mut()
        .filter(|run| run.status == "running" && !has_active_review)
        .map(|run| {
            run.status = "interrupted".to_string();
            run.ended_at_ms = Some(timestamp_ms);
            run.id.clone()
        })
        .collect::<Vec<_>>();
    for review in &mut task.implementation_reviews {
        if interrupted_review_run_ids.contains(&review.run_id) && review.status == "running" {
            review.status = "interrupted".to_string();
            review.ended_at_ms = Some(timestamp_ms);
        }
    }
    for review_run_id in &interrupted_review_run_ids {
        task.events.push(TaskEvent {
            id: format!("event-{timestamp_ms}-review-recovery-{review_run_id}"),
            task_id: task.id.clone(),
            timestamp_ms,
            actor: "system".to_string(),
            status: task.status,
            input_summary: Some("Implementation Review".to_string()),
            output_summary: Some(
                "Implementation Review was interrupted by an application restart and must be run again."
                    .to_string(),
            ),
            evidence_ref: Some(review_run_id.clone()),
        });
    }

    if !interrupted_runs.is_empty() || !interrupted_review_run_ids.is_empty() {
        task.updated_at_ms = timestamp_ms;
    }
    interrupted_runs.len() + interrupted_review_run_ids.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{CommandRun, CommandRunIntent, PlanTodoItem, PlanTodoStatus, TaskStatus};

    fn task_fixture() -> Task {
        Task {
            id: "task-recovery".to_string(),
            project_path: "/repo".to_string(),
            title: "Recover runs".to_string(),
            raw_requirement: "Keep history across restart".to_string(),
            status: TaskStatus::Debugging,
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
                task_id: "task-recovery".to_string(),
                title: "Recover".to_string(),
                description: "Recover".to_string(),
                status: PlanTodoStatus::Done,
                order: 0,
                plan_ref: None,
            }],
            loop_trace: Vec::new(),
            events: Vec::new(),
            command_runs: vec![
                CommandRun {
                    id: "run-running".to_string(),
                    task_id: "task-recovery".to_string(),
                    command: "pnpm test".to_string(),
                    cwd: "/repo".to_string(),
                    intent: CommandRunIntent::Validation,
                    loop_id: Some("loop-1".to_string()),
                    iteration: Some(1),
                    attempt: Some(0),
                    termination_reason: None,
                    session_id: Some("session-1".to_string()),
                    resume_command: Some("codex resume session-1".to_string()),
                    started_at_ms: 1,
                    ended_at_ms: None,
                    status: CommandRunStatus::Running,
                    exit_code: None,
                    stdout_log_ref: None,
                    stderr_log_ref: None,
                    error_summary: None,
                },
                CommandRun {
                    id: "run-finished".to_string(),
                    task_id: "task-recovery".to_string(),
                    command: "pnpm build".to_string(),
                    cwd: "/repo".to_string(),
                    intent: CommandRunIntent::Validation,
                    loop_id: None,
                    iteration: None,
                    attempt: None,
                    termination_reason: None,
                    session_id: None,
                    resume_command: None,
                    started_at_ms: 2,
                    ended_at_ms: Some(3),
                    status: CommandRunStatus::Succeeded,
                    exit_code: Some(0),
                    stdout_log_ref: None,
                    stderr_log_ref: None,
                    error_summary: None,
                },
            ],
            feedback: Vec::new(),
            loop_compact_summary: None,
            repair_context_preview: None,
            git_baseline: None,
            summary: None,
            created_at_ms: 1,
            updated_at_ms: 2,
        }
    }

    #[test]
    fn marks_only_orphaned_running_commands_as_interrupted() {
        let mut task = task_fixture();

        assert_eq!(reconcile_task(&mut task), 1);
        assert_eq!(task.command_runs[0].status, CommandRunStatus::Interrupted);
        assert_eq!(
            task.command_runs[0].termination_reason.as_deref(),
            Some("app_restarted")
        );
        assert_eq!(task.command_runs[1].status, CommandRunStatus::Succeeded);
        assert_eq!(task.events.len(), 1);
        assert_eq!(task.loop_trace.len(), 1);
        assert!(task.events[0]
            .output_summary
            .as_deref()
            .is_some_and(|summary| summary.contains("resumed")));

        assert_eq!(reconcile_task(&mut task), 0);
        assert_eq!(task.events.len(), 1);
    }

    #[test]
    fn leaves_runs_registered_in_the_current_process_untouched() {
        let mut task = task_fixture();
        task.id = "task-active-recovery".to_string();
        task.command_runs[0].task_id = task.id.clone();
        task.command_runs[1].task_id = task.id.clone();
        task.implementation_review_runs
            .push(crate::models::ImplementationReviewRun {
                id: "review-run-active".to_string(),
                task_id: task.id.clone(),
                reviewer_agent_ids: vec!["reviewer".to_string()],
                status: "running".to_string(),
                context_ref: "/repo/.loom/review.md".to_string(),
                review_ids: Vec::new(),
                started_at_ms: 1,
                ended_at_ms: None,
            });
        let supervisor = process_supervisor::supervisor();
        supervisor
            .register(crate::process_supervisor::ProcessMetadata::new(
                "run-running",
                &task.id,
                ProcessKind::Command,
                (i32::MAX as u32) - 1,
                None,
            ))
            .unwrap();
        supervisor
            .register(crate::process_supervisor::ProcessMetadata::new(
                "review-active",
                &task.id,
                ProcessKind::ImplementationReview,
                i32::MAX as u32,
                None,
            ))
            .unwrap();

        assert_eq!(reconcile_task(&mut task), 0);
        assert_eq!(task.command_runs[0].status, CommandRunStatus::Running);
        assert_eq!(task.implementation_review_runs[0].status, "running");

        supervisor.complete("run-running");
        supervisor.complete("review-active");
    }

    #[test]
    fn marks_orphaned_implementation_reviews_as_interrupted() {
        let mut task = task_fixture();
        task.command_runs.clear();
        task.implementation_review_runs
            .push(crate::models::ImplementationReviewRun {
                id: "review-run-1".to_string(),
                task_id: task.id.clone(),
                reviewer_agent_ids: vec!["reviewer".to_string()],
                status: "running".to_string(),
                context_ref: "/repo/.loom/review.md".to_string(),
                review_ids: Vec::new(),
                started_at_ms: 1,
                ended_at_ms: None,
            });

        assert_eq!(reconcile_task(&mut task), 1);
        assert_eq!(task.implementation_review_runs[0].status, "interrupted");
        assert!(task.implementation_review_runs[0].ended_at_ms.is_some());
        assert!(task.events[0]
            .output_summary
            .as_deref()
            .unwrap_or_default()
            .contains("must be run again"));
    }
}
