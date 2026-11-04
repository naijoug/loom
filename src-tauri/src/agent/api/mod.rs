//! Mode C: an `api` agent sends the prompt straight to a model provider from
//! `providers.json` and streams the answer back. It has no tools, file access
//! or native resume, so only features that declare `needs_tools = false` can
//! use it. Runs share the process runner's lifecycle (an operation guard, the
//! same stop reasons and `AgentOutcome` meaning) but register no process.
mod anthropic;
mod openai_chat;
mod openai_responses;
mod sse;

use super::{
    event::{AgentEvent, AgentOutcome, MessagePart},
    invocation::{validate_model_name, AdapterInvocationRequest, PreparedAgentInvocation},
    lifecycle,
    providers::{self, ProviderKind, ResolvedProvider},
    runner::{stop_outcome, Observation, RunSpec, STOP_REASON_SHUTDOWN, STOP_REASON_TIMEOUT},
};
use crate::models::AgentConfig;
use std::time::Duration;
use tokio::time::{interval, Instant};

/// Shown as the program of an API run in execution records.
const PROGRAM: &str = "api";
pub(super) const OUTPUT_MODE: &str = "api";

/// One request to a model API. The key is never serialized and `Debug` hides it.
#[derive(Clone)]
pub struct ApiCall {
    kind: ProviderKind,
    base_url: String,
    model: String,
    api_key: Option<String>,
    system: Option<String>,
    prompt: String,
}

impl std::fmt::Debug for ApiCall {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApiCall")
            .field("kind", &self.kind)
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("api_key", &self.api_key.as_ref().map(|_| "[REDACTED]"))
            .field("prompt_chars", &self.prompt.chars().count())
            .finish()
    }
}

/// One parsed stream event, independent of the protocol.
#[derive(Debug, PartialEq, Eq)]
enum Step {
    Text(String),
    /// The model stopped at its output limit.
    Truncated,
    Done,
    /// The provider reported an error; only its short code is kept.
    Failed(Option<String>),
    /// The event data is not JSON.
    Invalid,
}

/// Keeps an upstream error code only when it is a short identifier, so a
/// provider message can never reach the outcome through it.
fn error_code(value: &serde_json::Value) -> Option<String> {
    value
        .as_str()
        .filter(|code| {
            !code.is_empty()
                && code.len() <= 64
                && code
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
        })
        .map(str::to_owned)
}

fn endpoint(call: &ApiCall) -> String {
    match call.kind {
        ProviderKind::Anthropic => anthropic::endpoint(&call.base_url),
        ProviderKind::OpenaiResponses => openai_responses::endpoint(&call.base_url),
        ProviderKind::OpenaiChat => openai_chat::endpoint(&call.base_url),
    }
}

fn steps(kind: ProviderKind, event: &sse::Event) -> Vec<Step> {
    match kind {
        ProviderKind::Anthropic => anthropic::steps(event),
        ProviderKind::OpenaiResponses => openai_responses::steps(event),
        ProviderKind::OpenaiChat => openai_chat::steps(event),
    }
}

