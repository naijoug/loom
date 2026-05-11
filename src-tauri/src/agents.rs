use crate::{
    models::{
        now_ms, AgentConfig, AgentConfigInput, AgentInvocation, IdGenerator,
        PlanningDiscussionInput, PlanningRun, TaskEvent,
    },
    storage, tasks,
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, State};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command as TokioCommand,
    time::sleep,
};

const AGENTS_FILE: &str = "agents.json";
const ADAPTER_CODEX: &str = "codex_cli";
const ADAPTER_CLAUDE_CODE: &str = "claude_code_cli";
const ADAPTER_AMP: &str = "amp_cli";
const ADAPTER_CLI: &str = "cli";
const ADAPTER_DUMMY: &str = "dummy";
const PLANNING_TIMEOUT_MS: u64 = 120_000;

struct PlanningPrompt {
    content: String,
}

struct CliProfile {
    command: String,
    args: Vec<String>,
    stdin_prompt: bool,
}

struct PlanningInvocationResult {
    status: String,
    stdout: String,
    stderr: String,
    output_summary: String,
    evidence_ref: Option<String>,
    exit_code: Option<i32>,
    timed_out: bool,
    started_at_ms: u128,
    ended_at_ms: u128,
}

#[tauri::command]
pub fn list_agents(app: AppHandle) -> Result<Vec<AgentConfig>, String> {
    load_agents(&app)
}

#[tauri::command]
pub fn create_agent(
    app: AppHandle,
    ids: State<'_, IdGenerator>,
    input: AgentConfigInput,
) -> Result<AgentConfig, String> {
    let mut agents = load_agents(&app)?;
    let mut agent = AgentConfig {
        id: ids.next("agent"),
        name: input.name,
        command: input.command,
        args: input.args,
        working_directory_policy: input.working_directory_policy,
        capabilities: input.capabilities,
        adapter_type: input.adapter_type,
        can_write_files: input.can_write_files,
        can_run_commands: input.can_run_commands,
        enabled: input.enabled,
        available: false,
    };
    agent.available = command_available(&agent);
    agents.push(agent.clone());
    save_agents(&app, &agents)?;

    Ok(agent)
}

#[tauri::command]
pub fn set_agent_enabled(
    app: AppHandle,
    agent_id: String,
    enabled: bool,
) -> Result<Vec<AgentConfig>, String> {
    let mut agents = load_agents(&app)?;

    if let Some(agent) = agents.iter_mut().find(|agent| agent.id == agent_id) {
        agent.enabled = enabled;
    }

    save_agents(&app, &agents)?;
    Ok(agents)
}

#[tauri::command]
pub async fn run_planning_discussion(
    app: AppHandle,
    ids: State<'_, IdGenerator>,
    input: PlanningDiscussionInput,
) -> Result<crate::models::Task, String> {
    let agents = load_agents(&app)?;
    let mut task = tasks::load_task(Path::new(&input.project_path), &input.task_id)?;
    let selected_agents = resolve_planning_agents(&agents, &input.agent_ids);

    if selected_agents.is_empty() {
        return Err("no enabled planning agents are available".to_string());
    }

    let started_at_ms = now_ms();
    let planning_run_id = ids.next("planning");
    let requirement = if input.requirement.trim().is_empty() {
        task.raw_requirement.clone()
    } else {
        input.requirement.trim().to_string()
    };
    let prompt_summary = format!("Planning discussion for task '{}'", task.title);
    let prompt = render_planning_prompt(&task.title, &input.project_path, &requirement);

    let mut invocations = Vec::new();
    for agent in &selected_agents {
        let result = run_planning_agent(
            agent,
            Path::new(&input.project_path),
            &task.id,
            &planning_run_id,
            &prompt,
        )
        .await?;

        invocations.push(AgentInvocation {
            id: ids.next("invoke"),
            planning_run_id: planning_run_id.clone(),
            task_id: task.id.clone(),
            agent_id: agent.id.clone(),
            agent_name: agent.name.clone(),
            status: result.status,
            prompt_summary: prompt_summary.clone(),
            raw_output: result.stdout,
            output_summary: result.output_summary,
            evidence_ref: result.evidence_ref,
            stderr_tail: stderr_tail(&result.stderr),
            exit_code: result.exit_code,
            timed_out: result.timed_out,
            started_at_ms: result.started_at_ms,
            ended_at_ms: Some(result.ended_at_ms),
        });
    }

    let successful_invocations = invocations
        .iter()
        .filter(|invocation| invocation.status == "succeeded")
        .count();
    let discussion_summary = summarize_discussion(&selected_agents, &requirement, &invocations);
    let final_plan =
        render_final_plan(&task.title, &requirement, &discussion_summary, &invocations);
    let plan_path = next_project_plan_path(Path::new(&input.project_path), &task.title)?;
    fs::create_dir_all(
        plan_path
            .parent()
            .ok_or_else(|| "invalid plan path".to_string())?,
    )
    .map_err(|error| format!("failed to create plans directory: {error}"))?;
    fs::write(&plan_path, &final_plan)
        .map_err(|error| format!("failed to write final plan: {error}"))?;

    let finished_at_ms = now_ms();
    task.status = "plan_review".to_string();
    task.raw_requirement = requirement.clone();
    task.final_plan = Some(final_plan);
    task.final_plan_path = Some(plan_path.display().to_string());
    task.discussion_summary = Some(discussion_summary.clone());
    task.selected_planning_agent_ids = selected_agents
        .iter()
        .map(|agent| agent.id.clone())
        .collect();
    task.planning_runs.push(PlanningRun {
        id: planning_run_id.clone(),
        task_id: task.id.clone(),
        requirement,
        selected_agent_ids: task.selected_planning_agent_ids.clone(),
        status: if successful_invocations > 0 {
            "succeeded".to_string()
        } else {
            "failed".to_string()
        },
        summary: discussion_summary,
        started_at_ms,
        ended_at_ms: Some(finished_at_ms),
    });
    task.agent_invocations.extend(invocations);
    task.plan_todos = Vec::new();
    task.updated_at_ms = finished_at_ms;
    task.events.push(TaskEvent {
        id: ids.next("event"),
        task_id: task.id.clone(),
        timestamp_ms: finished_at_ms,
        actor: "agent".to_string(),
        status: task.status.clone(),
        input_summary: Some(prompt_summary),
        output_summary: Some(if successful_invocations > 0 {
            "Planning discussion generated a final plan for review.".to_string()
        } else {
            "Planning discussion failed for all selected real agents.".to_string()
        }),
        evidence_ref: task.final_plan_path.clone(),
    });
    tasks::save_task(&task)?;

    Ok(task)
}

