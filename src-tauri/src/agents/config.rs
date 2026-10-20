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
    let agents = [
        ("agent-codex", "Codex", "codex", ADAPTER_CODEX, true, true),
        (
            "agent-claude",
            "Claude Code",
            "claude",
            ADAPTER_CLAUDE_CODE,
            true,
            true,
        ),
        (
            "agent-opencode",
            "OpenCode",
            "opencode",
            crate::agent_adapter::ADAPTER_OPENCODE,
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

    #[cfg(any(debug_assertions, test))]
    let agents = {
        let mut agents = agents;
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
    };

    agents
}

pub(super) fn merge_missing_default_agents(agents: &mut Vec<AgentConfig>) {
    // Drop retired built-ins (and any custom config on the same adapter) from
    // previously saved files so they stop showing up in the UI.
    agents.retain(|agent| {
        agent.id != RETIRED_AGENT_AMP_ID
            && agent.adapter_type != RETIRED_ADAPTER_AMP
            && (cfg!(any(debug_assertions, test)) || agent.adapter_type != ADAPTER_DUMMY)
    });

    for default_agent in discover_default_agents() {
        if let Some(existing) = agents.iter_mut().find(|agent| agent.id == default_agent.id) {
            let preserve_enabled = existing.adapter_type != ADAPTER_DUMMY;
            existing.name = default_agent.name.clone();
            if existing.command.trim().is_empty() {
                existing.command = default_agent.command.clone();
            }
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
        "agent-codex" | "agent-claude" | "agent-grok" | "agent-opencode" | "agent-dummy"
    )
}

/// Prefer an absolute path when PATH is thin (common for GUI-launched Tauri apps).
/// Basename fallback keeps Settings editable when nothing is installed yet.
pub(super) fn resolve_default_command(basename: &str) -> String {
    if let Some(resolved) = which_command(basename) {
        return resolved;
    }
    for candidate in default_command_candidates(basename) {
        if executable_file(Path::new(&candidate)) {
            return candidate;
        }
    }
    basename.to_string()
}

pub(crate) fn executable_file(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .is_ok_and(|meta| meta.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn which_command(basename: &str) -> Option<String> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|directory| {
        let candidate = directory.join(basename);
        if executable_file(&candidate) {
            return Some(candidate.to_string_lossy().into_owned());
        }
        #[cfg(windows)]
        {
            for extension in ["exe", "cmd", "bat"] {
                let candidate = candidate.with_extension(extension);
                if executable_file(&candidate) {
                    return Some(candidate.to_string_lossy().into_owned());
                }
            }
        }
        None
    })
}

fn default_command_candidates(basename: &str) -> Vec<String> {
    let mut directories = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        let root = PathBuf::from(home);
        for suffix in [
            ".local/bin",
            ".npm-global/bin",
            ".opencode/bin",
            ".grok/bin",
        ] {
            directories.push(root.join(suffix));
        }
    }
    directories.extend([
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
    ]);
    directories
        .into_iter()
        .map(|directory| directory.join(basename).to_string_lossy().into_owned())
        .collect()
}

pub(crate) fn resolve_executable(command: &str) -> Option<String> {
    let command = command.trim();
    if command.is_empty() {
        return None;
    }
    let path = Path::new(command);
    if path.is_absolute() || path.components().count() > 1 {
        return executable_file(path).then(|| path.to_string_lossy().into_owned());
    }
    which_command(command).or_else(|| {
        default_command_candidates(command)
            .into_iter()
            .find(|candidate| executable_file(Path::new(candidate)))
    })
}

pub(super) fn command_available(agent: &AgentConfig) -> bool {
    agent.adapter_type == ADAPTER_DUMMY || resolve_executable(&agent.command).is_some()
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_include_opencode_and_preserve_command_overrides() {
        let mut agents = discover_default_agents();
        assert!(agents.iter().any(|agent| agent.id == "agent-opencode"));
        let agent = agents
            .iter_mut()
            .find(|agent| agent.id == "agent-claude")
            .unwrap();
        agent.command = "/custom/claude".into();
        agent.enabled = false;
        merge_missing_default_agents(&mut agents);
        let agent = agents
            .iter()
            .find(|agent| agent.id == "agent-claude")
            .unwrap();
        assert_eq!(agent.command, "/custom/claude");
        assert!(!agent.enabled);
    }
    #[test]
    fn resolution_prefers_path_and_candidates_are_home_relative() {
        if let Some(expected) = which_command("sh") {
            assert_eq!(resolve_default_command("sh"), expected);
        }
        let home = std::env::var_os("HOME").map(PathBuf::from);
        for candidate in default_command_candidates("opencode") {
            let path = Path::new(&candidate);
            assert!(
                home.as_ref().is_some_and(|home| path.starts_with(home))
                    || path.starts_with("/opt/homebrew/bin")
                    || path.starts_with("/usr/local/bin")
            );
            assert_eq!(path.file_name().unwrap(), "opencode");
        }
    }
}