/// Builds the invocation of an `api` agent. `program` and `args` only describe
/// the request for execution records: the endpoint and model, never the key or
/// the prompt.
pub(super) fn prepare(
    agent: &AgentConfig,
    request: &AdapterInvocationRequest<'_>,
    provider: Option<&ResolvedProvider>,
) -> Result<PreparedAgentInvocation, String> {
    let provider = provider.ok_or("API agent has no model provider")?;
    if request.needs_tools || request.needs_write_access() {
        return Err(format!(
            "{} calls a model API directly and has no tools; this feature needs a local CLI agent",
            agent.name
        ));
    }
    if request.execution_profile.is_some() || request.resume_command.is_some() {
        return Err(format!("{} cannot resume a CLI session", agent.name));
    }
    if !agent.enabled {
        return Err(format!("{} is disabled", agent.name));
    }
    if !usable(provider) {
        return Err(format!(
            "{}'s model provider has no API key; add one in Settings",
            agent.name
        ));
    }
    if request.prompt.trim().is_empty() {
        return Err("prompt is empty".into());
    }
    let model = request
        .model
        .or(provider.model.as_deref())
        .ok_or("the model provider has no model; set one on the provider or the run")?;
    validate_model_name(model)?;
    let call = ApiCall {
        kind: provider.kind,
        base_url: provider.base_url.clone(),
        model: model.to_owned(),
        api_key: provider.api_key.clone(),
        system: request
            .system_append
            .filter(|text| !text.trim().is_empty())
            .map(str::to_owned),
        prompt: request.prompt.to_owned(),
    };
    Ok(PreparedAgentInvocation {
        environment: Default::default(),
        environment_remove_prefixes: Vec::new(),
        secret_environment: Default::default(),
        program: PROGRAM.into(),
        args: vec![
            "POST".into(),
            endpoint(&call),
            "--model".into(),
            call.model.clone(),
        ],
        cwd: request.project_path.to_string_lossy().into_owned(),
        stdin_prompt: false,
        output_mode: OUTPUT_MODE.into(),
        resumed: false,
        api: Some(call),
    })
}

/// Checks an `api` agent's provider without a network request: it exists, and
/// it has a key unless it is a gateway on this machine or network.
pub(super) fn provider_ready(agent: &AgentConfig) -> Result<(), String> {
    provider_ready_at(&crate::storage::system_config_dir()?, agent)
}

pub(super) fn provider_ready_at(dir: &std::path::Path, agent: &AgentConfig) -> Result<(), String> {
    let id = agent
        .provider_id
        .as_deref()
        .ok_or("API agent has no model provider")?;
    let provider = providers::resolve_at(dir, id, &providers::process_environment)?;
    if usable(&provider) {
        Ok(())
    } else {
        Err(format!("model provider '{id}' has no API key"))
    }
}

/// A provider needs a key unless it is a gateway on this machine or network.
pub(super) fn usable(provider: &ResolvedProvider) -> bool {
    provider.api_key.is_some() || providers::local_base_url(&provider.base_url)
}

fn request(client: &reqwest::Client, call: &ApiCall) -> Result<reqwest::RequestBuilder, String> {
    let body = match call.kind {
        ProviderKind::Anthropic => anthropic::body(call),
        ProviderKind::OpenaiResponses => openai_responses::body(call),
        ProviderKind::OpenaiChat => openai_chat::body(call),
    };
    let mut request = client
        .post(endpoint(call))
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(reqwest::header::ACCEPT, "text/event-stream")
        .body(serde_json::to_vec(&body).map_err(|_| "model API request is invalid")?);
    let invalid_key = "the model provider key cannot be sent as a header";
    if call.kind == ProviderKind::Anthropic {
        request = request.header("anthropic-version", "2023-06-01");
        if let Some(key) = &call.api_key {
            request = request.header("x-api-key", providers::sensitive(key).ok_or(invalid_key)?);
        }
    } else if let Some(key) = &call.api_key {
        let value = providers::sensitive(&format!("Bearer {key}")).ok_or(invalid_key)?;
        request = request.header(reqwest::header::AUTHORIZATION, value);
    }
    Ok(request)
}

/// Readable summary and termination code for a non-success HTTP status. The
/// response body is never read, so nothing the provider echoes is shown.
fn http_failure(status: u16) -> (String, &'static str) {
    let (what, code) = match status {
        400 | 422 => (
            "rejected the request; check the model name",
            "api_bad_request",
        ),
        401 | 403 => ("rejected the API key", "api_auth_failed"),
        404 => ("has no such model or endpoint", "api_not_found"),
        413 => ("rejected the request as too large", "api_request_too_large"),
        429 => ("is rate limiting or out of quota", "api_rate_limited"),
        500..=599 => ("had a server error", "api_server_error"),
        _ => ("returned an unexpected status", "api_http_error"),
    };
    (format!("The model provider {what} (HTTP {status})"), code)
}

