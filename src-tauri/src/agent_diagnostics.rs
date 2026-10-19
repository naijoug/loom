use crate::{agents, models::now_ms};
use serde::Serialize;
use std::{path::Path, process::Command, time::Duration};
use tauri::AppHandle;
use tokio::{process::Command as TokioCommand, time::timeout};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDiagnostic {
    pub agent_id: String,
    pub status: String,
    pub resolved_path: Option<String>,
    pub version: Option<String>,
    pub auth_status: String,
    pub detail: String,
    pub checked_at_ms: u128,
}

fn resolve_command_path(command: &str) -> Option<String> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return None;
    }
    let direct = Path::new(trimmed);
    if direct.is_file() {
        return direct
            .canonicalize()
            .ok()
            .map(|path| path.display().to_string());
    }

    #[cfg(windows)]
    let output = Command::new("where").arg(trimmed).output().ok()?;
    #[cfg(not(windows))]
    let output = Command::new("which").arg(trimmed).output().ok()?;

    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

fn compact_version(output: &[u8]) -> Option<String> {
    String::from_utf8_lossy(output)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.chars().take(160).collect())
}

async fn diagnose_agent(agent: crate::models::AgentConfig) -> AgentDiagnostic {
    let checked_at_ms = now_ms();
    if agent.adapter_type == "dummy" {
        return AgentDiagnostic {
            agent_id: agent.id,
            status: if agent.enabled { "ready" } else { "disabled" }.to_string(),
            resolved_path: None,
            version: Some("built-in test adapter".to_string()),
            auth_status: "not_applicable".to_string(),
            detail: "No external process is used.".to_string(),
            checked_at_ms,
        };
    }

    let resolved_path = resolve_command_path(&agent.command);
    if resolved_path.is_none() {
        return AgentDiagnostic {
            agent_id: agent.id,
            status: "missing".to_string(),
            resolved_path: None,
            version: None,
            auth_status: "unknown".to_string(),
            detail: format!("Command '{}' was not found on PATH.", agent.command),
            checked_at_ms,
        };
    }

    let mut command = TokioCommand::new(&agent.command);
    command.arg("--version").kill_on_drop(true);
    let version_result = timeout(Duration::from_secs(5), command.output()).await;
    let (version, detail) = match version_result {
        Ok(Ok(output)) if output.status.success() => (
            compact_version(&output.stdout).or_else(|| compact_version(&output.stderr)),
            "Executable and version probe succeeded.".to_string(),
        ),
        Ok(Ok(output)) => (
            None,
            format!(
                "Executable was found, but '--version' exited with {}.",
                output
                    .status
                    .code()
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "a signal".to_string())
            ),
        ),
        Ok(Err(error)) => (None, format!("Version probe failed: {error}")),
        Err(_) => (None, "Version probe timed out after 5 seconds.".to_string()),
    };

    AgentDiagnostic {
        agent_id: agent.id,
        status: if agent.enabled { "ready" } else { "disabled" }.to_string(),
        resolved_path,
        version,
        auth_status: "unknown".to_string(),
        detail: format!("{detail} Authentication is verified by the next real Agent invocation."),
        checked_at_ms,
    }
}

#[tauri::command]
pub async fn diagnose_agents(app: AppHandle) -> Result<Vec<AgentDiagnostic>, String> {
    let mut diagnostics = Vec::new();
    for agent in agents::load_agents(&app)? {
        diagnostics.push(diagnose_agent(agent).await);
    }
    Ok(diagnostics)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_version_uses_first_non_empty_line_and_limits_length() {
        let input = format!("\n{}\nsecond", "x".repeat(200));
        let version = compact_version(input.as_bytes()).expect("version");
        assert_eq!(version.len(), 160);
    }

    #[test]
    fn missing_command_has_no_resolved_path() {
        assert!(resolve_command_path("loom-command-that-does-not-exist-42").is_none());
    }
}
