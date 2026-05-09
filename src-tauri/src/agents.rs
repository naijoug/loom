use crate::{
    models::{
        now_ms, AgentConfig, AgentConfigInput, AgentInvocation, IdGenerator, PlanTodoItem,
        PlanningDiscussionInput, PlanningRun, TaskEvent,
    },
    storage, tasks,
};
use std::{fs, path::Path, process::Command};
use tauri::{AppHandle, State};

const AGENTS_FILE: &str = "agents.json";

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
pub fn run_planning_discussion(
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

    let mut invocations = Vec::new();
    for agent in &selected_agents {
        let invocation_started_at_ms = now_ms();
        let output = deterministic_planning_output(agent, &task.title, &requirement);
        let evidence_path = write_agent_invocation_output(
            Path::new(&input.project_path),
            &task.id,
            &planning_run_id,
            &agent.id,
            &output,
        )?;
        invocations.push(AgentInvocation {
            id: ids.next("invoke"),
            planning_run_id: planning_run_id.clone(),
            task_id: task.id.clone(),
            agent_id: agent.id.clone(),
            agent_name: agent.name.clone(),
            status: "succeeded".to_string(),
            prompt_summary: prompt_summary.clone(),
            raw_output: output.clone(),
            output_summary: summarize_agent_output(agent, &output),
            evidence_ref: Some(evidence_path.display().to_string()),
            started_at_ms: invocation_started_at_ms,
            ended_at_ms: Some(now_ms()),
        });
    }

    let discussion_summary = summarize_discussion(&selected_agents, &requirement);
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

    let plan_todos = derive_plan_todos(&ids, &task.id, &plan_path.display().to_string());
    let finished_at_ms = now_ms();
    task.status = "ready_to_implement".to_string();
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
        status: "succeeded".to_string(),
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
        output_summary: Some(
            "Planning discussion generated a final plan and implementation todo list.".to_string(),
        ),
        evidence_ref: task.final_plan_path.clone(),
    });
    tasks::save_task(&task)?;

    Ok(task)
}

#[tauri::command]
pub fn run_dummy_planning(
    ids: State<'_, IdGenerator>,
    project_path: String,
    task_id: String,
    agent_id: String,
) -> Result<crate::models::Task, String> {
    let mut task = tasks::load_task(Path::new(&project_path), &task_id)?;
    let plan = format!(
        "# {}\n\n目标：实现任务「{}」的功能闭环。\n\n风险：保持变更小而可回滚，命令执行必须记录 cwd、输出和退出状态。\n\n验证建议：先运行项目推荐命令，再记录失败摘要和人工反馈。",
        task.title, task.title
    );
    let plan_path = storage::project_plans_dir(Path::new(&project_path))
        .join(format!("{}-final-plan.md", task.id));
    fs::create_dir_all(
        plan_path
            .parent()
            .ok_or_else(|| "invalid plan path".to_string())?,
    )
    .map_err(|error| format!("failed to create plans directory: {error}"))?;
    fs::write(&plan_path, &plan).map_err(|error| format!("failed to write plan draft: {error}"))?;

    task.status = "planning".to_string();
    task.final_plan = Some(plan.clone());
    task.selected_planning_agent_ids = vec![agent_id];
    task.updated_at_ms = now_ms();
    task.events.push(TaskEvent {
        id: ids.next("event"),
        task_id: task.id.clone(),
        timestamp_ms: now_ms(),
        actor: "agent".to_string(),
        status: task.status.clone(),
        input_summary: Some(task.raw_requirement.clone()),
        output_summary: Some(
            "Dummy planning generated a deterministic implementation plan.".to_string(),
        ),
        evidence_ref: Some(plan_path.display().to_string()),
    });
    tasks::save_task(&task)?;

    Ok(task)
}

fn resolve_planning_agents(agents: &[AgentConfig], requested_ids: &[String]) -> Vec<AgentConfig> {
    let requested: Vec<AgentConfig> = requested_ids
        .iter()
        .filter_map(|id| agents.iter().find(|agent| &agent.id == id))
        .filter(|agent| agent.enabled && agent.available && has_planning_capability(agent))
        .cloned()
        .collect();

    if !requested.is_empty() {
        return requested;
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

fn deterministic_planning_output(
    agent: &AgentConfig,
    task_title: &str,
    requirement: &str,
) -> String {
    format!(
        "Agent: {}\nTask: {}\n\nPlan:\n- Capture the raw requirement and selected planning agents.\n- Persist each agent discussion output with an evidence reference.\n- Generate a final Markdown plan and derive implementation todo items.\n\nRisk:\n- Keep CLI planning non-interactive for the first slice.\n- Do not start implementation until the final plan is confirmed.\n\nRequirement:\n{}",
        agent.name, task_title, requirement
    )
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

fn summarize_discussion(agents: &[AgentConfig], requirement: &str) -> String {
    let names = agents
        .iter()
        .map(|agent| agent.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "Planning agents ({names}) agreed to keep the first MVP slice focused on requirement capture, agent discussion records, final plan generation, and todo handoff. Requirement: {requirement}"
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
                "- **{}**: {}",
                invocation.agent_name, invocation.output_summary
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

fn write_agent_invocation_output(
    project_path: &Path,
    task_id: &str,
    planning_run_id: &str,
    agent_id: &str,
    output: &str,
) -> Result<std::path::PathBuf, String> {
    let dir = storage::project_loom_dir(project_path)
        .join("planning")
        .join(task_id)
        .join(planning_run_id);
    fs::create_dir_all(&dir)
        .map_err(|error| format!("failed to create planning output directory: {error}"))?;
    let path = dir.join(format!("{agent_id}.md"));
    fs::write(&path, output)
        .map_err(|error| format!("failed to write planning output: {error}"))?;
    Ok(path)
}

fn load_agents(app: &AppHandle) -> Result<Vec<AgentConfig>, String> {
    let path = agents_path(app)?;

    if !path.exists() {
        let agents = discover_default_agents();
        save_agents(app, &agents)?;
        return Ok(agents);
    }

    let mut agents: Vec<AgentConfig> = storage::read_json_file(&path)?;
    for agent in &mut agents {
        agent.available = command_available(agent);
    }

    Ok(agents)
}

fn save_agents(app: &AppHandle, agents: &[AgentConfig]) -> Result<(), String> {
    storage::atomic_write_json(&agents_path(app)?, &agents)
}

fn agents_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(storage::global_config_dir(app)?.join(AGENTS_FILE))
}

fn discover_default_agents() -> Vec<AgentConfig> {
    ["codex", "claude-code", "amp"]
        .into_iter()
        .filter_map(|command| {
            let mut agent = AgentConfig {
                id: format!("agent-{command}"),
                name: command.to_string(),
                command: command.to_string(),
                args: Vec::new(),
                working_directory_policy: "project_root".to_string(),
                capabilities: vec![
                    "planning".to_string(),
                    "implementation".to_string(),
                    "review".to_string(),
                ],
                adapter_type: "cli".to_string(),
                can_write_files: command != "amp",
                can_run_commands: command != "amp",
                enabled: true,
                available: false,
            };
            agent.available = command_available(&agent);
            agent.available.then_some(agent)
        })
        .collect()
}

fn command_available(agent: &AgentConfig) -> bool {
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

#[cfg(not(windows))]
fn shell_escape(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
