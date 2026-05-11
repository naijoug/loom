use crate::{
    models::{
        now_ms, AgentConfig, AgentConfigInput, AgentInvocation, IdGenerator, PlanTodoItem,
        PlanningDiscussionInput, PlanningRun, TaskEvent,
    },
    storage, tasks,
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
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
    let plan_path = storage::project_plans_dir(Path::new(&input.project_path))
        .join(format!("{}-final-plan.md", task.id));
    fs::create_dir_all(
        plan_path
            .parent()
            .ok_or_else(|| "invalid plan path".to_string())?,
    )
    .map_err(|error| format!("failed to create plans directory: {error}"))?;
    fs::write(&plan_path, &final_plan)
        .map_err(|error| format!("failed to write final plan: {error}"))?;

    let plan_todos = if successful_invocations > 0 {
        derive_plan_todos(&ids, &task.id, &plan_path.display().to_string())
    } else {
        Vec::new()
    };
    let finished_at_ms = now_ms();
    task.status = if successful_invocations > 0 {
        "ready_to_implement".to_string()
    } else {
        "plan_review".to_string()
    };
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
    task.plan_todos = plan_todos;
    task.updated_at_ms = finished_at_ms;
    task.events.push(TaskEvent {
        id: ids.next("event"),
        task_id: task.id.clone(),
        timestamp_ms: finished_at_ms,
        actor: "agent".to_string(),
        status: task.status.clone(),
        input_summary: Some(prompt_summary),
        output_summary: Some(if successful_invocations > 0 {
            "Planning discussion generated a final plan and implementation todo list.".to_string()
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
    fs::write(&prompt_path, &prompt.content)
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
    let result = run_cli_profile(&profile, project_path, &prompt.content).await;

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
            let stderr = error;
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

    format!(
        "# {task_title} — Final Plan\n\n## Requirement\n\n{requirement}\n\n## Discussion Summary\n\n{discussion_summary}\n\n## Agent Notes\n\n{agent_notes}\n\n## Implementation Todo\n\n1. Implement the planning Agent adapter contract.\n2. Persist planning runs and Agent invocation evidence.\n3. Generate the final plan document and todo list.\n4. Add the implementation handoff view.\n\n## Acceptance Criteria\n\n- The planning discussion is visible in the right-side conversation stream.\n- The final plan is written to `.loom/plans/`.\n- The task reaches `ready_to_implement` with actionable todo items.\n"
    )
}

fn derive_plan_todos(
    ids: &State<'_, IdGenerator>,
    task_id: &str,
    plan_ref: &str,
) -> Vec<PlanTodoItem> {
    [
        (
            "Agent adapter 最小执行协议",
            "实现计划阶段的 Agent 调用契约，第一版保留非交互 prompt-file 路径。",
        ),
        (
            "规划讨论事件与原始输出持久化",
            "保存 PlanningRun、AgentInvocation、原始输出、摘要和 evidenceRef。",
        ),
        (
            "最终计划生成与进入实施按钮",
            "写入最终计划 Markdown，派生 todo，并让任务进入 ready_to_implement。",
        ),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (title, description))| PlanTodoItem {
        id: ids.next("todo"),
        task_id: task_id.to_string(),
        title: title.to_string(),
        description: description.to_string(),
        status: "pending".to_string(),
        order: index as u32,
        plan_ref: Some(plan_ref.to_string()),
    })
    .collect()
}

fn planning_evidence_dir(project_path: &Path, task_id: &str, planning_run_id: &str) -> PathBuf {
    storage::project_loom_dir(project_path)
        .join("planning")
        .join(task_id)
        .join(planning_run_id)
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

    #[test]
    fn renders_prompt_with_requirement_and_project_path() {
        let prompt = render_planning_prompt("Add adapter", "/tmp/project", "Use real CLIs");

        assert!(prompt.content.contains("Add adapter"));
        assert!(prompt.content.contains("/tmp/project"));
        assert!(prompt.content.contains("Use real CLIs"));
        assert!(prompt.content.contains("do not modify files"));
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
