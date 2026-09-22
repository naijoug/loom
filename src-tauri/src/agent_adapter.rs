use crate::models::AgentConfig;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const ADAPTER_CODEX: &str = "codex_cli";
pub const ADAPTER_CLAUDE: &str = "claude_code_cli";
pub const ADAPTER_GROK: &str = "grok_cli";

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
    /// Chat permissions are independent of the advanced Task stage protocol.
    pub chat_permission_mode: Option<&'a str>,
    pub embed_prompt: bool,
}

impl AdapterInvocationRequest<'_> {
    fn needs_write_access(&self) -> bool {
        self.chat_permission_mode.map_or_else(
            || self.stage.needs_write_access(),
            |mode| matches!(mode, "ask" | "auto"),
        )
    }
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
struct GrokAdapter;
struct CustomCliAdapter;

pub fn prepare_invocation(
    agent: &AgentConfig,
    request: &AdapterInvocationRequest<'_>,
) -> Result<PreparedAgentInvocation, String> {
    if let Some(mode) = request.chat_permission_mode {
        if !matches!(mode, "explore" | "ask" | "auto") {
            return Err("unsupported chat permission mode".into());
        }
        if !agent.enabled || !agent.available {
            return Err("chat agent is disabled or unavailable".into());
        }
        if request.needs_write_access() && (!agent.can_write_files || !agent.can_run_commands) {
            return Err("chat agent does not allow writable execution".into());
        }
    } else {
        validate_stage_permissions(agent, request.stage)?;
    }
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
        ADAPTER_GROK => Box::new(GrokAdapter),
        _ => Box::new(CustomCliAdapter),
    }
}

