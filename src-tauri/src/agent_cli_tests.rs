//! `loom-agent` in-process: argument parsing, the request it prepares, and
//! the NDJSON stream and run record for a stub CLI and a fake model API.
use super::*;
use crate::migrations::{write_versioned_json, AGENT_STORE_SCHEMA_VERSION};
use std::{
    net::TcpListener,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
};

const KEY: &str = "sk-cli-fixture-secret-0123456789";
const PROMPT: &str = "explain the fixture prompt";
static ROOT: AtomicU64 = AtomicU64::new(0);

fn test_root(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "loom-agent-cli-{label}-{}-{}",
        now_ms(),
        ROOT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).unwrap();
    root.canonicalize().unwrap()
}

fn no_environment(_: &str) -> Option<String> {
    None
}

fn args(words: &[&str]) -> Vec<String> {
    words.iter().map(|word| (*word).to_owned()).collect()
}

struct Output {
    code: i32,
    out: String,
    err: String,
}

async fn call(dir: &Path, words: &[&str], stdin: &str, terminal: bool) -> Output {
    let mut input = stdin.as_bytes();
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let mut io = Io {
        stdin: &mut input,
        stdin_terminal: terminal,
        out: &mut out,
        err: &mut err,
        env: &no_environment,
        cwd: dir.to_path_buf(),
    };
    let code = run_with(&args(words), dir, &mut io, &Interrupt::new()).await;
    Output {
        code,
        out: String::from_utf8(out).unwrap(),
        err: String::from_utf8(err).unwrap(),
    }
}

