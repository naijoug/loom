//! The production agent CLI runner. No Tauri dependency: tests drive real
//! processes through this same cancellation, output and cleanup boundary.
//! Feature modules choose the process owner and map `AgentEvent` to their own
//! events; spawn, stdin, limits, timeout, cancellation and process-group
//! cleanup live here.
use super::{
    error::AgentError,
    event::{AgentEvent, AgentOutcome, MessagePart},
    invocation::PreparedAgentInvocation,
    stream::{block_separator, parse_line, redact_stream_line},
    tools::merge_part,
    ADAPTER_CLAUDE, ADAPTER_CODEX, ADAPTER_GROK, ADAPTER_OPENCODE,
};
use crate::{
    process_supervisor::{self, ProcessKind, ProcessMetadata, ProcessOwner},
    session_capture,
};
use std::{
    process::{ExitStatus, Stdio},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
    time::{interval, Instant},
};

/// Default wall-clock budget for one run, also Chat's turn timeout. On expiry
/// the run is stopped via `ProcessSupervisor::request_stop(..., "chat_timeout")`
/// so a Chat session becomes sendable again with an aborted message.
pub const DEFAULT_TIMEOUT_MS: u64 = 10 * 60 * 1000;
// Persisted termination reasons. The `chat_` prefix predates the agent layer
// and is kept so stored turns keep their meaning.
pub const STOP_REASON_ABORT: &str = "chat_abort";
pub const STOP_REASON_TIMEOUT: &str = "chat_timeout";
pub const STOP_REASON_SHUTDOWN: &str = "app_shutdown";

#[derive(Clone, Copy)]
pub(crate) struct Limits {
    pub timeout: Duration,
    pub terminate_grace: Duration,
    pub drain_grace: Duration,
    pub reap_grace: Duration,
    pub max_bytes: usize,
    pub max_line_bytes: usize,
    pub max_lines: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            timeout: Duration::from_millis(DEFAULT_TIMEOUT_MS),
            terminate_grace: Duration::from_millis(1_000),
            drain_grace: Duration::from_millis(300),
            reap_grace: Duration::from_secs(2),
            max_bytes: 8 * 1024 * 1024,
            max_line_bytes: 512 * 1024,
            max_lines: 20_000,
        }
    }
}

/// Adds feature-specific detail to the parts parsed from one stdout line
/// (Chat attaches file diffs to writable Codex turns).
pub(crate) type LineEnricher = Box<dyn FnMut(&str, &mut [MessagePart]) + Send>;

pub(crate) struct RunSpec {
    pub prepared: PreparedAgentInvocation,
    /// Process registry id: the Chat turn id or the Learn job attempt id.
    pub run_id: String,
    pub owner: ProcessOwner,
    pub kind: ProcessKind,
    pub stdin_text: Option<String>,
    pub limits: Limits,
    pub enrich: Option<LineEnricher>,
}

pub(crate) enum Observation {
    Started(u32),
    Log { stderr: bool, line: String },
}

struct ProcessGuard {
    pid: u32,
    run_id: String,
    registered: bool,
}

struct InputWriter(tokio::task::JoinHandle<std::io::Result<()>>);
impl Drop for InputWriter {
    fn drop(&mut self) {
        self.0.abort();
    }
}
impl Drop for ProcessGuard {
    fn drop(&mut self) {
        // Also runs if the runtime future is dropped while its parent exits.
        if self.pid > 0 {
            let _ = process_supervisor::force_terminate_process_group(self.pid);
        }
        if self.registered {
            process_supervisor::supervisor().complete(&self.run_id);
        }
    }
}

#[derive(Default)]
struct Lines {
    pending: Vec<u8>,
}
impl Lines {
    fn feed(&mut self, bytes: &[u8], eof: bool, limit: usize) -> Result<Vec<String>, String> {
        self.pending.extend_from_slice(bytes);
        let mut start = 0;
        let mut lines = Vec::new();
        for (end, byte) in self.pending.iter().enumerate() {
            if *byte == b'\n' {
                if end - start > limit {
                    return Err("output_limit: CLI line is too large".into());
                }
                let text = std::str::from_utf8(&self.pending[start..end])
                    .map_err(|_| "invalid_utf8: CLI output is not UTF-8")?;
                lines.push(text.trim_end_matches('\r').to_string());
                start = end + 1;
            }
        }
        self.pending.drain(..start);
        if self.pending.len() > limit {
            return Err("output_limit: CLI line is too large".into());
        }
        if eof && !self.pending.is_empty() {
            let text = std::str::from_utf8(&self.pending)
                .map_err(|_| "invalid_utf8: CLI output is not UTF-8")?;
            lines.push(text.to_string());
            self.pending.clear();
        }
        Ok(lines)
    }
}

