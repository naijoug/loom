//! The API runner against a local fake model server: one request per test,
//! answered with scripted server-sent events.
use super::*;
use crate::{
    agent::{
        runner::{Limits, STOP_REASON_ABORT},
        AgentStage,
    },
    models::AgentConfig,
    process_supervisor::{ProcessKind, ProcessOwner},
};
use std::{
    io::{Read, Write},
    net::TcpListener,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
};

const KEY: &str = "fixture-provider-key-0123456789";

enum Reply {
    /// Status line and body, sent at once.
    Status(u16, &'static str),
    /// A 200 event stream: each chunk is written, then the server waits.
    Stream(Vec<(&'static str, u64)>),
    /// Accepts the connection and never answers.
    Hang,
}

struct Request {
    head: String,
    body: serde_json::Value,
}

fn server(reply: Reply) -> (String, mpsc::Receiver<Request>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
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
            assert!(n > 0, "request head ended early");
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
            assert!(n > 0, "request body ended early");
            request.extend_from_slice(&buffer[..n]);
        }
        let body = serde_json::from_slice(&request[split..split + length]).unwrap();
        let _ = tx.send(Request { head, body });
        match reply {
            Reply::Status(status, body) => {
                let response = format!(
                    "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = socket.write_all(response.as_bytes());
            }
            Reply::Stream(chunks) => {
                let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n";
                let _ = socket.write_all(head.as_bytes());
                for (chunk, wait_ms) in chunks {
                    if socket.write_all(chunk.as_bytes()).is_err() {
                        return;
                    }
                    let _ = socket.flush();
                    thread::sleep(Duration::from_millis(wait_ms));
                }
            }
            Reply::Hang => thread::sleep(Duration::from_secs(5)),
        }
    });
    (url, rx)
}

fn agent() -> AgentConfig {
    serde_json::from_value(serde_json::json!({
        "id":"agent-api","name":"Relay","command":"","args":[],"workingDirectoryPolicy":"project_root","capabilities":["planning"],"adapterType":"api","canWriteFiles":false,"canRunCommands":false,"enabled":true,"available":true,"providerId":"relay"
    }))
    .unwrap()
}

fn resolved(kind: ProviderKind, base_url: String, api_key: Option<&str>) -> ResolvedProvider {
    ResolvedProvider {
        kind,
        base_url,
        model: Some("fixture-model".into()),
        api_key: api_key.map(str::to_owned),
    }
}

fn request<'a>(prompt: &'a str) -> AdapterInvocationRequest<'a> {
    AdapterInvocationRequest {
        execution_profile: None,
        project_path: Path::new("/tmp"),
        prompt,
        prompt_file: None,
        stage: AgentStage::Planning,
        resume_command: None,
        chat_permission_mode: Some("explore"),
        embed_prompt: true,
        system_append: Some("Answer briefly."),
        model: None,
        needs_tools: false,
    }
}

fn spec(prepared: PreparedAgentInvocation, limits: Limits) -> RunSpec {
    RunSpec {
        prepared,
        run_id: crate::models::IdGenerator::default().next("api-run"),
        owner: ProcessOwner::Feature {
            feature: "fixture".into(),
            scope_id: "api".into(),
            run_id: "run".into(),
        },
        kind: ProcessKind::Feature,
        stdin_text: None,
        limits,
        enrich: None,
    }
}

fn limits() -> Limits {
    Limits {
        timeout: Duration::from_secs(5),
        ..Limits::default()
    }
}

/// Runs a prepared call through `agent::run`, the dispatch features use.
async fn run_with(
    prepared: PreparedAgentInvocation,
    limits: Limits,
    cancel: Option<Arc<AtomicBool>>,
) -> (AgentOutcome, Vec<AgentEvent>, Vec<String>) {
    let mut events = Vec::new();
    let mut logs = Vec::new();
    let outcome = crate::agent::run(
        spec(prepared, limits),
        || {
            cancel
                .as_ref()
                .is_some_and(|flag| flag.load(Ordering::SeqCst))
                .then_some(STOP_REASON_ABORT)
        },
        |event| {
            events.push(event);
            Ok(())
        },
        |observation| {
            if let Observation::Log { line, .. } = observation {
                logs.push(line);
            }
            Ok(())
        },
    )
    .await
    .unwrap();
    (outcome, events, logs)
}

