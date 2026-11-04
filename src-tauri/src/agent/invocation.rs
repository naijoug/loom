use super::capabilities::{
    capabilities, ADAPTER_API, ADAPTER_CLAUDE, ADAPTER_CODEX, ADAPTER_GROK, ADAPTER_OPENCODE,
};
use super::injection;
use super::providers::{self, ResolvedProvider};
use crate::models::AgentConfig;
use serde::{Deserialize, Serialize};
use std::path::Path;

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
    pub execution_profile: Option<&'a super::EvidenceProfile>,
    pub project_path: &'a Path,
    pub prompt: &'a str,
    pub prompt_file: Option<&'a Path>,
    pub stage: AgentStage,
    pub resume_command: Option<&'a str>,
    /// Chat permissions are independent of the advanced Task stage protocol.
    pub chat_permission_mode: Option<&'a str>,
    pub embed_prompt: bool,
    /// Extra system prompt text; adapters without `inline_cards` ignore it.
    pub system_append: Option<&'a str>,
    /// Model name passed to the CLI; `None` keeps the CLI default. Rejected
    /// for adapters without `model_selection` instead of being ignored.
    pub model: Option<&'a str>,
    /// The feature relies on tools (reading the project, editing, running
    /// commands), so an `api` agent is refused instead of answering blind.
    pub needs_tools: bool,
}

impl AdapterInvocationRequest<'_> {
    pub(super) fn needs_write_access(&self) -> bool {
        self.chat_permission_mode.map_or_else(
            || self.stage.needs_write_access(),
            |mode| matches!(mode, "ask" | "auto"),
        )
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedAgentInvocation {
    #[serde(default)]
    pub environment: std::collections::BTreeMap<String, String>,
    /// Inherited variables with these prefixes are removed before
    /// `environment` applies, so a shell's own key or endpoint cannot mix
    /// into a provider run.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub environment_remove_prefixes: Vec<String>,
    /// Credential variables: never serialized and redacted from `Debug`.
    #[serde(skip)]
    pub secret_environment: SecretEnvironment,
    pub program: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub stdin_prompt: bool,
    pub output_mode: String,
    pub resumed: bool,
    /// Set for an `api` agent: the runner sends this request instead of
    /// starting `program`.
    #[serde(skip)]
    pub api: Option<super::api::ApiCall>,
}

#[derive(Clone, Default)]
pub struct SecretEnvironment(std::collections::BTreeMap<String, String>);

impl SecretEnvironment {
    pub(super) fn insert(&mut self, name: &str, value: &str) {
        self.0.insert(name.into(), value.into());
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.0.iter()
    }
}

impl std::fmt::Debug for SecretEnvironment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_map()
            .entries(self.0.keys().map(|name| (name, "[REDACTED]")))
            .finish()
    }
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
struct OpencodeAdapter;

/// OpenCode's inline configuration; provider injection adds to it.
pub(super) const OPENCODE_CONFIG: &str = "OPENCODE_CONFIG_CONTENT";

pub fn prepare_invocation(
    agent: &AgentConfig,
    request: &AdapterInvocationRequest<'_>,
) -> Result<PreparedAgentInvocation, String> {
    let provider = agent
        .provider_id
        .as_deref()
        .map(|id| {
            let dir = crate::storage::system_config_dir()?;
            providers::resolve_at(&dir, id, &providers::process_environment)
        })
        .transpose()?;
    prepare_with_provider(agent, request, provider.as_ref())
}

/// Interactive permission mode → stage: `explore` reads, `ask`/`auto` write.
pub fn permission_to_stage(mode: &str) -> AgentStage {
    match mode {
        "ask" | "auto" => AgentStage::Debugging,
        _ => AgentStage::Planning,
    }
}

/// Codex and Claude Code read the prompt from stdin; other adapters take it
/// in argv.
pub fn embeds_prompt(adapter: &str) -> bool {
    !matches!(adapter, ADAPTER_CODEX | ADAPTER_CLAUDE)
}

/// The resume handle an adapter accepts for a native session id.
pub(crate) fn resume_command(agent: &AgentConfig, session_id: &str) -> Result<String, String> {
    if !valid_session_id(session_id) {
        return Err("session id must be letters, digits, '-' or '_'".into());
    }
    crate::session_capture::resume_command_for_adapter(
        &agent.adapter_type,
        &agent.command,
        session_id,
    )
    .ok_or_else(|| format!("{} cannot resume a session", agent.name))
}

