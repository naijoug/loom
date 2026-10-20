use crate::agent_adapter::{ADAPTER_CLAUDE as ADAPTER_CLAUDE_CODE, ADAPTER_CODEX, ADAPTER_GROK};
use crate::{
    migrations::{self, AGENT_STORE_SCHEMA_VERSION},
    models::{AgentConfig, AgentConfigInput, IdGenerator},
    storage,
};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, State};
pub(crate) mod config;
pub(crate) mod stream;
use config::*;
const AGENTS_FILE: &str = "agents.json";
const ADAPTER_CLI: &str = "cli";
const ADAPTER_DUMMY: &str = "dummy";
const RETIRED_ADAPTER_AMP: &str = "amp_cli";
const RETIRED_AGENT_AMP_ID: &str = "agent-amp";
pub(crate) fn load_agents(app: &AppHandle) -> Result<Vec<AgentConfig>, String> {
    config::load_agents_inner(app)
}
pub(crate) fn redact_sensitive_text(input: &str) -> String {
    config::redact_sensitive_text_inner(input)
}
fn effective_adapter_type(agent: &AgentConfig) -> String {
    agent.adapter_type.clone()
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
    validate_input(&input)?;
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
pub fn update_agent(
    app: AppHandle,
    agent_id: String,
    input: AgentConfigInput,
) -> Result<Vec<AgentConfig>, String> {
    validate_input(&input)?;
    let mut agents = load_agents(&app)?;
    let mut updated = AgentConfig {
        id: agent_id.clone(),
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
    updated.available = command_available(&updated);
    apply_agent_update(&mut agents, &agent_id, updated)?;
    save_agents(&app, &agents)?;

    Ok(agents)
}

#[tauri::command]
pub fn delete_agent(app: AppHandle, agent_id: String) -> Result<Vec<AgentConfig>, String> {
    let mut agents = load_agents(&app)?;
    delete_agent_config(&mut agents, &agent_id)?;
    save_agents(&app, &agents)?;

    Ok(agents)
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

fn apply_agent_update(
    agents: &mut [AgentConfig],
    agent_id: &str,
    updated: AgentConfig,
) -> Result<(), String> {
    let Some(agent) = agents.iter_mut().find(|agent| agent.id == agent_id) else {
        return Err("agent not found".to_string());
    };

    if is_default_agent_id(agent_id) && agent.adapter_type != updated.adapter_type {
        return Err("built-in adapter type cannot be changed".into());
    }
    *agent = updated;
    Ok(())
}

fn delete_agent_config(agents: &mut Vec<AgentConfig>, agent_id: &str) -> Result<(), String> {
    if is_default_agent_id(agent_id) {
        return Err("built-in Agent profiles can only be disabled, not deleted".to_string());
    }

    let initial_len = agents.len();
    agents.retain(|agent| agent.id != agent_id);

    if agents.len() == initial_len {
        return Err("agent not found".to_string());
    }

    Ok(())
}

fn validate_input(input: &AgentConfigInput) -> Result<(), String> {
    if input.name.trim().is_empty() || input.command.trim().is_empty() {
        return Err("Agent name and executable are required".into());
    }
    let name = Path::new(input.command.trim())
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if matches!(
        name.as_str(),
        "sh" | "bash" | "zsh" | "fish" | "cmd" | "cmd.exe" | "powershell" | "pwsh"
    ) {
        return Err("Use a dedicated executable, not a shell wrapper".into());
    }
    if !matches!(
        input.adapter_type.as_str(),
        ADAPTER_CLI
            | ADAPTER_DUMMY
            | ADAPTER_CODEX
            | ADAPTER_CLAUDE_CODE
            | ADAPTER_GROK
            | crate::agent_adapter::ADAPTER_OPENCODE
    ) {
        return Err("Unsupported CLI adapter".into());
    }
    Ok(())
}