async fn run_planning_agent(
    agent: &AgentConfig,
    project_path: &Path,
    task_id: &str,
    planning_run_id: &str,
    prompt: &PlanningPrompt,
) -> Result<PlanningInvocationResult, String> {
    let evidence_dir = planning_evidence_dir(project_path, task_id, planning_run_id);
    fs::create_dir_all(&evidence_dir)
        .map_err(|error| format!("failed to create planning evidence directory: {error}"))?;

    let prompt_path = evidence_dir.join(format!("{}.prompt.md", agent.id));
    let stdout_path = evidence_dir.join(format!("{}.stdout.md", agent.id));
    let stderr_path = evidence_dir.join(format!("{}.stderr.log", agent.id));
    let redacted_prompt = redact_sensitive_text(&prompt.content);
    fs::write(&prompt_path, &redacted_prompt)
        .map_err(|error| format!("failed to write planning prompt: {error}"))?;

    if effective_adapter_type(agent) == ADAPTER_DUMMY {
        let started_at_ms = now_ms();
        let output = deterministic_planning_output(agent, prompt);
        fs::write(&stdout_path, &output)
            .map_err(|error| format!("failed to write dummy planning output: {error}"))?;
        fs::write(&stderr_path, "")
            .map_err(|error| format!("failed to write dummy planning stderr: {error}"))?;

        return Ok(PlanningInvocationResult {
            status: "succeeded".to_string(),
            stdout: output.clone(),
            stderr: String::new(),
            output_summary: summarize_agent_output(agent, &output),
            evidence_ref: Some(stdout_path.display().to_string()),
            exit_code: Some(0),
            timed_out: false,
            started_at_ms,
            ended_at_ms: now_ms(),
        });
    }

    let profile = build_cli_profile(agent, project_path, &prompt_path)?;
    let result = run_cli_profile(&profile, project_path, &redacted_prompt).await;

    match result {
        Ok(mut result) => {
            fs::write(&stdout_path, &result.stdout)
                .map_err(|error| format!("failed to write planning stdout: {error}"))?;
            fs::write(&stderr_path, &result.stderr)
                .map_err(|error| format!("failed to write planning stderr: {error}"))?;
            result.evidence_ref = Some(stdout_path.display().to_string());
            Ok(result)
        }
        Err(error) => {
            let started_at_ms = now_ms();
            let stderr = redact_sensitive_text(&error);
            fs::write(&stdout_path, "")
                .map_err(|error| format!("failed to write empty planning stdout: {error}"))?;
            fs::write(&stderr_path, &stderr)
                .map_err(|error| format!("failed to write planning stderr: {error}"))?;
            Ok(PlanningInvocationResult {
                status: "failed".to_string(),
                stdout: String::new(),
                stderr: stderr.clone(),
                output_summary: format!("{} failed before producing output.", agent.name),
                evidence_ref: Some(stderr_path.display().to_string()),
                exit_code: None,
                timed_out: false,
                started_at_ms,
                ended_at_ms: now_ms(),
            })
        }
    }
}

