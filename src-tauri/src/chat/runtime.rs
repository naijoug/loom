//! The production CLI runner. No Tauri dependency: tests drive real processes
//! through this same cancellation, output and cleanup boundary.
use super::{
    chat_stop_outcome, parse_chat_stream_line, ChatMessagePart, ChatStreamEvent, ChatTurnOutcome,
    CHAT_TURN_TIMEOUT_MS, STOP_REASON_TIMEOUT,
};
use crate::{
    agent_adapter::PreparedAgentInvocation,
    agents,
    process_supervisor::{self, ProcessMetadata},
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

#[derive(Clone, Copy)]
pub(super) struct Limits {
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
            timeout: Duration::from_millis(CHAT_TURN_TIMEOUT_MS),
            terminate_grace: Duration::from_millis(1_000),
            drain_grace: Duration::from_millis(300),
            reap_grace: Duration::from_secs(2),
            max_bytes: 8 * 1024 * 1024,
            max_line_bytes: 512 * 1024,
            max_lines: 20_000,
        }
    }
}

pub(super) struct Request {
    pub prepared: PreparedAgentInvocation,
    pub project_key: String,
    pub session_id: String,
    pub turn_id: String,
    pub message_id: String,
    pub stdin_text: Option<String>,
    pub limits: Limits,
}

pub(super) enum Observation {
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

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::{models::IdGenerator, process_supervisor::ProcessOwner};
    use std::{
        fs,
        path::PathBuf,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
    };

