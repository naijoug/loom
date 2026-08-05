use super::*;

pub(super) fn classify_failure(result: &PlanningInvocationResult) -> Option<String> {
    if result.status == "succeeded" && !result.stdout.trim().is_empty() {
        return None;
    }

    let haystack = format!("{}\n{}", result.stderr, result.output_summary).to_lowercase();
    if NOT_RETRYABLE_PATTERNS
        .iter()
        .any(|pattern| haystack.contains(pattern))
    {
        return Some(FAILURE_NOT_RETRYABLE.to_string());
    }
    if result.timed_out {
        return Some(FAILURE_TIMEOUT.to_string());
    }
    if result.stdout.trim().is_empty() && result.status == "succeeded" {
        return Some(FAILURE_EMPTY_OUTPUT.to_string());
    }
    Some(FAILURE_NONZERO_EXIT.to_string())
}

pub(super) fn failure_detail_for_result(kind: &str, result: &PlanningInvocationResult) -> String {
    match kind {
        FAILURE_TIMEOUT => format!("Timed out after {}s", PLANNING_TIMEOUT_MS / 1000),
        FAILURE_EMPTY_OUTPUT => "Agent completed but produced no output".to_string(),
        FAILURE_NOT_RETRYABLE => extract_error_lines(&result.stderr)
            .into_iter()
            .next()
            .unwrap_or_else(|| "Configuration error; fix the agent setup, then retry".to_string()),
        _ => match result.exit_code {
            Some(code) => format!("Exited with code {code}"),
            None => "Agent process failed before an exit code was available".to_string(),
        },
    }
}

pub(super) fn extract_error_lines(stderr: &str) -> Vec<String> {
    stderr
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| {
            let lower = line.to_ascii_lowercase();
            !lower.starts_with("hook:")
                && (lower.starts_with("error:")
                    || lower.starts_with("fatal:")
                    || lower.starts_with("panic")
                    || lower.contains("panic"))
        })
        .take(5)
        .map(str::to_string)
        .collect()
}