async fn run_cli_profile(
    profile: &CliProfile,
    project_path: &Path,
    prompt: &str,
) -> Result<PlanningInvocationResult, String> {
    let started_at_ms = now_ms();
    let mut command = TokioCommand::new(&profile.command);
    command
        .args(&profile.args)
        .current_dir(project_path)
        .stdin(if profile.stdin_prompt {
            std::process::Stdio::piped()
        } else {
            std::process::Stdio::null()
        })
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = command
        .spawn()
        .map_err(|error| format!("failed to start {}: {error}", profile.command))?;

    if profile.stdin_prompt {
        if let Some(mut stdin) = child.stdin.take() {
            let prompt = prompt.to_string();
            tauri::async_runtime::spawn(async move {
                let _ = stdin.write_all(prompt.as_bytes()).await;
                let _ = stdin.shutdown().await;
            });
        }
    }

    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| "failed to capture agent stdout".to_string())?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| "failed to capture agent stderr".to_string())?;
    let stdout_task = tauri::async_runtime::spawn(async move {
        let mut buffer = Vec::new();
        let _ = stdout.read_to_end(&mut buffer).await;
        String::from_utf8_lossy(&buffer).to_string()
    });
    let stderr_task = tauri::async_runtime::spawn(async move {
        let mut buffer = Vec::new();
        let _ = stderr.read_to_end(&mut buffer).await;
        String::from_utf8_lossy(&buffer).to_string()
    });

    let timeout = sleep(Duration::from_millis(PLANNING_TIMEOUT_MS));
    tokio::pin!(timeout);
    let (status, timed_out) = tokio::select! {
        status = child.wait() => {
            (status.map_err(|error| format!("failed to wait for {}: {error}", profile.command))?, false)
        }
        _ = &mut timeout => {
            let _ = child.start_kill();
            let status = child
                .wait()
                .await
                .map_err(|error| format!("failed to kill timed out {}: {error}", profile.command))?;
            (status, true)
        }
    };

    let stdout = stdout_task
        .await
        .map_err(|error| format!("failed to join stdout reader: {error}"))?;
    let stderr = stderr_task
        .await
        .map_err(|error| format!("failed to join stderr reader: {error}"))?;
    let stdout = redact_sensitive_text(&stdout);
    let stderr = redact_sensitive_text(&stderr);
    let exit_code = status.code();
    let stderr_only_error = status.success()
        && stdout.trim().is_empty()
        && stderr
            .lines()
            .map(str::trim_start)
            .any(|line| line.starts_with("Error:") || line.starts_with("error:"));
    let status_text = if status.success() && !timed_out && !stderr_only_error {
        "succeeded"
    } else {
        "failed"
    }
    .to_string();
    let output_summary = if status_text == "succeeded" {
        summarize_cli_output(&stdout)
    } else if timed_out {
        format!(
            "{} timed out after {} ms.",
            profile.command, PLANNING_TIMEOUT_MS
        )
    } else if stderr_only_error {
        format!(
            "{} reported an error without stdout. {}",
            profile.command,
            stderr_tail(&stderr).join(" ")
        )
    } else {
        format!(
            "{} exited with status {:?}. {}",
            profile.command,
            exit_code,
            stderr_tail(&stderr).join(" ")
        )
    };

    Ok(PlanningInvocationResult {
        status: status_text,
        stdout,
        stderr,
        output_summary,
        evidence_ref: None,
        exit_code,
        timed_out,
        started_at_ms,
        ended_at_ms: now_ms(),
    })
}

fn resolve_planning_agents(agents: &[AgentConfig], requested_ids: &[String]) -> Vec<AgentConfig> {
    if !requested_ids.is_empty() {
        return requested_ids
            .iter()
            .filter_map(|id| agents.iter().find(|agent| &agent.id == id))
            .filter(|agent| agent.enabled && agent.available && has_planning_capability(agent))
            .cloned()
            .collect();
    }

    agents
        .iter()
        .filter(|agent| agent.enabled && agent.available && has_planning_capability(agent))
        .cloned()
        .collect()
}

fn has_planning_capability(agent: &AgentConfig) -> bool {
    agent
        .capabilities
        .iter()
        .any(|capability| capability == "planning")
}

fn render_planning_prompt(
    task_title: &str,
    project_path: &str,
    requirement: &str,
) -> PlanningPrompt {
    PlanningPrompt {
        content: format!(
            "# Loom Planning Request\n\n## Task\n\n{task_title}\n\n## Project\n\n{project_path}\n\n## Requirement\n\n{requirement}\n\n## Constraints\n\n- Planning stage only: do not modify files.\n- Return a practical implementation plan with risks and verification steps.\n- Call out assumptions and blockers explicitly.\n\n## Expected Output\n\nUse these sections:\n\n1. Goal\n2. Proposed approach\n3. Files or modules likely affected\n4. Risks\n5. Verification plan\n"
        ),
    }
}

fn deterministic_planning_output(agent: &AgentConfig, prompt: &PlanningPrompt) -> String {
    format!(
        "Agent: {}\nAdapter: dummy\n\nPlan:\n- Capture the raw requirement and selected planning agents.\n- Persist each agent discussion output with an evidence reference.\n- Generate a final Markdown plan and derive implementation todo items.\n\nRisk:\n- Keep dummy output clearly marked as test-only.\n- Do not treat this output as real Agent reasoning.\n\nPrompt excerpt:\n{}",
        agent.name,
        prompt.content.lines().take(12).collect::<Vec<_>>().join("\n")
    )
}

fn build_cli_profile(
    agent: &AgentConfig,
    project_path: &Path,
    prompt_path: &Path,
) -> Result<CliProfile, String> {
    let adapter_type = effective_adapter_type(agent);
    let command = agent.command.trim().to_string();
    if command.is_empty() {
        return Err(format!("agent '{}' has no command", agent.name));
    }

    let mut args = if agent.args.is_empty() {
        default_profile_args(&adapter_type, project_path)
    } else {
        agent.args.clone()
    };
    let had_prompt_file = args.iter().any(|arg| arg.contains("{promptFile}"));
    args = replace_arg_placeholders(args, project_path, prompt_path);
    let stdin_prompt = !had_prompt_file;

    Ok(CliProfile {
        command,
        args,
        stdin_prompt,
    })
}

fn default_profile_args(adapter_type: &str, project_path: &Path) -> Vec<String> {
    match adapter_type {
        ADAPTER_CODEX => vec![
            "exec".to_string(),
            "--cd".to_string(),
            project_path.display().to_string(),
            "--sandbox".to_string(),
            "read-only".to_string(),
            "-".to_string(),
        ],
        ADAPTER_CLAUDE_CODE => vec![
            "-p".to_string(),
            "--permission-mode".to_string(),
            "plan".to_string(),
            "--output-format".to_string(),
            "text".to_string(),
        ],
        ADAPTER_AMP => vec!["-x".to_string()],
        _ => Vec::new(),
    }
}

