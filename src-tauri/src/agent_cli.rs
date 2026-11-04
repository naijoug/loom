//! `loom-agent`: a headless entry point to the agent layer. It reads the
//! desktop's `agents.json` and `providers.json`, prepares runs exactly like
//! the features do and streams the result as text or NDJSON. It never writes
//! agent settings; provider keys arrive only on stdin.
use crate::agent::{
    self, AdapterInvocationRequest, AgentEvent, AgentOutcome, Limits, MessagePart, Observation,
    PreparedAgentInvocation, ProcessKind, ProcessOwner, ProviderInput, ProviderKind,
    ResolvedProvider, RunSpec, STOP_REASON_ABORT, STOP_REASON_TIMEOUT,
};
use crate::models::{now_ms, AgentConfig};
use crate::storage::private;
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    collections::VecDeque,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

pub const HELP: &str = "Usage: loom-agent <command> [options]\n\nCommands:\n  list [--json]                          Configured agents\n  doctor [AGENT] [--json]                Check agents can start\n  provider list [--json]                 Model providers (keys are never shown)\n  provider add ID --kind KIND --base-url URL [--name NAME] [--model MODEL] [--key-env VAR]\n                                         KIND: anthropic|openai-responses|openai-chat\n  provider set-key ID [--clear]          Store the key read from stdin, or remove it\n  provider remove ID                     Delete an unused provider\n  provider test ID                       Send one minimal request\n  run [options]                          Run one prompt\n\nRun options:\n  --agent ID             Agent from Settings (its own login, or its provider)\n  --provider ID          Model provider: with --agent it replaces the agent's\n                         provider; alone it calls the model API directly\n  --project DIR          Working directory (default: current directory)\n  --mode MODE            explore|ask|auto (default explore)\n  --model NAME           Model override\n  --resume SESSION       Continue a native CLI session\n  --prompt TEXT          Prompt; otherwise --prompt-file FILE or stdin\n  --timeout-secs N       At most 1200 (default 600)\n  --json                 NDJSON events: started, text, part, log, done\n  --verbose              Include process output lines\n\nExit codes: 0 complete, 1 failed, 2 usage, 124 timed out, 130 interrupted.\n";

const EXIT_OK: i32 = 0;
const EXIT_FAILED: i32 = 1;
const EXIT_USAGE: i32 = 2;
const EXIT_TIMEOUT: i32 = 124;
const EXIT_INTERRUPTED: i32 = 130;
const MAX_TIMEOUT_SECS: u64 = 20 * 60;
const DEFAULT_TIMEOUT_SECS: u64 = 10 * 60;
const MAX_PROMPT_BYTES: u64 = 1024 * 1024;
const MAX_KEY_BYTES: u64 = 16 * 1024;
const STDERR_TAIL_LINES: usize = 40;
const RECORD_ARG_CHARS: usize = 256;

/// Process streams and the environment, injected so tests run in-process.
pub struct Io<'a> {
    pub stdin: &'a mut dyn Read,
    /// A terminal never supplies a key or a prompt: it would echo it.
    pub stdin_terminal: bool,
    pub out: &'a mut dyn Write,
    pub err: &'a mut dyn Write,
    pub env: &'a dyn Fn(&str) -> Option<String>,
    pub cwd: PathBuf,
}

/// Set by the first Ctrl-C; a second one force-stops the process group.
pub struct Interrupt {
    requested: AtomicBool,
}

impl Interrupt {
    pub const fn new() -> Self {
        Self {
            requested: AtomicBool::new(false),
        }
    }
    fn requested(&self) -> bool {
        self.requested.load(Ordering::SeqCst)
    }
}

impl Default for Interrupt {
    fn default() -> Self {
        Self::new()
    }
}

/// The binary's entry: the desktop's configuration directory, real stdio and
/// Ctrl-C handling.
pub fn main(args: &[String]) -> i32 {
    static INTERRUPT: Interrupt = Interrupt::new();
    let mut stdin = std::io::stdin();
    let stdin_terminal = std::io::IsTerminal::is_terminal(&stdin);
    let mut io = Io {
        stdin: &mut stdin,
        stdin_terminal,
        out: &mut std::io::stdout(),
        err: &mut std::io::stderr(),
        env: &agent::process_environment,
        cwd: std::env::current_dir().unwrap_or_default(),
    };
    let dir = match crate::storage::system_config_dir() {
        Ok(dir) => dir,
        Err(error) => {
            let _ = writeln!(io.err, "error: {error}");
            return EXIT_FAILED;
        }
    };
    tauri::async_runtime::block_on(async {
        let watcher = tauri::async_runtime::spawn(watch_interrupts(&INTERRUPT));
        let code = run_with(args, &dir, &mut io, &INTERRUPT).await;
        watcher.abort();
        code
    })
}