fn texts(events: &[AgentEvent]) -> String {
    events
        .iter()
        .filter_map(|event| match event {
            AgentEvent::Text(text) => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn each_protocol_streams_text_with_its_own_request_and_credential() {
    let anthropic = vec![
        ("event: message_start\ndata: {\"type\":\"message_start\",\"message\":{}}\n\n", 0),
        ("event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hel\"}}\n\n", 20),
        ("data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"lo\"}}\n\n", 0),
        ("data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"}}\n\ndata: {\"type\":\"message_stop\"}\n\n", 0),
    ];
    let responses = vec![
        ("data: {\"type\":\"response.created\",\"response\":{}}\n\n", 0),
        ("data: {\"type\":\"response.output_text.delta\",\"delta\":\"Hel\"}\n\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"lo\"}\n\n", 20),
        ("data: {\"type\":\"response.completed\",\"response\":{}}\n\n", 0),
    ];
    let chat = vec![
        ("data: {\"choices\":[{\"delta\":{\"role\":\"assistant\",\"content\":\"Hel\"},\"finish_reason\":null}]}\n\n", 20),
        ("data: {\"choices\":[{\"delta\":{\"content\":\"lo\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n", 0),
    ];
    for (kind, base, chunks, path) in [
        (ProviderKind::Anthropic, "", anthropic, "post /v1/messages "),
        (
            ProviderKind::OpenaiResponses,
            "/v1",
            responses,
            "post /v1/responses ",
        ),
        (
            ProviderKind::OpenaiChat,
            "/v1",
            chat,
            "post /v1/chat/completions ",
        ),
    ] {
        let (url, received) = server(Reply::Stream(chunks));
        let provider = resolved(kind, format!("{url}{base}"), Some(KEY));
        let prepared = prepare(&agent(), &request("Say hello"), Some(&provider)).unwrap();
        assert_eq!(prepared.program, "api");
        assert_eq!(prepared.args[0], "POST");
        assert!(prepared.args[1].ends_with(path.trim().trim_start_matches("post ")));
        let visible = format!("{prepared:?} {}", serde_json::to_string(&prepared).unwrap());
        assert!(!visible.contains(KEY), "{visible}");
        assert!(!visible.contains("Say hello"), "{visible}");

        let (outcome, events, logs) = run_with(prepared, limits(), None).await;
        assert_eq!(
            outcome.status, "complete",
            "{kind:?} {:?}",
            outcome.error_summary
        );
        assert_eq!(outcome.content, "Hello");
        assert_eq!(texts(&events), "Hello");
        assert_eq!(events.last(), Some(&AgentEvent::Done));
        assert!(outcome.termination_reason.is_none());
        assert!(logs.iter().all(|line| !line.contains(KEY)));

        let request = received.recv().unwrap();
        assert!(request.head.starts_with(path), "{}", request.head);
        let key = KEY.to_ascii_lowercase();
        if kind == ProviderKind::Anthropic {
            assert!(request.head.contains(&format!("x-api-key: {key}")));
            assert!(request.head.contains("anthropic-version: 2023-06-01"));
            assert!(!request.head.contains("authorization:"));
            assert_eq!(request.body["system"], "Answer briefly.");
            assert_eq!(request.body["messages"][0]["content"], "Say hello");
            assert!(request.body["max_tokens"].is_u64());
        } else {
            assert!(request
                .head
                .contains(&format!("authorization: bearer {key}")));
            assert!(!request.head.contains("x-api-key"));
        }
        if kind == ProviderKind::OpenaiResponses {
            assert_eq!(request.body["instructions"], "Answer briefly.");
            assert_eq!(request.body["input"], "Say hello");
            assert_eq!(request.body["store"], false);
        }
        if kind == ProviderKind::OpenaiChat {
            assert_eq!(request.body["messages"][0]["role"], "system");
            assert_eq!(request.body["messages"][1]["content"], "Say hello");
        }
        assert_eq!(request.body["model"], "fixture-model");
        assert_eq!(request.body["stream"], true);
    }
}

#[tokio::test]
async fn a_local_gateway_needs_no_key_and_sends_no_credential() {
    let (url, received) = server(Reply::Stream(vec![(
        "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}\n\n",
        0,
    )]));
    let provider = resolved(ProviderKind::OpenaiChat, format!("{url}/v1"), None);
    let prepared = prepare(&agent(), &request("hi"), Some(&provider)).unwrap();
    let (outcome, _, _) = run_with(prepared, limits(), None).await;
    assert_eq!(outcome.content, "ok");
    assert!(!received.recv().unwrap().head.contains("authorization:"));

    let remote = provider_at("https://models.example.com/v1", None);
    let error = prepare(&agent(), &request("hi"), Some(&remote)).unwrap_err();
    assert!(error.contains("no API key"), "{error}");
}

fn provider_at(base_url: &str, api_key: Option<&str>) -> ResolvedProvider {
    resolved(ProviderKind::OpenaiChat, base_url.into(), api_key)
}

#[test]
fn features_that_need_tools_writes_or_resume_are_refused() {
    let provider = provider_at("https://models.example.com/v1", Some(KEY));
    let mut needs_tools = request("hi");
    needs_tools.needs_tools = true;
    let error = prepare(&agent(), &needs_tools, Some(&provider)).unwrap_err();
    assert!(error.contains("needs a local CLI agent"), "{error}");
    let mut writes = request("hi");
    writes.chat_permission_mode = Some("ask");
    assert!(prepare(&agent(), &writes, Some(&provider)).is_err());
    let mut resume = request("hi");
    resume.resume_command = Some("codex exec resume abc");
    assert!(prepare(&agent(), &resume, Some(&provider)).is_err());
    assert!(prepare(&agent(), &request("hi"), None).is_err());
    assert!(prepare(&agent(), &request("  "), Some(&provider)).is_err());

    let mut model = request("hi");
    model.model = Some("other-model");
    let prepared = prepare(&agent(), &model, Some(&provider)).unwrap();
    assert_eq!(prepared.args[3], "other-model");
    model.model = Some("bad model;");
    assert!(prepare(&agent(), &model, Some(&provider)).is_err());
}

#[tokio::test]
async fn a_stop_request_ends_the_stream_and_keeps_the_partial_answer() {
    let (url, _received) = server(Reply::Stream(vec![
        (
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"partial\"}\n\n",
            5_000,
        ),
        (
            "data: {\"type\":\"response.completed\",\"response\":{}}\n\n",
            0,
        ),
    ]));
    let provider = resolved(
        ProviderKind::OpenaiResponses,
        format!("{url}/v1"),
        Some(KEY),
    );
    let prepared = prepare(&agent(), &request("hi"), Some(&provider)).unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = cancel.clone();
    let mut texts = String::new();
    let started = Instant::now();
    let outcome = crate::agent::run(
        spec(prepared, limits()),
        || flag.load(Ordering::SeqCst).then_some(STOP_REASON_ABORT),
        |event| {
            if let AgentEvent::Text(text) = event {
                texts.push_str(&text);
                cancel.store(true, Ordering::SeqCst);
            }
            Ok(())
        },
        |_| Ok(()),
    )
    .await
    .unwrap();
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(texts, "partial");
    assert_eq!(outcome.status, "aborted");
    assert_eq!(outcome.content, "partial");
    assert_eq!(
        outcome.termination_reason.as_deref(),
        Some(STOP_REASON_ABORT)
    );
}

#[tokio::test]
async fn the_run_timeout_covers_a_provider_that_never_answers() {
    let (url, _received) = server(Reply::Hang);
    let provider = resolved(ProviderKind::Anthropic, url, Some(KEY));
    let prepared = prepare(&agent(), &request("hi"), Some(&provider)).unwrap();
    let started = Instant::now();
    let (outcome, _, _) = run_with(
        prepared,
        Limits {
            timeout: Duration::from_millis(300),
            ..Limits::default()
        },
        None,
    )
    .await;
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(outcome.status, "aborted");
    assert_eq!(
        outcome.termination_reason.as_deref(),
        Some(STOP_REASON_TIMEOUT)
    );
    assert!(outcome.error_summary.is_some());
}

#[tokio::test]
async fn truncation_by_the_model_or_the_output_budget_is_reported() {
    let (url, _received) = server(Reply::Stream(vec![(
        "data: {\"choices\":[{\"delta\":{\"content\":\"cut\"},\"finish_reason\":\"length\"}]}\n\n",
        0,
    )]));
    let provider = resolved(ProviderKind::OpenaiChat, format!("{url}/v1"), Some(KEY));
    let prepared = prepare(&agent(), &request("hi"), Some(&provider)).unwrap();
    let (outcome, events, _) = run_with(prepared, limits(), None).await;
    assert_eq!(outcome.status, "complete");
    assert_eq!(outcome.content, "cut");
    let truncated = |part: &MessagePart| matches!(part, MessagePart::Error { code: Some(code), .. } if code == "api_output_truncated");
    assert!(outcome.parts.iter().any(truncated));
    assert!(events
        .iter()
        .any(|event| matches!(event, AgentEvent::Part(part) if truncated(part))));

    let (url, _received) = server(Reply::Stream(vec![(
        "data: {\"choices\":[{\"delta\":{\"content\":\"0123456789abcdefghij\"},\"finish_reason\":null}]}\n\n",
        0,
    )]));
    let provider = provider_at(&format!("{url}/v1"), Some(KEY));
    let prepared = prepare(&agent(), &request("hi"), Some(&provider)).unwrap();
    let (outcome, events, _) = run_with(
        prepared,
        Limits {
            max_bytes: 32,
            ..limits()
        },
        None,
    )
    .await;
    assert_eq!(outcome.status, "error");
    assert_eq!(
        outcome.termination_reason.as_deref(),
        Some("chat_runtime_error")
    );
    assert!(outcome.error_summary.unwrap().starts_with("output_limit:"));
    assert!(!events.contains(&AgentEvent::Done));
}

#[tokio::test]
async fn failures_are_classified_without_the_upstream_body() {
    for (status, code, words) in [
        (401, "api_auth_failed", "rejected the API key (HTTP 401)"),
        (404, "api_not_found", "HTTP 404"),
        (429, "api_rate_limited", "HTTP 429"),
        (503, "api_server_error", "HTTP 503"),
    ] {
        let (url, _received) = server(Reply::Status(
            status,
            r#"{"error":{"message":"echo sk-upstream"}}"#,
        ));
        let provider = provider_at(&format!("{url}/v1"), Some(KEY));
        let prepared = prepare(&agent(), &request("hi"), Some(&provider)).unwrap();
        let (outcome, events, _) = run_with(prepared, limits(), None).await;
        let summary = outcome.error_summary.clone().unwrap();
        assert_eq!(outcome.status, "error");
        assert_eq!(outcome.termination_reason.as_deref(), Some(code));
        assert!(summary.contains(words), "{summary}");
        assert!(!format!("{:?}", outcome.parts).contains("sk-upstream"));
        assert_eq!(events.last(), Some(&AgentEvent::Done));
    }

    let (url, _received) = server(Reply::Stream(vec![(
        "event: error\ndata: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"echo sk-upstream\"}}\n\n",
        0,
    )]));
    let provider = resolved(ProviderKind::Anthropic, url, Some(KEY));
    let prepared = prepare(&agent(), &request("hi"), Some(&provider)).unwrap();
    let (outcome, _, _) = run_with(prepared, limits(), None).await;
    assert_eq!(
        outcome.termination_reason.as_deref(),
        Some("api_provider_error")
    );
    let summary = outcome.error_summary.unwrap();
    assert!(summary.contains("overloaded_error"), "{summary}");
    assert!(!summary.contains("sk-upstream"));

    let (url, _received) = server(Reply::Stream(vec![(
        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"half\"}\n\n",
        0,
    )]));
    let provider = resolved(
        ProviderKind::OpenaiResponses,
        format!("{url}/v1"),
        Some(KEY),
    );
    let prepared = prepare(&agent(), &request("hi"), Some(&provider)).unwrap();
    let (outcome, _, _) = run_with(prepared, limits(), None).await;
    assert_eq!(
        outcome.termination_reason.as_deref(),
        Some("api_incomplete_stream")
    );
    assert_eq!(outcome.content, "half");

    let (url, _received) = server(Reply::Stream(vec![("data: <html>\n\n", 0)]));
    let provider = provider_at(&format!("{url}/v1"), Some(KEY));
    let prepared = prepare(&agent(), &request("hi"), Some(&provider)).unwrap();
    let (outcome, _, _) = run_with(prepared, limits(), None).await;
    assert_eq!(
        outcome.termination_reason.as_deref(),
        Some("api_invalid_stream")
    );

    let closed = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/v1", closed.local_addr().unwrap());
    drop(closed);
    let provider = provider_at(&url, Some(KEY));
    let prepared = prepare(&agent(), &request("hi"), Some(&provider)).unwrap();
    let (outcome, _, _) = run_with(prepared, limits(), None).await;
    assert_eq!(
        outcome.termination_reason.as_deref(),
        Some("api_unreachable")
    );
    assert!(outcome.error_summary.unwrap().contains(&url));
}