/// Like [`prepare_invocation`] with a provider the caller already resolved,
/// for a headless entry point that reads its own configuration directory.
pub(crate) fn prepare_invocation_with(
    agent: &AgentConfig,
    request: &AdapterInvocationRequest<'_>,
    provider: Option<&ResolvedProvider>,
) -> Result<PreparedAgentInvocation, String> {
    prepare_with_provider(agent, request, provider)
}

/// Mode B keeps the adapter's own arguments and permissions and only adds the
/// provider: its model becomes the default `--model`.
fn prepare_with_provider(
    agent: &AgentConfig,
    request: &AdapterInvocationRequest<'_>,
    provider: Option<&ResolvedProvider>,
) -> Result<PreparedAgentInvocation, String> {
    if agent.adapter_type == ADAPTER_API {
        return super::api::prepare(agent, request, provider);
    }
    let Some(provider) = provider else {
        return prepare_for_cli(agent, request);
    };
    if !injection::provider_kinds(&agent.adapter_type).contains(&provider.kind) {
        return Err(format!(
            "agent '{}' cannot use a {} model provider",
            agent.name,
            provider.kind.label()
        ));
    }
    let model = request
        .model
        .or(provider.model.as_deref())
        .ok_or("the model provider has no model; set one on the provider or the run")?;
    validate_model_name(model)?;
    let model_arg = injection::model_arg(&agent.adapter_type, model);
    let request = AdapterInvocationRequest {
        model: Some(&model_arg),
        ..request.clone()
    };
    let mut prepared = prepare_for_cli(agent, &request)?;
    injection::apply(&agent.adapter_type, &mut prepared, provider, model)?;
    Ok(prepared)
}

fn prepare_for_cli(
    agent: &AgentConfig,
    request: &AdapterInvocationRequest<'_>,
) -> Result<PreparedAgentInvocation, String> {
    if let Some(profile) = request.execution_profile {
        return profile.prepare(agent, request);
    }
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
    if let Some(model) = request.model {
        validate_model_name(model)?;
        if !capabilities(&agent.adapter_type).model_selection {
            return Err(format!(
                "agent '{}' does not support model selection",
                agent.name
            ));
        }
    }
    adapter_for(agent).prepare(agent, request)
}

/// Real model names include `[]@#/:` (for example `auto/glm-5.3[1m]` or
/// `provider/model#variant`), so only shape and option injection are checked.
pub fn validate_model_name(model: &str) -> Result<(), String> {
    if model.is_empty()
        || model.chars().count() > 128
        || model.starts_with('-')
        || model.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return Err("invalid model name".into());
    }
    Ok(())
}

