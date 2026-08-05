use super::*;

pub(super) struct ParsedCliStdout {
    pub(super) stdout: Option<String>,
    pub(super) session_id: Option<String>,
}

pub(super) async fn read_planning_stream<E, Reader>(
    emitter: E,
    context: Option<PlanningLogContext>,
    output_mode: CliOutputMode,
    stream: &'static str,
    reader: Reader,
) -> String
where
    E: PlanningEventEmitter,
    Reader: AsyncRead + Unpin + Send + 'static,
{
    let mut lines = BufReader::new(reader).lines();
    let mut raw_lines = Vec::new();
    let mut pending_log_lines = Vec::new();
    let mut ticker = interval(Duration::from_millis(100));

    loop {
        tokio::select! {
            line = lines.next_line() => {
                match line {
                    Ok(Some(line)) => {
                        raw_lines.push(line.clone());
                        if context.is_some() {
                            pending_log_lines.extend(log_lines_for_stream(output_mode, stream, &line));
                            if pending_log_lines.len() >= 32 {
                                flush_planning_log(&emitter, context.as_ref(), stream, &mut pending_log_lines);
                            }
                        }
                    }
                    Ok(None) => break,
                    Err(error) => {
                        if context.is_some() {
                            pending_log_lines.push(format!("failed to read {stream}: {error}"));
                        }
                        break;
                    }
                }
            }
            _ = ticker.tick() => {
                flush_planning_log(&emitter, context.as_ref(), stream, &mut pending_log_lines);
            }
        }
    }

    flush_planning_log(&emitter, context.as_ref(), stream, &mut pending_log_lines);
    raw_lines.join("\n")
}

pub(super) fn flush_planning_log<E: PlanningEventEmitter>(
    emitter: &E,
    context: Option<&PlanningLogContext>,
    stream: &str,
    pending_log_lines: &mut Vec<String>,
) {
    if pending_log_lines.is_empty() {
        return;
    }
    let Some(context) = context else {
        pending_log_lines.clear();
        return;
    };
    let lines = std::mem::take(pending_log_lines)
        .into_iter()
        .flat_map(|line| {
            redact_sensitive_text(&line)
                .lines()
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>();
    if lines.is_empty() {
        return;
    }

    emitter.emit_planning_agent_log(PlanningAgentLogEvent {
        task_id: context.task_id.clone(),
        planning_run_id: context.planning_run_id.clone(),
        agent_id: context.agent_id.clone(),
        agent_name: context.agent_name.clone(),
        phase: context.phase.clone(),
        attempt: context.attempt,
        stream: stream.to_string(),
        lines,
        timestamp_ms: now_ms(),
    });
}

pub(super) fn log_lines_for_stream(
    output_mode: CliOutputMode,
    stream: &str,
    line: &str,
) -> Vec<String> {
    if stream != "stdout" {
        return vec![line.to_string()];
    }

    match output_mode {
        CliOutputMode::Plain => vec![line.to_string()],
        CliOutputMode::ClaudeStreamJson => serde_json::from_str::<serde_json::Value>(line)
            .map(|value| claude_log_lines(&value))
            .unwrap_or_else(|_| vec![line.to_string()]),
        CliOutputMode::CodexJson => serde_json::from_str::<serde_json::Value>(line)
            .map(|value| codex_log_lines(&value))
            .unwrap_or_else(|_| vec![line.to_string()]),
    }
}

pub(super) fn parse_cli_stdout(output_mode: CliOutputMode, stdout: &str) -> ParsedCliStdout {
    match output_mode {
        CliOutputMode::Plain => ParsedCliStdout {
            stdout: None,
            session_id: None,
        },
        CliOutputMode::ClaudeStreamJson => parse_claude_stream(stdout),
        CliOutputMode::CodexJson => parse_codex_stream(stdout),
    }
}

pub(super) fn normalize_agent_stdout_inner(output_mode: &str, stdout: &str) -> String {
    let mode = match output_mode {
        "claude_stream_json" => CliOutputMode::ClaudeStreamJson,
        "codex_json" => CliOutputMode::CodexJson,
        _ => CliOutputMode::Plain,
    };
    let parsed = parse_cli_stdout(mode, stdout);
    redact_sensitive_text(parsed.stdout.as_deref().unwrap_or(stdout))
}

pub(super) fn parse_claude_stream(stdout: &str) -> ParsedCliStdout {
    let mut session_id = None;
    let mut result_stdout = None;
    let mut assistant_chunks = Vec::new();

    for line in stdout.lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            assistant_chunks.push(line.to_string());
            continue;
        };
        if session_id.is_none() {
            session_id = find_session_id(&value);
        }

        match event_type(&value).as_deref() {
            Some("result") => {
                if let Some(text) = string_field(&value, "result") {
                    if !text.trim().is_empty() {
                        result_stdout = Some(text.to_string());
                    }
                }
            }
            Some("assistant") => {
                assistant_chunks.extend(assistant_text_chunks(&value));
            }
            _ => {}
        }
    }

    ParsedCliStdout {
        stdout: result_stdout.or_else(|| join_chunks(assistant_chunks)),
        session_id,
    }
}