impl AgentAdapter for CodexAdapter {
    fn prepare(
        &self,
        agent: &AgentConfig,
        request: &AdapterInvocationRequest<'_>,
    ) -> Result<PreparedAgentInvocation, String> {
        let resume_id = request
            .resume_command
            .map(|value| parse_resume_session_id(&agent.adapter_type, &agent.command, value))
            .transpose()?;

        let sandbox = if request.needs_write_access() {
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
        // Exec-level options precede the resume subcommand. Always reassert
        // policy instead of inheriting a previous session's broader sandbox.
        args.extend(["--config".into(), "approval_policy=\"never\"".into()]);
        if let Some(id) = &resume_id {
            args.extend(["resume".into(), "--json".into(), "--".into(), id.clone()]);
        } else {
            args.push("--".into());
        }
        let stdin_prompt = !request.embed_prompt;
        args.push(if request.embed_prompt {
            request.prompt.to_string()
        } else {
            "-".into()
        });

        Ok(prepared(
            agent,
            args,
            stdin_prompt,
            "codex_json",
            resume_id.is_some(),
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
        let permission_mode = if request.needs_write_access() {
            Some("acceptEdits")
        } else if request.chat_permission_mode.is_some() {
            Some("plan")
        } else {
            None
        }; // Task planning captures printed prose, not ExitPlanMode output.
        let resume_id = request
            .resume_command
            .map(|value| parse_resume_session_id(&agent.adapter_type, &agent.command, value))
            .transpose()?;
        let mut args = vec![
            "-p".into(),
            "--verbose".into(),
            "--output-format".into(),
            "stream-json".into(),
            "--include-partial-messages".into(),
        ];
        if let Some(mode) = permission_mode {
            args.extend(["--permission-mode".into(), mode.into()]);
        }
        if let Some(id) = &resume_id {
            args.extend(["--resume".into(), id.clone()]);
        }
        if request.embed_prompt {
            args.extend(["--".into(), request.prompt.to_string()]);
        }
        let resumed = resume_id.is_some();

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

impl AgentAdapter for GrokAdapter {
    fn prepare(
        &self,
        agent: &AgentConfig,
        request: &AdapterInvocationRequest<'_>,
    ) -> Result<PreparedAgentInvocation, String> {
        let mut args: Vec<String> = Vec::new();
        let resume_id = request
            .resume_command
            .map(|value| parse_resume_session_id(&agent.adapter_type, &agent.command, value))
            .transpose()?;
        if let Some(id) = &resume_id {
            args.extend(["--resume".into(), id.clone()]);
        }
        let resumed = resume_id.is_some();

        args.extend([
            "--cwd".to_string(),
            request.project_path.display().to_string(),
            "--output-format".to_string(),
            "streaming-messages-json".to_string(),
            "--include-partial-messages".to_string(),
        ]);

        let permission_mode = if request.needs_write_access() {
            "acceptEdits"
        } else {
            "plan"
        };
        args.extend(["--permission-mode".to_string(), permission_mode.to_string()]);

        // Headless single-turn
        args.push("-p".to_string());
        if request.embed_prompt {
            if request.prompt.starts_with('-') {
                args.pop(); // Use an attached option value for option-looking user text.
                args.push(format!("--single={}", request.prompt));
            } else {
                args.push(request.prompt.to_string());
            }
        } else if let Some(prompt_file) = request.prompt_file {
            args.extend([
                "--prompt-file".to_string(),
                prompt_file.display().to_string(),
            ]);
            // remove the empty -p value path: use --prompt-file only
            args.retain(|arg| arg != "-p");
        } else {
            return Err("Grok chat requires an embedded prompt or prompt file".to_string());
        }

        Ok(prepared(
            agent,
            args,
            false,
            "streaming_json",
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

/// Resume strings are a legacy interchange format, never executable commands.
/// Match the configured program as one literal prefix (including spaces), then
/// accept exactly one known operation and one bounded identifier.
pub(crate) fn parse_resume_session_id(
    adapter: &str,
    program: &str,
    value: &str,
) -> Result<String, String> {
    let suffix = value
        .strip_prefix(program.trim())
        .and_then(|suffix| suffix.strip_prefix(' '))
        .ok_or("resume command does not match the configured executable")?;
    let fields: Vec<_> = suffix.split_whitespace().collect();
    let operation = match adapter {
        ADAPTER_CODEX => fields.first().is_some_and(|v| *v == "resume"),
        ADAPTER_CLAUDE => fields.first().is_some_and(|v| *v == "--resume"),
        ADAPTER_GROK => fields
            .first()
            .is_some_and(|v| matches!(*v, "--resume" | "-r")),
        _ => false,
    };
    if fields.len() != 2 || !operation || !valid_session_id(fields[1]) {
        return Err(
            "invalid resume handle: expected only the adapter resume operation and session id"
                .into(),
        );
    }
    Ok(fields[1].to_string())
}

pub(crate) fn valid_session_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 160
        && id.as_bytes()[0].is_ascii_alphanumeric()
        && id
            .bytes()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, b'-' | b'_'))
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
            } else if adapter_type == ADAPTER_GROK {
                "grok".to_string()
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
                chat_permission_mode: None,
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
                chat_permission_mode: None,
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
                chat_permission_mode: None,
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
                chat_permission_mode: None,
                embed_prompt: true,
            },
        )
        .expect_err("shell wrappers should be rejected");
        assert!(error.contains("dedicated executable"));
    }

    #[test]
    fn grok_adapter_uses_single_turn_and_streaming_json() {
        let root = Path::new("/repo");
        let agent = agent(ADAPTER_GROK);
        let prepared = prepare_invocation(
            &agent,
            &AdapterInvocationRequest {
                project_path: root,
                prompt: "hello",
                prompt_file: None,
                stage: AgentStage::Planning,
                resume_command: None,
                chat_permission_mode: None,
                embed_prompt: true,
            },
        )
        .expect("grok prepare");
        assert_eq!(prepared.program, "grok");
        assert!(prepared.args.iter().any(|arg| arg == "-p"));
        assert!(prepared
            .args
            .iter()
            .any(|arg| arg == "streaming-messages-json"));
        assert!(prepared.args.iter().any(|arg| arg == "plan"));
        assert_eq!(prepared.output_mode, "streaming_json");
        assert!(!prepared.stdin_prompt);
    }

    #[test]
    fn native_resume_reasserts_current_policy_and_preserves_stdin() {
        for adapter in [ADAPTER_CODEX, ADAPTER_CLAUDE, ADAPTER_GROK] {
            let mut agent = agent(adapter);
            agent.command = format!("/Applications/Agent Tools/{}", agent.command);
            let command = crate::session_capture::resume_command_for_adapter(
                adapter,
                &agent.command,
                "session-123",
            )
            .unwrap();
            for stage in [AgentStage::Planning, AgentStage::Debugging] {
                for embed_prompt in [false, true] {
                    if adapter == ADAPTER_GROK && !embed_prompt {
                        continue;
                    }
                    let request = AdapterInvocationRequest {
                        project_path: Path::new("/project with spaces"),
                        prompt: "--dangerously-skip-permissions",
                        prompt_file: None,
                        stage,
                        resume_command: Some(&command),
                        chat_permission_mode: Some(if stage.needs_write_access() {
                            "auto"
                        } else {
                            "explore"
                        }),
                        embed_prompt,
                    };
                    let invocation = prepare_invocation(&agent, &request).unwrap();
                    assert!(invocation.resumed);
                    assert_eq!(invocation.program, agent.command);
                    assert_eq!(invocation.cwd, "/project with spaces");
                    if adapter == ADAPTER_CODEX {
                        let expected = if stage.needs_write_access() {
                            "workspace-write"
                        } else {
                            "read-only"
                        };
                        assert!(invocation
                            .args
                            .windows(2)
                            .any(|p| p == ["--sandbox", expected]));
                        assert!(
                            invocation
                                .args
                                .iter()
                                .position(|a| a == "--sandbox")
                                .unwrap()
                                < invocation.args.iter().position(|a| a == "resume").unwrap()
                        );
                        assert!(invocation
                            .args
                            .windows(2)
                            .any(|p| p == ["--config", "approval_policy=\"never\""]));
                        assert_eq!(invocation.stdin_prompt, !embed_prompt);
                    } else {
                        let expected = if stage.needs_write_access() {
                            "acceptEdits"
                        } else {
                            "plan"
                        };
                        assert!(invocation
                            .args
                            .windows(2)
                            .any(|p| p == ["--permission-mode", expected]));
                    }
                    if embed_prompt {
                        if adapter == ADAPTER_GROK {
                            assert_eq!(
                                invocation.args.last().unwrap(),
                                "--single=--dangerously-skip-permissions"
                            );
                        } else {
                            let separator = invocation.args.iter().position(|a| a == "--").unwrap();
                            let prompt = invocation
                                .args
                                .iter()
                                .position(|a| a == request.prompt)
                                .unwrap();
                            assert!(separator < prompt, "user text became an option");
                        }
                    } else {
                        assert!(!invocation.args.iter().any(|a| a == request.prompt));
                    }
                }
            }
        }
    }

    #[test]
    fn resume_never_accepts_old_options_cross_programs_or_option_shaped_ids() {
        for adapter in [ADAPTER_CODEX, ADAPTER_CLAUDE, ADAPTER_GROK] {
            let agent = agent(adapter);
            let good = crate::session_capture::resume_command_for_adapter(
                adapter,
                &agent.command,
                "session-123",
            )
            .unwrap();
            for command in [
                format!("{good} --dangerously-skip-permissions"),
                good.replace("session-123", "--last"),
                good.replace("session-123", ""),
                good.replace("session-123", "../other"),
                format!("other-{good}"),
                good.replace("session-123", &"x".repeat(161)),
            ] {
                assert!(
                    prepare_invocation(
                        &agent,
                        &AdapterInvocationRequest {
                            project_path: Path::new("/repo"),
                            prompt: "new input",
                            prompt_file: None,
                            stage: AgentStage::Planning,
                            resume_command: Some(&command),
                            chat_permission_mode: None,
                            embed_prompt: true,
                        }
                    )
                    .is_err(),
                    "accepted {command}"
                );
            }
        }
    }
}
