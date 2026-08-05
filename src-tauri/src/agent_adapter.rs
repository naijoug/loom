use crate::models::AgentConfig;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const ADAPTER_CODEX: &str = "codex_cli";
pub const ADAPTER_CLAUDE: &str = "claude_code_cli";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStage {
    Planning,
    Implementation,
    Review,
    Debugging,
    Testing,
    Documentation,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareAgentInvocationInput {
    pub project_path: String,
    pub task_id: String,
    pub agent_id: String,
    pub stage: AgentStage,
    pub prompt: String,
    #[serde(default)]
    pub resume_command: Option<String>,
}

impl AgentStage {
    pub const fn capability(self) -> &'static str {
        match self {
            Self::Planning => "planning",
            Self::Implementation => "implementation",
            Self::Review => "review",
            Self::Debugging => "debugging",
            Self::Testing => "testing",
            Self::Documentation => "documentation",
        }
    }

    pub const fn needs_write_access(self) -> bool {
        matches!(self, Self::Implementation | Self::Debugging)
    }

    pub const fn needs_command_access(self) -> bool {
        matches!(self, Self::Implementation | Self::Debugging | Self::Testing)
    }
}

#[derive(Clone, Debug)]
pub struct AdapterInvocationRequest<'a> {
    pub project_path: &'a Path,
    pub prompt: &'a str,
    pub prompt_file: Option<&'a Path>,
    pub stage: AgentStage,
    pub resume_command: Option<&'a str>,
    pub embed_prompt: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedAgentInvocation {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub stdin_prompt: bool,
    pub output_mode: String,
    pub resumed: bool,
}

pub trait AgentAdapter {
    fn prepare(
        &self,
        agent: &AgentConfig,
        request: &AdapterInvocationRequest<'_>,
    ) -> Result<PreparedAgentInvocation, String>;
}

struct CodexAdapter;
struct ClaudeAdapter;
struct CustomCliAdapter;

pub fn prepare_invocation(
    agent: &AgentConfig,
    request: &AdapterInvocationRequest<'_>,
) -> Result<PreparedAgentInvocation, String> {
    validate_stage_permissions(agent, request.stage)?;
    adapter_for(agent).prepare(agent, request)
}

pub fn validate_stage_permissions(agent: &AgentConfig, stage: AgentStage) -> Result<(), String> {
    if !agent.enabled {
        return Err(format!("agent '{}' is disabled", agent.name));
    }
    if !agent.available {
        return Err(format!("agent '{}' command is unavailable", agent.name));
    }
    let required = stage.capability();
    let has_capability = agent
        .capabilities
        .iter()
        .any(|capability| capability == required)
        || stage == AgentStage::Debugging
            && agent
                .capabilities
                .iter()
                .any(|capability| capability == "implementation");
    if !has_capability {
        return Err(format!(
            "agent '{}' does not support the {required} stage",
            agent.name
        ));
    }
    if stage.needs_write_access() && !agent.can_write_files {
        return Err(format!(
            "agent '{}' is not allowed to write files for the {required} stage",
            agent.name
        ));
    }
    if stage.needs_command_access() && !agent.can_run_commands {
        return Err(format!(
            "agent '{}' is not allowed to run commands for the {required} stage",
            agent.name
        ));
    }
    Ok(())
}

fn adapter_for(agent: &AgentConfig) -> Box<dyn AgentAdapter> {
    match agent.adapter_type.as_str() {
        ADAPTER_CODEX => Box::new(CodexAdapter),
        ADAPTER_CLAUDE => Box::new(ClaudeAdapter),
        _ => Box::new(CustomCliAdapter),
    }
}