    struct Fixture {
        path: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(IdGenerator::default().next("loom-chat-runtime"));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }
        fn request(&self, mode: &str) -> Request {
            Request {
                prepared: PreparedAgentInvocation {
                    program: "/bin/sh".into(),
                    args: vec![
                        format!(
                            "{}/../tests/fixtures/chat/fake-agent.sh",
                            env!("CARGO_MANIFEST_DIR")
                        ),
                        mode.into(),
                    ],
                    cwd: self.path.to_string_lossy().into_owned(),
                    stdin_prompt: false,
                    output_mode: "plain".into(),
                    resumed: false,
                },
                project_key: self.path.to_string_lossy().into_owned(),
                session_id: IdGenerator::default().next("session"),
                turn_id: IdGenerator::default().next("turn"),
                message_id: "message-fixture".into(),
                stdin_text: None,
                limits: Limits {
                    timeout: Duration::from_secs(5),
                    terminate_grace: Duration::from_millis(80),
                    drain_grace: Duration::from_millis(50),
                    reap_grace: Duration::from_millis(400),
                    ..Limits::default()
                },
            }
        }
        async fn assert_descendant_gone(&self) {
            let pid: i32 = fs::read_to_string(self.path.join("child.pid"))
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            unsafe extern "C" {
                fn kill(pid: i32, signal: i32) -> i32;
            }
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                let result = unsafe { kill(pid, 0) };
                if result == -1 && std::io::Error::last_os_error().raw_os_error() == Some(3) {
                    break;
                }
                assert!(Instant::now() < deadline, "descendant {pid} was not reaped");
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[tokio::test]
    async fn batched_tool_results_all_reach_the_stream_and_final_transcript() {
        let f = Fixture::new();
        let mut request = f.request("batched-tools");
        request.prepared.output_mode = "streaming_json".into();
        let mut events = Vec::new();
        let result = run(
            request,
            || false,
            |event| {
                events.push(event);
                Ok(())
            },
        )
        .await
        .unwrap();
        assert_eq!(result.parts.len(), 3);
        assert_eq!(
            events
                .iter()
                .filter_map(|event| event.part.clone())
                .collect::<Vec<_>>(),
            result.parts
        );
        assert!(
            matches!(&result.parts[1], ChatMessagePart::Tool { name, output_summary: Some(output), .. }
            if name == "read-2" && output.contains("SENTINEL-SECOND"))
        );
        assert!(
            matches!(&result.parts[2], ChatMessagePart::Tool { status: Some(status), .. } if status == "error")
        );
    }

    #[tokio::test]
    async fn failed_log_observer_stops_the_process_and_releases_ownership() {
        let f = Fixture::new();
        let mut request = f.request("ignore-term");
        request.limits.terminate_grace = Duration::from_millis(40);
        let turn = request.turn_id.clone();
        let result = run_observed(
            request,
            || None,
            |_| Ok(()),
            |event| match event {
                Observation::Started(_) => Ok(()),
                Observation::Log { .. } => {
                    Err("chat run log write failed: synthetic disk failure".into())
                }
            },
        )
        .await
        .unwrap();
        assert_eq!(result.status, "error");
        assert!(result.error_summary.unwrap().contains("log write failed"));
        assert!(process_supervisor::supervisor().metadata(&turn).is_none());
    }

    #[tokio::test]
    async fn stderr_pressure_does_not_deadlock_stdout_and_owner_is_chat() {
        let f = Fixture::new();
        let request = f.request("stderr-flood");
        let turn = request.turn_id.clone();
        let project = request.project_key.clone();
        let mut seen = String::new();
        let result = run(request, || false, |event| {
            if !event.done {
                let meta = process_supervisor::supervisor().metadata(&event.turn_id).unwrap();
                assert!(matches!(meta.owner, ProcessOwner::Chat { project_key, .. } if project_key == project));
            }
            seen.push_str(&event.delta); Ok(())
        }).await.unwrap();
        assert_eq!(result.status, "complete");
        assert_eq!(seen, "answer after stderr\n");
        assert_eq!(result.content, seen);
        assert!(process_supervisor::supervisor().metadata(&turn).is_none());
    }

    #[tokio::test]
    async fn nonzero_exit_stays_failed_even_with_partial_output() {
        let f = Fixture::new();
        let result = run(f.request("partial-failure"), || false, |_| Ok(()))
            .await
            .unwrap();
        assert_eq!(result.status, "error");
        assert!(result.content.contains("partial answer"));
        assert!(result.error_summary.unwrap().contains("fixture failed"));
        assert!(result.resume_command.is_none());
    }

    #[tokio::test]
    async fn cancellation_escalates_past_ignored_term_and_keeps_partial() {
        let f = Fixture::new();
        let cancelled = Arc::new(AtomicBool::new(false));
        let stop = cancelled.clone();
        let started = Instant::now();
        let result = run(
            f.request("ignore-term"),
            || cancelled.load(Ordering::Acquire),
            |event| {
                if event.delta.contains("READY") {
                    stop.store(true, Ordering::Release);
                }
                Ok(())
            },
        )
        .await
        .unwrap();
        assert!(started.elapsed() < Duration::from_secs(3));
        assert_eq!(result.status, "aborted");
        assert!(result.content.contains("READY"));
        assert!(result.error_summary.is_none());
    }

    #[tokio::test]
    async fn timeout_escalates_past_ignored_term_and_cleans_registry() {
        let f = Fixture::new();
        let mut request = f.request("ignore-term");
        request.limits.timeout = Duration::from_millis(200);
        let turn = request.turn_id.clone();
        let result = run(request, || false, |_| Ok(())).await.unwrap();
        assert_eq!(result.status, "aborted");
        assert!(result.error_summary.unwrap().contains("超时"));
        assert!(process_supervisor::supervisor().metadata(&turn).is_none());
    }

    #[tokio::test]
    async fn exited_parent_with_inherited_pipes_does_not_hang_or_leave_child() {
        // Repetition exercises the short interval between KILL and OS reaping.
        for _ in 0..20 {
            let f = Fixture::new();
            let started = Instant::now();
            let result = run(f.request("descendant-pipes"), || false, |_| Ok(()))
                .await
                .unwrap();
            assert_eq!(result.status, "complete");
            assert!(result.content.contains("parent finished"));
            assert!(started.elapsed() < Duration::from_secs(3));
            f.assert_descendant_gone().await;
        }
    }

    #[tokio::test]
    async fn descendants_are_stopped_even_when_they_close_output_pipes() {
        let f = Fixture::new();
        let result = run(f.request("descendant-closed"), || false, |_| Ok(()))
            .await
            .unwrap();
        assert_eq!(result.status, "complete");
        f.assert_descendant_gone().await;
    }

    #[tokio::test]
    async fn stdin_is_written_concurrently_and_closed_at_end() {
        let f = Fixture::new();
        let mut request = f.request("stdin");
        request.prepared.stdin_prompt = true;
        let input = "中文 context\n".repeat(20_000);
        request.limits.max_lines = 30_000;
        request.stdin_text = Some(input.clone());
        let result = run(request, || false, |_| Ok(())).await.unwrap();
        assert_eq!(result.status, "complete");
        assert_eq!(result.content, input);
    }

    #[tokio::test]
    async fn utf8_across_reads_and_final_structured_messages_are_preserved() {
        let f = Fixture::new();
        let result = run(f.request("utf8"), || false, |_| Ok(())).await.unwrap();
        assert_eq!(result.content, "中\n");
        let mut request = f.request("unknown-json");
        request.prepared.output_mode = "codex_json".into();
        let result = run(request, || false, |_| Ok(())).await.unwrap();
        assert_eq!(result.content, "structured answer");
    }

    #[tokio::test]
    async fn output_budget_and_callback_failure_terminate_the_process() {
        let f = Fixture::new();
        let mut request = f.request("huge-line");
        request.limits.max_line_bytes = 100;
        let turn = request.turn_id.clone();
        let result = run(request, || false, |_| Ok(())).await.unwrap();
        assert_eq!(result.status, "error");
        assert!(result.error_summary.unwrap().contains("output_limit"));
        assert!(process_supervisor::supervisor().metadata(&turn).is_none());
        for bytes_budget in [true, false] {
            let mut request = f.request("stderr-flood");
            if bytes_budget {
                request.limits.max_bytes = 128;
            } else {
                request.limits.max_lines = 3;
            }
            let result = run(request, || false, |_| Ok(())).await.unwrap();
            assert_eq!(result.status, "error");
            assert!(result.error_summary.unwrap().starts_with("output_limit:"));
        }
        let result = run(
            f.request("ignore-term"),
            || false,
            |_| Err("consumer failed".into()),
        )
        .await
        .unwrap();
        assert_eq!(result.status, "error");
        assert!(result.content.contains("READY"));
        assert_eq!(result.error_summary.as_deref(), Some("consumer failed"));
        let result = run(
            f.request("utf8"),
            || false,
            |event| {
                if event.done {
                    Err("terminal consumer failed".into())
                } else {
                    Ok(())
                }
            },
        )
        .await
        .unwrap();
        assert_eq!(result.status, "error");
        assert_eq!(result.content, "中\n");
        assert_eq!(
            result.error_summary.as_deref(),
            Some("terminal consumer failed")
        );
    }

    #[tokio::test]
    async fn cancellation_before_spawn_and_spawn_errors_leave_no_registry_entry() {
        let f = Fixture::new();
        let mut request = f.request("stdin");
        request.prepared.program = "/nonexistent/loom-test-agent".into();
        let turn = request.turn_id.clone();
        let result = run(request, || true, |_| Ok(())).await.unwrap();
        assert_eq!(result.status, "aborted");
        assert!(process_supervisor::supervisor().metadata(&turn).is_none());
        let mut request = f.request("stdin");
        request.prepared.program = "/nonexistent/loom-test-agent".into();
        assert!(run(request, || false, |_| Ok(()))
            .await
            .unwrap_err()
            .contains("failed to start agent"));
    }

    #[tokio::test]
    async fn dropping_runtime_future_kills_its_process_and_releases_ownership() {
        let f = Fixture::new();
        let request = f.request("ignore-term");
        let turn = request.turn_id.clone();
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let mut sender = Some(sender);
            run(
                request,
                || false,
                |event| {
                    if event.delta.contains("READY") {
                        let pid = process_supervisor::supervisor()
                            .metadata(&event.turn_id)
                            .unwrap()
                            .process_id;
                        if let Some(sender) = sender.take() {
                            let _ = sender.send(pid);
                        }
                    }
                    Ok(())
                },
            )
            .await
        });
        let pid = tokio::time::timeout(Duration::from_secs(3), receiver)
            .await
            .unwrap()
            .unwrap();
        fs::write(f.path.join("child.pid"), pid.to_string()).unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert!(process_supervisor::supervisor().metadata(&turn).is_none());
        f.assert_descendant_gone().await;
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
    bytes: usize,
    lines: usize,
    stdout: Vec<String>,
    stderr: Vec<String>,
    content: String,
    parts: Vec<ChatMessagePart>,
}