fn push_model(args: &mut Vec<String>, request: &AdapterInvocationRequest<'_>) {
    if let Some(model) = request.model {
        args.extend(["--model".into(), model.into()]);
    }
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
        ADAPTER_OPENCODE => Box::new(OpencodeAdapter),
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
            "--skip-git-repo-check".to_string(),
            "--cd".to_string(),
            request.project_path.display().to_string(),
            "--sandbox".to_string(),
            sandbox.to_string(),
        ];
        // Exec-level options precede the resume subcommand. Always reassert
        // policy instead of inheriting a previous session's broader sandbox.
        args.extend(["--config".into(), "approval_policy=\"never\"".into()]);
        push_model(&mut args, request);
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
        let permission_mode = if request.chat_permission_mode == Some("auto") {
            Some("auto")
        } else if request.needs_write_access() {
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
        push_model(&mut args, request);
        // Claude Code does not persist an appended prompt with the session,
        // so it is passed on every turn, including resumed ones.
        if let Some(text) = request.system_append {
            args.extend(["--append-system-prompt".into(), text.into()]);
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

        let permission_mode = if request.chat_permission_mode == Some("auto") {
            "auto"
        } else if request.needs_write_access() {
            "acceptEdits"
        } else {
            "plan"
        };
        args.extend(["--permission-mode".to_string(), permission_mode.to_string()]);
        push_model(&mut args, request);

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

impl AgentAdapter for OpencodeAdapter {
    fn prepare(
        &self,
        agent: &AgentConfig,
        request: &AdapterInvocationRequest<'_>,
    ) -> Result<PreparedAgentInvocation, String> {
        let resume = request
            .resume_command
            .map(|value| parse_resume_session_id(ADAPTER_OPENCODE, &agent.command, value))
            .transpose()?;
        // OpenCode 2.x reads per-run configuration only in a private server;
        // the shared background service ignores this process's environment.
        let mut args: Vec<String> = vec![
            "run".into(),
            "--standalone".into(),
            "--format".into(),
            "json".into(),
            "--agent".into(),
            if request.needs_write_access() {
                "build"
            } else {
                "plan"
            }
            .into(),
        ];
        if let Some(id) = &resume {
            args.extend(["--session".into(), id.clone()]);
        }
        push_model(&mut args, request);
        if request.embed_prompt {
            args.extend(["--".into(), request.prompt.into()]);
        }
        let mut invocation = prepared(
            agent,
            args,
            !request.embed_prompt,
            "opencode_json",
            resume.is_some(),
            request.project_path,
        );
        let permissions = if !request.needs_write_access() {
            serde_json::json!({"*":"deny", "read":"allow", "glob":"allow", "grep":"allow", "webfetch":"allow", "websearch":"allow"})
        } else if request.chat_permission_mode == Some("auto") {
            serde_json::json!({"*":"allow", "external_directory":"deny"})
        } else {
            serde_json::json!({"*":"ask", "read":"allow", "glob":"allow", "grep":"allow", "edit":"allow", "external_directory":"deny"})
        };
        // OpenCode takes its project from `PWD`, which the process would
        // otherwise inherit from Loom.
        invocation
            .environment
            .insert("PWD".into(), request.project_path.display().to_string());
        // 2.x ignores `OPENCODE_PERMISSION`; inline configuration is honored.
        invocation.environment.insert(
            OPENCODE_CONFIG.into(),
            serde_json::json!({ "permission": permissions }).to_string(),
        );
        Ok(invocation)
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
        environment: Default::default(),
        environment_remove_prefixes: Vec::new(),
        secret_environment: Default::default(),
        program: agent.command.trim().to_string(),
        args,
        cwd: project_path.display().to_string(),
        stdin_prompt,
        output_mode: output_mode.to_string(),
        resumed,
        api: None,
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
        ADAPTER_OPENCODE => fields.first().is_some_and(|v| *v == "--session"),
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
            provider_id: None,
        }
    }

    #[test]
    fn codex_adapter_maps_stage_to_sandbox() {
        let root = Path::new("/repo");
        let agent = agent(ADAPTER_CODEX);
        let read_only = prepare_invocation(
            &agent,
            &AdapterInvocationRequest {
                execution_profile: None,
                project_path: root,
                prompt: "review",
                prompt_file: None,
                stage: AgentStage::Review,
                resume_command: None,
                chat_permission_mode: None,
                embed_prompt: true,
                system_append: None,
                model: None,
                needs_tools: true,
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
                execution_profile: None,
                project_path: root,
                prompt: "implement",
                prompt_file: None,
                stage: AgentStage::Implementation,
                resume_command: None,
                chat_permission_mode: None,
                embed_prompt: true,
                system_append: None,
                model: None,
                needs_tools: true,
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
                execution_profile: None,
                project_path: Path::new("/repo"),
                prompt: "fix it",
                prompt_file: None,
                stage: AgentStage::Debugging,
                resume_command: None,
                chat_permission_mode: None,
                embed_prompt: true,
                system_append: None,
                model: None,
                needs_tools: true,
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
                execution_profile: None,
                project_path: Path::new("/repo"),
                prompt: "review",
                prompt_file: None,
                stage: AgentStage::Review,
                resume_command: None,
                chat_permission_mode: None,
                embed_prompt: true,
                system_append: None,
                model: None,
                needs_tools: true,
            },
        )
        .expect_err("shell wrappers should be rejected");
        assert!(error.contains("dedicated executable"));
    }

    #[test]
    fn built_in_adapters_pass_selected_models_and_reject_invalid_ones() {
        let root = Path::new("/repo");
        for (adapter, model) in [
            (ADAPTER_CODEX, "gpt-6-luna"),
            (ADAPTER_CLAUDE, "auto/glm-5.3[1m]"),
            (ADAPTER_GROK, "grok-4.7@500k"),
            (ADAPTER_OPENCODE, "provider/model#variant"),
        ] {
            assert!(capabilities(adapter).model_selection);
            let request = AdapterInvocationRequest {
                execution_profile: None,
                project_path: root,
                prompt: "study",
                prompt_file: None,
                stage: AgentStage::Planning,
                resume_command: None,
                chat_permission_mode: Some("explore"),
                embed_prompt: adapter == ADAPTER_GROK || adapter == ADAPTER_OPENCODE,
                system_append: None,
                model: Some(model),
                needs_tools: true,
            };
            let prepared = prepare_invocation(&agent(adapter), &request).unwrap();
            let at = prepared
                .args
                .iter()
                .position(|arg| arg == "--model")
                .unwrap_or_else(|| panic!("{adapter} has no --model"));
            assert_eq!(prepared.args[at + 1], model);
            if let Some(separator) = prepared.args.iter().position(|arg| arg == "--") {
                assert!(
                    at < separator,
                    "{adapter} model follows the prompt separator"
                );
            }
            let without = prepare_invocation(
                &agent(adapter),
                &AdapterInvocationRequest {
                    execution_profile: None,
                    model: None,
                    ..request.clone()
                },
            )
            .unwrap();
            assert!(!without.args.iter().any(|arg| arg == "--model"));
            for bad in ["", "-x", "a b", "a\nb", &"m".repeat(129)] {
                let error = prepare_invocation(
                    &agent(adapter),
                    &AdapterInvocationRequest {
                        execution_profile: None,
                        model: Some(bad),
                        ..request.clone()
                    },
                )
                .unwrap_err();
                assert_eq!(error, "invalid model name", "{adapter} accepted {bad:?}");
            }
        }
        assert!(!capabilities("cli").model_selection);
        let error = prepare_invocation(
            &agent("cli"),
            &AdapterInvocationRequest {
                execution_profile: None,
                project_path: root,
                prompt: "study",
                prompt_file: None,
                stage: AgentStage::Planning,
                resume_command: None,
                chat_permission_mode: Some("explore"),
                embed_prompt: true,
                system_append: None,
                model: Some("any"),
                needs_tools: true,
            },
        )
        .unwrap_err();
        assert!(error.contains("does not support model selection"));
    }

    #[test]
    fn grok_adapter_uses_single_turn_and_streaming_json() {
        let root = Path::new("/repo");
        let agent = agent(ADAPTER_GROK);
        let prepared = prepare_invocation(
            &agent,
            &AdapterInvocationRequest {
                execution_profile: None,
                project_path: root,
                prompt: "hello",
                prompt_file: None,
                stage: AgentStage::Planning,
                resume_command: None,
                chat_permission_mode: None,
                embed_prompt: true,
                system_append: None,
                model: None,
                needs_tools: true,
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
    fn only_claude_appends_a_system_prompt_on_every_turn() {
        for adapter in [
            ADAPTER_CODEX,
            ADAPTER_CLAUDE,
            ADAPTER_GROK,
            ADAPTER_OPENCODE,
            "cli",
        ] {
            assert_eq!(
                capabilities(adapter).inline_cards,
                adapter == ADAPTER_CLAUDE
            );
        }
        let agent = agent(ADAPTER_CLAUDE);
        let command = crate::session_capture::resume_command_for_adapter(
            ADAPTER_CLAUDE,
            &agent.command,
            "session-123",
        )
        .unwrap();
        let request = AdapterInvocationRequest {
            execution_profile: None,
            project_path: Path::new("/repo"),
            prompt: "hi",
            prompt_file: None,
            stage: AgentStage::Planning,
            resume_command: Some(&command),
            chat_permission_mode: Some("explore"),
            embed_prompt: false,
            system_append: Some("CARDS"),
            model: None,
            needs_tools: true,
        };
        let args = prepare_invocation(&agent, &request).unwrap().args;
        let at = args
            .iter()
            .position(|arg| arg == "--append-system-prompt")
            .unwrap();
        assert_eq!(args[at + 1], "CARDS");
        assert!(at < args.iter().position(|arg| arg == "--resume").unwrap());
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
                        execution_profile: None,
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
                        system_append: None,
                        model: None,
                        needs_tools: true,
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
                            "auto"
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
                            execution_profile: None,
                            project_path: Path::new("/repo"),
                            prompt: "new input",
                            prompt_file: None,
                            stage: AgentStage::Planning,
                            resume_command: Some(&command),
                            chat_permission_mode: None,
                            embed_prompt: true,
                            system_append: None,
                            model: None,
                            needs_tools: true,
                        }
                    )
                    .is_err(),
                    "accepted {command}"
                );
            }
        }
    }
}

