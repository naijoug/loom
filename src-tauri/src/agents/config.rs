use super::*;

pub(super) fn load_agents_inner(app: &AppHandle) -> Result<Vec<AgentConfig>, String> {
    let path = agents_path(app)?;

    if !path.exists() {
        let agents = discover_default_agents();
        save_agents(app, &agents)?;
        return Ok(agents);
    }

    let mut agents = load_agent_store(&path)?;
    merge_missing_default_agents(&mut agents);
    for agent in &mut agents {
        agent.available = command_available(agent);
    }
    save_agents(app, &agents)?;

    Ok(agents)
}

pub(super) fn save_agents(app: &AppHandle, agents: &[AgentConfig]) -> Result<(), String> {
    save_agent_store(&agents_path(app)?, agents)
}

pub(super) fn load_agent_store(path: &Path) -> Result<Vec<AgentConfig>, String> {
    migrations::read_versioned_json(path, "agents", AGENT_STORE_SCHEMA_VERSION)
}

pub(super) fn save_agent_store(path: &Path, agents: &[AgentConfig]) -> Result<(), String> {
    migrations::write_versioned_json(path, AGENT_STORE_SCHEMA_VERSION, &agents)
}

pub(super) fn agents_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(storage::global_config_dir(app)?.join(AGENTS_FILE))
}

pub(super) fn discover_default_agents() -> Vec<AgentConfig> {
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
        ("agent-grok", "Grok", "grok", ADAPTER_GROK, true, true),
    ]
    .into_iter()
    .map(
        |(id, name, command, adapter_type, can_write_files, can_run_commands)| {
            let mut agent = AgentConfig {
                id: id.to_string(),
                name: name.to_string(),
                command: resolve_default_command(command),
                args: Vec::new(),
                working_directory_policy: "project_root".to_string(),
                capabilities: vec![
                    "planning".to_string(),
                    "implementation".to_string(),
                    "review".to_string(),
                    "debugging".to_string(),
                    "testing".to_string(),
                    "documentation".to_string(),
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

pub(super) fn merge_missing_default_agents(agents: &mut Vec<AgentConfig>) {
    // Drop retired built-ins (and any custom config on the same adapter) from
    // previously saved files so they stop showing up in the UI.
    agents.retain(|agent| {
        agent.id != RETIRED_AGENT_AMP_ID && agent.adapter_type != RETIRED_ADAPTER_AMP
    });

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

pub(super) fn is_default_agent_id(agent_id: &str) -> bool {
    matches!(
        agent_id,
        "agent-codex" | "agent-claude" | "agent-grok" | "agent-dummy"
    )
}


/// Prefer an absolute path when PATH is thin (common for GUI-launched Tauri apps).
/// Basename fallback keeps Settings editable when nothing is installed yet.
pub(super) fn resolve_default_command(basename: &str) -> String {
    if let Some(resolved) = which_command(basename) {
        return resolved;
    }
    for candidate in default_command_candidates(basename) {
        if Path::new(&candidate).is_file() {
            return candidate;
        }
    }
    basename.to_string()
}

fn which_command(basename: &str) -> Option<String> {
    #[cfg(windows)]
    {
        let output = Command::new("where").arg(basename).output().ok()?;
        if !output.status.success() {
            return None;
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let first = stdout.lines().next()?.trim();
        if first.is_empty() {
            None
        } else {
            Some(first.to_string())
        }
    }
    #[cfg(not(windows))]
    {
        let output = Command::new("sh")
            .arg("-c")
            .arg(format!("command -v {}", shell_escape(basename)))
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let first = stdout.lines().next()?.trim();
        if first.is_empty() {
            None
        } else {
            Some(first.to_string())
        }
    }
}

fn default_command_candidates(basename: &str) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(home) = std::env::var("HOME") {
        let home = PathBuf::from(home);
        match basename {
            "grok" => {
                out.push(home.join(".grok/bin/grok").display().to_string());
            }
            "codex" => {
                out.push(home.join(".npm-global/bin/codex").display().to_string());
                out.push(home.join(".local/bin/codex").display().to_string());
            }
            "claude" => {
                out.push(home.join(".local/bin/claude").display().to_string());
            }
            _ => {}
        }
    }
    // Mac dogfood absolute paths (ignored when missing).
    if basename == "grok" {
        out.push("/Users/guojian/.grok/bin/grok".to_string());
    }
    if basename == "codex" {
        out.push("/Users/guojian/.npm-global/bin/codex".to_string());
    }
    if basename == "claude" {
        out.push("/Users/guojian/.local/bin/claude".to_string());
    }
    out
}

pub(super) fn command_available(agent: &AgentConfig) -> bool {
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

pub(super) fn stderr_tail(stderr: &str) -> Vec<String> {
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

pub(super) fn redact_sensitive_text_inner(input: &str) -> String {
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

pub(super) fn redact_sensitive_line(line: &str) -> String {
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

pub(super) fn redact_bearer_tokens(input: &str) -> String {
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
pub(super) fn shell_escape(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
