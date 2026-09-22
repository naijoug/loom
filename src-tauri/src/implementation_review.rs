use crate::{
    agent_adapter::{self, AdapterInvocationRequest, AgentStage, PreparedAgentInvocation},
    agents,
    execution_policy::{self, ExecutionDecision, ExecutionRequest},
    models::{
        now_ms, IdGenerator, ImplementationReview, ImplementationReviewDecision,
        ImplementationReviewFinding, ImplementationReviewRun, Task, TaskEvent, TaskStatus,
    },
    process_supervisor::{self, ProcessKind, ProcessMetadata},
    storage, tasks,
};
use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};
use tauri::{AppHandle, State};
use tokio::{process::Command as TokioCommand, time::timeout};

const REVIEW_TIMEOUT_SECONDS: u64 = 300;
const MAX_CONTEXT_CHARS: usize = 90_000;

pub(crate) fn stop_task_runs(task_id: &str) -> Result<usize, String> {
    process_supervisor::supervisor()
        .stop_task_runs(
            task_id,
            &[ProcessKind::ImplementationReview],
            "task_lifecycle_changed",
        )
        .map(|runs| runs.len())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunImplementationReviewsInput {
    pub project_path: String,
    pub task_id: String,
    pub reviewer_agent_ids: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImplementationReviewDecisionInput {
    pub project_path: String,
    pub task_id: String,
    pub finding_id: String,
    pub decision: String,
    pub reason: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReviewPayload {
    #[serde(default)]
    summary: String,
    #[serde(default)]
    findings: Vec<ReviewFindingPayload>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReviewFindingPayload {
    severity: String,
    title: String,
    detail: String,
    #[serde(default)]
    file: Option<String>,
    #[serde(default)]
    line: Option<u32>,
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let mut truncated = value.chars().take(max_chars).collect::<String>();
    truncated.push_str("\n\n[truncated by Loom]");
    truncated
}

fn git_output(project_path: &Path, args: &[&str], max_chars: usize) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(project_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();
    match output {
        Ok(output) if output.status.success() => {
            truncate_chars(&String::from_utf8_lossy(&output.stdout), max_chars)
        }
        _ => "Unavailable (project is not a readable Git worktree).".to_string(),
    }
}

fn build_review_context(task: &Task) -> String {
    let project_path = Path::new(&task.project_path);
    let todo_summary = task
        .plan_todos
        .iter()
        .map(|todo| format!("- [{}] {} — {}", todo.status, todo.title, todo.description))
        .collect::<Vec<_>>()
        .join("\n");
    let command_summary = task
        .command_runs
        .iter()
        .rev()
        .take(20)
        .map(|run| {
            format!(
                "- {} | {} | exit={:?} | evidence={}",
                run.status,
                run.command,
                run.exit_code,
                run.stdout_log_ref
                    .as_deref()
                    .or(run.stderr_log_ref.as_deref())
                    .unwrap_or("none")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let decisions = task
        .planning_decisions
        .iter()
        .map(|decision| format!("- {}: {}", decision.title, decision.content))
        .collect::<Vec<_>>()
        .join("\n");
    let context = format!(
        "# Loom Implementation Review Context\n\n\
         ## Requirement\n\n{}\n\n\
         ## Confirmed Plan\n\n{}\n\n\
         ## Implementation Todos\n\n{}\n\n\
         ## Human Decisions\n\n{}\n\n\
         ## Git Status\n\n```text\n{}\n```\n\n\
         ## Git Diff Stat\n\n```text\n{}\n```\n\n\
         ## Git Diff (working tree and staged)\n\n```diff\n{}\n{}\n```\n\n\
         ## Recent Commands and Validation Evidence\n\n{}\n",
        task.raw_requirement,
        task.final_plan
            .as_deref()
            .unwrap_or("No final plan was persisted."),
        todo_summary,
        decisions,
        git_output(project_path, &["status", "--short"], 10_000),
        git_output(project_path, &["diff", "--stat"], 10_000),
        git_output(project_path, &["diff", "--no-ext-diff"], 45_000),
        git_output(project_path, &["diff", "--cached", "--no-ext-diff"], 20_000),
        command_summary,
    );
    truncate_chars(&context, MAX_CONTEXT_CHARS)
}

fn review_prompt(context: &str) -> String {
    format!(
        "You are an independent implementation reviewer. Review the actual code changes against the requirement and confirmed plan. Focus on correctness, security, regressions, missing tests, and unverifiable claims. Do not modify files or run destructive commands.\n\nReturn only one JSON object with this shape:\n{{\"summary\":\"short overall assessment\",\"findings\":[{{\"severity\":\"blocker|risk|suggestion|info\",\"title\":\"concise title\",\"detail\":\"specific evidence and remediation\",\"file\":\"optional relative path\",\"line\":123}}]}}\n\nUse blocker only when the task must not enter testing. If there are no findings, return an empty findings array.\n\n{context}"
    )
}

fn extract_json_object(raw: &str) -> Option<&str> {
    let start = raw.find('{')?;
    let end = raw.rfind('}')?;
    (end >= start).then_some(&raw[start..=end])
}

fn parse_review_payload(raw: &str) -> Result<ReviewPayload, String> {
    serde_json::from_str(raw)
        .or_else(|_| {
            let candidate = extract_json_object(raw).ok_or_else(|| {
                serde_json::Error::io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "no JSON object found",
                ))
            })?;
            serde_json::from_str(candidate)
        })
        .map_err(|error| format!("reviewer returned invalid JSON: {error}"))
}

fn normalize_review_output(output_mode: &str, raw_stdout: &str) -> String {
    agents::redact_sensitive_text(&agents::normalize_agent_stdout(output_mode, raw_stdout))
}

fn normalized_severity(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "blocker" => "blocker",
        "risk" => "risk",
        "suggestion" => "suggestion",
        _ => "info",
    }
}

async fn run_prepared_review(
    task_id: &str,
    agent_id: &str,
    prompt: &str,
    prepared: &PreparedAgentInvocation,
) -> Result<(String, String, Option<i32>), String> {
    let policy_args = prepared
        .args
        .iter()
        .map(|arg| arg.replace(prompt, "<loom-review-prompt>"))
        .collect();
    let assessment = execution_policy::evaluate_execution(
        &ExecutionRequest {
            program: prepared.program.clone(),
            args: policy_args,
            cwd: prepared.cwd.clone(),
            project_path: prepared.cwd.clone(),
            agent_id: Some(agent_id.to_string()),
        },
        true,
    );
    if assessment.decision != ExecutionDecision::Allowed {
        return Err(format!(
            "implementation review execution rejected by policy: {}",
            assessment.detail
        ));
    }

    let mut command = TokioCommand::new(&prepared.program);
    command
        .args(&prepared.args)
        .current_dir(&prepared.cwd)
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    process_supervisor::configure_process_group(&mut command);
    let mut operation = process_supervisor::supervisor().begin_operation()?;
    let child = command
        .spawn()
        .map_err(|error| format!("failed to start review Agent: {error}"))?;
    let process_id = child.id();
    let process_run_id = process_id.map(|process_id| format!("review-{process_id}"));
    if let (Some(process_id), Some(run_id)) = (process_id, process_run_id.as_deref()) {
        operation.register(ProcessMetadata::new(
            run_id,
            task_id,
            ProcessKind::ImplementationReview,
            process_id,
            Some(REVIEW_TIMEOUT_SECONDS * 1_000),
        ))?;
    }
    let output_result = timeout(
        Duration::from_secs(REVIEW_TIMEOUT_SECONDS),
        child.wait_with_output(),
    )
    .await;
    let output = match output_result {
        Ok(Ok(output)) => {
            if let Some(run_id) = process_run_id.as_deref() {
                let _ = process_supervisor::supervisor().force_stop(run_id);
                process_supervisor::supervisor().complete(run_id);
            }
            output
        }
        Ok(Err(error)) => {
            if let Some(run_id) = process_run_id.as_deref() {
                let _ = process_supervisor::supervisor().force_stop(run_id);
                process_supervisor::supervisor().complete(run_id);
            }
            return Err(format!("failed to wait for review Agent: {error}"));
        }
        Err(_) => {
            if let Some(run_id) = process_run_id.as_deref() {
                let _ = process_supervisor::supervisor().request_stop(run_id, "timeout");
                let _ = process_supervisor::supervisor().force_stop(run_id);
                process_supervisor::supervisor().complete(run_id);
            }
            return Err(format!(
                "review Agent timed out after {REVIEW_TIMEOUT_SECONDS}s"
            ));
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = agents::redact_sensitive_text(&String::from_utf8_lossy(&output.stderr));
    Ok((stdout, stderr, output.status.code()))
}

fn review_evidence_dir(project_path: &Path, task_id: &str, run_id: &str) -> PathBuf {
    storage::project_logs_dir(project_path)
        .join(task_id)
        .join("implementation-reviews")
        .join(run_id)
}

#[tauri::command]
pub async fn run_implementation_reviews(
    app: AppHandle,
    ids: State<'_, IdGenerator>,
    input: RunImplementationReviewsInput,
) -> Result<Task, String> {
    let project_path = Path::new(&input.project_path);
    let mut task = tasks::load_task(project_path, &input.task_id)?;
    tasks::ensure_task_active(&task)?;
    if task.status != TaskStatus::Reviewing {
        return Err("implementation Review can only run while the task is reviewing".to_string());
    }
    if task.plan_todos.iter().any(|todo| todo.status != "done") {
        return Err(
            "implementation Review requires all implementation todos to be done".to_string(),
        );
    }
    let primary_agent_id = task.primary_agent_id.clone().ok_or_else(|| {
        "implementation Review requires a primary implementation Agent".to_string()
    })?;
    let configured_agents = agents::load_agents(&app)?;
    let mut reviewers = Vec::new();
    for reviewer_id in &input.reviewer_agent_ids {
        if reviewer_id == &primary_agent_id {
            return Err("the primary implementation Agent cannot review its own work".to_string());
        }
        if reviewers
            .iter()
            .any(|reviewer: &crate::models::AgentConfig| reviewer.id == *reviewer_id)
        {
            continue;
        }
        let reviewer = configured_agents
            .iter()
            .find(|agent| agent.id == *reviewer_id)
            .cloned()
            .ok_or_else(|| format!("review Agent '{reviewer_id}' was not found"))?;
        agent_adapter::validate_stage_permissions(&reviewer, AgentStage::Review)?;
        reviewers.push(reviewer);
    }
    if reviewers.is_empty() {
        return Err("select at least one independent Review Agent".to_string());
    }

    let context = build_review_context(&task);
    let run_id = ids.next("implementation-review-run");
    let evidence_dir = review_evidence_dir(project_path, &task.id, &run_id);
    fs::create_dir_all(&evidence_dir)
        .map_err(|error| format!("failed to create implementation Review evidence: {error}"))?;
    let context_path = evidence_dir.join("context.md");
    storage::atomic_write_text(&context_path, &agents::redact_sensitive_text(&context))
        .map_err(|error| format!("failed to write implementation Review context: {error}"))?;
    let prompt = review_prompt(&context);
    let prepared_reviewers = reviewers
        .into_iter()
        .map(|reviewer| {
            let prepared = agent_adapter::prepare_invocation(
                &reviewer,
                &AdapterInvocationRequest {
                    project_path,
                    prompt: &prompt,
                    prompt_file: Some(&context_path),
                    stage: AgentStage::Review,
                    resume_command: None,
                    chat_permission_mode: None,
                    embed_prompt: true,
                },
            )?;
            Ok((reviewer, prepared))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let started_at_ms = now_ms();
    let mut review_run = ImplementationReviewRun {
        id: run_id.clone(),
        task_id: task.id.clone(),
        reviewer_agent_ids: prepared_reviewers
            .iter()
            .map(|(reviewer, _)| reviewer.id.clone())
            .collect(),
        status: "running".to_string(),
        context_ref: context_path.display().to_string(),
        review_ids: Vec::new(),
        started_at_ms,
        ended_at_ms: None,
    };
    task = tasks::update_task(project_path, &input.task_id, |current| {
        tasks::ensure_task_active(current)?;
        if current.status != TaskStatus::Reviewing
            || current.plan_todos.iter().any(|todo| todo.status != "done")
        {
            return Err(
                "task changed before implementation Review could start; reload and retry"
                    .to_string(),
            );
        }
        current.review_agent_ids = review_run.reviewer_agent_ids.clone();
        current.implementation_review_runs.push(review_run.clone());
        current.updated_at_ms = started_at_ms;
        Ok(())
    })?
    .0;

    let mut completed_reviews = Vec::new();
    for (reviewer, prepared) in prepared_reviewers {
        let review_id = ids.next("implementation-review");
        review_run.review_ids.push(review_id.clone());
        let review_started_at_ms = now_ms();
        let result = run_prepared_review(&task.id, &reviewer.id, &prompt, &prepared).await;
        let (status, summary, raw_output, stderr, exit_code, failure_detail, findings) =
            match result {
                Ok((raw_stdout, stderr, exit_code)) => {
                    let normalized = normalize_review_output(&prepared.output_mode, &raw_stdout);
                    if exit_code == Some(0) {
                        match parse_review_payload(&normalized) {
                            Ok(payload) => {
                                let findings = payload
                                    .findings
                                    .into_iter()
                                    .map(|finding| ImplementationReviewFinding {
                                        id: ids.next("implementation-review-finding"),
                                        review_id: review_id.clone(),
                                        severity: normalized_severity(&finding.severity)
                                            .to_string(),
                                        title: finding.title,
                                        detail: finding.detail,
                                        file: finding.file,
                                        line: finding.line,
                                        status: "open".to_string(),
                                        created_at_ms: now_ms(),
                                    })
                                    .collect();
                                (
                                    "succeeded".to_string(),
                                    if payload.summary.trim().is_empty() {
                                        "Review completed.".to_string()
                                    } else {
                                        payload.summary
                                    },
                                    normalized,
                                    stderr,
                                    exit_code,
                                    None,
                                    findings,
                                )
                            }
                            Err(error) => {
                                let finding_detail = error.clone();
                                (
                                    "failed".to_string(),
                                    error.clone(),
                                    normalized,
                                    stderr,
                                    exit_code,
                                    Some(error),
                                    vec![ImplementationReviewFinding {
                                        id: ids.next("implementation-review-finding"),
                                        review_id: review_id.clone(),
                                        severity: "blocker".to_string(),
                                        title: "Review output could not be parsed".to_string(),
                                        detail: finding_detail,
                                        file: None,
                                        line: None,
                                        status: "open".to_string(),
                                        created_at_ms: now_ms(),
                                    }],
                                )
                            }
                        }
                    } else {
                        let detail = format!("Review Agent exited with {exit_code:?}.");
                        (
                            "failed".to_string(),
                            detail.clone(),
                            normalized,
                            stderr,
                            exit_code,
                            Some(detail),
                            Vec::new(),
                        )
                    }
                }
                Err(error) => {
                    let detail = error.clone();
                    (
                        "failed".to_string(),
                        error.clone(),
                        error,
                        String::new(),
                        None,
                        Some(detail),
                        Vec::new(),
                    )
                }
            };
        let output_path = evidence_dir.join(format!("{review_id}.output.txt"));
        let stderr_path = evidence_dir.join(format!("{review_id}.stderr.log"));
        storage::atomic_write_text(&output_path, &agents::redact_sensitive_text(&raw_output))
            .map_err(|error| format!("failed to write Review output: {error}"))?;
        storage::atomic_write_text(&stderr_path, &agents::redact_sensitive_text(&stderr))
            .map_err(|error| format!("failed to write Review stderr: {error}"))?;
        completed_reviews.push(ImplementationReview {
            id: review_id,
            run_id: run_id.clone(),
            task_id: task.id.clone(),
            reviewer_agent_id: reviewer.id,
            reviewer_agent_name: reviewer.name,
            status,
            summary,
            raw_output,
            evidence_ref: Some(output_path.display().to_string()),
            stderr_ref: Some(stderr_path.display().to_string()),
            exit_code,
            failure_detail,
            findings,
            started_at_ms: review_started_at_ms,
            ended_at_ms: Some(now_ms()),
        });
    }

    let succeeded = completed_reviews
        .iter()
        .any(|review| review.status == "succeeded");
    review_run.status = if succeeded { "succeeded" } else { "failed" }.to_string();
    review_run.ended_at_ms = Some(now_ms());
    tasks::update_task(project_path, &input.task_id, move |current| {
        if let Some(stored_run) = current
            .implementation_review_runs
            .iter_mut()
            .find(|stored| stored.id == run_id)
        {
            *stored_run = review_run.clone();
        }
        current.implementation_reviews.extend(completed_reviews);
        current.updated_at_ms = now_ms();
        current.events.push(TaskEvent {
            id: ids.next("event"),
            task_id: current.id.clone(),
            timestamp_ms: current.updated_at_ms,
            actor: "agent".to_string(),
            status: current.status,
            input_summary: Some(format!(
                "Implementation Review by {}",
                review_run.reviewer_agent_ids.join(", ")
            )),
            output_summary: Some(format!("Implementation Review {}", review_run.status)),
            evidence_ref: Some(review_run.context_ref.clone()),
        });
        Ok(())
    })
    .map(|(task, ())| task)
}

#[tauri::command]
pub fn decide_implementation_review_finding(
    ids: State<'_, IdGenerator>,
    input: ImplementationReviewDecisionInput,
) -> Result<Task, String> {
    let reason = input.reason.trim().to_string();
    if reason.len() < 5 {
        return Err("a Review decision requires a specific reason".to_string());
    }
    tasks::update_task(
        Path::new(&input.project_path),
        &input.task_id,
        move |task| {
            tasks::ensure_task_active(task)?;
            let finding = task
                .implementation_reviews
                .iter_mut()
                .flat_map(|review| review.findings.iter_mut())
                .find(|finding| finding.id == input.finding_id)
                .ok_or_else(|| "implementation Review finding was not found".to_string())?;
            let next_status = match input.decision.as_str() {
                "resolved" => "pending_re_review",
                "accepted_risk" => "accepted_risk",
                "dismissed" if finding.severity != "blocker" => "dismissed",
                "dismissed" => return Err(
                    "a blocker cannot be dismissed; accept the risk explicitly or re-review after a fix"
                        .to_string(),
                ),
                _ => return Err("unsupported implementation Review decision".to_string()),
            };
            finding.status = next_status.to_string();
            let finding_title = finding.title.clone();
            let timestamp_ms = now_ms();
            task.implementation_review_decisions
                .push(ImplementationReviewDecision {
                    id: ids.next("implementation-review-decision"),
                    finding_id: input.finding_id.clone(),
                    decision: input.decision.clone(),
                    reason: reason.clone(),
                    actor: "user".to_string(),
                    created_at_ms: timestamp_ms,
                });
            task.updated_at_ms = timestamp_ms;
            task.events.push(TaskEvent {
                id: ids.next("event"),
                task_id: task.id.clone(),
                timestamp_ms,
                actor: "user".to_string(),
                status: task.status,
                input_summary: Some(format!("Review decision for {finding_title}")),
                output_summary: Some(format!("{}: {reason}", next_status)),
                evidence_ref: None,
            });
            Ok(())
        },
    )
    .map(|(task, ())| task)
}

pub(crate) fn ensure_review_gate(task: &Task) -> Result<(), String> {
    let latest_run = task
        .implementation_review_runs
        .iter()
        .rev()
        .find(|run| run.status != "running")
        .ok_or_else(|| {
            "cannot enter Testing before an independent implementation Review".to_string()
        })?;
    if latest_run.status != "succeeded" {
        return Err("the latest implementation Review did not complete successfully".to_string());
    }
    let primary_agent_id = task
        .primary_agent_id
        .as_deref()
        .ok_or_else(|| "task has no primary implementation Agent".to_string())?;
    let reviews = task
        .implementation_reviews
        .iter()
        .filter(|review| review.run_id == latest_run.id && review.status == "succeeded")
        .collect::<Vec<_>>();
    if !reviews
        .iter()
        .any(|review| review.reviewer_agent_id != primary_agent_id)
    {
        return Err("Testing requires a successful Review by an Agent other than the primary implementation Agent".to_string());
    }
    let unresolved_blockers = reviews
        .iter()
        .flat_map(|review| review.findings.iter())
        .filter(|finding| {
            finding.severity == "blocker"
                && matches!(finding.status.as_str(), "open" | "pending_re_review")
        })
        .map(|finding| finding.title.clone())
        .collect::<Vec<_>>();
    if !unresolved_blockers.is_empty() {
        return Err(format!(
            "Testing is blocked by unresolved Review findings: {}",
            unresolved_blockers.join("; ")
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_json_inside_a_markdown_fence() {
        let payload = parse_review_payload("```json\n{\"summary\":\"ok\",\"findings\":[]}\n```")
            .expect("payload");
        assert_eq!(payload.summary, "ok");
        assert!(payload.findings.is_empty());
    }

    #[test]
    fn severity_is_normalized_to_known_values() {
        assert_eq!(normalized_severity("BLOCKER"), "blocker");
        assert_eq!(normalized_severity("unknown"), "info");
    }

    #[test]
    fn review_output_is_redacted_before_parsing_or_persistence() {
        let normalized = normalize_review_output(
            "plain",
            "{\"summary\":\"Authorization: Bearer secret-review-token\",\"findings\":[]}",
        );
        assert!(!normalized.contains("secret-review-token"));
        assert!(normalized.contains("[REDACTED]"));
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn prepared_review_process_captures_structured_output() {
        let root = std::env::temp_dir().join(format!("loom-review-process-{}", now_ms()));
        fs::create_dir_all(&root).expect("review fixture");
        let prepared = PreparedAgentInvocation {
            program: "printf".to_string(),
            args: vec!["{\"summary\":\"pass\",\"findings\":[]}".to_string()],
            cwd: root.display().to_string(),
            stdin_prompt: false,
            output_mode: "plain".to_string(),
            resumed: false,
        };

        let (stdout, stderr, exit_code) =
            run_prepared_review("task-1", "reviewer", "unused prompt", &prepared)
                .await
                .expect("review process");

        assert_eq!(exit_code, Some(0));
        assert!(stderr.is_empty());
        assert_eq!(
            parse_review_payload(&stdout).expect("payload").summary,
            "pass"
        );
        fs::remove_dir_all(root).ok();
    }
}