impl AgentAdapter for CodexAdapter {
    fn prepare(
        &self,
        agent: &AgentConfig,
        request: &AdapterInvocationRequest<'_>,
    ) -> Result<PreparedAgentInvocation, String> {
        if let Some(resume) = request.resume_command.and_then(parse_resume_command) {
            if resume.program == agent.command
                && resume.args.first().is_some_and(|arg| arg == "resume")
            {
                let session_id = resume
                    .args
                    .get(1)
                    .ok_or_else(|| "Codex resume command has no session id".to_string())?;
                return Ok(prepared(
                    agent,
                    vec![
                        "exec".to_string(),
                        "resume".to_string(),
                        "--json".to_string(),
                        session_id.clone(),
                        request.prompt.to_string(),
                    ],
                    false,
                    "codex_json",
                    true,
                    request.project_path,
                ));
            }
        }

        let sandbox = if request.stage.needs_write_access() {
            "workspace-write"
        } else {
            "read-only"
        };
        let mut args = vec![
            "exec".to_string(),
            "--json".to_string(),
            "--cd".to_string(),
            request.project_path.display().to_string(),
            "--sandbox".to_string(),
            sandbox.to_string(),
        ];
        let stdin_prompt = !request.embed_prompt;
        args.push(if request.embed_prompt {
            request.prompt.to_string()
        } else {
            "-".to_string()
        });
        Ok(prepared(
            agent,
            args,
            stdin_prompt,
            "codex_json",
            false,
            request.project_path,
        ))
    }
}