#[cfg(test)]
mod opencode_tests {
    use super::*;
    #[test]
    fn opencode_maps_modes_without_shell_or_prompt_option_injection() {
        let agent: AgentConfig = serde_json::from_value(serde_json::json!({
            "id":"opencode","name":"OpenCode","command":"/tools/opencode","args":[],"workingDirectoryPolicy":"project_root","capabilities":[],"adapterType":ADAPTER_OPENCODE,"canWriteFiles":true,"canRunCommands":true,"enabled":true,"available":true
        })).unwrap();
        for mode in ["explore", "ask", "auto"] {
            let request = AdapterInvocationRequest {
                execution_profile: None,
                project_path: Path::new("/project"),
                prompt: "--prompt-looking-text",
                prompt_file: None,
                stage: AgentStage::Planning,
                resume_command: Some("/tools/opencode --session ses_123"),
                chat_permission_mode: Some(mode),
                embed_prompt: true,
                system_append: None,
                model: None,
                needs_tools: true,
            };
            let invocation = prepare_invocation(&agent, &request).unwrap();
            assert_eq!(invocation.output_mode, "opencode_json");
            assert!(invocation.resumed);
            assert!(invocation
                .args
                .windows(2)
                .any(|pair| pair == ["--session", "ses_123"]));
            assert_eq!(
                &invocation.args[invocation.args.len() - 2..],
                &["--", "--prompt-looking-text"]
            );
            assert_eq!(&invocation.args[..2], ["run", "--standalone"]);
            assert!(!invocation.args.iter().any(|arg| arg == "--dir"));
            assert_eq!(invocation.cwd, "/project");
            assert_eq!(invocation.environment["PWD"], "/project");
            let config: serde_json::Value =
                serde_json::from_str(&invocation.environment[OPENCODE_CONFIG]).unwrap();
            let policy = &config["permission"];
            if mode == "explore" {
                assert_eq!(policy["*"], "deny");
                assert_eq!(policy["read"], "allow");
            } else {
                assert_eq!(policy["external_directory"], "deny");
            }
        }
        assert!(capabilities(ADAPTER_OPENCODE).native_resume);
        assert!(parse_resume_session_id(
            ADAPTER_OPENCODE,
            "/tools/opencode",
            "/tools/opencode --session ses_123 --auto"
        )
        .is_err());
    }
}