impl Output {
    fn receive(
        &mut self,
        request: &Request,
        frame: &mut Lines,
        chunk: (&[u8], bool),
        stderr: bool,
        emit: &mut impl FnMut(ChatStreamEvent) -> Result<(), String>,
        observe: &mut impl FnMut(Observation) -> Result<(), String>,
    ) -> Result<(), String> {
        let (bytes, eof) = chunk;
        self.bytes += bytes.len();
        if self.bytes > request.limits.max_bytes {
            return Err("output_limit: CLI exceeded the turn output budget".into());
        }
        for line in frame.feed(bytes, eof, request.limits.max_line_bytes)? {
            self.lines += 1;
            if self.lines > request.limits.max_lines {
                return Err("output_limit: CLI emitted too many lines".into());
            }
            let line = agents::redact_sensitive_text(&line);
            observe(Observation::Log {
                stderr,
                line: line.clone(),
            })?;
            if stderr {
                self.stderr.push(line);
                continue;
            }
            let parsed = parse_chat_stream_line(&request.prepared.output_mode, &line);
            self.stdout.push(line);
            for part in parsed.parts {
                if self.parts.len() >= 1_000 {
                    return Err("output_limit: CLI emitted too many tool parts".into());
                }
                self.parts.push(part.clone());
                emit(event(request, String::new(), Some(part), false))?;
            }
            if !parsed.delta.is_empty() {
                self.content.push_str(&parsed.delta);
                emit(event(request, parsed.delta, None, false))?;
            }
        }
        Ok(())
    }
}