/// A custom agent whose executable prints one line on each stream.
fn stub_agent(dir: &Path, writable: bool) -> AgentConfig {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join("stub-agent");
    std::fs::write(
        &path,
        "#!/bin/sh\nprintf 'hello from stub\\n'\necho 'stub warning' >&2\n",
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    let agent = AgentConfig {
        id: "agent-stub".into(),
        name: "Stub".into(),
        command: path.display().to_string(),
        args: Vec::new(),
        working_directory_policy: "project_root".into(),
        capabilities: Vec::new(),
        adapter_type: "custom".into(),
        can_write_files: writable,
        can_run_commands: writable,
        enabled: true,
        available: true,
        provider_id: None,
    };
    write_versioned_json(
        &agent::agents_file(dir),
        AGENT_STORE_SCHEMA_VERSION,
        &vec![agent.clone()],
    )
    .unwrap();
    agent
}

fn ndjson(out: &str) -> Vec<Value> {
    out.lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn records(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir.join("cli").join("runs"))
        .unwrap()
        .map(|entry| std::fs::read_to_string(entry.unwrap().path()).unwrap())
        .collect()
}

#[test]
fn arguments_parse_into_commands_or_usage_errors() {
    assert_eq!(Command::parse(&[]), Ok(Command::Help));
    assert_eq!(
        Command::parse(&args(&["doctor", "agent-claude", "--json"])),
        Ok(Command::Doctor {
            agent: Some("agent-claude".into()),
            json: true
        })
    );
    assert_eq!(
        Command::parse(&args(&[
            "provider",
            "add",
            "gw",
            "--kind",
            "openai-chat",
            "--base-url",
            "http://127.0.0.1:1/v1"
        ])),
        Ok(Command::Provider(ProviderCommand::Add {
            id: "gw".into(),
            kind: ProviderKind::OpenaiChat,
            base_url: "http://127.0.0.1:1/v1".into(),
            name: None,
            model: None,
            key_env: None,
        }))
    );
    let Ok(Command::Run(run)) = Command::parse(&args(&[
        "run",
        "--agent",
        "a",
        "--mode",
        "ask",
        "--timeout-secs",
        "1200",
        "--json",
    ])) else {
        panic!("run did not parse");
    };
    assert_eq!(
        (
            run.agent.as_deref(),
            run.mode.as_str(),
            run.timeout_secs,
            run.json
        ),
        (Some("a"), "ask", 1200, true)
    );

    for (words, message) in [
        (&["run", "--prompt", "x"][..], "needs --agent or --provider"),
        (&["run", "--agent", "a", "--mode", "yolo"], "--mode"),
        (
            &["run", "--agent", "a", "--timeout-secs", "1201"],
            "1 to 1200",
        ),
        (&["run", "--agent", "a", "--timeout-secs", "0"], "1 to 1200"),
        (&["run", "--agent", "a", "--agent", "b"], "twice"),
        (&["run", "--agent"], "needs a value"),
        (&["run", "--agent", "a", "--api-key", "k"], "unknown option"),
        (
            &[
                "provider",
                "add",
                "gw",
                "--kind",
                "gemini",
                "--base-url",
                "u",
            ],
            "unknown provider kind",
        ),
        (&["provider", "set-key"], "usage"),
        (&["frobnicate"], "unknown command"),
    ] {
        let error = Command::parse(&args(words)).unwrap_err();
        assert!(error.contains(message), "{words:?}: {error}");
    }
}

#[tokio::test]
async fn usage_errors_exit_2_and_print_help() {
    let dir = test_root("usage");
    let output = call(&dir, &["run", "--prompt", "x"], "", false).await;
    assert_eq!(output.code, EXIT_USAGE);
    assert!(output.err.contains("Usage: loom-agent"), "{}", output.err);
}

#[test]
fn a_cli_run_prepares_what_a_chat_turn_prepares() {
    let dir = test_root("same-request");
    let agent = stub_agent(&dir, true);
    for mode in ["explore", "ask", "auto"] {
        let options = RunOptions {
            agent: Some(agent.id.clone()),
            mode: mode.into(),
            timeout_secs: DEFAULT_TIMEOUT_SECS,
            ..RunOptions::default()
        };
        let cli = prepare_run(&dir, &options, PROMPT.into(), &dir, &no_environment).unwrap();
        // The fields `chat_context::prepare_chat_invocation` sets for a turn.
        let chat = agent::prepare_invocation(
            &agent,
            &AdapterInvocationRequest {
                execution_profile: None,
                project_path: &dir,
                prompt: PROMPT,
                prompt_file: None,
                stage: crate::chat::permission_to_stage(mode),
                resume_command: None,
                chat_permission_mode: Some(mode),
                embed_prompt: agent::embeds_prompt(&agent.adapter_type),
                system_append: None,
                model: None,
                needs_tools: true,
            },
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(&cli.prepared).unwrap(),
            serde_json::to_value(&chat).unwrap(),
            "{mode}"
        );
    }
}

#[test]
fn a_cli_run_keeps_the_agents_permissions() {
    let dir = test_root("permissions");
    let agent = stub_agent(&dir, false);
    let options = RunOptions {
        agent: Some(agent.id),
        mode: "auto".into(),
        ..RunOptions::default()
    };
    let error = prepare_run(&dir, &options, PROMPT.into(), &dir, &no_environment)
        .err()
        .unwrap();
    assert!(
        error.contains("does not allow writable execution"),
        "{error}"
    );
}

#[tokio::test]
async fn a_cli_run_streams_ndjson_and_records_the_run_without_the_prompt() {
    let dir = test_root("stub-run");
    stub_agent(&dir, false);
    let output = call(
        &dir,
        &[
            "run",
            "--agent",
            "agent-stub",
            "--prompt",
            PROMPT,
            "--json",
            "--verbose",
        ],
        "",
        false,
    )
    .await;
    assert_eq!(output.code, EXIT_OK, "{}{}", output.out, output.err);
    let events = ndjson(&output.out);
    assert_eq!(events[0]["type"], "started");
    assert!(events[0]["pid"].as_u64().is_some_and(|pid| pid > 0));
    let text: String = events
        .iter()
        .filter(|e| e["type"] == "text")
        .filter_map(|e| e["text"].as_str())
        .collect();
    assert!(text.contains("hello from stub"), "{text}");
    assert!(events
        .iter()
        .any(|e| e["type"] == "log" && e["stream"] == "stderr" && e["line"] == "stub warning"));
    let done = events.last().unwrap();
    assert_eq!(
        (&done["type"], &done["status"]),
        (&json!("done"), &json!("complete"))
    );

    let records = records(&dir);
    assert_eq!(records.len(), 1);
    let record: Value = serde_json::from_str(&records[0]).unwrap();
    assert_eq!(record["status"], "complete");
    assert_eq!(record["args"], json!(["<prompt>"]));
    assert_eq!(record["stderrTail"], json!(["stub warning"]));
    assert!(!records[0].contains(PROMPT), "{}", records[0]);
}

#[tokio::test]
async fn human_output_prints_the_answer_on_stdout_only() {
    let dir = test_root("stub-human");
    stub_agent(&dir, false);
    let output = call(&dir, &["run", "--agent", "agent-stub"], PROMPT, false).await;
    assert_eq!(output.code, EXIT_OK, "{}", output.err);
    assert_eq!(output.out.trim(), "hello from stub");
    assert!(!output.err.contains("stub warning"), "{}", output.err);
}

#[tokio::test]
async fn keys_and_prompts_are_never_read_from_a_terminal() {
    let dir = test_root("terminal");
    let added = call(
        &dir,
        &[
            "provider",
            "add",
            "gw",
            "--kind",
            "openai-chat",
            "--base-url",
            "https://gateway.example/v1",
        ],
        "",
        false,
    )
    .await;
    assert_eq!(added.code, EXIT_OK, "{}", added.err);
    let key = call(&dir, &["provider", "set-key", "gw"], KEY, true).await;
    assert_eq!(key.code, EXIT_FAILED);
    assert!(key.err.contains("pipe the API key"), "{}", key.err);
    stub_agent(&dir, false);
    let prompt = call(&dir, &["run", "--agent", "agent-stub"], PROMPT, true).await;
    assert_eq!(prompt.code, EXIT_FAILED);
    assert!(prompt.err.contains("pipe the prompt"), "{}", prompt.err);
}

/// Answers one Chat Completions request with a two-delta stream.
fn chat_server() -> (String, mpsc::Receiver<String>) {
    use std::io::{Read, Write};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut request = Vec::new();
        let mut buffer = [0; 8192];
        let split = loop {
            if let Some(at) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                break at + 4;
            }
            let n = socket.read(&mut buffer).unwrap();
            request.extend_from_slice(&buffer[..n]);
        };
        let head = String::from_utf8_lossy(&request[..split]).to_ascii_lowercase();
        let length = head
            .lines()
            .find_map(|line| line.strip_prefix("content-length: "))
            .and_then(|value| value.trim().parse::<usize>().ok())
            .unwrap_or(0);
        while request.len() < split + length {
            let n = socket.read(&mut buffer).unwrap();
            request.extend_from_slice(&buffer[..n]);
        }
        let _ = tx.send(String::from_utf8_lossy(&request).into_owned());
        let _ = socket.write_all(
            concat!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n",
                "data: {\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}\n\n",
                "data: {\"choices\":[{\"delta\":{\"content\":\"lo\"},\"finish_reason\":\"stop\"}]}\n\n",
                "data: [DONE]\n\n"
            )
            .as_bytes(),
        );
    });
    (url, rx)
}