#[cfg(test)]
mod provider_tests {
    use super::*;
    use crate::agent::providers::ProviderKind;
    use crate::agent::{run, Limits, RunSpec};
    use crate::process_supervisor::{ProcessKind, ProcessOwner};
    use std::time::Duration;

    const KEY: &str = "sk-fixture-provider-0123456789";

    fn agent(adapter: &str) -> AgentConfig {
        serde_json::from_value(serde_json::json!({
            "id":"agent","name":"Agent","command":"/tools/cli","args":[],"workingDirectoryPolicy":"project_root","capabilities":["planning"],"adapterType":adapter,"canWriteFiles":true,"canRunCommands":true,"enabled":true,"available":true,"providerId":"fixture"
        }))
        .unwrap()
    }

    fn provider(kind: ProviderKind, key: Option<&str>) -> ResolvedProvider {
        ResolvedProvider {
            kind,
            base_url: "https://provider.example/v1".into(),
            model: Some("provider-model".into()),
            api_key: key.map(str::to_owned),
        }
    }

    fn request(model: Option<&str>) -> AdapterInvocationRequest<'_> {
        AdapterInvocationRequest {
            execution_profile: None,
            project_path: Path::new("/project"),
            prompt: "hello",
            prompt_file: None,
            stage: AgentStage::Planning,
            resume_command: None,
            chat_permission_mode: Some("explore"),
            embed_prompt: false,
            system_append: None,
            model,
            needs_tools: true,
        }
    }

    fn prepare(adapter: &str, provider: &ResolvedProvider) -> PreparedAgentInvocation {
        prepare_with_provider(&agent(adapter), &request(None), Some(provider)).unwrap()
    }

    fn assert_key_hidden(prepared: &PreparedAgentInvocation) {
        assert!(!prepared.args.iter().any(|arg| arg.contains(KEY)));
        assert!(!prepared
            .environment
            .values()
            .any(|value| value.contains(KEY)));
        assert!(!format!("{prepared:?}").contains(KEY));
        assert!(!serde_json::to_string(prepared).unwrap().contains(KEY));
    }

    fn secret(prepared: &PreparedAgentInvocation, name: &str) -> Option<String> {
        prepared
            .secret_environment
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value.clone())
    }

    #[test]
    fn claude_gets_endpoint_model_and_key_without_settings_credentials() {
        let prepared = prepare(
            ADAPTER_CLAUDE,
            &provider(ProviderKind::Anthropic, Some(KEY)),
        );
        assert!(prepared
            .args
            .windows(2)
            .any(|pair| pair == ["--model", "provider-model"]));
        let env = &prepared.environment;
        assert_eq!(env["ANTHROPIC_BASE_URL"], "https://provider.example/v1");
        assert_eq!(env["ANTHROPIC_SMALL_FAST_MODEL"], "provider-model");
        assert_eq!(env["ANTHROPIC_AUTH_TOKEN"], "");
        assert_eq!(env["CLAUDE_CODE_USE_BEDROCK"], "");
        assert_eq!(
            prepared.environment_remove_prefixes,
            ["ANTHROPIC_", "CLAUDE_CODE_USE_"]
        );
        assert_eq!(secret(&prepared, "ANTHROPIC_API_KEY").as_deref(), Some(KEY));
        assert_key_hidden(&prepared);

        // A keyless local provider must not fall back to the CLI's own login.
        let keyless = prepare(ADAPTER_CLAUDE, &provider(ProviderKind::Anthropic, None));
        assert_eq!(
            secret(&keyless, "ANTHROPIC_API_KEY").as_deref(),
            Some("loom-no-key")
        );
    }

    #[test]
    fn codex_defines_a_responses_provider_before_the_prompt() {
        let prepared = prepare(
            ADAPTER_CODEX,
            &provider(ProviderKind::OpenaiResponses, Some(KEY)),
        );
        assert_eq!(prepared.args[0], "exec");
        let configs: Vec<_> = prepared
            .args
            .windows(2)
            .filter(|pair| pair[0] == "--config")
            .map(|pair| pair[1].as_str())
            .collect();
        assert_eq!(
            &configs[..5],
            [
                "model_provider=\"loom\"",
                "model_providers.loom.name=\"Loom\"",
                "model_providers.loom.base_url=\"https://provider.example/v1\"",
                "model_providers.loom.wire_api=\"responses\"",
                "model_providers.loom.env_key=\"LOOM_PROVIDER_API_KEY\"",
            ]
        );
        let position = |arg: &str| prepared.args.iter().position(|item| item == arg).unwrap();
        assert!(position("model_provider=\"loom\"") < position("--"));
        assert!(prepared
            .args
            .windows(2)
            .any(|pair| pair == ["--model", "provider-model"]));
        assert_eq!(
            secret(&prepared, "LOOM_PROVIDER_API_KEY").as_deref(),
            Some(KEY)
        );
        assert_key_hidden(&prepared);

        let keyless = prepare(
            ADAPTER_CODEX,
            &provider(ProviderKind::OpenaiResponses, None),
        );
        assert!(!keyless.args.iter().any(|arg| arg.contains("env_key")));
        assert!(secret(&keyless, "LOOM_PROVIDER_API_KEY").is_none());
    }

    #[test]
    fn opencode_runs_standalone_with_an_inline_provider() {
        for (kind, package, base_url) in [
            (
                ProviderKind::Anthropic,
                "@ai-sdk/anthropic",
                "https://provider.example/v1/v1",
            ),
            (
                ProviderKind::OpenaiResponses,
                "@ai-sdk/openai",
                "https://provider.example/v1",
            ),
            (
                ProviderKind::OpenaiChat,
                "@ai-sdk/openai-compatible",
                "https://provider.example/v1",
            ),
        ] {
            let prepared = prepare(ADAPTER_OPENCODE, &provider(kind, Some(KEY)));
            assert_eq!(&prepared.args[..2], ["run", "--standalone"]);
            assert!(prepared
                .args
                .windows(2)
                .any(|pair| pair == ["--model", "loom/provider-model"]));
            let config: serde_json::Value =
                serde_json::from_str(&prepared.environment["OPENCODE_CONFIG_CONTENT"]).unwrap();
            let loom = &config["provider"]["loom"];
            assert_eq!(loom["npm"], package);
            assert_eq!(loom["options"]["baseURL"], base_url);
            assert_eq!(loom["options"]["apiKey"], "{env:LOOM_PROVIDER_API_KEY}");
            assert!(loom["models"]["provider-model"].is_object());
            assert_eq!(config["permission"]["*"], "deny");
            assert_key_hidden(&prepared);
        }
    }

    #[test]
    fn run_model_overrides_the_provider_model() {
        let prepared = prepare_with_provider(
            &agent(ADAPTER_CLAUDE),
            &request(Some("run-model")),
            Some(&provider(ProviderKind::Anthropic, Some(KEY))),
        )
        .unwrap();
        assert!(prepared
            .args
            .windows(2)
            .any(|pair| pair == ["--model", "run-model"]));
        assert_eq!(
            prepared.environment["ANTHROPIC_DEFAULT_OPUS_MODEL"],
            "run-model"
        );
    }

    #[test]
    fn unsupported_combinations_are_rejected_clearly() {
        let error = |adapter: &str, kind: ProviderKind| {
            prepare_with_provider(
                &agent(adapter),
                &request(None),
                Some(&provider(kind, Some(KEY))),
            )
            .err()
            .unwrap()
        };
        assert_eq!(
            error(ADAPTER_CODEX, ProviderKind::OpenaiChat),
            "agent 'Agent' cannot use a openai-chat model provider"
        );
        assert!(error(ADAPTER_CODEX, ProviderKind::Anthropic).contains("anthropic"));
        assert!(error(ADAPTER_CLAUDE, ProviderKind::OpenaiResponses).contains("openai-responses"));
        assert!(error(ADAPTER_GROK, ProviderKind::Anthropic).contains("cannot use"));
        assert!(error("cli", ProviderKind::OpenaiChat).contains("cannot use"));

        let mut modelless = provider(ProviderKind::Anthropic, Some(KEY));
        modelless.model = None;
        let message =
            prepare_with_provider(&agent(ADAPTER_CLAUDE), &request(None), Some(&modelless))
                .err()
                .unwrap();
        assert!(message.contains("no model"));
        assert!(!message.contains(KEY));
    }

    #[test]
    fn provider_id_round_trips_and_old_configs_load_without_one() {
        let mut config = agent(ADAPTER_CLAUDE);
        assert_eq!(config.provider_id.as_deref(), Some("fixture"));
        config.provider_id = None;
        let value = serde_json::to_value(&config).unwrap();
        assert!(value.get("providerId").is_none());
        let reloaded: AgentConfig = serde_json::from_value(value).unwrap();
        assert!(reloaded.provider_id.is_none());
    }

    #[tokio::test]
    async fn runner_removes_inherited_prefixes_then_applies_both_environments() {
        assert!(std::env::var_os("HOME").is_some());
        let mut secret_environment = SecretEnvironment::default();
        secret_environment.insert("LOOM_FIXTURE_HIDDEN", "two");
        let prepared = PreparedAgentInvocation {
            environment: [("LOOM_FIXTURE_PUBLIC".to_string(), "one".to_string())].into(),
            environment_remove_prefixes: vec!["HOME".into()],
            secret_environment,
            program: "/bin/sh".into(),
            args: vec![
                "-c".into(),
                r#"printf '%s|%s|%s' "${HOME-unset}" "$LOOM_FIXTURE_PUBLIC" "$LOOM_FIXTURE_HIDDEN""#
                    .into(),
            ],
            cwd: std::env::temp_dir().to_string_lossy().into_owned(),
            stdin_prompt: false,
            output_mode: "plain".into(),
            resumed: false,
            api: None,
        };
        let outcome = run(
            RunSpec {
                prepared,
                run_id: format!("provider-env-{}", crate::models::now_ms()),
                owner: ProcessOwner::Feature {
                    feature: "fixture".into(),
                    scope_id: "provider".into(),
                    run_id: "env".into(),
                },
                kind: ProcessKind::Feature,
                stdin_text: None,
                limits: Limits {
                    timeout: Duration::from_secs(5),
                    ..Default::default()
                },
                enrich: None,
            },
            || None,
            |_| Ok(()),
            |_| Ok(()),
        )
        .await
        .unwrap();
        assert_eq!(outcome.content.trim(), "unset|one|two");
    }
}