fn failed(
    content: String,
    mut parts: Vec<MessagePart>,
    summary: String,
    code: &str,
) -> AgentOutcome {
    parts.push(MessagePart::Error {
        message: summary.clone(),
        code: Some(code.into()),
    });
    AgentOutcome {
        content,
        parts,
        resume_command: None,
        status: "error".into(),
        error_summary: Some(summary),
        exit_code: None,
        termination_reason: Some(code.into()),
    }
}

/// Emits `Done` as the process runner does once the run's output is complete,
/// even when the provider failed; a failing `emit` turns into a runtime error.
fn finish(
    emit: &mut impl FnMut(AgentEvent) -> Result<(), String>,
    outcome: AgentOutcome,
) -> AgentOutcome {
    match emit(AgentEvent::Done) {
        Ok(()) => outcome,
        Err(error) => failed(outcome.content, outcome.parts, error, "chat_runtime_error"),
    }
}

fn runtime_error(content: String, parts: Vec<MessagePart>, error: String) -> AgentOutcome {
    failed(content, parts, error, "chat_runtime_error")
}

/// Runs one API request with the process runner's contract: `cancelled` is
/// polled while connecting and between chunks, the run's timeout and output
/// budgets apply, and an `emit` or `observe` failure ends it as an error.
pub(super) async fn run(
    spec: RunSpec,
    call: ApiCall,
    cancelled: impl Fn() -> Option<&'static str>,
    mut emit: impl FnMut(AgentEvent) -> Result<(), String>,
    mut observe: impl FnMut(Observation) -> Result<(), String>,
) -> Result<AgentOutcome, String> {
    if let Some(reason) = cancelled() {
        return Ok(stop_outcome(Some(reason), String::new(), Vec::new()));
    }
    // Shutdown waits for this guard, so the request ends before Loom exits.
    let _operation = match lifecycle::begin_operation() {
        Ok(operation) => operation,
        Err(_) if lifecycle::is_shutting_down() => {
            return Ok(stop_outcome(
                Some(STOP_REASON_SHUTDOWN),
                String::new(),
                Vec::new(),
            ))
        }
        Err(error) => return Err(error),
    };
    let limits = spec.limits;
    let started = Instant::now();
    let stop = || {
        cancelled()
            .or_else(|| lifecycle::is_shutting_down().then_some(STOP_REASON_SHUTDOWN))
            .or_else(|| (started.elapsed() >= limits.timeout).then_some(STOP_REASON_TIMEOUT))
    };
    let Ok(client) = providers::http_client(&call.base_url, None) else {
        let error = "could not create the HTTP client".to_string();
        return Ok(runtime_error(String::new(), Vec::new(), error));
    };
    let request = match request(&client, &call) {
        Ok(request) => request,
        Err(error) => return Ok(runtime_error(String::new(), Vec::new(), error)),
    };
    let mut log = |line: String| observe(Observation::Log { stderr: true, line });
    if let Err(error) = log(format!(
        "[api] POST {} model={}",
        endpoint(&call),
        call.model
    )) {
        return Ok(runtime_error(String::new(), Vec::new(), error));
    }
    let mut tick = interval(Duration::from_millis(25));
    let send = request.send();
    tokio::pin!(send);
    let sent = loop {
        if let Some(reason) = stop() {
            return Ok(stop_outcome(Some(reason), String::new(), Vec::new()));
        }
        tokio::select! {
            sent = &mut send => break sent,
            _ = tick.tick() => {}
        }
    };
    let Ok(mut response) = sent else {
        let summary = format!("Could not reach the model provider at {}", call.base_url);
        let outcome = failed(String::new(), Vec::new(), summary, "api_unreachable");
        return Ok(finish(&mut emit, outcome));
    };
    let status = response.status().as_u16();
    if let Err(error) = log(format!("[api] HTTP {status}")) {
        return Ok(runtime_error(String::new(), Vec::new(), error));
    }
    if !response.status().is_success() {
        let (summary, code) = http_failure(status);
        let outcome = failed(String::new(), Vec::new(), summary, code);
        return Ok(finish(&mut emit, outcome));
    }

    let mut decoder = sse::Decoder::default();
    let (mut content, mut parts) = (String::new(), Vec::new());
    let (mut bytes, mut events) = (0usize, 0usize);
    let (mut truncated, mut done, mut ended) = (false, false, false);
    let mut provider_error: Option<Option<String>> = None;
    while !done && !ended && provider_error.is_none() {
        if let Some(reason) = stop() {
            return Ok(stop_outcome(Some(reason), content, parts));
        }
        // `chunk()` only yields data it has fully read, so losing the race
        // to the tick drops nothing.
        let chunk = tokio::select! {
            chunk = response.chunk() => chunk,
            _ = tick.tick() => continue,
        };
        let decoded = match chunk {
            Ok(Some(chunk)) => {
                bytes += chunk.len();
                if bytes > limits.max_bytes {
                    let error = "output_limit: model API exceeded the turn output budget";
                    return Ok(runtime_error(content, parts, error.into()));
                }
                match decoder.feed(&chunk, limits.max_line_bytes) {
                    Ok(decoded) => decoded,
                    Err(error) => return Ok(runtime_error(content, parts, error)),
                }
            }
            Ok(None) => {
                ended = true;
                decoder.finish().into_iter().collect()
            }
            Err(_) => {
                let summary = "The connection to the model provider was interrupted".to_string();
                let outcome = failed(content, parts, summary, "api_stream_interrupted");
                return Ok(finish(&mut emit, outcome));
            }
        };
        'events: for event in decoded {
            events += 1;
            if events > limits.max_lines {
                let error = "output_limit: model API sent too many events".to_string();
                return Ok(runtime_error(content, parts, error));
            }
            for step in steps(call.kind, &event) {
                match step {
                    Step::Text(text) => {
                        content.push_str(&text);
                        if let Err(error) = emit(AgentEvent::Text(text)) {
                            return Ok(runtime_error(content, parts, error));
                        }
                    }
                    Step::Truncated => truncated = true,
                    Step::Done => done = true,
                    Step::Failed(code) => provider_error = Some(code),
                    Step::Invalid => {
                        let summary = "The model provider sent an unreadable stream".to_string();
                        let outcome = failed(content, parts, summary, "api_invalid_stream");
                        return Ok(finish(&mut emit, outcome));
                    }
                }
                if done || provider_error.is_some() {
                    break 'events;
                }
            }
        }
    }
    // Closes the connection: anything after the end of the answer is ignored.
    drop(response);
    if let Some(code) = provider_error {
        let summary = match code {
            Some(code) => format!("The model provider reported an error ({code})"),
            None => "The model provider reported an error".to_string(),
        };
        let outcome = failed(content, parts, summary, "api_provider_error");
        return Ok(finish(&mut emit, outcome));
    }
    if !done {
        let summary = "The model API stream ended before the answer was complete".to_string();
        let outcome = failed(content, parts, summary, "api_incomplete_stream");
        return Ok(finish(&mut emit, outcome));
    }
    if truncated {
        let part = MessagePart::Error {
            message: "The model stopped at its output limit; the answer may be incomplete".into(),
            code: Some("api_output_truncated".into()),
        };
        parts.push(part.clone());
        if let Err(error) = emit(AgentEvent::Part(part)) {
            return Ok(runtime_error(content, parts, error));
        }
    }
    let outcome = AgentOutcome {
        content: if content.is_empty() {
            "（Agent 没有返回可见文本）".into()
        } else {
            content
        },
        parts,
        resume_command: None,
        status: "complete".into(),
        error_summary: None,
        exit_code: None,
        termination_reason: None,
    };
    Ok(finish(&mut emit, outcome))
}

#[cfg(test)]
mod tests;