pub(super) fn parse_codex_stream(stdout: &str) -> ParsedCliStdout {
    let mut session_id = None;
    let mut last_agent_message = None;
    let mut fallback_chunks = Vec::new();

    for line in stdout.lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            fallback_chunks.push(line.to_string());
            continue;
        };
        if session_id.is_none() {
            session_id = find_session_id(&value);
        }

        let event = event_type(&value).unwrap_or_default();
        if is_agent_message_event(&event, &value) {
            let text = assistant_text_chunks(&value).join("\n");
            if !text.trim().is_empty() {
                last_agent_message = Some(text);
            }
        } else {
            fallback_chunks.extend(codex_log_lines(&value));
        }
    }

    ParsedCliStdout {
        stdout: last_agent_message.or_else(|| join_chunks(fallback_chunks)),
        session_id,
    }
}

pub(super) fn claude_log_lines(value: &serde_json::Value) -> Vec<String> {
    match event_type(value).as_deref() {
        Some("system") => find_session_id(value)
            .map(|id| vec![format!("Session started: {id}")])
            .unwrap_or_default(),
        Some("assistant") => assistant_text_lines(value),
        Some("stream_event") => string_field_deep(value, &["text"])
            .map(split_log_text)
            .unwrap_or_default(),
        Some("result") => {
            if bool_field(value, "is_error") == Some(true) {
                string_field(value, "result")
                    .map(split_log_text)
                    .unwrap_or_default()
            } else {
                Vec::new()
            }
        }
        Some(kind) => vec![format!("[{kind}]")],
        None => Vec::new(),
    }
}

pub(super) fn codex_log_lines(value: &serde_json::Value) -> Vec<String> {
    let event = event_type(value).unwrap_or_default();
    if event == "thread.started" {
        return find_session_id(value)
            .map(|id| vec![format!("Session started: {id}")])
            .unwrap_or_default();
    }
    if is_agent_message_event(&event, value) {
        return assistant_text_lines(value);
    }
    if event.contains("exec_command") || event.contains("command") {
        if let Some(command) = string_field_deep(value, &["command", "cmd"]) {
            return vec![format!("$ {command}")];
        }
        if let Some(output) = string_field_deep(value, &["output", "text", "delta"]) {
            return split_log_text(output);
        }
    }
    if event.is_empty() {
        Vec::new()
    } else {
        vec![format!("[{event}]")]
    }
}

pub(super) fn event_type(value: &serde_json::Value) -> Option<String> {
    string_field(value, "type")
        .or_else(|| value.get("msg").and_then(|msg| string_field(msg, "type")))
        .or_else(|| {
            value
                .get("event")
                .and_then(|event| string_field(event, "type"))
        })
        .or_else(|| string_field(value, "event"))
        .map(str::to_string)
}

pub(super) fn is_agent_message_event(event: &str, value: &serde_json::Value) -> bool {
    event.contains("agent_message")
        || event.contains("assistant")
        || contains_type_value(value, &["agent_message", "assistant"])
        || string_field_deep(value, &["role"]) == Some("assistant")
}