fn replace_arg_placeholders(
    args: Vec<String>,
    project_path: &Path,
    prompt_path: &Path,
) -> Vec<String> {
    let project_path = project_path.display().to_string();
    let prompt_path = prompt_path.display().to_string();
    args.into_iter()
        .map(|arg| {
            arg.replace("{projectPath}", &project_path)
                .replace("{promptFile}", &prompt_path)
        })
        .collect()
}

fn effective_adapter_type(agent: &AgentConfig) -> String {
    match agent.adapter_type.as_str() {
        ADAPTER_CLI => match agent.command.as_str() {
            "codex" => ADAPTER_CODEX.to_string(),
            "claude" => ADAPTER_CLAUDE_CODE.to_string(),
            "amp" => ADAPTER_AMP.to_string(),
            _ => ADAPTER_CLI.to_string(),
        },
        value => value.to_string(),
    }
}

fn summarize_agent_output(agent: &AgentConfig, output: &str) -> String {
    let first_plan_line = output
        .lines()
        .find(|line| line.trim_start().starts_with("- "))
        .unwrap_or("Produced a planning recommendation.");
    format!(
        "{}: {}",
        agent.name,
        first_plan_line.trim_start_matches("- ")
    )
}

fn summarize_cli_output(output: &str) -> String {
    output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.chars().take(220).collect())
        .unwrap_or_else(|| "Agent completed without text output.".to_string())
}

fn summarize_discussion(
    agents: &[AgentConfig],
    requirement: &str,
    invocations: &[AgentInvocation],
) -> String {
    let names = agents
        .iter()
        .map(|agent| agent.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let succeeded = invocations
        .iter()
        .filter(|invocation| invocation.status == "succeeded")
        .map(|invocation| invocation.agent_name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let failed = invocations
        .iter()
        .filter(|invocation| invocation.status != "succeeded")
        .map(|invocation| invocation.agent_name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "Planning agents requested: {names}. Successful agents: {}. Failed agents: {}. Requirement: {requirement}",
        if succeeded.is_empty() { "(none)" } else { &succeeded },
        if failed.is_empty() { "(none)" } else { &failed },
    )
}

fn render_final_plan(
    task_title: &str,
    requirement: &str,
    discussion_summary: &str,
    invocations: &[AgentInvocation],
) -> String {
    let has_successful_agent = invocations
        .iter()
        .any(|invocation| invocation.status == "succeeded");
    let agent_notes = invocations
        .iter()
        .map(|invocation| {
            format!(
                "- **{}** ({}) : {}",
                invocation.agent_name, invocation.status, invocation.output_summary
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let agent_todos = collect_implementation_todos_from_invocations(invocations);
    let implementation_todo = if !agent_todos.is_empty() {
        agent_todos
            .iter()
            .enumerate()
            .map(|(index, todo)| format!("{}. {}", index + 1, todo))
            .collect::<Vec<_>>()
            .join("\n")
    } else if has_successful_agent {
        "1. Review successful Agent output and select the implementation slice.\n2. Persist planning runs and Agent invocation evidence.\n3. Generate the final plan document and todo list.\n4. Add the implementation handoff view."
            .to_string()
    } else {
        "No implementation todo items were generated because all selected planning agents failed."
            .to_string()
    };
    let acceptance_criteria = if has_successful_agent {
        "- The planning discussion is visible in the right-side conversation stream.\n- The final plan is written to `docs/plans/` in the selected project.\n- The user can confirm the plan and move the task to `ready_to_implement` with actionable todo items."
    } else {
        "- Failed Agent diagnostics are visible in the right-side conversation stream.\n- The final plan is written to `docs/plans/` in the selected project for troubleshooting.\n- The task remains in plan review until a successful planning run is available."
    };

    format!(
        "# {task_title} — Final Plan\n\n## Requirement\n\n{requirement}\n\n## Discussion Summary\n\n{discussion_summary}\n\n## Agent Notes\n\n{agent_notes}\n\n## Implementation Todo\n\n{implementation_todo}\n\n## Acceptance Criteria\n\n{acceptance_criteria}\n"
    )
}

fn collect_implementation_todos_from_invocations(invocations: &[AgentInvocation]) -> Vec<String> {
    let mut todos = Vec::new();

    for invocation in invocations
        .iter()
        .filter(|invocation| invocation.status == "succeeded")
    {
        for todo in implementation_todos_from_agent_output(&invocation.raw_output) {
            let duplicate = todos
                .iter()
                .any(|existing: &String| existing.eq_ignore_ascii_case(&todo));
            if !duplicate {
                todos.push(todo);
            }
            if todos.len() >= 8 {
                return todos;
            }
        }
    }

    todos
}

fn implementation_todos_from_agent_output(output: &str) -> Vec<String> {
    let mut in_section = false;
    let mut todos = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            in_section = is_implementation_todo_heading(trimmed);
            continue;
        }

        if !in_section {
            continue;
        }

        if let Some(todo) = strip_agent_todo_marker(trimmed) {
            if !todo.is_empty() && !todo.to_lowercase().starts_with("no implementation todo") {
                todos.push(todo.to_string());
            }
        }
    }

    todos
}

fn is_implementation_todo_heading(line: &str) -> bool {
    let heading = line.trim_start_matches('#').trim().to_lowercase();
    matches!(
        heading.as_str(),
        "implementation todo"
            | "implementation todos"
            | "implementation tasks"
            | "implementation task list"
            | "implementation plan"
    )
}

fn strip_agent_todo_marker(line: &str) -> Option<&str> {
    let bullet = line.strip_prefix("- ").or_else(|| line.strip_prefix("* "));
    if let Some(value) = bullet {
        return Some(strip_agent_checkbox_marker(value.trim()));
    }

    let (number, rest) = line.split_once(". ").or_else(|| line.split_once(") "))?;
    if number.chars().all(|char| char.is_ascii_digit()) {
        Some(strip_agent_checkbox_marker(rest.trim()))
    } else {
        None
    }
}

fn strip_agent_checkbox_marker(line: &str) -> &str {
    ["[ ] ", "[x] ", "[X] "]
        .iter()
        .find_map(|marker| line.strip_prefix(marker))
        .unwrap_or(line)
        .trim()
}

fn planning_evidence_dir(project_path: &Path, task_id: &str, planning_run_id: &str) -> PathBuf {
    storage::project_loom_dir(project_path)
        .join("planning")
        .join(task_id)
        .join(planning_run_id)
}

fn next_project_plan_path(project_path: &Path, task_title: &str) -> Result<PathBuf, String> {
    next_project_plan_path_for_stamp(
        project_path,
        &local_date_string(),
        &local_time_string(),
        task_title,
    )
}

fn next_project_plan_path_for_stamp(
    project_path: &Path,
    local_date: &str,
    local_time: &str,
    task_title: &str,
) -> Result<PathBuf, String> {
    let plans_dir = storage::project_plans_dir(project_path).join(local_date);
    let base_name = format!("{}-{}", local_time, slugify_plan_title(task_title));
    let mut candidate = plans_dir.join(format!("{base_name}.md"));
    let mut suffix = 2;

    while candidate.exists() {
        candidate = plans_dir.join(format!("{base_name}-{suffix}.md"));
        suffix += 1;
    }

    Ok(candidate)
}

fn slugify_plan_title(title: &str) -> String {
    let mut slug = String::new();
    let mut previous_dash = false;

    for character in title.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            slug.push(character);
            previous_dash = false;
        } else if !previous_dash && !slug.is_empty() {
            slug.push('-');
            previous_dash = true;
        }

        if slug.len() >= 64 {
            break;
        }
    }

    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "plan".to_string()
    } else {
        slug
    }
}