fn event(
    request: &Request,
    delta: String,
    part: Option<ChatMessagePart>,
    done: bool,
) -> ChatStreamEvent {
    ChatStreamEvent {
        session_id: request.session_id.clone(),
        turn_id: request.turn_id.clone(),
        message_id: request.message_id.clone(),
        delta,
        part,
        done,
    }
}

#[cfg(test)]
pub(super) async fn run(
    request: Request,
    cancelled: impl Fn() -> bool,
    emit: impl FnMut(ChatStreamEvent) -> Result<(), String>,
) -> Result<ChatTurnOutcome, String> {
    run_observed(
        request,
        || cancelled().then_some(super::STOP_REASON_ABORT),
        emit,
        |_| Ok(()),
    )
    .await
}

pub(super) async fn run_observed(
    request: Request,
    cancelled: impl Fn() -> Option<&'static str>,
    mut emit: impl FnMut(ChatStreamEvent) -> Result<(), String>,
    mut observe: impl FnMut(Observation) -> Result<(), String>,
) -> Result<ChatTurnOutcome, String> {
    if let Some(reason) = cancelled() {
        return Ok(chat_stop_outcome(Some(reason), String::new(), Vec::new()));
    }
    let mut operation = match process_supervisor::supervisor().begin_operation() {
        Ok(operation) => operation,
        Err(_) if process_supervisor::supervisor().is_shutting_down() => {
            return Ok(chat_stop_outcome(
                Some(super::STOP_REASON_SHUTDOWN),
                String::new(),
                Vec::new(),
            ))
        }
        Err(error) => return Err(error),
    };
    if request.prepared.stdin_prompt && request.stdin_text.is_none() {
        return Err("stdin prompt was requested but no input was supplied".into());
    }
    let mut command = Command::new(&request.prepared.program);
    command
        .args(&request.prepared.args)
        .current_dir(&request.prepared.cwd)
        .kill_on_drop(true)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(if request.stdin_text.is_some() {
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
        run_id: request.turn_id.clone(),
        registered: false,
    };
    if let Err(error) = operation.register(ProcessMetadata::chat(
        &request.turn_id,
        &request.project_key,
        &request.session_id,
        pid,
        request.limits.timeout.as_millis().min(u64::MAX as u128) as u64,
    )) {
        process_supervisor::force_terminate_process_group(pid)?;
        let _ = child.wait().await;
        return Err(error);
    }
    guard.registered = true;
    observe(Observation::Started(pid))?;
    let mut stdout = child.stdout.take().ok_or("agent stdout missing")?;
    let mut stderr = child.stderr.take().ok_or("agent stderr missing")?;
    let mut input_task = request.stdin_text.as_ref().map(|text| {
        let text = text.clone();
        let mut stdin = child.stdin.take().expect("stdin configured above");
        InputWriter(tokio::spawn(async move {
            stdin.write_all(text.as_bytes()).await?;
            stdin.shutdown().await
        }))
    });
    let mut output = Output::default();
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
                .metadata(&request.turn_id)
                .and_then(|meta| meta.termination_reason);
            reason = if let Some(reason) = cancelled() {
                Some(reason.into())
            } else if requested.is_some() {
                requested
            } else if now.duration_since(started) >= request.limits.timeout {
                Some(STOP_REASON_TIMEOUT.into())
            } else if failure.is_some() {
                Some("chat_io_error".into())
            } else if exited_at
                .is_some_and(|at| now.duration_since(at) >= request.limits.drain_grace)
                && (!out_eof || !err_eof)
            {
                Some("chat_descendant_cleanup".into())
            } else {
                None
            };
            if let Some(why) = reason.as_deref() {
                process_supervisor::supervisor().request_stop(&request.turn_id, why)?;
                stop_at = Some(now);
                if let Some(task) = input_task.take() {
                    task.0.abort();
                }
            }
        }
        if stop_at.is_some_and(|at| now.duration_since(at) >= request.limits.terminate_grace)
            && killed_at.is_none()
        {
            process_supervisor::force_terminate_process_group(pid)?;
            killed_at = Some(now);
        }
        if status.is_some() && out_eof && err_eof && input_task.is_none() {
            break;
        }
        if killed_at.is_some_and(|at| now.duration_since(at) >= request.limits.reap_grace) {
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
                            if let Err(error) = output.receive(&request, &mut out_frame, (&out_buf[..n], out_eof), false, &mut emit, &mut observe) { failure = Some(error); }
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
                            if let Err(error) = output.receive(&request, &mut err_frame, (&err_buf[..n], err_eof), true, &mut emit, &mut observe) { failure = Some(error); }
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
    if killed_at.is_none() {
        process_supervisor::force_terminate_process_group(pid)?;
    }
    if let Some(task) = input_task.take() {
        task.0.abort();
    }
    process_supervisor::supervisor().complete(&request.turn_id);
    guard.registered = false;
    // Disarm the guard after cleanup so a later PID reuse is never signalled.
    guard.pid = 0;
    if failure.is_none() {
        if let Err(error) = emit(event(&request, String::new(), None, true)) {
            failure = Some(error);
        }
    }
    if let Some(error) = failure {
        output.parts.push(ChatMessagePart::Error {
            message: error.clone(),
            code: Some("chat_runtime_error".into()),
        });
        return Ok(ChatTurnOutcome {
            content: output.content,
            parts: output.parts,
            resume_command: None,
            status: "error".into(),
            error_summary: Some(error),
            exit_code: status.as_ref().and_then(ExitStatus::code),
            termination_reason: Some("chat_io_error".into()),
        });
    }
    if super::is_chat_stop_reason(reason.as_deref()) {
        let mut outcome = chat_stop_outcome(reason.as_deref(), output.content, output.parts);
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
        let error = format!("agent exited with status {status}; stderr: {diagnostic}");
        output.parts.push(ChatMessagePart::Error {
            message: error.clone(),
            code: Some("agent_exit_failed".into()),
        });
        return Ok(ChatTurnOutcome {
            content: output.content,
            parts: output.parts,
            resume_command: None,
            status: "error".into(),
            error_summary: Some(error),
            exit_code: status.code(),
            termination_reason: Some(status.to_string()),
        });
    }
    let captured = session_capture::capture_session_from_lines(
        &request.prepared.program,
        &output.stdout,
        &output.stderr,
    );
    Ok(ChatTurnOutcome {
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