impl AgentAdapter for ClaudeAdapter {
    fn prepare(
        &self,
        agent: &AgentConfig,
        request: &AdapterInvocationRequest<'_>,
    ) -> Result<PreparedAgentInvocation, String> {
        let permission_mode = if request.stage.needs_write_access() {
            Some("acceptEdits")
        } else {
            None
        };
        let mut args = if let Some(resume) = request.resume_command.and_then(parse_resume_command) {
            if resume.program == agent.command
                && resume.args.first().is_some_and(|arg| arg == "--resume")
            {
                resume.args
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
        let resumed = !args.is_empty();
        if !args
            .iter()
            .any(|arg| matches!(arg.as_str(), "-p" | "--print"))
        {
            args.insert(0, "-p".to_string());
        }
        if let Some(mode) = permission_mode {
            if !args.iter().any(|arg| arg == "--permission-mode") {
                args.extend(["--permission-mode".to_string(), mode.to_string()]);
            }
        }
        if !args.iter().any(|arg| arg == "--verbose") {
            args.push("--verbose".to_string());
        }
        if !args.iter().any(|arg| arg == "--output-format") {
            args.extend(["--output-format".to_string(), "stream-json".to_string()]);
        }
        if !args.iter().any(|arg| arg == "--include-partial-messages") {
            args.push("--include-partial-messages".to_string());
        }
        if request.embed_prompt {
            args.push(request.prompt.to_string());
        }
        Ok(prepared(
            agent,
            args,
            !request.embed_prompt,
            "claude_stream_json",
            resumed,
            request.project_path,
        ))
    }
}

impl AgentAdapter for CustomCliAdapter {
    fn prepare(
        &self,
        agent: &AgentConfig,
        request: &AdapterInvocationRequest<'_>,
    ) -> Result<PreparedAgentInvocation, String> {
        let executable = Path::new(agent.command.trim())
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if matches!(
            executable.as_str(),
            "sh" | "bash" | "zsh" | "fish" | "cmd" | "cmd.exe" | "powershell" | "pwsh"
        ) {
            return Err(format!(
                "custom agent '{}' must reference a dedicated executable, not a command shell",
                agent.name
            ));
        }
        if request.resume_command.is_some() {
            return Err(format!(
                "custom agent '{}' does not declare a safe resume protocol",
                agent.name
            ));
        }
        let project_path = request.project_path.display().to_string();
        let prompt_file = request
            .prompt_file
            .map(|path| path.display().to_string())
            .unwrap_or_default();
        let had_prompt =
            request.embed_prompt && agent.args.iter().any(|arg| arg.contains("{prompt}"));
        let had_prompt_file = agent.args.iter().any(|arg| arg.contains("{promptFile}"));
        let mut args = agent
            .args
            .iter()
            .map(|arg| {
                arg.replace("{projectPath}", &project_path)
                    .replace("{promptFile}", &prompt_file)
                    .replace("{prompt}", request.prompt)
                    .replace("{stage}", request.stage.capability())
            })
            .collect::<Vec<_>>();
        let stdin_prompt = !request.embed_prompt && !had_prompt && !had_prompt_file;
        if request.embed_prompt && !had_prompt {
            args.push(request.prompt.to_string());
        }
        Ok(prepared(
            agent,
            args,
            stdin_prompt,
            "plain",
            false,
            request.project_path,
        ))
    }
}

fn prepared(
    agent: &AgentConfig,
    args: Vec<String>,
    stdin_prompt: bool,
    output_mode: &str,
    resumed: bool,
    project_path: &Path,
) -> PreparedAgentInvocation {
    PreparedAgentInvocation {
        program: agent.command.trim().to_string(),
        args,
        cwd: project_path.display().to_string(),
        stdin_prompt,
        output_mode: output_mode.to_string(),
        resumed,
    }
}

struct ParsedCommand {
    program: String,
    args: Vec<String>,
}

fn parse_resume_command(value: &str) -> Option<ParsedCommand> {
    let mut parts = value.split_whitespace();
    let program = parts.next()?.to_string();
    Some(ParsedCommand {
        program,
        args: parts.map(str::to_string).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(adapter_type: &str) -> AgentConfig {
        AgentConfig {
            id: "agent-test".to_string(),
            name: "Test".to_string(),
            command: if adapter_type == ADAPTER_CLAUDE {
                "claude".to_string()
            } else {
                "codex".to_string()
            },
            args: Vec::new(),
            working_directory_policy: "project_root".to_string(),
            capabilities: vec![
                "planning".to_string(),
                "implementation".to_string(),
                "review".to_string(),
                "debugging".to_string(),
            ],
            adapter_type: adapter_type.to_string(),
            can_write_files: true,
            can_run_commands: true,
            enabled: true,
            available: true,
        }
    }

    #[test]
    fn codex_adapter_maps_stage_to_sandbox() {
        let root = Path::new("/repo");
        let agent = agent(ADAPTER_CODEX);
        let read_only = prepare_invocation(
            &agent,
            &AdapterInvocationRequest {
                project_path: root,
                prompt: "review",
                prompt_file: None,
                stage: AgentStage::Review,
                resume_command: None,
                embed_prompt: true,
            },
        )
        .expect("review invocation");
        assert!(read_only
            .args
            .windows(2)
            .any(|pair| pair == ["--sandbox", "read-only"]));

        let writable = prepare_invocation(
            &agent,
            &AdapterInvocationRequest {
                project_path: root,
                prompt: "implement",
                prompt_file: None,
                stage: AgentStage::Implementation,
                resume_command: None,
                embed_prompt: true,
            },
        )
        .expect("implementation invocation");
        assert!(writable
            .args
            .windows(2)
            .any(|pair| pair == ["--sandbox", "workspace-write"]));
    }

    #[test]
    fn stage_permissions_reject_missing_write_or_command_access() {
        let mut agent = agent(ADAPTER_CODEX);
        agent.can_write_files = false;
        assert!(
            validate_stage_permissions(&agent, AgentStage::Implementation)
                .unwrap_err()
                .contains("write files")
        );
        agent.can_write_files = true;
        agent.can_run_commands = false;
        assert!(validate_stage_permissions(&agent, AgentStage::Debugging)
            .unwrap_err()
            .contains("run commands"));
    }

    #[test]
    fn custom_adapter_expands_shared_placeholders() {
        let mut agent = agent("cli");
        agent.command = "my-agent".to_string();
        agent.args = vec![
            "--root={projectPath}".to_string(),
            "--stage={stage}".to_string(),
            "{prompt}".to_string(),
        ];
        let prepared = prepare_invocation(
            &agent,
            &AdapterInvocationRequest {
                project_path: Path::new("/repo"),
                prompt: "fix it",
                prompt_file: None,
                stage: AgentStage::Debugging,
                resume_command: None,
                embed_prompt: true,
            },
        )
        .expect("custom invocation");
        assert_eq!(
            prepared.args,
            vec!["--root=/repo", "--stage=debugging", "fix it"]
        );
    }

    #[test]
    fn custom_adapter_rejects_shell_wrappers() {
        let mut agent = agent("cli");
        agent.command = "sh".to_string();
        let error = prepare_invocation(
            &agent,
            &AdapterInvocationRequest {
                project_path: Path::new("/repo"),
                prompt: "review",
                prompt_file: None,
                stage: AgentStage::Review,
                resume_command: None,
                embed_prompt: true,
            },
        )
        .expect_err("shell wrappers should be rejected");
        assert!(error.contains("dedicated executable"));
    }
}
