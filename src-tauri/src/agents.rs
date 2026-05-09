use crate::{
    models::{now_ms, AgentConfig, AgentConfigInput, IdGenerator, TaskEvent},
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

fn load_agents(app: &AppHandle) -> Result<Vec<AgentConfig>, String> {
    let path = agents_path(app)?;

    if !path.exists() {
        let agents = vec![default_dummy_agent()];
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

fn default_dummy_agent() -> AgentConfig {
    AgentConfig {
        id: "agent-dummy".to_string(),
        name: "Dummy Planner".to_string(),
        command: "dummy".to_string(),
        args: Vec::new(),
        working_directory_policy: "project_root".to_string(),
        capabilities: vec![
            "planning".to_string(),
            "review".to_string(),
            "debugging".to_string(),
        ],
        adapter_type: "dummy".to_string(),
        can_write_files: false,
        can_run_commands: false,
        enabled: true,
        available: true,
    }
}

fn command_available(agent: &AgentConfig) -> bool {
    if agent.adapter_type == "dummy" {
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

#[cfg(not(windows))]
fn shell_escape(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