fn local_date_string() -> String {
    platform_local_date().unwrap_or_else(|| utc_date_string(SystemTime::now()))
}

fn local_time_string() -> String {
    platform_local_time().unwrap_or_else(|| utc_time_string(SystemTime::now()))
}

#[cfg(not(windows))]
fn platform_local_date() -> Option<String> {
    Command::new("date")
        .arg("+%Y-%m-%d")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| value.len() == 10)
}

#[cfg(windows)]
fn platform_local_date() -> Option<String> {
    Command::new("powershell")
        .args(["-NoProfile", "-Command", "Get-Date -Format yyyy-MM-dd"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| value.len() == 10)
}

#[cfg(not(windows))]
fn platform_local_time() -> Option<String> {
    Command::new("date")
        .arg("+%H:%M")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| value.len() == 5)
}

#[cfg(windows)]
fn platform_local_time() -> Option<String> {
    Command::new("powershell")
        .args(["-NoProfile", "-Command", "Get-Date -Format HH:mm"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| value.len() == 5)
}

fn utc_date_string(time: SystemTime) -> String {
    let seconds = time
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default();
    let days = seconds.div_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

fn utc_time_string(time: SystemTime) -> String {
    let seconds = time
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default();
    let seconds_in_day = seconds.rem_euclid(86_400);
    let hour = seconds_in_day.div_euclid(3_600);
    let minute = seconds_in_day.rem_euclid(3_600).div_euclid(60);
    format!("{hour:02}:{minute:02}")
}

// Howard Hinnant's civil-from-days algorithm, using days since Unix epoch.
fn civil_from_days(days_since_epoch: i64) -> (i32, u32, u32) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 }.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096).div_euclid(365);
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2).div_euclid(153);
    let day = doy - (153 * mp + 2).div_euclid(5) + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += if month <= 2 { 1 } else { 0 };

    (year as i32, month as u32, day as u32)
}

fn load_agents(app: &AppHandle) -> Result<Vec<AgentConfig>, String> {
    let path = agents_path(app)?;

    if !path.exists() {
        let agents = discover_default_agents();
        save_agents(app, &agents)?;
        return Ok(agents);
    }

    let mut agents: Vec<AgentConfig> = storage::read_json_file(&path)?;
    merge_missing_default_agents(&mut agents);
    for agent in &mut agents {
        agent.available = command_available(agent);
    }
    save_agents(app, &agents)?;

    Ok(agents)
}

fn save_agents(app: &AppHandle, agents: &[AgentConfig]) -> Result<(), String> {
    storage::atomic_write_json(&agents_path(app)?, &agents)
}

fn agents_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(storage::global_config_dir(app)?.join(AGENTS_FILE))
}

