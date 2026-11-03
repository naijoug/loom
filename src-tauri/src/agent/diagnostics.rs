use super::{capabilities, registry, AgentError};
use crate::models::now_ms;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::{Duration, SystemTime},
};
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
    pub capabilities: capabilities::AgentCapabilities,
}

fn resolve_command_path(command: &str) -> Option<String> {
    registry::resolve_executable(command)
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
    let mut capabilities = capabilities::capabilities(&agent.adapter_type);
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
    // Starts but `--version` fails: typically a half-installed package.
    let broken = matches!(&version_result, Ok(Ok(output)) if output.status.code().is_some_and(|code| code != 0));
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

    capabilities.evidence_only = super::evidence::supported(&agent, version.as_deref())
        && super::verify_evidence_profile(&agent).await.is_ok();
    AgentDiagnostic {
        agent_id: agent.id,
        status: if !agent.enabled {
            "disabled"
        } else if cannot_start {
            "unavailable"
        } else if broken {
            "broken"
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

const STARTUP_PROBE_TIMEOUT: Duration = Duration::from_secs(15);
const STARTUP_DETAIL_LIMIT: usize = 4000;

/// Agents whose last startup check passed, keyed by agent id, with the resolved
/// executable and its modification time. Lives for the app session.
type StartableKey = (String, Option<SystemTime>);

fn startable_cache() -> &'static Mutex<HashMap<String, StartableKey>> {
    static CACHE: OnceLock<Mutex<HashMap<String, StartableKey>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

fn startable_key(path: &str) -> StartableKey {
    let modified = std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok();
    (path.to_string(), modified)
}

/// Drops cached startup checks for one agent, or for all agents when `agent_id`
/// is `None`. Called when agent settings change, a run fails or Settings re-checks.
pub fn forget_startable(agent_id: Option<&str>) {
    let mut cache = startable_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    match agent_id {
        Some(id) => {
            cache.remove(id);
        }
        None => cache.clear(),
    }
}

/// Checks that an agent CLI can start before a run, so a broken install fails
/// fast with a readable summary and the raw output as details. Built-in adapters
/// run `<command> --version`; custom CLIs only need to resolve, since `--version`
/// is not part of their contract. Passing checks are cached until the executable
/// changes or [`forget_startable`] is called; failures are never cached.
pub async fn check_startable(agent: &crate::models::AgentConfig) -> Result<(), AgentError> {
    if agent.adapter_type == "dummy" {
        return Ok(());
    }
    let path = resolve_command_path(&agent.command)
        .ok_or_else(|| AgentError::new(format!("{} was not found on this machine", agent.name)))?;
    if !capabilities::capabilities(&agent.adapter_type).structured_stream {
        return Ok(());
    }
    let key = startable_key(&path);
    if startable_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(&agent.id)
        == Some(&key)
    {
        return Ok(());
    }
    probe_version(agent, &path).await?;
    startable_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(agent.id.clone(), key);
    Ok(())
}

async fn probe_version(agent: &crate::models::AgentConfig, path: &str) -> Result<(), AgentError> {
    let mut command = TokioCommand::new(path);
    command.arg("--version").kill_on_drop(true);
    let output = match timeout(STARTUP_PROBE_TIMEOUT, command.output()).await {
        Err(_) => {
            return Err(AgentError::new(format!(
                "{} did not answer `--version` within {} seconds; check the installation and try again.",
                agent.name,
                STARTUP_PROBE_TIMEOUT.as_secs()
            )))
        }
        Ok(Err(error)) => {
            return Err(AgentError::new(format!(
                "{} could not start; repair or reinstall it, then try again.",
                agent.name
            ))
            .with_details(format!("{path}: {error}")))
        }
        Ok(Ok(output)) if output.status.success() => return Ok(()),
        Ok(Ok(output)) => output,
    };
    let status = output
        .status
        .code()
        .map(|code| format!("exited with {code}"))
        .unwrap_or_else(|| "was stopped by a signal".into());
    let raw = [output.stderr, output.stdout]
        .iter()
        .map(|bytes| String::from_utf8_lossy(bytes).trim().to_string())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    let mut detail = format!("{path} --version {status}");
    if !raw.is_empty() {
        detail.push('\n');
        detail.extend(raw.chars().take(STARTUP_DETAIL_LIMIT));
    }
    Err(AgentError::new(format!(
        "{} is installed but cannot run (`--version` {status}); repair or reinstall it, then try again.",
        agent.name
    ))
    .with_details(detail))
}

#[tauri::command]
pub async fn diagnose_agents(
    app: AppHandle,
    agent_id: Option<String>,
) -> Result<Vec<AgentDiagnostic>, String> {
    forget_startable(agent_id.as_deref());
    let mut diagnostics = Vec::new();
    for agent in registry::load_agents(&app)?
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

    fn script(name: &str, body: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(crate::models::IdGenerator::default().next(name));
        std::fs::write(&path, body).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        path
    }

    fn agent(id: &str, adapter: &str, path: &std::path::Path) -> crate::models::AgentConfig {
        serde_json::from_value(serde_json::json!({
            "id":id, "name":"Codex", "command":path, "args":[], "workingDirectoryPolicy":"project_root", "capabilities":[], "adapterType":adapter, "canWriteFiles":false, "canRunCommands":false, "enabled":true, "available":true
        })).unwrap()
    }

    #[tokio::test]
    async fn startup_check_summarises_broken_install_and_keeps_raw_output() {
        let broken = script(
            "loom-broken-version",
            "#!/bin/sh\necho 'Error: Missing optional dependency @openai/codex-darwin-arm64' >&2\nexit 1\n",
        );
        let id = crate::models::IdGenerator::default().next("broken");
        let error = check_startable(&agent(&id, "codex_cli", &broken))
            .await
            .unwrap_err()
            .to_string();
        let (summary, detail) = error.split_once("\n\n").unwrap();
        assert!(
            summary.starts_with("Codex is installed but cannot run (`--version` exited with 1)")
        );
        assert!(!summary.contains("Missing optional dependency"));
        assert!(detail.contains("Missing optional dependency @openai/codex-darwin-arm64"));
        let diagnostic = diagnose_agent(agent(&id, "codex_cli", &broken)).await;
        assert_eq!(diagnostic.status, "broken");
        let working = script("loom-good-version", "#!/bin/sh\necho 'codex 1.0'\n");
        check_startable(&agent(&id, "codex_cli", &working))
            .await
            .unwrap();
        assert_eq!(
            diagnose_agent(agent(&id, "codex_cli", &working))
                .await
                .status,
            "ready"
        );
        // Custom CLIs are not asked for `--version`; resolving is enough.
        check_startable(&agent(&id, "cli", &broken)).await.unwrap();
        std::fs::remove_file(broken).unwrap();
        std::fs::remove_file(working).unwrap();
        forget_startable(Some(&id));
    }

    #[tokio::test]
    async fn passing_checks_are_cached_until_the_executable_changes_or_is_forgotten() {
        let counter = std::env::temp_dir()
            .join(crate::models::IdGenerator::default().next("loom-version-count"));
        let path = script(
            "loom-counted-version",
            &format!(
                "#!/bin/sh\necho x >> '{}'\necho 'codex 1.0'\n",
                counter.display()
            ),
        );
        let probes = || {
            std::fs::read_to_string(&counter)
                .map(|text| text.lines().count())
                .unwrap_or(0)
        };
        let id = crate::models::IdGenerator::default().next("cached");
        let codex = agent(&id, "codex_cli", &path);
        check_startable(&codex).await.unwrap();
        check_startable(&codex).await.unwrap();
        assert_eq!(probes(), 1);
        let file = std::fs::File::options().write(true).open(&path).unwrap();
        file.set_modified(SystemTime::now() + Duration::from_secs(60))
            .unwrap();
        drop(file);
        check_startable(&codex).await.unwrap();
        assert_eq!(probes(), 2);
        forget_startable(Some(&id));
        check_startable(&codex).await.unwrap();
        assert_eq!(probes(), 3);
        forget_startable(Some(&id));
        std::fs::remove_file(path).unwrap();
        std::fs::remove_file(counter).unwrap();
    }
}