#[derive(Default)]
struct Output {
    reported_error: Option<String>,
    enrich: Option<LineEnricher>,
    streamed_text: bool,
    block_pending: bool,
    bytes: usize,
    lines: usize,
    stdout: Vec<String>,
    stderr: Vec<String>,
    content: String,
    parts: Vec<MessagePart>,
}

impl Output {
    fn receive(
        &mut self,
        spec: &RunSpec,
        frame: &mut Lines,
        chunk: (&[u8], bool),
        stderr: bool,
        emit: &mut impl FnMut(AgentEvent) -> Result<(), String>,
        observe: &mut impl FnMut(Observation) -> Result<(), String>,
    ) -> Result<(), String> {
        let (bytes, eof) = chunk;
        self.bytes += bytes.len();
        if self.bytes > spec.limits.max_bytes {
            return Err("output_limit: CLI exceeded the turn output budget".into());
        }
        for line in frame.feed(bytes, eof, spec.limits.max_line_bytes)? {
            self.lines += 1;
            if self.lines > spec.limits.max_lines {
                return Err("output_limit: CLI emitted too many lines".into());
            }
            let line = redact_stream_line(&line);
            observe(Observation::Log {
                stderr,
                line: line.clone(),
            })?;
            if stderr {
                self.stderr.push(line);
                continue;
            }
            let mut parsed = parse_line(&spec.prepared.output_mode, &line);
            if let Some(warning) = &parsed.warning {
                observe(Observation::Log {
                    stderr: true,
                    line: format!("[warning] {warning}"),
                })?;
            }
            if let Some(enrich) = &mut self.enrich {
                enrich(&line, &mut parsed.parts);
            }
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) {
                if value["type"] == "stream_event"
                    && value.pointer("/event/delta/type").and_then(|v| v.as_str())
                        == Some("text_delta")
                {
                    self.streamed_text = true;
                } else if value["type"] == "assistant" && !parsed.delta.is_empty() {
                    if self.streamed_text {
                        parsed.delta.clear();
                    }
                    self.streamed_text = false;
                }
            }
            self.stdout.push(line);
            for part in parsed.parts {
                if let MessagePart::Error {
                    message,
                    code: Some(code),
                } = &part
                {
                    if code == "cli_reported_error" {
                        self.reported_error = Some(message.clone());
                    }
                }
                if self.parts.len() >= 1_000 {
                    return Err("output_limit: CLI emitted too many tool parts".into());
                }
                merge_part(&mut self.parts, part.clone());
                emit(AgentEvent::Part(part))?;
            }
            self.block_pending |= parsed.block_start;
            if !parsed.delta.is_empty() {
                if std::mem::take(&mut self.block_pending) {
                    parsed.delta.insert_str(0, block_separator(&self.content));
                }
                self.content.push_str(&parsed.delta);
                emit(AgentEvent::Text(parsed.delta))?;
            }
        }
        Ok(())
    }
}

/// True when ProcessSupervisor recorded a cooperative stop (user abort or turn timeout).
pub(crate) fn is_stop_reason(reason: Option<&str>) -> bool {
    matches!(
        reason,
        Some(STOP_REASON_ABORT) | Some(STOP_REASON_TIMEOUT) | Some(STOP_REASON_SHUTDOWN)
    )
}

pub(crate) fn timeout_summary() -> String {
    let minutes = DEFAULT_TIMEOUT_MS / 60_000;
    format!("回合超时（超过 {minutes} 分钟，已自动停止）")
}

pub(crate) fn stop_outcome(
    reason: Option<&str>,
    content: String,
    mut parts: Vec<MessagePart>,
) -> AgentOutcome {
    let timed_out = reason == Some(STOP_REASON_TIMEOUT);
    let error_summary = if timed_out {
        Some(timeout_summary())
    } else {
        None
    };
    if let Some(summary) = error_summary.as_ref() {
        parts.push(MessagePart::Error {
            message: summary.clone(),
            code: Some("chat_timeout".to_string()),
        });
    }
    let content = if content.trim().is_empty() {
        if timed_out {
            format!("（已超时）{}", error_summary.as_deref().unwrap_or(""))
        } else {
            "（已停止）".to_string()
        }
    } else {
        content
    };
    AgentOutcome {
        content,
        parts,
        resume_command: None,
        status: "aborted".to_string(),
        error_summary,
        exit_code: None,
        termination_reason: Some(reason.unwrap_or(STOP_REASON_ABORT).into()),
    }
}