pub(super) fn contains_type_value(value: &serde_json::Value, expected: &[&str]) -> bool {
    match value {
        serde_json::Value::Object(map) => {
            map.get("type")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|value| expected.contains(&value))
                || map
                    .values()
                    .any(|child| contains_type_value(child, expected))
        }
        serde_json::Value::Array(items) => items
            .iter()
            .any(|child| contains_type_value(child, expected)),
        _ => false,
    }
}

pub(super) fn assistant_text_lines(value: &serde_json::Value) -> Vec<String> {
    assistant_text_chunks(value)
        .into_iter()
        .flat_map(|text| split_log_text(&text))
        .collect()
}

pub(super) fn assistant_text_chunks(value: &serde_json::Value) -> Vec<String> {
    let mut output = Vec::new();
    collect_text_values(value, &mut output);
    output
        .into_iter()
        .filter(|text| !text.trim().is_empty())
        .collect()
}

pub(super) fn collect_text_values(value: &serde_json::Value, output: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                if matches!(
                    key.as_str(),
                    "text" | "message" | "content" | "delta" | "result"
                ) {
                    if let Some(text) = child.as_str() {
                        if !text.trim().is_empty() {
                            output.push(text.to_string());
                        }
                    }
                }
                collect_text_values(child, output);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_text_values(item, output);
            }
        }
        _ => {}
    }
}

pub(super) fn string_field<'a>(value: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(serde_json::Value::as_str)
}

pub(super) fn bool_field(value: &serde_json::Value, key: &str) -> Option<bool> {
    value.get(key).and_then(serde_json::Value::as_bool)
}

pub(super) fn string_field_deep<'a>(
    value: &'a serde_json::Value,
    keys: &[&str],
) -> Option<&'a str> {
    match value {
        serde_json::Value::Object(map) => {
            for key in keys {
                if let Some(text) = map.get(*key).and_then(serde_json::Value::as_str) {
                    return Some(text);
                }
            }
            map.values()
                .find_map(|child| string_field_deep(child, keys))
        }
        serde_json::Value::Array(items) => items
            .iter()
            .find_map(|child| string_field_deep(child, keys)),
        _ => None,
    }
}

pub(super) fn split_log_text(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim_end)
        .filter(|line| !line.trim().is_empty())
        .map(str::to_string)
        .collect()
}