fn discover_default_agents() -> Vec<AgentConfig> {
    let mut agents = [
        ("agent-codex", "Codex", "codex", ADAPTER_CODEX, true, true),
        (
            "agent-claude",
            "Claude Code",
            "claude",
            ADAPTER_CLAUDE_CODE,
            true,
            true,
        ),
        ("agent-amp", "Amp", "amp", ADAPTER_AMP, false, false),
    ]
    .into_iter()
    .map(
        |(id, name, command, adapter_type, can_write_files, can_run_commands)| {
            let mut agent = AgentConfig {
                id: id.to_string(),
                name: name.to_string(),
                command: command.to_string(),
                args: Vec::new(),
                working_directory_policy: "project_root".to_string(),
                capabilities: vec![
                    "planning".to_string(),
                    "implementation".to_string(),
                    "review".to_string(),
                ],
                adapter_type: adapter_type.to_string(),
                can_write_files,
                can_run_commands,
                enabled: true,
                available: false,
            };
            agent.available = command_available(&agent);
            agent
        },
    )
    .collect::<Vec<_>>();

    agents.push(AgentConfig {
        id: "agent-dummy".to_string(),
        name: "Dummy Agent (test)".to_string(),
        command: "dummy".to_string(),
        args: Vec::new(),
        working_directory_policy: "project_root".to_string(),
        capabilities: vec!["planning".to_string()],
        adapter_type: ADAPTER_DUMMY.to_string(),
        can_write_files: false,
        can_run_commands: false,
        enabled: false,
        available: true,
    });

    agents
}

fn merge_missing_default_agents(agents: &mut Vec<AgentConfig>) {
    for default_agent in discover_default_agents() {
        if let Some(existing) = agents.iter_mut().find(|agent| agent.id == default_agent.id) {
            let preserve_enabled = existing.adapter_type != ADAPTER_DUMMY;
            existing.name = default_agent.name.clone();
            existing.command = default_agent.command.clone();
            existing.args = default_agent.args.clone();
            existing.working_directory_policy = default_agent.working_directory_policy.clone();
            existing.capabilities = default_agent.capabilities.clone();
            existing.adapter_type = default_agent.adapter_type.clone();
            existing.can_write_files = default_agent.can_write_files;
            existing.can_run_commands = default_agent.can_run_commands;
            existing.available = default_agent.available;
            if !preserve_enabled {
                existing.enabled = default_agent.enabled;
            }
            continue;
        }

        let already_present = agents.iter().any(|agent| {
            agent.command == default_agent.command
                && effective_adapter_type(agent) == default_agent.adapter_type
        });

        if !already_present {
            agents.push(default_agent);
        }
    }
}

