use crate::{agents, models::now_ms};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tauri::AppHandle;
use tokio::{process::Command as TokioCommand, time::timeout};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDiagnostic {
    pub agent_id: String,
    pub status: String,
    pub resolved_path: Option<String>,
    pub version: Option<String>,
    pub auth_status: String,
    pub detail: String,
    pub checked_at_ms: u128,
    pub capabilities: crate::agent_adapter::AgentCapabilities,
}

fn resolve_command_path(command: &str) -> Option<String> {
    agents::config::resolve_executable(command)
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
    let capabilities = crate::agent_adapter::capabilities(&agent.adapter_type);
    if agent.adapter_type == "dummy" {
        return AgentDiagnostic {
            agent_id: agent.id,
            status: if agent.enabled { "ready" } else { "disabled" }.to_string(),
            resolved_path: None,
            version: Some("built-in test adapter".to_string()),
            auth_status: "not_applicable".to_string(),
            detail: "No external process is used.".to_string(),
            checked_at_ms,
            capabilities,
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
            capabilities,
        };
    }

    let mut command = TokioCommand::new(resolved_path.as_ref().expect("resolved executable"));
    command.arg("--version").kill_on_drop(true);
    let version_result = timeout(Duration::from_secs(2), command.output()).await;
    let cannot_start = matches!(&version_result, Ok(Err(_)))
        || matches!(&version_result, Ok(Ok(output)) if output.status.code().is_none());
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
        Err(_) => (None, "Version probe timed out after 2 seconds.".to_string()),
    };

    AgentDiagnostic {
        agent_id: agent.id,
        status: if !agent.enabled {
            "disabled"
        } else if cannot_start {
            "unavailable"
        } else {
            "ready"
        }
        .to_string(),
        resolved_path,
        version: Some(version.unwrap_or_else(|| "unknown".into())),
        auth_status: "unknown".to_string(),
        detail: format!("{detail} Authentication is verified by the next real Agent invocation."),
        checked_at_ms,
        capabilities,
    }
}

#[tauri::command]
pub async fn diagnose_agents(
    app: AppHandle,
    agent_id: Option<String>,
) -> Result<Vec<AgentDiagnostic>, String> {
    let mut diagnostics = Vec::new();
    for agent in agents::load_agents(&app)?
        .into_iter()
        .filter(|agent| agent_id.as_ref().is_none_or(|id| id == &agent.id))
    {
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

#[cfg(all(test, unix))]
mod timeout_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[tokio::test]
    async fn version_probe_is_bounded_and_unknown_is_explicit() {
        let path = std::env::temp_dir()
            .join(crate::models::IdGenerator::default().next("loom-slow-version"));
        std::fs::write(&path, "#!/bin/sh\nexec /bin/sleep 10\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        let agent: crate::models::AgentConfig = serde_json::from_value(serde_json::json!({
            "id":"slow", "name":"Slow", "command":path, "args":[], "workingDirectoryPolicy":"project_root", "capabilities":[], "adapterType":"cli", "canWriteFiles":false, "canRunCommands":false, "enabled":true, "available":true
        })).unwrap();
        let started = std::time::Instant::now();
        let result = diagnose_agent(agent).await;
        assert!(started.elapsed() < Duration::from_secs(4));
        assert_eq!(result.version.as_deref(), Some("unknown"));
        assert!(result.detail.contains("timed out"));
        std::fs::remove_file(path).unwrap();
    }
}