/// Runs one prepared invocation to completion. `cancelled` is polled for a
/// stop reason; an `emit` or `observe` failure stops the process and becomes
/// an error outcome.
pub(crate) async fn run(
    mut spec: RunSpec,
    cancelled: impl Fn() -> Option<&'static str>,
    mut emit: impl FnMut(AgentEvent) -> Result<(), String>,
    mut observe: impl FnMut(Observation) -> Result<(), String>,
) -> Result<AgentOutcome, String> {
    if let Some(call) = spec.prepared.api.take() {
        return super::api::run(spec, call, cancelled, emit, observe).await;
    }
    if let Some(reason) = cancelled() {
        return Ok(stop_outcome(Some(reason), String::new(), Vec::new()));
    }
    let mut operation = match process_supervisor::supervisor().begin_operation() {
        Ok(operation) => operation,
        Err(_) if process_supervisor::supervisor().is_shutting_down() => {
            return Ok(stop_outcome(
                Some(STOP_REASON_SHUTDOWN),
                String::new(),
                Vec::new(),
            ))
        }
        Err(error) => return Err(error),
    };
    if spec.prepared.stdin_prompt && spec.stdin_text.is_none() {
        return Err("stdin prompt was requested but no input was supplied".into());
    }
    if let Some(reason) = cancelled() {
        return Ok(stop_outcome(Some(reason), String::new(), Vec::new()));
    }
    let mut command = Command::new(&spec.prepared.program);
    let prefixes = &spec.prepared.environment_remove_prefixes;
    for (name, _) in std::env::vars_os() {
        if name.to_str().is_some_and(|name| {
            prefixes
                .iter()
                .any(|prefix| name.starts_with(prefix.as_str()))
        }) {
            command.env_remove(name);
        }
    }
    command
        .args(&spec.prepared.args)
        .envs(&spec.prepared.environment)
        .envs(spec.prepared.secret_environment.iter())
        .current_dir(&spec.prepared.cwd)
        .kill_on_drop(true)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(if spec.stdin_text.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        });
    process_supervisor::configure_process_group(&mut command);
    let mut child = command
        .spawn()
        .map_err(|error| format!("failed to start agent: {error}"))?;
    let pid = child.id().ok_or("agent process id missing")?;
    let mut guard = ProcessGuard {
        pid,
        run_id: spec.run_id.clone(),
        registered: false,
    };
    let meta = ProcessMetadata::owned(
        &spec.run_id,
        spec.owner.clone(),
        spec.kind,
        pid,
        spec.limits.timeout.as_millis().min(u64::MAX as u128) as u64,
    );
    if let Err(error) = operation.register(meta) {
        process_supervisor::force_terminate_process_group(pid)?;
        let _ = child.wait().await;
        return Err(error);
    }
    guard.registered = true;
    observe(Observation::Started(pid))?;
    let mut stdout = child.stdout.take().ok_or("agent stdout missing")?;
    let mut stderr = child.stderr.take().ok_or("agent stderr missing")?;
    let mut input_task = spec.stdin_text.as_ref().map(|text| {
        let text = text.clone();
        let mut stdin = child.stdin.take().expect("stdin configured above");
        InputWriter(tokio::spawn(async move {
            stdin.write_all(text.as_bytes()).await?;
            stdin.shutdown().await
        }))
    });
    let mut output = Output {
        enrich: spec.enrich.take(),
        ..Default::default()
    };
    let (mut out_frame, mut err_frame) = (Lines::default(), Lines::default());
    let (mut out_buf, mut err_buf) = ([0u8; 8192], [0u8; 8192]);
    let (mut out_eof, mut err_eof) = (false, false);
    let mut status: Option<ExitStatus> = None;
    let started = Instant::now();
    let mut exited_at: Option<Instant> = None;
    let mut stop_at: Option<Instant> = None;
    let mut killed_at: Option<Instant> = None;
    let mut reason: Option<String> = None;
    let mut failure: Option<String> = None;
    let mut tick = interval(Duration::from_millis(25));
    loop {
        let now = Instant::now();
        if stop_at.is_none() {
            let requested = process_supervisor::supervisor()
                .metadata(&spec.run_id)
                .and_then(|meta| meta.termination_reason);
            reason = if let Some(reason) = cancelled() {
                Some(reason.into())
            } else if requested.is_some() {
                requested
            } else if now.duration_since(started) >= spec.limits.timeout {
                Some(STOP_REASON_TIMEOUT.into())
            } else if failure.is_some() {
                Some("chat_io_error".into())
            } else if exited_at.is_some_and(|at| now.duration_since(at) >= spec.limits.drain_grace)
                && (!out_eof || !err_eof)
            {
                Some("chat_descendant_cleanup".into())
            } else {
                None
            };
            if let Some(why) = reason.as_deref() {
                process_supervisor::supervisor().request_stop(&spec.run_id, why)?;
                stop_at = Some(now);
                if let Some(task) = input_task.take() {
                    task.0.abort();
                }
            }
        }
        if stop_at.is_some_and(|at| now.duration_since(at) >= spec.limits.terminate_grace)
            && killed_at.is_none()
        {
            process_supervisor::force_terminate_process_group(pid)?;
            killed_at = Some(now);
        }
        if status.is_some() && out_eof && err_eof && input_task.is_none() {
            break;
        }
        if killed_at.is_some_and(|at| now.duration_since(at) >= spec.limits.reap_grace) {
            failure.get_or_insert_with(|| {
                "cleanup_timeout: agent or descendant did not release its output pipes".into()
            });
            if status.is_none() {
                let _ = child.start_kill();
                let _ = tokio::time::timeout(Duration::from_millis(250), child.wait()).await;
            }
            break;
        }
        tokio::select! {
            _ = tick.tick() => {}
            read = stdout.read(&mut out_buf), if !out_eof => {
                match read {
                    Ok(n) => {
                        out_eof = n == 0;
                        if failure.is_none() {
                            if let Err(error) = output.receive(&spec, &mut out_frame, (&out_buf[..n], out_eof), false, &mut emit, &mut observe) { failure = Some(error); }
                        }
                    }
                    Err(error) => { out_eof = true; failure.get_or_insert_with(|| format!("stdout read failed: {error}")); }
                }
            }
            read = stderr.read(&mut err_buf), if !err_eof => {
                match read {
                    Ok(n) => {
                        err_eof = n == 0;
                        if failure.is_none() {
                            if let Err(error) = output.receive(&spec, &mut err_frame, (&err_buf[..n], err_eof), true, &mut emit, &mut observe) { failure = Some(error); }
                        }
                    }
                    Err(error) => { err_eof = true; failure.get_or_insert_with(|| format!("stderr read failed: {error}")); }
                }
            }
            result = child.wait(), if status.is_none() => {
                status = Some(result.map_err(|error| format!("failed waiting for agent: {error}"))?);
                exited_at = Some(Instant::now());
            }
            result = async { (&mut input_task.as_mut().expect("guarded input task").0).await }, if input_task.is_some() => {
                input_task.take();
                match result {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => { failure = Some(format!("stdin write failed: {error}")); }
                    Err(error) => { failure = Some(format!("stdin writer failed: {error}")); }
                }
            }
        }
    }
    // Descendants may close the pipes while continuing to run. Clean the group
    // on every outcome, including normal parent exit, before releasing its id.
    // The loop only exits here after wait() reaped the parent, so tolerate the
    // brief zombie-only group macOS rejects signals for.
    if killed_at.is_none() {
        process_supervisor::cleanup_reaped_group(pid).await?;
    }
    if let Some(task) = input_task.take() {
        task.0.abort();
    }
    process_supervisor::supervisor().complete(&spec.run_id);
    guard.registered = false;
    // Disarm the guard after cleanup so a later PID reuse is never signalled.
    guard.pid = 0;
    if failure.is_none() {
        if let Err(error) = emit(AgentEvent::Done) {
            failure = Some(error);
        }
    }
    if let Some(error) = failure {
        output.parts.push(MessagePart::Error {
            message: error.clone(),
            code: Some("chat_runtime_error".into()),
        });
        return Ok(AgentOutcome {
            content: output.content,
            parts: output.parts,
            resume_command: None,
            status: "error".into(),
            error_summary: Some(error),
            exit_code: status.as_ref().and_then(ExitStatus::code),
            termination_reason: Some("chat_io_error".into()),
        });
    }
    if is_stop_reason(reason.as_deref()) {
        let mut outcome = stop_outcome(reason.as_deref(), output.content, output.parts);
        outcome.exit_code = status.as_ref().and_then(ExitStatus::code);
        return Ok(outcome);
    }
    let status = status.ok_or("agent exit status missing")?;
    if !status.success() {
        // Startup warnings can be long; the fatal CLI diagnostic is usually at
        // the end. Keep the bounded tail so it is not hidden by plugin notices.
        let diagnostic: String = output
            .stderr
            .join("\n")
            .chars()
            .rev()
            .take(2_000)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let summary = status
            .code()
            .map(|code| format!("agent exited with code {code}"))
            .unwrap_or_else(|| "agent was stopped by a signal".into());
        let error = AgentError::new(summary)
            .with_details(diagnostic)
            .to_string();
        output.parts.push(MessagePart::Error {
            message: error.clone(),
            code: Some("agent_exit_failed".into()),
        });
        return Ok(AgentOutcome {
            content: output.content,
            parts: output.parts,
            resume_command: None,
            status: "error".into(),
            error_summary: Some(error),
            exit_code: status.code(),
            termination_reason: Some(status.to_string()),
        });
    }
    if let Some(error) = output.reported_error {
        return Ok(AgentOutcome {
            content: output.content,
            parts: output.parts,
            resume_command: None,
            status: "error".into(),
            error_summary: Some(error),
            exit_code: status.code(),
            termination_reason: Some("agent_reported_error".into()),
        });
    }
    let adapter = match spec.prepared.output_mode.as_str() {
        "codex_json" => ADAPTER_CODEX,
        "claude_stream_json" => ADAPTER_CLAUDE,
        "streaming_json" => ADAPTER_GROK,
        "opencode_json" => ADAPTER_OPENCODE,
        _ => "cli",
    };
    let captured = session_capture::capture_session_for_adapter(
        adapter,
        &spec.prepared.program,
        &output.stdout,
        &output.stderr,
    );
    Ok(AgentOutcome {
        content: if output.content.is_empty() {
            "（Agent 没有返回可见文本）".into()
        } else {
            output.content
        },
        parts: output.parts,
        resume_command: captured.resume_command,
        status: "complete".into(),
        error_summary: None,
        exit_code: status.code(),
        termination_reason: None,
    })
}