fn command_available(agent: &AgentConfig) -> bool {
    if agent.adapter_type == ADAPTER_DUMMY {
        return true;
    }

    if agent.command.trim().is_empty() {
        return false;
    }

    #[cfg(windows)]
    let available = Command::new("where").arg(&agent.command).output();

    #[cfg(not(windows))]
    let available = Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {}", shell_escape(&agent.command)))
        .output();

    available
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn stderr_tail(stderr: &str) -> Vec<String> {
    stderr
        .lines()
        .rev()
        .take(20)
        .map(str::to_string)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

fn redact_sensitive_text(input: &str) -> String {
    let mut output = input
        .lines()
        .map(redact_sensitive_line)
        .collect::<Vec<_>>()
        .join("\n");

    if input.ends_with('\n') {
        output.push('\n');
    }

    redact_bearer_tokens(&output)
}

fn redact_sensitive_line(line: &str) -> String {
    let lower = line.to_ascii_lowercase();
    let sensitive_keys = [
        "api_key",
        "apikey",
        "access_token",
        "auth_token",
        "token",
        "secret",
        "password",
        "passwd",
    ];

    if !sensitive_keys.iter().any(|key| lower.contains(key)) {
        return line.to_string();
    }

    match line
        .char_indices()
        .find(|(_, character)| matches!(character, '=' | ':'))
    {
        Some((index, separator)) => format!("{}{} [REDACTED]", &line[..index], separator),
        None => "[REDACTED sensitive line]".to_string(),
    }
}

fn redact_bearer_tokens(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut remaining = input;

    while let Some(index) = remaining.to_ascii_lowercase().find("bearer ") {
        output.push_str(&remaining[..index]);
        output.push_str(&remaining[index..index + 7]);
        output.push_str("[REDACTED]");

        let token_start = index + 7;
        let token_end = remaining[token_start..]
            .char_indices()
            .find(|(_, character)| character.is_whitespace())
            .map(|(offset, _)| token_start + offset)
            .unwrap_or(remaining.len());
        remaining = &remaining[token_end..];
    }

    output.push_str(remaining);
    output
}

#[cfg(not(windows))]
fn shell_escape(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_agent(command: &str, adapter_type: &str, args: Vec<String>) -> AgentConfig {
        AgentConfig {
            id: format!("agent-{command}"),
            name: command.to_string(),
            command: command.to_string(),
            args,
            working_directory_policy: "project_root".to_string(),
            capabilities: vec!["planning".to_string()],
            adapter_type: adapter_type.to_string(),
            can_write_files: false,
            can_run_commands: false,
            enabled: true,
            available: true,
        }
    }

    fn test_invocation(agent_name: &str, status: &str, raw_output: &str) -> AgentInvocation {
        AgentInvocation {
            id: format!("invocation-{agent_name}"),
            planning_run_id: "planning-run-test".to_string(),
            task_id: "task-test".to_string(),
            agent_id: format!("agent-{agent_name}"),
            agent_name: agent_name.to_string(),
            status: status.to_string(),
            prompt_summary: "test prompt".to_string(),
            raw_output: raw_output.to_string(),
            output_summary: "test summary".to_string(),
            evidence_ref: None,
            stderr_tail: Vec::new(),
            exit_code: Some(0),
            timed_out: false,
            started_at_ms: 1,
            ended_at_ms: Some(2),
        }
    }

    #[test]
    fn renders_prompt_with_requirement_and_project_path() {
        let prompt = render_planning_prompt("Add adapter", "/tmp/project", "Use real CLIs");

        assert!(prompt.content.contains("Add adapter"));
        assert!(prompt.content.contains("/tmp/project"));
        assert!(prompt.content.contains("Use real CLIs"));
        assert!(prompt.content.contains("do not modify files"));
    }

    #[test]
    fn final_plan_uses_successful_agent_implementation_todos() {
        let invocations = vec![
            test_invocation(
                "codex",
                "succeeded",
                "# Plan\n\n## Implementation Tasks\n\n1. Add task bridge\n- [ ] Persist review evidence\n\n## Risks\n\n- keep small",
            ),
            test_invocation(
                "claude",
                "failed",
                "## Implementation Todo\n\n1. Should be ignored because invocation failed",
            ),
        ];

        let final_plan = render_final_plan("Task Bridge", "Ship bridge", "summary", &invocations);

        assert!(final_plan.contains("1. Add task bridge"));
        assert!(final_plan.contains("2. Persist review evidence"));
        assert!(!final_plan.contains("Should be ignored"));
        assert!(!final_plan.contains("Review successful Agent output"));
    }

    #[test]
    fn final_plan_deduplicates_agent_todos() {
        let invocations = vec![
            test_invocation(
                "codex",
                "succeeded",
                "## Implementation Todo\n\n- Add task bridge\n- Add validation",
            ),
            test_invocation(
                "claude",
                "succeeded",
                "## Implementation Todo\n\n1. add task bridge\n2. Persist evidence",
            ),
        ];

        assert_eq!(
            collect_implementation_todos_from_invocations(&invocations),
            vec![
                "Add task bridge".to_string(),
                "Add validation".to_string(),
                "Persist evidence".to_string()
            ]
        );
    }

    #[test]
    fn builds_codex_claude_and_amp_profiles() {
        let project_path = Path::new("/tmp/project");
        let prompt_path = Path::new("/tmp/project/.loom/planning/prompt.md");

        let codex = build_cli_profile(
            &test_agent("codex", ADAPTER_CODEX, Vec::new()),
            project_path,
            prompt_path,
        )
        .expect("codex profile should build");
        assert_eq!(codex.command, "codex");
        assert!(codex.args.contains(&"exec".to_string()));
        assert!(codex.args.contains(&"read-only".to_string()));
        assert!(!codex
            .args
            .iter()
            .any(|arg| arg.contains("ask-for-approval")));
        assert!(codex.stdin_prompt);

        let claude = build_cli_profile(
            &test_agent("claude", ADAPTER_CLAUDE_CODE, Vec::new()),
            project_path,
            prompt_path,
        )
        .expect("claude profile should build");
        assert_eq!(claude.command, "claude");
        assert!(claude.args.contains(&"-p".to_string()));
        assert!(!claude.args.iter().any(|arg| arg.contains("claude-code")));

        let amp = build_cli_profile(
            &test_agent("amp", ADAPTER_AMP, Vec::new()),
            project_path,
            prompt_path,
        )
        .expect("amp profile should build");
        assert_eq!(amp.command, "amp");
        assert!(amp.args.contains(&"-x".to_string()));
        assert!(amp.stdin_prompt);
    }

    #[test]
    fn syncs_default_dummy_to_disabled_test_fixture() {
        let mut agents = vec![AgentConfig {
            id: "agent-dummy".to_string(),
            name: "Dummy Planner".to_string(),
            command: "dummy".to_string(),
            args: Vec::new(),
            working_directory_policy: "project_root".to_string(),
            capabilities: vec!["planning".to_string(), "review".to_string()],
            adapter_type: ADAPTER_DUMMY.to_string(),
            can_write_files: false,
            can_run_commands: false,
            enabled: true,
            available: true,
        }];

        merge_missing_default_agents(&mut agents);

        let dummy = agents
            .iter()
            .find(|agent| agent.id == "agent-dummy")
            .expect("dummy fixture should remain present");
        assert_eq!(dummy.name, "Dummy Agent (test)");
        assert!(!dummy.enabled);
        assert_eq!(dummy.capabilities, vec!["planning".to_string()]);
    }

    #[test]
    fn requested_agents_do_not_fall_back_to_every_enabled_agent() {
        let agents = vec![
            test_agent("codex", ADAPTER_CODEX, Vec::new()),
            AgentConfig {
                id: "agent-disabled".to_string(),
                name: "Disabled Claude".to_string(),
                command: "claude".to_string(),
                args: Vec::new(),
                working_directory_policy: "project_root".to_string(),
                capabilities: vec!["planning".to_string()],
                adapter_type: ADAPTER_CLAUDE_CODE.to_string(),
                can_write_files: false,
                can_run_commands: false,
                enabled: false,
                available: true,
            },
        ];

        let selected = resolve_planning_agents(&agents, &["agent-disabled".to_string()]);

        assert!(selected.is_empty());
    }

    #[test]
    fn empty_agent_request_uses_enabled_planning_agents() {
        let agents = vec![test_agent("codex", ADAPTER_CODEX, Vec::new())];

        let selected = resolve_planning_agents(&agents, &[]);

        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].id, "agent-codex");
    }

    #[test]
    fn slugifies_plan_title_for_project_docs_plans_filename() {
        assert_eq!(
            slugify_plan_title("Real CLI Agent Adapter!"),
            "real-cli-agent-adapter"
        );
        assert_eq!(slugify_plan_title("计划功能"), "plan");
    }

    #[test]
    fn fallback_utc_date_and_time_format_plan_stamp_parts() {
        assert_eq!(utc_date_string(UNIX_EPOCH), "1970-01-01");
        assert_eq!(utc_time_string(UNIX_EPOCH), "00:00");
        assert_eq!(
            utc_date_string(UNIX_EPOCH + Duration::from_secs(86_400 * 20_000)),
            "2024-10-04"
        );
        assert_eq!(
            utc_time_string(UNIX_EPOCH + Duration::from_secs(23 * 3_600 + 59 * 60)),
            "23:59"
        );
    }

    #[test]
    fn next_project_plan_path_uses_date_directory_time_and_slug() {
        let root = std::env::temp_dir().join(format!("loom-plan-path-test-{}", now_ms()));
        let path = next_project_plan_path_for_stamp(
            &root,
            "2026-05-12",
            "04:00",
            "Real CLI Agent Adapter",
        )
        .expect("plan path should be generated");

        assert_eq!(
            path.parent(),
            Some(root.join("docs").join("plans").join("2026-05-12")).as_deref()
        );
        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("04:00-real-cli-agent-adapter.md")
        );
    }

    #[test]
    fn next_project_plan_path_suffixes_colliding_same_minute_plan() {
        let root = std::env::temp_dir().join(format!("loom-plan-path-collision-test-{}", now_ms()));
        let plans_dir = root.join("docs").join("plans").join("2026-05-12");
        fs::create_dir_all(&plans_dir).expect("test dir should be created");
        fs::write(
            plans_dir.join("04:00-real-cli-agent-adapter.md"),
            "existing",
        )
        .expect("collision file should be written");

        let path = next_project_plan_path_for_stamp(
            &root,
            "2026-05-12",
            "04:00",
            "Real CLI Agent Adapter",
        )
        .expect("plan path should be generated");

        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("04:00-real-cli-agent-adapter-2.md")
        );

        fs::remove_dir_all(root).expect("test dir should be removed");
    }

    #[test]
    fn replaces_prompt_and_project_placeholders() {
        let project_path = Path::new("/tmp/project");
        let prompt_path = Path::new("/tmp/project/.loom/prompt.md");
        let agent = test_agent(
            "agent-cli",
            ADAPTER_CLI,
            vec![
                "--project".to_string(),
                "{projectPath}".to_string(),
                "--prompt".to_string(),
                "{promptFile}".to_string(),
            ],
        );

        let profile = build_cli_profile(&agent, project_path, prompt_path)
            .expect("custom cli profile should build");

        assert!(profile.args.contains(&"/tmp/project".to_string()));
        assert!(profile
            .args
            .contains(&"/tmp/project/.loom/prompt.md".to_string()));
        assert!(!profile.stdin_prompt);
    }

    #[tokio::test]
    async fn runs_cli_profile_with_stdin_and_captures_stdout() {
        let profile = CliProfile {
            command: "sh".to_string(),
            args: vec!["-c".to_string(), "cat".to_string()],
            stdin_prompt: true,
        };

        let result = run_cli_profile(&profile, Path::new("."), "hello from prompt")
            .await
            .expect("shell profile should run");

        assert_eq!(result.status, "succeeded");
        assert_eq!(result.exit_code, Some(0));
        assert!(result.stdout.contains("hello from prompt"));
    }

    #[test]
    fn redacts_sensitive_tokens_from_planning_logs() {
        let bearer_line = format!("Authorization: {} {}", "Bearer", "samplecredential");
        let input = format!("api_key=sk-live-123\n{bearer_line}\nkeep this line");
        let redacted = redact_sensitive_text(&input);

        assert!(redacted.contains("api_key= [REDACTED]"));
        assert!(redacted.contains("Bearer [REDACTED]"));
        assert!(redacted.contains("keep this line"));
        assert!(!redacted.contains("sk-live-123"));
        assert!(!redacted.contains("samplecredential"));
    }

    #[tokio::test]
    async fn cli_profile_redacts_stdout_and_stderr_before_storage() {
        let profile = CliProfile {
            command: "sh".to_string(),
            args: vec![
                "-c".to_string(),
                "printf 'token=secret-stdout\\n'; printf 'password=hunter2\\n' >&2".to_string(),
            ],
            stdin_prompt: false,
        };

        let result = run_cli_profile(&profile, Path::new("."), "")
            .await
            .expect("shell profile should run");

        assert!(!result.stdout.contains("secret-stdout"));
        assert!(!result.stderr.contains("hunter2"));
        assert!(result.stdout.contains("token= [REDACTED]"));
        assert!(result.stderr.contains("password= [REDACTED]"));
    }

    #[tokio::test]
    async fn failed_cli_profile_preserves_stderr_tail() {
        let profile = CliProfile {
            command: "sh".to_string(),
            args: vec![
                "-c".to_string(),
                "printf 'fatal: nope\\n' >&2; exit 7".to_string(),
            ],
            stdin_prompt: false,
        };

        let result = run_cli_profile(&profile, Path::new("."), "")
            .await
            .expect("failing shell profile should still return result");

        assert_eq!(result.status, "failed");
        assert_eq!(result.exit_code, Some(7));
        assert_eq!(stderr_tail(&result.stderr), vec!["fatal: nope".to_string()]);
    }

    #[tokio::test]
    async fn stderr_only_error_is_failed_even_with_zero_exit() {
        let profile = CliProfile {
            command: "sh".to_string(),
            args: vec![
                "-c".to_string(),
                "printf 'Error: paid credits required\\n' >&2".to_string(),
            ],
            stdin_prompt: false,
        };

        let result = run_cli_profile(&profile, Path::new("."), "")
            .await
            .expect("stderr-only error profile should return result");

        assert_eq!(result.status, "failed");
        assert_eq!(result.exit_code, Some(0));
        assert!(result.output_summary.contains("reported an error"));
    }
}