pub(super) fn join_chunks(chunks: Vec<String>) -> Option<String> {
    let text = chunks
        .into_iter()
        .filter(|chunk| !chunk.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    if text.trim().is_empty() {
        None
    } else {
        Some(text)
    }
}

pub(super) async fn run_cli_profile<E: PlanningEventEmitter>(
    profile: &CliProfile,
    project_path: &Path,
    prompt: &str,
    emitter: E,
    log_context: Option<PlanningLogContext>,
) -> Result<PlanningInvocationResult, String> {
    if log_context
        .as_ref()
        .is_some_and(|context| task_stop_requested(&context.task_id))
    {
        return Err("agent invocation stopped because the task lifecycle changed".to_string());
    }
    let assessment = execution_policy::evaluate_execution(
        &ExecutionRequest {
            program: profile.command.clone(),
            args: profile.args.clone(),
            cwd: project_path.display().to_string(),
            project_path: project_path.display().to_string(),
            agent_id: None,
        },
        true,
    );
    if assessment.decision != ExecutionDecision::Allowed {
        return Err(format!(
            "planning agent execution rejected by policy: {}",
            assessment.detail
        ));
    }
    let started_at_ms = now_ms();
    let mut command = TokioCommand::new(&profile.command);
    command
        .args(&profile.args)
        .current_dir(project_path)
        .kill_on_drop(true)
        .stdin(if profile.stdin_prompt {
            std::process::Stdio::piped()
        } else {
            std::process::Stdio::null()
        })
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    process_supervisor::configure_process_group(&mut command);

    let mut child = command
        .spawn()
        .map_err(|error| format!("failed to start {}: {error}", profile.command))?;

    if profile.stdin_prompt {
        if let Some(mut stdin) = child.stdin.take() {
            let prompt = prompt.to_string();
            tauri::async_runtime::spawn(async move {
                let _ = stdin.write_all(prompt.as_bytes()).await;
                let _ = stdin.shutdown().await;
            });
        }
    }

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "failed to capture agent stdout".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "failed to capture agent stderr".to_string())?;
    let process_id = child.id();
    let registered_run_id = match (process_id, log_context.as_ref()) {
        (Some(process_id), Some(context)) => {
            let run_id = format!("agent-{process_id}");
            process_supervisor::supervisor().register(ProcessMetadata::new(
                &run_id,
                &context.task_id,
                ProcessKind::Agent,
                process_id,
                Some(PLANNING_TIMEOUT_MS),
            ))?;
            Some(run_id)
        }
        _ => None,
    };
    let stdout_task = tauri::async_runtime::spawn(read_planning_stream(
        emitter.clone(),
        log_context.clone(),
        profile.output_mode,
        "stdout",
        stdout,
    ));
    let stderr_task = tauri::async_runtime::spawn(read_planning_stream(
        emitter,
        log_context,
        profile.output_mode,
        "stderr",
        stderr,
    ));

    let timeout = sleep(Duration::from_millis(PLANNING_TIMEOUT_MS));
    tokio::pin!(timeout);
    let wait_result = tokio::select! {
        status = child.wait() => {
            status
                .map(|status| (status, false))
                .map_err(|error| format!("failed to wait for {}: {error}", profile.command))
        }
        _ = &mut timeout => {
            if let Some(run_id) = registered_run_id.as_deref() {
                let _ = process_supervisor::supervisor().request_stop(run_id, "timeout");
            } else if let Some(process_id) = process_id {
                let _ = process_supervisor::terminate_process_group(process_id);
            }
            let _ = child.start_kill();
            child
                .wait()
                .await
                .map(|status| (status, true))
                .map_err(|error| format!("failed to kill timed out {}: {error}", profile.command))
        }
    };
    if let Some(run_id) = registered_run_id.as_deref() {
        process_supervisor::supervisor().complete(run_id);
    }
    let (status, timed_out) = wait_result?;

    let raw_stdout = stdout_task
        .await
        .map_err(|error| format!("failed to join stdout reader: {error}"))?;
    let raw_stderr = stderr_task
        .await
        .map_err(|error| format!("failed to join stderr reader: {error}"))?;
    let parsed = parse_cli_stdout(profile.output_mode, &raw_stdout);
    let stdout = redact_sensitive_text(&parsed.stdout.unwrap_or(raw_stdout));
    let stderr = redact_sensitive_text(&raw_stderr);
    let session_id = parsed.session_id;
    let resume_command = session_id
        .as_deref()
        .and_then(|session_id| resume_command_for_profile(profile, session_id));
    let error_lines = extract_error_lines(&stderr);
    let exit_code = status.code();
    let stderr_only_error = status.success()
        && stdout.trim().is_empty()
        && stderr
            .lines()
            .map(str::trim_start)
            .any(|line| line.starts_with("Error:") || line.starts_with("error:"));
    let status_text = if status.success() && !timed_out && !stderr_only_error {
        "succeeded"
    } else {
        "failed"
    }
    .to_string();
    let output_summary = if status_text == "succeeded" {
        summarize_cli_output(&stdout)
    } else if timed_out {
        format!(
            "{} timed out after {} ms.",
            profile.command, PLANNING_TIMEOUT_MS
        )
    } else if stderr_only_error {
        format!(
            "{} reported an error without stdout. {}",
            profile.command,
            stderr_tail(&stderr).join(" ")
        )
    } else {
        format!(
            "{} exited with status {:?}. {}",
            profile.command,
            exit_code,
            stderr_tail(&stderr).join(" ")
        )
    };

    Ok(PlanningInvocationResult {
        status: status_text,
        stdout,
        stderr,
        output_summary,
        evidence_ref: None,
        plan_path: None,
        exit_code,
        timed_out,
        attempt: 1,
        failure_kind: None,
        failure_detail: None,
        error_lines,
        stderr_ref: None,
        session_id,
        resume_command,
        started_at_ms,
        ended_at_ms: now_ms(),
    })
}