#[tokio::test]
async fn a_provider_run_calls_the_model_api_without_showing_the_key() {
    let dir = test_root("api-run");
    let (url, requests) = chat_server();
    for (words, stdin) in [
        (
            &[
                "provider",
                "add",
                "gw",
                "--kind",
                "openai-chat",
                "--base-url",
                url.as_str(),
                "--model",
                "fixture-model",
            ][..],
            "",
        ),
        (&["provider", "set-key", "gw"], KEY),
    ] {
        let output = call(&dir, words, stdin, false).await;
        assert_eq!(output.code, EXIT_OK, "{words:?}: {}", output.err);
    }
    let listed = call(&dir, &["provider", "list", "--json"], "", false).await;
    assert!(listed.out.contains("\"storedKey\": true"), "{}", listed.out);

    let output = call(
        &dir,
        &["run", "--provider", "gw", "--prompt", PROMPT, "--json"],
        "",
        false,
    )
    .await;
    assert_eq!(output.code, EXIT_OK, "{}", output.err);
    let events = ndjson(&output.out);
    assert_eq!(
        (&events[0]["type"], &events[0]["pid"]),
        (&json!("started"), &Value::Null)
    );
    let text: String = events
        .iter()
        .filter(|e| e["type"] == "text")
        .filter_map(|e| e["text"].as_str())
        .collect();
    assert_eq!(text, "Hello");
    assert_eq!(events.last().unwrap()["status"], "complete");

    let request = requests.recv_timeout(Duration::from_secs(3)).unwrap();
    assert!(request.to_ascii_lowercase().contains(&format!(
        "authorization: bearer {}",
        KEY.to_ascii_lowercase()
    )));
    assert!(request.contains("fixture-model") && request.contains(PROMPT));

    let records = records(&dir);
    let record: Value = serde_json::from_str(&records[0]).unwrap();
    assert_eq!(record["program"], "api");
    for text in [&output.out, &output.err, &listed.out, &records[0]] {
        assert!(!text.contains(KEY), "{text}");
    }
    assert!(!records[0].contains(PROMPT), "{}", records[0]);
}

#[tokio::test]
async fn a_provider_in_use_cannot_be_removed() {
    let dir = test_root("in-use");
    let mut agent = stub_agent(&dir, false);
    let added = call(
        &dir,
        &[
            "provider",
            "add",
            "gw",
            "--kind",
            "anthropic",
            "--base-url",
            "https://gateway.example",
        ],
        "",
        false,
    )
    .await;
    assert_eq!(added.code, EXIT_OK, "{}", added.err);
    agent.provider_id = Some("gw".into());
    write_versioned_json(
        &agent::agents_file(&dir),
        AGENT_STORE_SCHEMA_VERSION,
        &vec![agent],
    )
    .unwrap();
    let removed = call(&dir, &["provider", "remove", "gw"], "", false).await;
    assert_eq!(removed.code, EXIT_FAILED);
    assert!(removed.err.contains("used by agent"), "{}", removed.err);
}

#[test]
fn outcomes_map_to_exit_codes() {
    let outcome = |status: &str, reason: Option<&str>| AgentOutcome {
        status: status.into(),
        termination_reason: reason.map(str::to_owned),
        ..AgentOutcome::default()
    };
    assert_eq!(exit_code(&outcome("complete", None)), EXIT_OK);
    assert_eq!(exit_code(&outcome("error", None)), EXIT_FAILED);
    assert_eq!(
        exit_code(&outcome("aborted", Some(STOP_REASON_TIMEOUT))),
        EXIT_TIMEOUT
    );
    assert_eq!(
        exit_code(&outcome("aborted", Some(STOP_REASON_ABORT))),
        EXIT_INTERRUPTED
    );
}

#[test]
fn recorded_arguments_drop_the_prompt_and_long_text() {
    assert_eq!(record_arg("--model", PROMPT), "--model");
    assert_eq!(record_arg(&format!("say: {PROMPT}"), PROMPT), "<prompt>");
    assert_eq!(record_arg(&"x".repeat(300), PROMPT), "<300 chars>");
}