pub(super) fn resume_command_for_profile(profile: &CliProfile, session_id: &str) -> Option<String> {
    resume_command_for_adapter(&profile.adapter_type, &profile.command, session_id)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn planning_status_event(
    agent: &AgentConfig,
    task_id: &str,
    planning_run_id: &str,
    phase: &str,
    status: &str,
    attempt: u32,
    started_at_ms: u128,
    ended_at_ms: Option<u128>,
) -> PlanningAgentStatusEvent {
    PlanningAgentStatusEvent {
        task_id: task_id.to_string(),
        planning_run_id: planning_run_id.to_string(),
        agent_id: agent.id.clone(),
        agent_name: agent.name.clone(),
        phase: phase.to_string(),
        status: status.to_string(),
        attempt,
        started_at_ms,
        ended_at_ms,
        elapsed_ms: ended_at_ms.map(|ended| ended.saturating_sub(started_at_ms)),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn run_planning_agent_with_status<E: PlanningEventEmitter>(
    emitter: E,
    agent: AgentConfig,
    project_path: PathBuf,
    task_id: String,
    planning_run_id: String,
    task_title: String,
    prompt: PlanningPrompt,
    phase: &'static str,
    start_attempt: u32,
    auto_retry: bool,
) -> PlanningInvocationResult {
    let max_attempt = if auto_retry {
        start_attempt + MAX_PLANNING_ATTEMPTS - 1
    } else {
        start_attempt
    };
    let mut attempt = start_attempt;

    loop {
        let started_at_ms = now_ms();
        emitter.emit_planning_agent_status(planning_status_event(
            &agent,
            &task_id,
            &planning_run_id,
            phase,
            "running",
            attempt,
            started_at_ms,
            None,
        ));

        let mut result = if phase == "synthesis" {
            run_synthesis_agent(
                &agent,
                &project_path,
                &task_title,
                &task_id,
                &planning_run_id,
                &prompt,
                attempt,
                emitter.clone(),
            )
            .await
        } else {
            run_planning_agent(
                &agent,
                &project_path,
                &task_title,
                &task_id,
                &planning_run_id,
                &prompt,
                attempt,
                emitter.clone(),
            )
            .await
        }
        .unwrap_or_else(|error| failed_planning_result(&agent, error, Some(started_at_ms)));
        result.attempt = attempt;

        let Some(kind) = classify_failure(&result) else {
            emitter.emit_planning_agent_status(planning_status_event(
                &agent,
                &task_id,
                &planning_run_id,
                phase,
                &result.status,
                attempt,
                result.started_at_ms,
                Some(result.ended_at_ms),
            ));
            return result;
        };

        if result.status == "succeeded" {
            // Exit 0 with no stdout is not a usable plan; surface it as a failure.
            result.status = "failed".to_string();
            result.output_summary = format!("{} produced no output.", agent.name);
        }
        result.failure_kind = Some(kind.clone());
        result.error_lines = extract_error_lines(&result.stderr);
        result.failure_detail = Some(failure_detail_for_result(&kind, &result));

        if kind != FAILURE_NOT_RETRYABLE && attempt < max_attempt {
            emitter.emit_planning_agent_status(planning_status_event(
                &agent,
                &task_id,
                &planning_run_id,
                phase,
                "retrying",
                attempt,
                result.started_at_ms,
                Some(result.ended_at_ms),
            ));
            attempt += 1;
            continue;
        }

        emitter.emit_planning_agent_status(planning_status_event(
            &agent,
            &task_id,
            &planning_run_id,
            phase,
            &result.status,
            attempt,
            result.started_at_ms,
            Some(result.ended_at_ms),
        ));
        return result;
    }
}

pub(super) fn agent_invocation_from_result(
    ids: &IdGenerator,
    task_id: &str,
    planning_run_id: &str,
    agent: &AgentConfig,
    prompt_summary: String,
    result: PlanningInvocationResult,
) -> AgentInvocation {
    AgentInvocation {
        id: ids.next("invoke"),
        planning_run_id: planning_run_id.to_string(),
        task_id: task_id.to_string(),
        agent_id: agent.id.clone(),
        agent_name: agent.name.clone(),
        status: result.status,
        prompt_summary,
        raw_output: result.stdout,
        output_summary: result.output_summary,
        evidence_ref: result.evidence_ref,
        plan_path: result.plan_path,
        stderr_tail: stderr_tail(&result.stderr),
        exit_code: result.exit_code,
        timed_out: result.timed_out,
        attempt: result.attempt,
        failure_kind: result.failure_kind,
        failure_detail: result.failure_detail,
        error_lines: result.error_lines,
        stderr_ref: result.stderr_ref,
        session_id: result.session_id,
        resume_command: result.resume_command,
        started_at_ms: result.started_at_ms,
        ended_at_ms: Some(result.ended_at_ms),
    }
}

pub(super) fn failed_planning_result(
    agent: &AgentConfig,
    error: String,
    started_at_ms: Option<u128>,
) -> PlanningInvocationResult {
    let started_at_ms = started_at_ms.unwrap_or_else(now_ms);
    PlanningInvocationResult {
        status: "failed".to_string(),
        stdout: String::new(),
        stderr: redact_sensitive_text(&error),
        output_summary: format!("{} failed before producing output.", agent.name),
        evidence_ref: None,
        plan_path: None,
        exit_code: None,
        timed_out: false,
        attempt: 1,
        failure_kind: None,
        failure_detail: None,
        error_lines: extract_error_lines(&error),
        stderr_ref: None,
        session_id: None,
        resume_command: None,
        started_at_ms,
        ended_at_ms: now_ms(),
    }
}

pub(super) fn successful_plan_invocations(invocations: &[AgentInvocation]) -> Vec<AgentInvocation> {
    invocations
        .iter()
        .filter(|invocation| {
            invocation.status == "succeeded" && !invocation.raw_output.trim().is_empty()
        })
        .cloned()
        .collect()
}

/// The effective drafting set for a planning run: the latest invocation per
/// agent (retries supersede earlier attempts), excluding synthesis records.
pub(super) fn latest_drafting_invocations(
    invocations: &[AgentInvocation],
    planning_run_id: &str,
) -> Vec<AgentInvocation> {
    let mut by_agent: Vec<AgentInvocation> = Vec::new();
    for invocation in invocations.iter().filter(|invocation| {
        invocation.planning_run_id == planning_run_id
            && invocation.prompt_summary != SYNTHESIS_PROMPT_SUMMARY
    }) {
        if let Some(existing) = by_agent
            .iter_mut()
            .find(|existing| existing.agent_id == invocation.agent_id)
        {
            *existing = invocation.clone();
        } else {
            by_agent.push(invocation.clone());
        }
    }
    by_agent
}

/// Run every reviewer-vs-target pair in parallel. A failed review becomes a
/// failed `PlanReview` record instead of aborting the round.
#[allow(clippy::too_many_arguments)]
pub(super) async fn run_cross_reviews<E: PlanningEventEmitter>(
    emitter: E,
    ids: &IdGenerator,
    project_path: &Path,
    task_title: &str,
    task_id: &str,
    planning_run_id: &str,
    agents: &[AgentConfig],
    candidates: &[AgentInvocation],
) -> Vec<PlanReview> {
    let mut handles = Vec::new();
    for reviewer in agents.iter().filter(|agent| {
        candidates
            .iter()
            .any(|candidate| candidate.agent_id == agent.id)
    }) {
        for target in candidates
            .iter()
            .filter(|candidate| candidate.agent_id != reviewer.id)
        {
            let emitter = emitter.clone();
            let reviewer = reviewer.clone();
            let target = target.clone();
            let project_path = project_path.to_path_buf();
            let task_title = task_title.to_string();
            let task_id = task_id.to_string();
            let planning_run_id = planning_run_id.to_string();
            handles.push(tauri::async_runtime::spawn(async move {
                // Progress events identify the pair, not just the reviewer, so
                // one reviewer covering several targets stays distinguishable.
                let pair = AgentConfig {
                    id: format!("{}->{}", reviewer.id, target.agent_id),
                    name: format!("{} → {}", reviewer.name, target.agent_name),
                    ..reviewer.clone()
                };
                let started_at_ms = now_ms();
                emitter.emit_planning_agent_status(planning_status_event(
                    &pair,
                    &task_id,
                    &planning_run_id,
                    "review",
                    "running",
                    1,
                    started_at_ms,
                    None,
                ));
                let result = run_plan_review_agent(
                    &reviewer,
                    &project_path,
                    &task_title,
                    &task_id,
                    &planning_run_id,
                    &target,
                    &pair,
                    emitter.clone(),
                )
                .await
                .unwrap_or_else(|error| PlanReviewInvocationResult {
                    status: "failed".to_string(),
                    finding: format!(
                        "{} failed to review {}: {error}",
                        reviewer.name, target.agent_name
                    ),
                    severity: "blocker".to_string(),
                    raw_output: redact_sensitive_text(&error),
                    evidence_ref: None,
                    stderr_ref: None,
                    session_id: None,
                    resume_command: None,
                    started_at_ms,
                    ended_at_ms: now_ms(),
                });
                emitter.emit_planning_agent_status(planning_status_event(
                    &pair,
                    &task_id,
                    &planning_run_id,
                    "review",
                    &result.status,
                    1,
                    result.started_at_ms,
                    Some(result.ended_at_ms),
                ));
                (reviewer, target, result)
            }));
        }
    }

    let mut reviews = Vec::new();
    for handle in handles {
        let Ok((reviewer, target, result)) = handle.await else {
            continue;
        };
        reviews.push(PlanReview {
            id: ids.next("review"),
            planning_run_id: planning_run_id.to_string(),
            task_id: task_id.to_string(),
            reviewer_agent_id: reviewer.id,
            reviewer_agent_name: reviewer.name,
            target_agent_id: target.agent_id,
            target_agent_name: target.agent_name,
            status: result.status,
            finding: result.finding,
            severity: result.severity,
            accepted: false,
            raw_output: result.raw_output,
            evidence_ref: result.evidence_ref,
            stderr_ref: result.stderr_ref,
            session_id: result.session_id,
            resume_command: result.resume_command,
            started_at_ms: result.started_at_ms,
            ended_at_ms: Some(result.ended_at_ms),
        });
    }
    reviews
}

pub(super) struct PlanningSynthesisOutcome {
    pub(super) final_plan: String,
    pub(super) reviews: Vec<PlanReview>,
    pub(super) synthesis_invocation: Option<AgentInvocation>,
    /// Sentence appended to the discussion summary describing where the final
    /// plan came from (synthesis / direct adoption / fallback).
    pub(super) source_note: String,
}

/// Shared tail of the planning pipeline: cross-review the candidates, then
/// synthesize (or adopt / fall back to) the final plan. Used by the initial
/// discussion and by per-agent retries.
#[allow(clippy::too_many_arguments)]
pub(super) async fn review_and_synthesize<E: PlanningEventEmitter>(
    emitter: E,
    ids: &IdGenerator,
    project_path: &Path,
    task_title: &str,
    task_id: &str,
    planning_run_id: &str,
    requirement: &str,
    selected_agents: &[AgentConfig],
    drafting_invocations: &[AgentInvocation],
    discussion_summary: &str,
) -> Result<PlanningSynthesisOutcome, String> {
    let candidates = successful_plan_invocations(drafting_invocations);

    if candidates.len() >= 2 {
        let reviews = run_cross_reviews(
            emitter.clone(),
            ids,
            project_path,
            task_title,
            task_id,
            planning_run_id,
            selected_agents,
            &candidates,
        )
        .await;
        let synthesis_agent = choose_synthesis_agent(selected_agents, &candidates)
            .ok_or_else(|| "failed to select a synthesis agent".to_string())?;
        let synthesis_prompt = render_synthesis_prompt(
            task_title,
            &project_path.display().to_string(),
            requirement,
            &candidates,
            &reviews,
        );
        let synthesis_result = run_planning_agent_with_status(
            emitter,
            synthesis_agent.clone(),
            project_path.to_path_buf(),
            task_id.to_string(),
            planning_run_id.to_string(),
            task_title.to_string(),
            synthesis_prompt,
            "synthesis",
            1,
            true,
        )
        .await;
        let synthesis_succeeded = synthesis_result.status == "succeeded";
        let synthesis_invocation = agent_invocation_from_result(
            ids,
            task_id,
            planning_run_id,
            &synthesis_agent,
            SYNTHESIS_PROMPT_SUMMARY.to_string(),
            synthesis_result,
        );

        if synthesis_succeeded {
            let successful_reviews = reviews
                .iter()
                .filter(|review| review.status == "succeeded")
                .count();
            Ok(PlanningSynthesisOutcome {
                final_plan: synthesis_invocation.raw_output.clone(),
                source_note: format!(
                    ". Final plan source: synthesized by {} from {} candidate plans and {} cross-review findings.",
                    synthesis_agent.name,
                    candidates.len(),
                    successful_reviews
                ),
                reviews,
                synthesis_invocation: Some(synthesis_invocation),
            })
        } else {
            let source_note =
                ". Final plan source: synthesis failed; used deterministic fallback from candidate plans."
                    .to_string();
            let mut all_invocations = drafting_invocations.to_vec();
            all_invocations.push(synthesis_invocation.clone());
            Ok(PlanningSynthesisOutcome {
                final_plan: render_final_plan(
                    task_title,
                    requirement,
                    &format!("{discussion_summary}{source_note}"),
                    &all_invocations,
                ),
                source_note,
                reviews,
                synthesis_invocation: Some(synthesis_invocation),
            })
        }
    } else if candidates.len() == 1 {
        let candidate = &candidates[0];
        Ok(PlanningSynthesisOutcome {
            final_plan: candidate.raw_output.clone(),
            source_note: format!(
                ". Final plan source: directly adopted {} candidate plan.",
                candidate.agent_name
            ),
            reviews: Vec::new(),
            synthesis_invocation: None,
        })
    } else {
        let source_note =
            ". Final plan source: deterministic fallback because all selected agents failed."
                .to_string();
        Ok(PlanningSynthesisOutcome {
            final_plan: render_final_plan(
                task_title,
                requirement,
                &format!("{discussion_summary}{source_note}"),
                drafting_invocations,
            ),
            source_note,
            reviews: Vec::new(),
            synthesis_invocation: None,
        })
    }
}

pub(super) fn choose_synthesis_agent(
    agents: &[AgentConfig],
    candidates: &[AgentInvocation],
) -> Option<AgentConfig> {
    let successful_agent_ids = candidates
        .iter()
        .map(|candidate| candidate.agent_id.as_str())
        .collect::<Vec<_>>();

    agents
        .iter()
        .find(|agent| {
            successful_agent_ids.contains(&agent.id.as_str())
                && effective_adapter_type(agent) == ADAPTER_CLAUDE_CODE
        })
        .or_else(|| {
            agents
                .iter()
                .find(|agent| successful_agent_ids.contains(&agent.id.as_str()))
        })
        .cloned()
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn run_plan_review_agent<E: PlanningEventEmitter>(
    reviewer: &AgentConfig,
    project_path: &Path,
    task_title: &str,
    task_id: &str,
    planning_run_id: &str,
    target: &AgentInvocation,
    log_agent: &AgentConfig,
    emitter: E,
) -> Result<PlanReviewInvocationResult, String> {
    let evidence_dir =
        planning_evidence_dir(project_path, task_id, planning_run_id).join("reviews");
    fs::create_dir_all(&evidence_dir)
        .map_err(|error| format!("failed to create review evidence directory: {error}"))?;

    let review_id = format!("{}-reviews-{}", reviewer.id, target.agent_id);
    let prompt_path = evidence_dir.join(format!("{review_id}.prompt.md"));
    let stdout_path = evidence_dir.join(format!("{review_id}.stdout.md"));
    let stderr_path = evidence_dir.join(format!("{review_id}.stderr.log"));
    let prompt = render_plan_review_prompt(reviewer, target);
    let redacted_prompt = redact_sensitive_text(&prompt.content);
    storage::atomic_write_text(&prompt_path, &redacted_prompt)
        .map_err(|error| format!("failed to write review prompt: {error}"))?;

    if effective_adapter_type(reviewer) == ADAPTER_DUMMY {
        let started_at_ms = now_ms();
        let output = deterministic_plan_review_output(reviewer, target);
        storage::atomic_write_text(&stdout_path, &output)
            .map_err(|error| format!("failed to write dummy review output: {error}"))?;
        storage::atomic_write_text(&stderr_path, "")
            .map_err(|error| format!("failed to write dummy review stderr: {error}"))?;

        return Ok(PlanReviewInvocationResult {
            status: "succeeded".to_string(),
            finding: summarize_review_output(&output),
            severity: infer_review_severity(&output),
            raw_output: output,
            evidence_ref: Some(stdout_path.display().to_string()),
            stderr_ref: Some(stderr_path.display().to_string()),
            session_id: None,
            resume_command: None,
            started_at_ms,
            ended_at_ms: now_ms(),
        });
    }

    let profile = build_cli_profile(
        reviewer,
        project_path,
        &prompt_path,
        Some(task_title),
        agent_adapter::AgentStage::Review,
    )?;
    let log_context = PlanningLogContext {
        task_id: task_id.to_string(),
        planning_run_id: planning_run_id.to_string(),
        agent_id: log_agent.id.clone(),
        agent_name: log_agent.name.clone(),
        phase: "review".to_string(),
        attempt: 1,
    };
    match run_cli_profile(
        &profile,
        project_path,
        &redacted_prompt,
        emitter,
        Some(log_context),
    )
    .await
    {
        Ok(result) => {
            storage::atomic_write_text(&stdout_path, &result.stdout)
                .map_err(|error| format!("failed to write review stdout: {error}"))?;
            storage::atomic_write_text(&stderr_path, &result.stderr)
                .map_err(|error| format!("failed to write review stderr: {error}"))?;
            Ok(PlanReviewInvocationResult {
                status: result.status,
                finding: summarize_review_output(&result.stdout),
                severity: infer_review_severity(&result.stdout),
                raw_output: result.stdout,
                evidence_ref: Some(stdout_path.display().to_string()),
                stderr_ref: Some(stderr_path.display().to_string()),
                session_id: result.session_id,
                resume_command: result.resume_command,
                started_at_ms: result.started_at_ms,
                ended_at_ms: result.ended_at_ms,
            })
        }
        Err(error) => {
            let started_at_ms = now_ms();
            let stderr = redact_sensitive_text(&error);
            storage::atomic_write_text(&stdout_path, "")
                .map_err(|error| format!("failed to write empty review stdout: {error}"))?;
            storage::atomic_write_text(&stderr_path, &stderr)
                .map_err(|error| format!("failed to write review stderr: {error}"))?;
            Ok(PlanReviewInvocationResult {
                status: "failed".to_string(),
                finding: format!("{} failed to review {}.", reviewer.name, target.agent_name),
                severity: "blocker".to_string(),
                raw_output: stderr,
                evidence_ref: Some(stdout_path.display().to_string()),
                stderr_ref: Some(stderr_path.display().to_string()),
                session_id: None,
                resume_command: None,
                started_at_ms,
                ended_at_ms: now_ms(),
            })
        }
    }
}

/// Evidence file suffix that keeps every attempt's artifacts on disk: the
/// first attempt keeps the historical names, retries get `.attempt-N`.
pub(super) fn attempt_suffix(attempt: u32) -> String {
    if attempt > 1 {
        format!(".attempt-{attempt}")
    } else {
        String::new()
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn run_planning_agent<E: PlanningEventEmitter>(
    agent: &AgentConfig,
    project_path: &Path,
    task_title: &str,
    task_id: &str,
    planning_run_id: &str,
    prompt: &PlanningPrompt,
    attempt: u32,
    emitter: E,
) -> Result<PlanningInvocationResult, String> {
    let evidence_dir = planning_evidence_dir(project_path, task_id, planning_run_id);
    fs::create_dir_all(&evidence_dir)
        .map_err(|error| format!("failed to create planning evidence directory: {error}"))?;

    let suffix = attempt_suffix(attempt);
    let prompt_path = evidence_dir.join(format!("{}{suffix}.prompt.md", agent.id));
    let stdout_path = evidence_dir.join(format!("{}{suffix}.stdout.md", agent.id));
    let plan_path = evidence_dir.join(format!("{}{suffix}.plan.md", agent.id));
    let stderr_path = evidence_dir.join(format!("{}{suffix}.stderr.log", agent.id));
    let redacted_prompt = redact_sensitive_text(&prompt.content);
    storage::atomic_write_text(&prompt_path, &redacted_prompt)
        .map_err(|error| format!("failed to write planning prompt: {error}"))?;

    if effective_adapter_type(agent) == ADAPTER_DUMMY {
        let started_at_ms = now_ms();
        let output = deterministic_planning_output(agent, prompt);
        storage::atomic_write_text(&stdout_path, &output)
            .map_err(|error| format!("failed to write dummy planning output: {error}"))?;
        storage::atomic_write_text(
            &plan_path,
            &render_candidate_plan_document(agent, prompt, &output, started_at_ms),
        )
        .map_err(|error| format!("failed to write dummy candidate plan: {error}"))?;
        storage::atomic_write_text(&stderr_path, "")
            .map_err(|error| format!("failed to write dummy planning stderr: {error}"))?;

        return Ok(PlanningInvocationResult {
            status: "succeeded".to_string(),
            stdout: output.clone(),
            stderr: String::new(),
            output_summary: summarize_agent_output(agent, &output),
            evidence_ref: Some(stdout_path.display().to_string()),
            plan_path: Some(plan_path.display().to_string()),
            exit_code: Some(0),
            timed_out: false,
            attempt: 1,
            failure_kind: None,
            failure_detail: None,
            error_lines: Vec::new(),
            stderr_ref: Some(stderr_path.display().to_string()),
            session_id: None,
            resume_command: None,
            started_at_ms,
            ended_at_ms: now_ms(),
        });
    }

    let profile = build_cli_profile(
        agent,
        project_path,
        &prompt_path,
        Some(task_title),
        agent_adapter::AgentStage::Planning,
    )?;
    let log_context = PlanningLogContext {
        task_id: task_id.to_string(),
        planning_run_id: planning_run_id.to_string(),
        agent_id: agent.id.clone(),
        agent_name: agent.name.clone(),
        phase: "planning".to_string(),
        attempt,
    };
    let result = run_cli_profile(
        &profile,
        project_path,
        &redacted_prompt,
        emitter,
        Some(log_context),
    )
    .await;

    match result {
        Ok(mut result) => {
            storage::atomic_write_text(&stdout_path, &result.stdout)
                .map_err(|error| format!("failed to write planning stdout: {error}"))?;
            storage::atomic_write_text(
                &plan_path,
                &render_candidate_plan_document(
                    agent,
                    prompt,
                    &result.stdout,
                    result.started_at_ms,
                ),
            )
            .map_err(|error| format!("failed to write candidate plan: {error}"))?;
            storage::atomic_write_text(&stderr_path, &result.stderr)
                .map_err(|error| format!("failed to write planning stderr: {error}"))?;
            result.evidence_ref = Some(stdout_path.display().to_string());
            result.plan_path = Some(plan_path.display().to_string());
            result.stderr_ref = Some(stderr_path.display().to_string());
            Ok(result)
        }
        Err(error) => {
            let started_at_ms = now_ms();
            let stderr = redact_sensitive_text(&error);
            storage::atomic_write_text(&stdout_path, "")
                .map_err(|error| format!("failed to write empty planning stdout: {error}"))?;
            storage::atomic_write_text(&stderr_path, &stderr)
                .map_err(|error| format!("failed to write planning stderr: {error}"))?;
            Ok(PlanningInvocationResult {
                status: "failed".to_string(),
                stdout: String::new(),
                stderr: stderr.clone(),
                output_summary: format!("{} failed before producing output.", agent.name),
                evidence_ref: Some(stdout_path.display().to_string()),
                plan_path: None,
                exit_code: None,
                timed_out: false,
                attempt: 1,
                failure_kind: None,
                failure_detail: None,
                error_lines: Vec::new(),
                stderr_ref: Some(stderr_path.display().to_string()),
                session_id: None,
                resume_command: None,
                started_at_ms,
                ended_at_ms: now_ms(),
            })
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn run_synthesis_agent<E: PlanningEventEmitter>(
    agent: &AgentConfig,
    project_path: &Path,
    task_title: &str,
    task_id: &str,
    planning_run_id: &str,
    prompt: &PlanningPrompt,
    attempt: u32,
    emitter: E,
) -> Result<PlanningInvocationResult, String> {
    let evidence_dir = planning_evidence_dir(project_path, task_id, planning_run_id);
    fs::create_dir_all(&evidence_dir)
        .map_err(|error| format!("failed to create synthesis evidence directory: {error}"))?;

    let suffix = attempt_suffix(attempt);
    let prompt_path = evidence_dir.join(format!("synthesis{suffix}.prompt.md"));
    let stdout_path = evidence_dir.join(format!("synthesis{suffix}.stdout.md"));
    let stderr_path = evidence_dir.join(format!("synthesis{suffix}.stderr.log"));
    let redacted_prompt = redact_sensitive_text(&prompt.content);
    storage::atomic_write_text(&prompt_path, &redacted_prompt)
        .map_err(|error| format!("failed to write synthesis prompt: {error}"))?;

    if effective_adapter_type(agent) == ADAPTER_DUMMY {
        let started_at_ms = now_ms();
        let output = deterministic_synthesis_output(prompt);
        storage::atomic_write_text(&stdout_path, &output)
            .map_err(|error| format!("failed to write dummy synthesis output: {error}"))?;
        storage::atomic_write_text(&stderr_path, "")
            .map_err(|error| format!("failed to write dummy synthesis stderr: {error}"))?;

        return Ok(PlanningInvocationResult {
            status: "succeeded".to_string(),
            stdout: output.clone(),
            stderr: String::new(),
            output_summary: "Synthesized final plan from candidate plans.".to_string(),
            evidence_ref: Some(stdout_path.display().to_string()),
            plan_path: None,
            exit_code: Some(0),
            timed_out: false,
            attempt: 1,
            failure_kind: None,
            failure_detail: None,
            error_lines: Vec::new(),
            stderr_ref: Some(stderr_path.display().to_string()),
            session_id: None,
            resume_command: None,
            started_at_ms,
            ended_at_ms: now_ms(),
        });
    }

    let profile = build_cli_profile(
        agent,
        project_path,
        &prompt_path,
        Some(task_title),
        agent_adapter::AgentStage::Planning,
    )?;
    let log_context = PlanningLogContext {
        task_id: task_id.to_string(),
        planning_run_id: planning_run_id.to_string(),
        agent_id: agent.id.clone(),
        agent_name: agent.name.clone(),
        phase: "synthesis".to_string(),
        attempt,
    };
    let result = run_cli_profile(
        &profile,
        project_path,
        &redacted_prompt,
        emitter,
        Some(log_context),
    )
    .await;

    match result {
        Ok(mut result) => {
            storage::atomic_write_text(&stdout_path, &result.stdout)
                .map_err(|error| format!("failed to write synthesis stdout: {error}"))?;
            storage::atomic_write_text(&stderr_path, &result.stderr)
                .map_err(|error| format!("failed to write synthesis stderr: {error}"))?;
            result.evidence_ref = Some(stdout_path.display().to_string());
            result.plan_path = None;
            result.stderr_ref = Some(stderr_path.display().to_string());
            Ok(result)
        }
        Err(error) => {
            let started_at_ms = now_ms();
            let stderr = redact_sensitive_text(&error);
            storage::atomic_write_text(&stdout_path, "")
                .map_err(|error| format!("failed to write empty synthesis stdout: {error}"))?;
            storage::atomic_write_text(&stderr_path, &stderr)
                .map_err(|error| format!("failed to write synthesis stderr: {error}"))?;
            Ok(PlanningInvocationResult {
                status: "failed".to_string(),
                stdout: String::new(),
                stderr: stderr.clone(),
                output_summary: format!("{} failed to synthesize the final plan.", agent.name),
                evidence_ref: Some(stdout_path.display().to_string()),
                plan_path: None,
                exit_code: None,
                timed_out: false,
                attempt: 1,
                failure_kind: None,
                failure_detail: None,
                error_lines: Vec::new(),
                stderr_ref: Some(stderr_path.display().to_string()),
                session_id: None,
                resume_command: None,
                started_at_ms,
                ended_at_ms: now_ms(),
            })
        }
    }
}