async fn watch_interrupts(interrupt: &'static Interrupt) {
    while tokio::signal::ctrl_c().await.is_ok() {
        if interrupt.requested.swap(true, Ordering::SeqCst) {
            for run_id in RUNNING.lock().map(|ids| ids.clone()).unwrap_or_default() {
                let _ = agent::force_stop(&run_id);
            }
        }
    }
}

/// Runs being executed by this process, for the second Ctrl-C.
static RUNNING: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Testable core: `dir` holds `agents.json` and `providers.json`.
pub async fn run_with(args: &[String], dir: &Path, io: &mut Io<'_>, interrupt: &Interrupt) -> i32 {
    let command = match Command::parse(args) {
        Ok(command) => command,
        Err(error) => {
            let _ = writeln!(io.err, "error: {error}\n\n{HELP}");
            return EXIT_USAGE;
        }
    };
    let result = match command {
        Command::Help => io.out.write_all(HELP.as_bytes()).map_err(|e| e.to_string()),
        Command::List { json } => list(dir, json, io),
        Command::Doctor { agent, json } => doctor(dir, agent.as_deref(), json, io).await,
        Command::Provider(command) => provider(dir, command, io).await,
        Command::Run(options) => return run(dir, *options, io, interrupt).await,
    };
    match result {
        Ok(()) => EXIT_OK,
        Err(error) => {
            let _ = writeln!(io.err, "error: {error}");
            EXIT_FAILED
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Command {
    Help,
    List { json: bool },
    Doctor { agent: Option<String>, json: bool },
    Provider(ProviderCommand),
    Run(Box<RunOptions>),
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ProviderCommand {
    List {
        json: bool,
    },
    Add {
        id: String,
        kind: ProviderKind,
        base_url: String,
        name: Option<String>,
        model: Option<String>,
        key_env: Option<String>,
    },
    SetKey {
        id: String,
        clear: bool,
    },
    Remove {
        id: String,
    },
    Test {
        id: String,
        json: bool,
    },
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct RunOptions {
    pub agent: Option<String>,
    pub provider: Option<String>,
    pub project: Option<String>,
    pub mode: String,
    pub model: Option<String>,
    pub resume: Option<String>,
    pub prompt: Option<String>,
    pub prompt_file: Option<String>,
    pub timeout_secs: u64,
    pub json: bool,
    pub verbose: bool,
}

/// Splits `--flag value` options; positional words stay in order.
struct Args<'a> {
    positional: Vec<&'a str>,
    values: Vec<(&'a str, Option<&'a str>)>,
}

impl<'a> Args<'a> {
    fn split(args: &'a [String], valued: &[&str], switches: &[&str]) -> Result<Self, String> {
        let mut out = Self {
            positional: Vec::new(),
            values: Vec::new(),
        };
        let mut iter = args.iter().map(String::as_str);
        while let Some(arg) = iter.next() {
            if valued.contains(&arg) {
                let value = iter.next().ok_or(format!("{arg} needs a value"))?;
                if out.values.iter().any(|(name, _)| *name == arg) {
                    return Err(format!("{arg} was given twice"));
                }
                out.values.push((arg, Some(value)));
            } else if switches.contains(&arg) {
                out.values.push((arg, None));
            } else if arg.starts_with("--") {
                return Err(format!("unknown option {arg}"));
            } else {
                out.positional.push(arg);
            }
        }
        Ok(out)
    }
    fn value(&self, name: &str) -> Option<String> {
        self.values
            .iter()
            .find(|(n, _)| *n == name)
            .and_then(|(_, v)| v.map(str::to_owned))
    }
    fn switch(&self, name: &str) -> bool {
        self.values.iter().any(|(n, _)| *n == name)
    }
    fn exactly(&self, count: usize, usage: &str) -> Result<(), String> {
        if self.positional.len() == count {
            Ok(())
        } else {
            Err(format!("usage: loom-agent {usage}"))
        }
    }
}

impl Command {
    pub(crate) fn parse(args: &[String]) -> Result<Self, String> {
        let Some((command, rest)) = args.split_first() else {
            return Ok(Self::Help);
        };
        match command.as_str() {
            "help" | "--help" | "-h" => Ok(Self::Help),
            "list" => {
                let a = Args::split(rest, &[], &["--json"])?;
                a.exactly(0, "list [--json]")?;
                Ok(Self::List {
                    json: a.switch("--json"),
                })
            }
            "doctor" => {
                let a = Args::split(rest, &[], &["--json"])?;
                if a.positional.len() > 1 {
                    return Err("usage: loom-agent doctor [AGENT] [--json]".into());
                }
                Ok(Self::Doctor {
                    agent: a.positional.first().map(|id| (*id).to_owned()),
                    json: a.switch("--json"),
                })
            }
            "provider" => Self::parse_provider(rest).map(Self::Provider),
            "run" => Self::parse_run(rest).map(|options| Self::Run(Box::new(options))),
            other => Err(format!("unknown command '{other}'")),
        }
    }

    fn parse_provider(args: &[String]) -> Result<ProviderCommand, String> {
        let Some((action, rest)) = args.split_first() else {
            return Err("usage: loom-agent provider list|add|set-key|remove|test".into());
        };
        let id = |a: &Args| a.positional.first().map(|id| (*id).to_owned());
        match action.as_str() {
            "list" => {
                let a = Args::split(rest, &[], &["--json"])?;
                a.exactly(0, "provider list [--json]")?;
                Ok(ProviderCommand::List {
                    json: a.switch("--json"),
                })
            }
            "add" => {
                let a = Args::split(
                    rest,
                    &["--kind", "--base-url", "--name", "--model", "--key-env"],
                    &[],
                )?;
                a.exactly(1, "provider add ID --kind KIND --base-url URL")?;
                let kind = a.value("--kind").ok_or("provider add needs --kind")?;
                let kind = serde_json::from_value(Value::String(kind.clone())).map_err(|_| {
                    format!(
                        "unknown provider kind '{kind}'; use anthropic, openai-responses or openai-chat"
                    )
                })?;
                Ok(ProviderCommand::Add {
                    id: id(&a).unwrap_or_default(),
                    kind,
                    base_url: a
                        .value("--base-url")
                        .ok_or("provider add needs --base-url")?,
                    name: a.value("--name"),
                    model: a.value("--model"),
                    key_env: a.value("--key-env"),
                })
            }
            "set-key" => {
                let a = Args::split(rest, &[], &["--clear"])?;
                a.exactly(1, "provider set-key ID [--clear] < key")?;
                Ok(ProviderCommand::SetKey {
                    id: id(&a).unwrap_or_default(),
                    clear: a.switch("--clear"),
                })
            }
            "remove" => {
                let a = Args::split(rest, &[], &[])?;
                a.exactly(1, "provider remove ID")?;
                Ok(ProviderCommand::Remove {
                    id: id(&a).unwrap_or_default(),
                })
            }
            "test" => {
                let a = Args::split(rest, &[], &["--json"])?;
                a.exactly(1, "provider test ID [--json]")?;
                Ok(ProviderCommand::Test {
                    id: id(&a).unwrap_or_default(),
                    json: a.switch("--json"),
                })
            }
            other => Err(format!("unknown provider command '{other}'")),
        }
    }

    fn parse_run(args: &[String]) -> Result<RunOptions, String> {
        let a = Args::split(
            args,
            &[
                "--agent",
                "--provider",
                "--project",
                "--mode",
                "--model",
                "--resume",
                "--prompt",
                "--prompt-file",
                "--timeout-secs",
            ],
            &["--json", "--verbose"],
        )?;
        a.exactly(0, "run [options]; see loom-agent help")?;
        let options = RunOptions {
            agent: a.value("--agent"),
            provider: a.value("--provider"),
            project: a.value("--project"),
            mode: a.value("--mode").unwrap_or_else(|| "explore".into()),
            model: a.value("--model"),
            resume: a.value("--resume"),
            prompt: a.value("--prompt"),
            prompt_file: a.value("--prompt-file"),
            timeout_secs: match a.value("--timeout-secs") {
                Some(value) => value
                    .parse::<u64>()
                    .ok()
                    .filter(|secs| (1..=MAX_TIMEOUT_SECS).contains(secs))
                    .ok_or(format!(
                        "--timeout-secs must be a whole number from 1 to {MAX_TIMEOUT_SECS}"
                    ))?,
                None => DEFAULT_TIMEOUT_SECS,
            },
            json: a.switch("--json"),
            verbose: a.switch("--verbose"),
        };
        if options.agent.is_none() && options.provider.is_none() {
            return Err("run needs --agent or --provider".into());
        }
        if !matches!(options.mode.as_str(), "explore" | "ask" | "auto") {
            return Err("--mode must be explore, ask or auto".into());
        }
        if options.prompt.is_some() && options.prompt_file.is_some() {
            return Err("use either --prompt or --prompt-file".into());
        }
        Ok(options)
    }
}

/// Agents as Settings shows them, with availability re-checked on this machine.
fn agents(dir: &Path) -> Result<Vec<AgentConfig>, String> {
    let mut agents = agent::read_agents_at(&agent::agents_file(dir))?;
    for agent in &mut agents {
        if agent.adapter_type != agent::ADAPTER_API {
            agent.available = agent::resolve_executable(&agent.command).is_some();
        }
    }
    Ok(agents)
}

fn find_agent(agents: Vec<AgentConfig>, id: &str) -> Result<AgentConfig, String> {
    agents
        .into_iter()
        .find(|agent| agent.id == id)
        .ok_or_else(|| format!("agent '{id}' not found; see loom-agent list"))
}

fn print_json(out: &mut dyn Write, value: &impl serde::Serialize) -> Result<(), String> {
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    writeln!(out, "{text}").map_err(|e| e.to_string())
}

fn list(dir: &Path, json: bool, io: &mut Io) -> Result<(), String> {
    let agents = agents(dir)?;
    if json {
        let rows: Vec<Value> = agents
            .iter()
            .map(|a| {
                json!({
                    "id": a.id,
                    "name": a.name,
                    "adapterType": a.adapter_type,
                    "providerId": a.provider_id,
                    "enabled": a.enabled,
                    "available": a.available,
                })
            })
            .collect();
        return print_json(io.out, &rows);
    }
    for a in &agents {
        let source = a.provider_id.as_deref().unwrap_or("own login");
        let state = match (
            a.enabled,
            a.available || a.adapter_type == agent::ADAPTER_API,
        ) {
            (false, _) => "disabled",
            (true, false) => "not installed",
            (true, true) => "enabled",
        };
        writeln!(
            io.out,
            "{}\t{}\t{} · {}\t{state}",
            a.id, a.name, a.adapter_type, source
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

async fn doctor(dir: &Path, id: Option<&str>, json: bool, io: &mut Io<'_>) -> Result<(), String> {
    let mut agents = agents(dir)?;
    if let Some(id) = id {
        agents = vec![find_agent(agents, id)?];
    }
    let mut results = Vec::new();
    for agent in agents {
        results.push(agent::diagnose(agent, dir).await);
    }
    if json {
        return print_json(io.out, &results);
    }
    for d in &results {
        let version = d.version.as_deref().unwrap_or("-");
        writeln!(
            io.out,
            "{}\t{}\t{version}\t{}",
            d.agent_id, d.status, d.detail
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Reads a bounded secret or prompt from a pipe or file, never a terminal.
fn read_stdin(io: &mut Io, what: &str, limit: u64) -> Result<String, String> {
    if io.stdin_terminal {
        return Err(format!(
            "pipe the {what} on stdin (for example `pbpaste | loom-agent …`); typing it would show it on screen"
        ));
    }
    let mut text = String::new();
    io.stdin
        .take(limit + 1)
        .read_to_string(&mut text)
        .map_err(|_| format!("could not read the {what} from stdin"))?;
    if text.len() as u64 > limit {
        return Err(format!("the {what} on stdin is too long"));
    }
    Ok(text)
}

async fn provider(dir: &Path, command: ProviderCommand, io: &mut Io<'_>) -> Result<(), String> {
    let env = io.env;
    match command {
        ProviderCommand::List { json } => {
            let list = agent::list_providers_at(dir, env)?;
            if json {
                return print_json(io.out, &list);
            }
            for p in &list.providers {
                let key = match (p.configured, p.key_env.as_deref()) {
                    (true, Some(var)) if !p.stored_key => format!("key from ${var}"),
                    (true, _) if p.stored_key => "key stored".into(),
                    (true, _) => "local, no key".into(),
                    (false, _) => "no key".into(),
                };
                writeln!(
                    io.out,
                    "{}\t{}\t{}\t{}\t{}\t{key}",
                    p.id,
                    p.name,
                    p.kind.label(),
                    p.base_url,
                    p.model.as_deref().unwrap_or("-")
                )
                .map_err(|e| e.to_string())?;
            }
            Ok(())
        }
        ProviderCommand::Add {
            id,
            kind,
            base_url,
            name,
            model,
            key_env,
        } => {
            let list = agent::list_providers_at(dir, env)?;
            if list.providers.iter().any(|p| p.id == id) {
                return Err(format!("provider '{id}' already exists"));
            }
            agent::save_provider_at(
                dir,
                ProviderInput {
                    name: name.unwrap_or_else(|| id.clone()),
                    id: id.clone(),
                    expected_revision: list.revision,
                    kind,
                    base_url,
                    model,
                    key: agent::KeyChange::Keep,
                    key_env,
                },
                env,
            )?;
            writeln!(
                io.err,
                "added provider '{id}'; store a key with `loom-agent provider set-key {id}`"
            )
            .map_err(|e| e.to_string())
        }
        ProviderCommand::SetKey { id, clear } => {
            let list = agent::list_providers_at(dir, env)?;
            let p = list
                .providers
                .iter()
                .find(|p| p.id == id)
                .ok_or_else(|| format!("provider '{id}' not found"))?;
            let key = if clear {
                agent::KeyChange::Clear
            } else {
                let key = read_stdin(io, "API key", MAX_KEY_BYTES)?;
                if key.trim().is_empty() {
                    return Err("no API key on stdin".into());
                }
                agent::KeyChange::Set(key)
            };
            agent::save_provider_at(
                dir,
                ProviderInput {
                    id: p.id.clone(),
                    expected_revision: list.revision,
                    name: p.name.clone(),
                    kind: p.kind,
                    base_url: p.base_url.clone(),
                    model: p.model.clone(),
                    key,
                    key_env: p.key_env.clone(),
                },
                env,
            )?;
            let done = if clear { "removed" } else { "stored" };
            writeln!(io.err, "{done} the key of provider '{id}'").map_err(|e| e.to_string())
        }
        ProviderCommand::Remove { id } => {
            agent::ensure_provider_unused(&agents(dir)?, &id)?;
            let revision = agent::list_providers_at(dir, env)?.revision;
            agent::delete_provider_at(dir, &id, revision, env)?;
            writeln!(io.err, "removed provider '{id}'").map_err(|e| e.to_string())
        }
        ProviderCommand::Test { id, json } => {
            let provider = agent::resolve_provider_at(dir, &id, env)?;
            let result = agent::test_provider(&provider).await;
            if json {
                return print_json(io.out, &result);
            }
            let outcome = serde_json::to_value(result.outcome).unwrap_or_default();
            let status = result
                .http_status
                .map(|s| format!(" (HTTP {s})"))
                .unwrap_or_default();
            writeln!(io.out, "{}{status}", outcome.as_str().unwrap_or("unknown"))
                .map_err(|e| e.to_string())?;
            if result.outcome == agent::ProviderTestOutcome::Ok {
                Ok(())
            } else {
                Err(format!("provider '{id}' did not answer correctly"))
            }
        }
    }
}

/// Everything a run needs after configuration is read and checked.
pub(crate) struct PreparedRun {
    pub agent: AgentConfig,
    pub prepared: PreparedAgentInvocation,
    pub prompt: String,
}

/// An agent for `--provider` alone: the model API called directly (mode C).
fn api_agent(provider_id: &str) -> AgentConfig {
    AgentConfig {
        id: format!("api:{provider_id}"),
        name: format!("API ({provider_id})"),
        command: String::new(),
        args: Vec::new(),
        working_directory_policy: "project_root".into(),
        capabilities: Vec::new(),
        adapter_type: agent::ADAPTER_API.into(),
        can_write_files: false,
        can_run_commands: false,
        enabled: true,
        available: true,
        provider_id: Some(provider_id.into()),
    }
}

/// Builds the same request Chat builds for a turn, minus Chat's own prompt
/// wrapping, session resume bookkeeping and inline cards.
pub(crate) fn prepare_run(
    dir: &Path,
    options: &RunOptions,
    prompt: String,
    cwd: &Path,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<PreparedRun, String> {
    let mut agent = match &options.agent {
        Some(id) => find_agent(agents(dir)?, id)?,
        None => api_agent(options.provider.as_deref().unwrap_or_default()),
    };
    if let Some(id) = &options.provider {
        agent.provider_id = Some(id.clone());
    }
    let provider: Option<ResolvedProvider> = agent
        .provider_id
        .as_deref()
        .map(|id| agent::resolve_provider_at(dir, id, env))
        .transpose()?;
    let project = match &options.project {
        Some(path) => crate::projects::canonical_project_path(path)?,
        None => cwd.to_path_buf(),
    };
    let resume = options
        .resume
        .as_deref()
        .map(|id| agent::resume_command(&agent, id))
        .transpose()?;
    let request = AdapterInvocationRequest {
        execution_profile: None,
        project_path: &project,
        prompt: &prompt,
        prompt_file: None,
        stage: agent::permission_to_stage(&options.mode),
        resume_command: resume.as_deref(),
        chat_permission_mode: Some(&options.mode),
        embed_prompt: agent::embeds_prompt(&agent.adapter_type),
        system_append: None,
        model: options.model.as_deref(),
        needs_tools: false,
    };
    let prepared = agent::prepare_invocation_with(&agent, &request, provider.as_ref())?;
    Ok(PreparedRun {
        agent,
        prepared,
        prompt,
    })
}

fn read_prompt(options: &RunOptions, io: &mut Io) -> Result<String, String> {
    let prompt = match (&options.prompt, &options.prompt_file) {
        (Some(prompt), _) => prompt.clone(),
        (None, Some(path)) => {
            let file = std::fs::File::open(path)
                .map_err(|error| format!("could not read --prompt-file: {error}"))?;
            let mut text = String::new();
            file.take(MAX_PROMPT_BYTES + 1)
                .read_to_string(&mut text)
                .map_err(|error| format!("could not read --prompt-file: {error}"))?;
            if text.len() as u64 > MAX_PROMPT_BYTES {
                return Err("--prompt-file is larger than 1 MiB".into());
            }
            text
        }
        (None, None) => read_stdin(io, "prompt", MAX_PROMPT_BYTES)?,
    };
    if prompt.trim().is_empty() {
        return Err("the prompt is empty".into());
    }
    Ok(prompt)
}

/// NDJSON or human output, shared by the event and observation callbacks.
struct Sink<'a, 'b> {
    io: &'a mut Io<'b>,
    json: bool,
    verbose: bool,
    stderr_tail: VecDeque<String>,
    text_ended_in_newline: bool,
}

impl Sink<'_, '_> {
    fn line(&mut self, value: Value) -> Result<(), String> {
        writeln!(self.io.out, "{value}")
            .and_then(|_| self.io.out.flush())
            .map_err(|error| format!("could not write output: {error}"))
    }
    fn human(&mut self, stderr: bool, text: &str) -> Result<(), String> {
        let stream: &mut dyn Write = if stderr { self.io.err } else { self.io.out };
        stream
            .write_all(text.as_bytes())
            .and_then(|_| stream.flush())
            .map_err(|error| format!("could not write output: {error}"))
    }
    fn event(&mut self, event: AgentEvent) -> Result<(), String> {
        match event {
            AgentEvent::Text(text) if self.json => self.line(json!({"type": "text", "text": text})),
            AgentEvent::Text(text) => {
                if !text.is_empty() {
                    self.text_ended_in_newline = text.ends_with('\n');
                }
                self.human(false, &text)
            }
            AgentEvent::Part(part) if self.json => self.line(json!({"type": "part", "part": part})),
            AgentEvent::Part(part) => match part {
                MessagePart::Tool {
                    name,
                    input_summary,
                    ..
                } => {
                    let summary = input_summary.map(|s| format!(" {s}")).unwrap_or_default();
                    self.human(true, &format!("· {name}{summary}\n"))
                }
                MessagePart::Error { message, .. } => self.human(true, &format!("! {message}\n")),
                MessagePart::Text { .. } => Ok(()),
            },
            AgentEvent::Done => Ok(()),
        }
    }
    fn observe(&mut self, observation: Observation, started: &Value) -> Result<(), String> {
        match observation {
            Observation::Started(pid) => {
                let mut started = started.clone();
                started["pid"] = pid.into();
                if self.json {
                    self.line(started)?;
                }
                Ok(())
            }
            Observation::Log { stderr, line } => {
                let line = agent::redact_sensitive_text(&line);
                if stderr {
                    if self.stderr_tail.len() == STDERR_TAIL_LINES {
                        self.stderr_tail.pop_front();
                    }
                    self.stderr_tail.push_back(line.clone());
                }
                match (self.verbose, self.json) {
                    (false, _) => Ok(()),
                    (true, true) => self.line(json!({
                        "type": "log",
                        "stream": if stderr { "stderr" } else { "stdout" },
                        "line": line,
                    })),
                    (true, false) => self.human(true, &format!("{line}\n")),
                }
            }
        }
    }
}

async fn run(dir: &Path, options: RunOptions, io: &mut Io<'_>, interrupt: &Interrupt) -> i32 {
    let prepared = read_prompt(&options, io).and_then(|prompt| {
        let cwd = io.cwd.clone();
        prepare_run(dir, &options, prompt, &cwd, io.env)
    });
    let PreparedRun {
        agent,
        prepared,
        prompt,
    } = match prepared {
        Ok(prepared) => prepared,
        Err(error) => {
            let _ = writeln!(io.err, "error: {error}");
            return EXIT_FAILED;
        }
    };
    let api = agent.adapter_type == agent::ADAPTER_API;
    if !api {
        if let Err(error) = agent::check_startable(&agent).await {
            let _ = writeln!(io.err, "error: {error}");
            return EXIT_FAILED;
        }
    }
    static RUNS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let run_id = format!(
        "cli-{}-{}-{}",
        now_ms(),
        std::process::id(),
        RUNS.fetch_add(1, Ordering::Relaxed)
    );
    let started_at_ms = now_ms();
    let record = Record::new(&run_id, &agent, &options, &prepared, &prompt);
    let started = json!({
        "type": "started",
        "runId": run_id,
        "agentId": agent.id,
        "adapterType": agent.adapter_type,
        "providerId": agent.provider_id,
    });
    let spec = RunSpec {
        stdin_text: prepared.stdin_prompt.then(|| prompt.clone()),
        prepared,
        run_id: run_id.clone(),
        owner: ProcessOwner::Feature {
            feature: "cli".into(),
            scope_id: agent.id.clone(),
            run_id: run_id.clone(),
        },
        kind: ProcessKind::Feature,
        limits: Limits {
            timeout: Duration::from_secs(options.timeout_secs),
            ..Limits::default()
        },
        enrich: None,
    };
    let sink = RefCell::new(Sink {
        io,
        json: options.json,
        verbose: options.verbose,
        stderr_tail: VecDeque::new(),
        text_ended_in_newline: true,
    });
    if api && options.json {
        // No process: the request starts as soon as the run does.
        let mut started = started.clone();
        started["pid"] = Value::Null;
        if let Err(error) = sink.borrow_mut().line(started) {
            let _ = writeln!(sink.borrow_mut().io.err, "error: {error}");
            return EXIT_FAILED;
        }
    }
    if let Ok(mut running) = RUNNING.lock() {
        running.push(run_id.clone());
    }
    let result = agent::run(
        spec,
        || interrupt.requested().then_some(STOP_REASON_ABORT),
        |event| sink.borrow_mut().event(event),
        |observation| sink.borrow_mut().observe(observation, &started),
    )
    .await;
    if let Ok(mut running) = RUNNING.lock() {
        running.retain(|id| id != &run_id);
    }
    let mut sink = sink.into_inner();
    let mut outcome = result.unwrap_or_else(|error| AgentOutcome {
        status: "error".into(),
        error_summary: Some(agent::redact_sensitive_text(&error)),
        ..AgentOutcome::default()
    });
    if outcome.termination_reason.as_deref() == Some(STOP_REASON_TIMEOUT) {
        outcome.error_summary = Some(format!(
            "timed out after {} seconds and was stopped",
            options.timeout_secs
        ));
    }
    let resume_session_id = outcome.resume_command.as_deref().and_then(|handle| {
        agent::parse_resume_session_id(&agent.adapter_type, &agent.command, handle).ok()
    });
    let code = exit_code(&outcome);
    let done = json!({
        "type": "done",
        "runId": run_id,
        "status": outcome.status,
        "exitCode": outcome.exit_code,
        "errorSummary": outcome.error_summary,
        "terminationReason": outcome.termination_reason,
        "resumeSessionId": resume_session_id,
    });
    let stderr_tail: Vec<String> = sink.stderr_tail.iter().cloned().collect();
    if let Err(error) = record.write(dir, started_at_ms, &outcome, stderr_tail) {
        let _ = writeln!(
            sink.io.err,
            "warning: could not save the run record: {error}"
        );
    }
    if sink.json {
        let _ = sink.line(done);
    } else {
        if !sink.text_ended_in_newline {
            let _ = sink.human(false, "\n");
        }
        if let Some(summary) = &outcome.error_summary {
            let _ = writeln!(sink.io.err, "error: {summary}");
        } else if code == EXIT_INTERRUPTED {
            let _ = writeln!(sink.io.err, "stopped");
        }
        if let Some(id) = &resume_session_id {
            let _ = writeln!(sink.io.err, "session: {id} (continue with --resume {id})");
        }
    }
    code
}

fn exit_code(outcome: &AgentOutcome) -> i32 {
    match (
        outcome.status.as_str(),
        outcome.termination_reason.as_deref(),
    ) {
        ("complete", _) => EXIT_OK,
        (_, Some(STOP_REASON_TIMEOUT)) => EXIT_TIMEOUT,
        ("aborted", _) => EXIT_INTERRUPTED,
        _ => EXIT_FAILED,
    }
}

/// The visible execution record at `<config>/cli/runs/<run id>.json`: what
/// ran and how it ended, never the prompt, the environment or a key.
struct Record {
    value: Value,
}

impl Record {
    fn new(
        run_id: &str,
        agent: &AgentConfig,
        options: &RunOptions,
        prepared: &PreparedAgentInvocation,
        prompt: &str,
    ) -> Self {
        let args: Vec<String> = prepared
            .args
            .iter()
            .map(|arg| record_arg(arg, prompt))
            .collect();
        Self {
            value: json!({
                "schemaVersion": 1,
                "runId": run_id,
                "agentId": agent.id,
                "adapterType": agent.adapter_type,
                "providerId": agent.provider_id,
                "mode": options.mode,
                "model": options.model,
                "program": prepared.program,
                "args": args,
                "cwd": prepared.cwd,
                "resumed": prepared.resumed,
                "promptChars": prompt.chars().count(),
            }),
        }
    }

    fn write(
        mut self,
        dir: &Path,
        started_at_ms: u128,
        outcome: &AgentOutcome,
        stderr_tail: Vec<String>,
    ) -> Result<(), String> {
        let value = &mut self.value;
        value["startedAtMs"] = json!(started_at_ms);
        value["finishedAtMs"] = json!(now_ms());
        value["status"] = json!(outcome.status);
        value["exitCode"] = json!(outcome.exit_code);
        value["terminationReason"] = json!(outcome.termination_reason);
        value["errorSummary"] = json!(outcome.error_summary);
        value["stderrTail"] = json!(stderr_tail);
        let runs = private::private_directory(&dir.join("cli").join("runs"))
            .map_err(|error| error.to_string())?;
        let file = runs.join(format!("{}.json", value["runId"].as_str().unwrap_or("run")));
        let bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
        private::atomic_bytes(&file, &bytes).map_err(|error| error.to_string())
    }
}

/// Arguments are recorded as run, except the prompt and other long text.
fn record_arg(arg: &str, prompt: &str) -> String {
    if !prompt.trim().is_empty() && arg.contains(prompt.trim()) {
        "<prompt>".into()
    } else if arg.chars().count() > RECORD_ARG_CHARS {
        format!("<{} chars>", arg.chars().count())
    } else {
        agent::redact_sensitive_text(arg)
    }
}

#[cfg(test)]
#[path = "agent_cli_tests.rs"]
mod tests;