#[cfg(test)]
mod stop_tests {
    use super::*;

    #[test]
    fn default_timeout_is_within_the_chat_product_window() {
        // Plan: sensible default 5–10 minutes.
        const {
            assert!(DEFAULT_TIMEOUT_MS >= 5 * 60 * 1000);
            assert!(DEFAULT_TIMEOUT_MS <= 10 * 60 * 1000);
        }
    }

    #[test]
    fn stop_reason_covers_abort_and_timeout() {
        assert!(is_stop_reason(Some("chat_abort")));
        assert!(is_stop_reason(Some("chat_timeout")));
        assert!(!is_stop_reason(Some("other")));
        assert!(!is_stop_reason(None));
    }

    #[test]
    fn timeout_summary_is_readable() {
        let summary = timeout_summary();
        assert!(summary.contains("超时"), "{summary}");
        assert!(summary.contains("自动停止"), "{summary}");
    }

    #[test]
    fn stop_outcome_marks_timeout_aborted_with_error() {
        let outcome = stop_outcome(Some("chat_timeout"), String::new(), Vec::new());
        assert_eq!(outcome.status, "aborted");
        assert!(outcome
            .error_summary
            .as_deref()
            .unwrap_or("")
            .contains("超时"));
        assert!(outcome.content.contains("超时"));
        assert!(matches!(
            outcome.parts.first(),
            Some(MessagePart::Error { code: Some(code), .. }) if code == "chat_timeout"
        ));
    }

    #[test]
    fn stop_outcome_user_abort_keeps_simple_stopped_copy() {
        let outcome = stop_outcome(Some("chat_abort"), String::new(), Vec::new());
        assert_eq!(outcome.status, "aborted");
        assert!(outcome.error_summary.is_none());
        assert_eq!(outcome.content, "（已停止）");
    }
}
