use crate::{
    agents::{self, redact_sensitive_text},
    execution_policy::{self, ExecutionApprovalRegistry, ExecutionRequest},
    models::{
        now_ms, CommandFinishedEvent, CommandLogEvent, CommandRun, CommandRunIntent,
        CommandRunStatus, CommandSpec, ErrorSummary, IdGenerator,
    },
    process_supervisor::{self, ProcessKind, ProcessMetadata},
    session_capture::{capture_session_from_lines, display_log_lines_for_command},
    settings, storage, tasks,
};
use std::{
    collections::HashMap,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::Arc,
};
use tauri::{AppHandle, Emitter, Runtime, State};
use tokio::{
    fs::OpenOptions,
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, Command},
    sync::Mutex,
    time::{sleep, timeout, Duration, Instant},
};

#[derive(Default)]
pub struct CommandRegistry {
    runs: Arc<Mutex<HashMap<String, ManagedCommandRun>>>,
}

#[derive(Clone)]
struct ManagedCommandRun {
    child: Arc<Mutex<Child>>,
    task_id: Option<String>,
    project_path: PathBuf,
}

struct LogReaderContext {
    task_id: String,
    run_id: String,
    stream: &'static str,
    log_path: PathBuf,
    command: String,
    captured_lines: Option<Arc<Mutex<Vec<String>>>>,
}

struct CommandMonitorContext<E: CommandEventEmitter> {
    app: E,
    runs: Arc<Mutex<HashMap<String, ManagedCommandRun>>>,
    child: Arc<Mutex<Child>>,
    project_path: PathBuf,
    task_id: String,
    run_id: String,
    command: String,
    stdout_lines: Arc<Mutex<Vec<String>>>,
    stderr_lines: Arc<Mutex<Vec<String>>>,
    reader_tasks: Vec<tauri::async_runtime::JoinHandle<()>>,
    timeout_seconds: u64,
    operation: process_supervisor::OperationGuard<'static>,
}

trait CommandEventEmitter: Clone + Send + Sync + 'static {
    fn emit_log(&self, event: CommandLogEvent);
    fn emit_finished(&self, event: CommandFinishedEvent);
}

impl<R: Runtime> CommandEventEmitter for AppHandle<R> {
    fn emit_log(&self, event: CommandLogEvent) {
        let _ = self.emit("loom://command-log", event);
    }

    fn emit_finished(&self, event: CommandFinishedEvent) {
        let _ = self.emit("loom://command-finished", event);
    }
}

#[tauri::command]
pub fn command_runner_ready(_spec: Option<CommandSpec>) -> bool {
    true
}

const MAX_HISTORY_LOG_BYTES: u64 = 2 * 1024 * 1024;
const MAX_HISTORY_LOG_LINES: usize = 5_000;

#[tauri::command]
pub fn read_command_run_logs(
    project_path: String,
    task_id: String,
    run_id: String,
) -> Result<Vec<CommandLogEvent>, String> {
    validate_record_id(&task_id, "task")?;
    validate_record_id(&run_id, "run")?;
    let project_root = std::fs::canonicalize(&project_path)
        .map_err(|error| format!("failed to resolve project path: {error}"))?;
    let task = tasks::load_task(&project_root, &task_id)?;
    let recorded_root = std::fs::canonicalize(&task.project_path)
        .map_err(|error| format!("failed to resolve recorded project path: {error}"))?;
    if project_root != recorded_root {
        return Err("task does not belong to the requested project".to_string());
    }
    let run = task
        .command_runs
        .iter()
        .find(|candidate| candidate.id == run_id)
        .ok_or_else(|| format!("command run '{run_id}' was not found"))?;
    let log_root = storage::project_logs_dir(&project_root).join(&task_id);
    let mut events = Vec::new();
    for (stream, reference) in [
        ("stdout", run.stdout_log_ref.as_deref()),
        ("stderr", run.stderr_log_ref.as_deref()),
    ] {
        let Some(reference) = reference else {
            continue;
        };
        let path = PathBuf::from(reference);
        let canonical_path = std::fs::canonicalize(&path)
            .map_err(|error| format!("failed to resolve {stream} log: {error}"))?;
        let canonical_log_root = std::fs::canonicalize(&log_root)
            .map_err(|error| format!("failed to resolve task log directory: {error}"))?;
        if !canonical_path.starts_with(&canonical_log_root) {
            return Err(format!("{stream} log is outside the task log directory"));
        }
        for line in read_log_tail(&canonical_path)? {
            events.push(CommandLogEvent {
                task_id: Some(task_id.clone()),
                run_id: run_id.clone(),
                stream: stream.to_string(),
                line,
                timestamp_ms: run.started_at_ms.saturating_add(events.len() as u128),
            });
        }
    }
    if events.len() > MAX_HISTORY_LOG_LINES {
        events.drain(..events.len() - MAX_HISTORY_LOG_LINES);
    }
    Ok(events)
}

fn validate_record_id(value: &str, kind: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 160
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err(format!("invalid {kind} id"));
    }
    Ok(())
}

fn read_log_tail(path: &Path) -> Result<Vec<String>, String> {
    let mut file = File::open(path)
        .map_err(|error| format!("failed to open log '{}': {error}", path.display()))?;
    let length = file
        .metadata()
        .map_err(|error| format!("failed to inspect log '{}': {error}", path.display()))?
        .len();
    let start = length.saturating_sub(MAX_HISTORY_LOG_BYTES);
    file.seek(SeekFrom::Start(start))
        .map_err(|error| format!("failed to seek log '{}': {error}", path.display()))?;
    let mut bytes = Vec::with_capacity((length - start) as usize);
    file.read_to_end(&mut bytes)
        .map_err(|error| format!("failed to read log '{}': {error}", path.display()))?;
    let content = String::from_utf8_lossy(&bytes);
    let mut lines = content.lines();
    if start > 0 {
        let _ = lines.next();
    }
    Ok(lines.map(str::to_string).collect())
}

#[tauri::command]
pub async fn start_command_run(
    app: AppHandle,
    registry: State<'_, CommandRegistry>,
    approvals: State<'_, ExecutionApprovalRegistry>,
    ids: State<'_, IdGenerator>,
    mut spec: CommandSpec,
) -> Result<CommandRun, String> {
    if spec.intent == Some(CommandRunIntent::AgentAction) {
        let agent_id = spec
            .agent_id
            .as_deref()
            .ok_or_else(|| "agentId is required for agent action runs".to_string())?;
        agents::validate_implementation_agent(&app, agent_id, &spec.program)?;
    }
    let project_path = spec
        .project_path
        .clone()
        .unwrap_or_else(|| spec.cwd.clone());
    let authorization = execution_policy::authorize_execution(
        &app,
        approvals.inner(),
        &ExecutionRequest {
            program: spec.program.clone(),
            args: spec.args.clone(),
            cwd: spec.cwd.clone(),
            project_path,
            agent_id: spec.agent_id.clone(),
        },
        spec.approval_id.as_deref(),
    )?;
    spec.project_path = Some(authorization.project_path.display().to_string());
    spec.cwd = authorization.cwd.display().to_string();
    let app_settings = settings::load_app_settings_for_app(&app)?;
    start_command_run_inner(
        app,
        registry.inner(),
        ids.inner(),
        spec,
        app_settings.command_timeout_seconds,
    )
    .await
}

async fn start_command_run_inner<E: CommandEventEmitter>(
    app: E,
    registry: &CommandRegistry,
    ids: &IdGenerator,
    spec: CommandSpec,
    timeout_seconds: u64,
) -> Result<CommandRun, String> {
    if spec.program.trim().is_empty() {
        return Err("command program is required".to_string());
    }

    let task_id = spec
        .task_id
        .clone()
        .ok_or_else(|| "taskId is required for command runs".to_string())?;
    let project_path = PathBuf::from(spec.project_path.as_deref().unwrap_or(spec.cwd.as_str()));
    let run_id = ids.next("run");
    let command_text = redact_sensitive_text(&command_text(&spec.program, &spec.args));
    let log_dir = storage::project_logs_dir(&project_path).join(&task_id);
    tokio::fs::create_dir_all(&log_dir)
        .await
        .map_err(|error| format!("failed to create log directory: {error}"))?;
    let stdout_log = log_dir.join(format!("{run_id}.stdout.log"));
    let stderr_log = log_dir.join(format!("{run_id}.stderr.log"));

    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .current_dir(&spec.cwd)
        .kill_on_drop(true)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    process_supervisor::configure_process_group(&mut command);

    let mut operation = process_supervisor::supervisor().begin_operation()?;
    let mut child = command
        .spawn()
        .map_err(|error| format!("failed to start command: {error}"))?;
    let pid = child.id();
    if let Some(process_id) = pid {
        operation.register(ProcessMetadata::new(
            &run_id,
            &task_id,
            ProcessKind::Command,
            process_id,
            (timeout_seconds > 0).then_some(timeout_seconds.saturating_mul(1_000)),
        ))?;
    }
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_lines = Arc::new(Mutex::new(Vec::new()));
    let stderr_lines = Arc::new(Mutex::new(Vec::new()));
    let mut reader_tasks = Vec::new();

    if let Some(stdout) = stdout {
        reader_tasks.push(spawn_log_reader(
            app.clone(),
            stdout,
            LogReaderContext {
                task_id: task_id.clone(),
                run_id: run_id.clone(),
                stream: "stdout",
                log_path: stdout_log.clone(),
                command: spec.program.clone(),
                captured_lines: Some(stdout_lines.clone()),
            },
        ));
    }

    if let Some(stderr) = stderr {
        reader_tasks.push(spawn_log_reader(
            app.clone(),
            stderr,
            LogReaderContext {
                task_id: task_id.clone(),
                run_id: run_id.clone(),
                stream: "stderr",
                log_path: stderr_log.clone(),
                command: spec.program.clone(),
                captured_lines: Some(stderr_lines.clone()),
            },
        ));
    }

    let run = CommandRun {
        id: run_id.clone(),
        task_id: task_id.clone(),
        command: command_text,
        cwd: spec.cwd.clone(),
        intent: spec.intent.clone().unwrap_or(CommandRunIntent::Validation),
        loop_id: spec.loop_id.clone(),
        iteration: spec.iteration,
        attempt: spec.attempt,
        termination_reason: spec.termination_reason.clone(),
        session_id: None,
        resume_command: None,
        started_at_ms: now_ms(),
        ended_at_ms: None,
        status: CommandRunStatus::Running,
        exit_code: None,
        stdout_log_ref: Some(stdout_log.display().to_string()),
        stderr_log_ref: Some(stderr_log.display().to_string()),
        error_summary: None,
    };
    tasks::add_command_run(&project_path, &task_id, run.clone())?;

    let child = Arc::new(Mutex::new(child));
    registry.runs.lock().await.insert(
        run_id.clone(),
        ManagedCommandRun {
            child: child.clone(),
            task_id: Some(task_id),
            project_path: project_path.clone(),
        },
    );
    spawn_command_monitor(CommandMonitorContext {
        app,
        runs: registry.runs.clone(),
        child,
        project_path,
        task_id: run.task_id.clone(),
        run_id: run.id.clone(),
        command: spec.program.clone(),
        stdout_lines,
        stderr_lines,
        reader_tasks,
        timeout_seconds,
        operation,
    });

    Ok(run)
}

#[tauri::command]
pub async fn stop_command_run(
    registry: State<'_, CommandRegistry>,
    run_id: String,
    termination_reason: Option<String>,
) -> Result<CommandRunStopResult, String> {
    stop_command_run_inner(registry.inner(), run_id, termination_reason).await
}

async fn stop_command_run_inner(
    registry: &CommandRegistry,
    run_id: String,
    termination_reason: Option<String>,
) -> Result<CommandRunStopResult, String> {
    let _operation = process_supervisor::supervisor().begin_control_operation(&run_id)?;
    let Some(managed) = registry.runs.lock().await.remove(&run_id) else {
        return Ok(CommandRunStopResult {
            run_id,
            stopped: false,
            exit_code: None,
        });
    };

    let stop_reason = termination_reason.as_deref().unwrap_or("cancelled");
    process_supervisor::supervisor().request_stop(&run_id, stop_reason)?;
    process_supervisor::supervisor().force_stop(&run_id)?;

    let mut child = managed.child.lock().await;
    let _ = child.kill().await;
    let status = child
        .wait()
        .await
        .map_err(|error| format!("failed to wait for command: {error}"))?;
    let exit_code = status.code();
    process_supervisor::supervisor().complete(&run_id);

    if let Some(task_id) = managed.task_id {
        let _ = tasks::finish_command_run(
            &managed.project_path,
            &task_id,
            &run_id,
            tasks::CommandRunCompletion {
                status: CommandRunStatus::Cancelled,
                exit_code,
                error_summary: None,
                session_id: None,
                resume_command: None,
                termination_reason,
            },
        );
    }

    Ok(CommandRunStopResult {
        run_id,
        stopped: true,
        exit_code,
    })
}

pub(crate) async fn stop_task_runs(
    registry: &CommandRegistry,
    task_id: &str,
    termination_reason: &str,
) -> Result<usize, String> {
    let run_ids = registry
        .runs
        .lock()
        .await
        .iter()
        .filter(|(_, run)| run.task_id.as_deref() == Some(task_id))
        .map(|(run_id, _)| run_id.clone())
        .collect::<Vec<_>>();
    let mut stopped = 0;
    for run_id in run_ids {
        let result =
            stop_command_run_inner(registry, run_id, Some(termination_reason.to_string())).await?;
        stopped += usize::from(result.stopped);
    }
    Ok(stopped)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandRunStopResult {
    pub run_id: String,
    pub stopped: bool,
    pub exit_code: Option<i32>,
}

fn spawn_log_reader<E, Reader>(
    app: E,
    reader: Reader,
    context: LogReaderContext,
) -> tauri::async_runtime::JoinHandle<()>
where
    E: CommandEventEmitter,
    Reader: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tauri::async_runtime::spawn(async move {
        let LogReaderContext {
            task_id,
            run_id,
            stream,
            log_path,
            command,
            captured_lines,
        } = context;
        let mut lines = BufReader::new(reader).lines();
        let mut log_file = match OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .await
        {
            Ok(file) => Some(file),
            Err(error) => {
                app.emit_log(CommandLogEvent {
                    task_id: Some(task_id.clone()),
                    run_id: run_id.clone(),
                    stream: "stderr".to_string(),
                    line: format!("failed to open log file: {error}"),
                    timestamp_ms: now_ms(),
                });
                None
            }
        };

        while let Ok(Some(line)) = lines.next_line().await {
            let redacted_line = redact_sensitive_text(&line);
            let display_lines = display_log_lines_for_command(&command, &redacted_line);

            if let Some(captured) = captured_lines.as_ref() {
                captured.lock().await.push(redacted_line);
            }

            for display_line in display_lines {
                if let Some(file) = log_file.as_mut() {
                    let _ = file.write_all(display_line.as_bytes()).await;
                    let _ = file.write_all(b"\n").await;
                }

                app.emit_log(CommandLogEvent {
                    task_id: Some(task_id.clone()),
                    run_id: run_id.clone(),
                    stream: stream.to_string(),
                    line: display_line,
                    timestamp_ms: now_ms(),
                });
            }
        }
    })
}

fn spawn_command_monitor<E: CommandEventEmitter>(context: CommandMonitorContext<E>) {
    tauri::async_runtime::spawn(async move {
        let CommandMonitorContext {
            app,
            runs,
            child,
            project_path,
            task_id,
            run_id,
            command,
            stdout_lines,
            stderr_lines,
            reader_tasks,
            timeout_seconds,
            operation: _operation,
        } = context;
        let started = Instant::now();
        let mut timed_out = false;
        loop {
            if !timed_out
                && timeout_seconds > 0
                && started.elapsed() >= Duration::from_secs(timeout_seconds)
            {
                timed_out = true;
                let _ = process_supervisor::supervisor().request_stop(&run_id, "timeout");
                let _ = process_supervisor::supervisor().force_stop(&run_id);
                let mut child = child.lock().await;
                let _ = child.kill().await;
            }

            let status = {
                let mut child = child.lock().await;
                match child.try_wait() {
                    Ok(Some(status)) => Some(status),
                    Ok(None) => None,
                    Err(_) => None,
                }
            };

            if let Some(status) = status {
                if runs.lock().await.remove(&run_id).is_none() {
                    break;
                }
                let recorded_reason = process_supervisor::supervisor()
                    .metadata(&run_id)
                    .and_then(|meta| meta.termination_reason);
                let _ = process_supervisor::supervisor().force_stop(&run_id);
                process_supervisor::supervisor().complete(&run_id);
                for mut task in reader_tasks {
                    if timeout(Duration::from_secs(2), &mut task).await.is_err() {
                        task.abort();
                    }
                }
                let exit_code = status.code();
                let command_status = if recorded_reason.as_deref() == Some("app_shutdown") {
                    CommandRunStatus::Cancelled
                } else if status.success() {
                    CommandRunStatus::Succeeded
                } else {
                    CommandRunStatus::Failed
                };
                let termination_reason =
                    recorded_reason.or_else(|| timed_out.then(|| "timeout".to_string()));
                let stdout_lines = stdout_lines.lock().await.clone();
                let stderr_lines = stderr_lines.lock().await.clone();
                let analysis = analyze_output(exit_code, &stdout_lines, &stderr_lines);
                let error_summary =
                    (analysis.failed || analysis_has_findings(&analysis)).then_some(analysis);
                let captured_session =
                    capture_session_from_lines(&command, &stdout_lines, &stderr_lines);
                let _ = tasks::finish_command_run(
                    &project_path,
                    &task_id,
                    &run_id,
                    tasks::CommandRunCompletion {
                        status: command_status,
                        exit_code,
                        error_summary: error_summary.clone(),
                        session_id: captured_session.session_id.clone(),
                        resume_command: captured_session.resume_command.clone(),
                        termination_reason: termination_reason.clone(),
                    },
                );
                app.emit_finished(CommandFinishedEvent {
                    task_id: task_id.clone(),
                    run_id: run_id.clone(),
                    status: command_status,
                    exit_code,
                    error_summary,
                    session_id: captured_session.session_id,
                    resume_command: captured_session.resume_command,
                    termination_reason,
                    timestamp_ms: now_ms(),
                });
                break;
            }

            sleep(Duration::from_millis(200)).await;
        }
    });
}

fn command_text(program: &str, args: &[String]) -> String {
    std::iter::once(program.to_string())
        .chain(args.iter().cloned())
        .collect::<Vec<_>>()
        .join(" ")
}

fn analyze_output(
    exit_code: Option<i32>,
    stdout_lines: &[String],
    stderr_lines: &[String],
) -> ErrorSummary {
    let stderr_tail = stderr_lines
        .iter()
        .rev()
        .take(20)
        .cloned()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>();
    let all_lines = stdout_lines.iter().chain(stderr_lines.iter());
    let matched_lines = all_lines
        .clone()
        .filter(|line| is_error_line(line))
        .cloned()
        .collect::<Vec<_>>();
    let warnings = unique_lines(
        all_lines
            .clone()
            .filter(|line| is_warning_line(line))
            .cloned(),
    );
    let test_failures = unique_lines(
        all_lines
            .clone()
            .filter(|line| is_test_failure_line(line))
            .cloned(),
    );
    let stack_trace_lines = unique_lines(
        all_lines
            .clone()
            .filter(|line| is_stack_trace_line(line))
            .cloned(),
    );
    let mut urls = Vec::new();
    let mut ports = Vec::new();
    for line in all_lines {
        for url in extract_urls(line) {
            push_unique(&mut urls, url);
        }
        for port in extract_ports(line) {
            push_unique(&mut ports, port);
        }
    }
    for url in &urls {
        if let Some(port) = port_from_url(url) {
            push_unique(&mut ports, port);
        }
    }

    ErrorSummary {
        exit_code,
        stderr_tail,
        matched_lines,
        urls,
        ports,
        warnings,
        test_failures,
        stack_trace_lines,
        failed: exit_code.unwrap_or_default() != 0,
    }
}

fn analysis_has_findings(summary: &ErrorSummary) -> bool {
    !summary.matched_lines.is_empty()
        || !summary.urls.is_empty()
        || !summary.ports.is_empty()
        || !summary.warnings.is_empty()
        || !summary.test_failures.is_empty()
        || !summary.stack_trace_lines.is_empty()
}

fn unique_lines(lines: impl Iterator<Item = String>) -> Vec<String> {
    let mut values = Vec::new();
    for line in lines.take(200) {
        push_unique(&mut values, line);
        if values.len() == 100 {
            break;
        }
    }
    values
}

fn push_unique<T: PartialEq>(values: &mut Vec<T>, value: T) {
    if values.len() < 100 && !values.contains(&value) {
        values.push(value);
    }
}

fn is_warning_line(line: &str) -> bool {
    let lower = line.trim_start().to_ascii_lowercase();
    lower.starts_with("warning")
        || lower.starts_with("[warn]")
        || lower.starts_with("warn:")
        || lower.contains(": warning ")
        || lower.contains(" warning:")
}

fn is_test_failure_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    let lower = trimmed.to_ascii_lowercase();
    lower.starts_with("fail ")
        || lower.starts_with("failed ")
        || lower.starts_with("failed:")
        || lower.starts_with("--- fail:")
        || lower.starts_with("test result: failed")
        || lower.contains(" tests failed")
        || lower.contains(" test failed")
        || lower.contains(" failures:")
        || trimmed.starts_with('✕')
        || trimmed.starts_with('×')
}

fn is_stack_trace_line(line: &str) -> bool {
    let lower = line.trim_start().to_ascii_lowercase();
    lower.starts_with("at ")
        || lower.starts_with("stack backtrace:")
        || lower.starts_with("traceback ")
        || lower.starts_with("caused by:")
        || lower.starts_with("goroutine ")
}

fn extract_urls(line: &str) -> Vec<String> {
    let mut urls = Vec::new();
    let mut remaining = line;
    while let Some(start) = [remaining.find("http://"), remaining.find("https://")]
        .into_iter()
        .flatten()
        .min()
    {
        let candidate = &remaining[start..];
        let end = candidate
            .find(|character: char| {
                character.is_whitespace() || matches!(character, '<' | '>' | '"' | '\'')
            })
            .unwrap_or(candidate.len());
        let url = candidate[..end]
            .trim_end_matches(['.', ',', ';', ':', ')', ']', '}'])
            .to_string();
        if !url.is_empty() {
            push_unique(&mut urls, url);
        }
        remaining = &candidate[end..];
        if end == 0 {
            break;
        }
    }
    urls
}

fn port_from_url(url: &str) -> Option<u16> {
    let authority = url.split_once("://")?.1.split('/').next()?;
    authority
        .rsplit_once(':')?
        .1
        .trim_end_matches(|character: char| !character.is_ascii_digit())
        .parse::<u16>()
        .ok()
}

fn extract_ports(line: &str) -> Vec<u16> {
    let lower = line.to_ascii_lowercase();
    let mut ports = Vec::new();
    for marker in ["port ", "port: ", "localhost:", "127.0.0.1:", "0.0.0.0:"] {
        let mut remaining = lower.as_str();
        while let Some(index) = remaining.find(marker) {
            let digits = remaining[index + marker.len()..]
                .trim_start()
                .chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>();
            if let Ok(port) = digits.parse::<u16>() {
                if port > 0 {
                    push_unique(&mut ports, port);
                }
            }
            remaining = &remaining[index + marker.len()..];
        }
    }
    ports
}

fn is_error_line(line: &str) -> bool {
    let lower = line.trim_start().to_ascii_lowercase();
    if ["error", "fail", "failed", "panic", "fatal"]
        .iter()
        .any(|prefix| {
            lower.starts_with(&format!("{prefix}:"))
                || lower.starts_with(&format!("{prefix} "))
                || lower.starts_with(&format!("{prefix}["))
        })
    {
        return true;
    }

    lower.starts_with("[error]")
        || lower.starts_with("traceback ")
        || lower.starts_with("uncaught ")
        || lower.starts_with("unhandled ")
        || lower.starts_with("exception ")
        || lower.starts_with("internal server error")
        || lower.contains(": error ")
        || lower.contains(" error:")
        || lower.contains("failed to compile")
        || lower.contains("thread '") && lower.contains(" panicked at ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{PlanTodoItem, PlanTodoStatus, Task, TaskStatus};
    use std::{fs, path::Path};

    #[derive(Clone)]
    struct NoopCommandEmitter;

    impl CommandEventEmitter for NoopCommandEmitter {
        fn emit_log(&self, _event: CommandLogEvent) {}

        fn emit_finished(&self, _event: CommandFinishedEvent) {}
    }

    #[derive(Clone, Default)]
    struct CapturingCommandEmitter {
        finished: Arc<std::sync::Mutex<Vec<CommandFinishedEvent>>>,
    }

    impl CommandEventEmitter for CapturingCommandEmitter {
        fn emit_log(&self, _event: CommandLogEvent) {}

        fn emit_finished(&self, event: CommandFinishedEvent) {
            self.finished.lock().unwrap().push(event);
        }
    }

    fn command_smoke_task(root: &Path) -> Task {
        Task {
            id: "task-command-smoke".to_string(),
            project_path: root.display().to_string(),
            title: "Verify command validation loop".to_string(),
            raw_requirement: "Run failing commands, prepare repairs, rerun, then accept."
                .to_string(),
            status: TaskStatus::Debugging,
            lifecycle: Default::default(),
            selected_planning_agent_ids: Vec::new(),
            primary_agent_id: None,
            review_agent_ids: Vec::new(),
            implementation_review_runs: Vec::new(),
            implementation_reviews: Vec::new(),
            implementation_review_decisions: Vec::new(),
            final_plan: Some("# Plan\n- Validate command loop".to_string()),
            final_plan_path: Some(
                root.join("docs/plans/command-smoke.md")
                    .display()
                    .to_string(),
            ),
            final_plan_html_path: None,
            discussion_summary: None,
            planning_runs: Vec::new(),
            agent_invocations: Vec::new(),
            plan_reviews: Vec::new(),
            planning_decisions: Vec::new(),
            plan_todos: vec![PlanTodoItem {
                id: "todo-command-smoke".to_string(),
                task_id: "task-command-smoke".to_string(),
                title: "Validate command loop".to_string(),
                description: "Exercise failing and succeeding validation commands.".to_string(),
                status: PlanTodoStatus::Done,
                order: 0,
                plan_ref: Some("docs/plans/command-smoke.md".to_string()),
            }],
            loop_trace: Vec::new(),
            events: Vec::new(),
            command_runs: Vec::new(),
            feedback: Vec::new(),
            loop_compact_summary: None,
            repair_context_preview: None,
            git_baseline: None,
            summary: None,
            created_at_ms: now_ms(),
            updated_at_ms: now_ms(),
        }
    }

    fn shell_spec(root: &Path, task_id: &str, script: &str) -> CommandSpec {
        CommandSpec {
            program: "/bin/sh".to_string(),
            args: vec!["-c".to_string(), script.to_string()],
            cwd: root.display().to_string(),
            project_path: Some(root.display().to_string()),
            task_id: Some(task_id.to_string()),
            agent_id: None,
            approval_id: None,
            intent: Some(CommandRunIntent::Validation),
            loop_id: None,
            iteration: None,
            attempt: None,
            termination_reason: None,
        }
    }

    async fn wait_for_run(root: &Path, task_id: &str, run_id: &str) -> Task {
        for _ in 0..60 {
            let task = tasks::load_task(root, task_id).expect("task should remain readable");
            if task
                .command_runs
                .iter()
                .any(|run| run.id == run_id && run.status != "running")
            {
                return task;
            }
            sleep(Duration::from_millis(100)).await;
        }

        panic!("timed out waiting for run {run_id}");
    }

    async fn wait_for_finished_event(
        app: &CapturingCommandEmitter,
        run_id: &str,
    ) -> CommandFinishedEvent {
        for _ in 0..60 {
            if let Some(event) = app
                .finished
                .lock()
                .unwrap()
                .iter()
                .find(|event| event.run_id == run_id)
                .cloned()
            {
                return event;
            }
            sleep(Duration::from_millis(100)).await;
        }
        panic!("timed out waiting for finish event {run_id}");
    }

    fn run_status<'a>(task: &'a Task, run_id: &str) -> &'a str {
        task.command_runs
            .iter()
            .find(|run| run.id == run_id)
            .map(|run| run.status.as_str())
            .expect("run should be recorded")
    }

    #[tokio::test]
    async fn command_runner_drives_two_repair_cycles_then_acceptance() {
        let root = std::env::temp_dir().join(format!("loom-command-runner-smoke-{}", now_ms()));
        fs::create_dir_all(&root).expect("test project should be created");

        let app = NoopCommandEmitter;
        let registry = CommandRegistry::default();
        let ids = IdGenerator::default();
        let task = command_smoke_task(&root);
        tasks::save_task(&task).expect("task should be persisted");

        let cancellable = start_command_run_inner(
            app.clone(),
            &registry,
            &ids,
            shell_spec(&root, &task.id, "sleep 20"),
            0,
        )
        .await
        .expect("cancellable command should start");
        sleep(Duration::from_millis(200)).await;
        let stop_result = stop_command_run_inner(
            &registry,
            cancellable.id.clone(),
            Some("timeout".to_string()),
        )
        .await
        .expect("running command should stop");
        assert!(stop_result.stopped);
        let cancelled_task = tasks::load_task(&root, &task.id).expect("task should be readable");
        assert_eq!(cancelled_task.status, "debugging");
        assert_eq!(run_status(&cancelled_task, &cancellable.id), "cancelled");
        let cancelled_run = cancelled_task
            .command_runs
            .iter()
            .find(|run| run.id == cancellable.id)
            .expect("cancelled run should be recorded");
        assert_eq!(cancelled_run.termination_reason.as_deref(), Some("timeout"));

        let failed_one = start_command_run_inner(
            app.clone(),
            &registry,
            &ids,
            shell_spec(
                &root,
                &task.id,
                "printf 'error: first smoke failure\\n' >&2; sleep 0.2; exit 7",
            ),
            0,
        )
        .await
        .expect("first command should start");
        let failed_one_task = wait_for_run(&root, &task.id, &failed_one.id).await;
        assert_eq!(failed_one_task.status, "debugging");
        assert_eq!(run_status(&failed_one_task, &failed_one.id), "failed");
        assert!(failed_one_task
            .command_runs
            .iter()
            .find(|run| run.id == failed_one.id)
            .and_then(|run| run.error_summary.as_ref())
            .is_some_and(|summary| summary
                .matched_lines
                .iter()
                .any(|line| line.contains("first smoke failure"))));
        let historical_logs = read_command_run_logs(
            root.display().to_string(),
            task.id.clone(),
            failed_one.id.clone(),
        )
        .expect("persisted command logs should be readable after the live stream ends");
        assert!(historical_logs
            .iter()
            .any(|entry| entry.line.contains("first smoke failure")));

        let repair_one =
            tasks::generate_repair_context(root.display().to_string(), task.id.clone())
                .expect("first repair context should be generated");
        assert_eq!(repair_one.status, "fixing");
        assert!(repair_one
            .repair_context_preview
            .as_deref()
            .unwrap_or_default()
            .contains(&failed_one.id));
        assert!(repair_one
            .loop_compact_summary
            .as_deref()
            .unwrap_or_default()
            .contains(&failed_one.id));

        let failed_two = start_command_run_inner(
            app.clone(),
            &registry,
            &ids,
            shell_spec(
                &root,
                &task.id,
                "printf 'panic: second smoke failure\\n' >&2; sleep 0.2; exit 9",
            ),
            0,
        )
        .await
        .expect("second command should start");
        let failed_two_task = wait_for_run(&root, &task.id, &failed_two.id).await;
        assert_eq!(failed_two_task.status, "debugging");
        assert_eq!(run_status(&failed_two_task, &failed_two.id), "failed");

        let repair_two =
            tasks::generate_repair_context(root.display().to_string(), task.id.clone())
                .expect("second repair context should be generated");
        assert_eq!(repair_two.status, "fixing");
        assert!(repair_two
            .repair_context_preview
            .as_deref()
            .unwrap_or_default()
            .contains(&failed_two.id));

        let succeeded = start_command_run_inner(
            app.clone(),
            &registry,
            &ids,
            shell_spec(&root, &task.id, "printf 'ok\\n'; sleep 0.1; exit 0"),
            0,
        )
        .await
        .expect("successful command should start");
        let verifying_task = wait_for_run(&root, &task.id, &succeeded.id).await;
        assert_eq!(verifying_task.status, "verifying");
        assert_eq!(run_status(&verifying_task, &succeeded.id), "succeeded");

        let completed =
            tasks::complete_task_inner(&ids, root.display().to_string(), task.id.clone())
                .expect("verifying task should accept completion");
        assert_eq!(completed.status, "completed");
        assert!(completed
            .events
            .iter()
            .any(|event| { event.evidence_ref.as_deref() == Some(succeeded.id.as_str()) }));
        let summary = completed
            .summary
            .as_ref()
            .expect("delivery summary should persist");
        assert!(Path::new(&summary.json_path).is_file());
        assert!(Path::new(&summary.markdown_path).is_file());
        assert!(completed.events.iter().any(|event| {
            event.evidence_ref.as_deref() == Some(summary.markdown_path.as_str())
        }));
        let exported_path = root.join("exported-summary.md");
        crate::task_summary::export_task_summary(crate::task_summary::ExportTaskSummaryInput {
            project_path: root.display().to_string(),
            task_id: task.id.clone(),
            target_path: exported_path.display().to_string(),
            format: "markdown".to_string(),
        })
        .expect("delivery summary should export");
        assert!(fs::read_to_string(&exported_path)
            .expect("exported summary should be readable")
            .contains("# Verify command validation loop"));
        assert_eq!(completed.command_runs.len(), 4);

        fs::remove_dir_all(root).expect("test project should be cleaned up");
    }

    #[tokio::test]
    async fn command_runner_persists_captured_session_metadata() {
        let root = std::env::temp_dir().join(format!("loom-command-session-{}", now_ms()));
        fs::create_dir_all(&root).expect("test project should be created");

        let app = CapturingCommandEmitter::default();
        let registry = CommandRegistry::default();
        let ids = IdGenerator::default();
        let task = command_smoke_task(&root);
        tasks::save_task(&task).expect("task should be persisted");

        let run = start_command_run_inner(
            app.clone(),
            &registry,
            &ids,
            shell_spec(
                &root,
                &task.id,
                "printf '%s\\n' '{\"session_id\":\"agent-session-1\"}'",
            ),
            0,
        )
        .await
        .expect("session command should start");
        let finished_task = wait_for_run(&root, &task.id, &run.id).await;
        let persisted_run = finished_task
            .command_runs
            .iter()
            .find(|candidate| candidate.id == run.id)
            .expect("finished run should be persisted");

        assert_eq!(persisted_run.status, "succeeded");
        assert_eq!(persisted_run.session_id.as_deref(), Some("agent-session-1"));
        assert_eq!(persisted_run.resume_command, None);

        let event = wait_for_finished_event(&app, &run.id).await;
        assert_eq!(event.session_id.as_deref(), Some("agent-session-1"));
        assert_eq!(event.resume_command, None);

        fs::remove_dir_all(root).expect("test project should be cleaned up");
    }

    #[tokio::test]
    async fn command_runner_times_out_long_running_commands() {
        let root = std::env::temp_dir().join(format!("loom-command-timeout-{}", now_ms()));
        fs::create_dir_all(&root).expect("test project should be created");

        let app = CapturingCommandEmitter::default();
        let registry = CommandRegistry::default();
        let ids = IdGenerator::default();
        let task = command_smoke_task(&root);
        tasks::save_task(&task).expect("task should be persisted");

        let run = start_command_run_inner(
            app.clone(),
            &registry,
            &ids,
            shell_spec(&root, &task.id, "sleep 20"),
            1,
        )
        .await
        .expect("command should start");
        let finished_task = wait_for_run(&root, &task.id, &run.id).await;
        let persisted_run = finished_task
            .command_runs
            .iter()
            .find(|candidate| candidate.id == run.id)
            .expect("finished run should be persisted");

        assert_eq!(persisted_run.status, "failed");
        assert_eq!(persisted_run.termination_reason.as_deref(), Some("timeout"));

        let event = wait_for_finished_event(&app, &run.id).await;
        assert_eq!(event.termination_reason.as_deref(), Some("timeout"));

        fs::remove_dir_all(root).expect("test project should be cleaned up");
    }

    #[tokio::test]
    async fn command_runner_finishes_when_descendant_keeps_stdout_open() {
        let root = std::env::temp_dir().join(format!("loom-command-reader-timeout-{}", now_ms()));
        fs::create_dir_all(&root).expect("test project should be created");

        let app = CapturingCommandEmitter::default();
        let registry = CommandRegistry::default();
        let ids = IdGenerator::default();
        let task = command_smoke_task(&root);
        tasks::save_task(&task).expect("task should be persisted");

        let run = start_command_run_inner(
            app.clone(),
            &registry,
            &ids,
            shell_spec(
                &root,
                &task.id,
                "printf 'parent done\\n'; (sleep 20) & echo $! > bg.pid; exit 0",
            ),
            0,
        )
        .await
        .expect("command with inherited stdout should start");
        let finished_task = wait_for_run(&root, &task.id, &run.id).await;
        assert_eq!(run_status(&finished_task, &run.id), "succeeded");

        if let Ok(pid) = fs::read_to_string(root.join("bg.pid")) {
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                let alive = std::process::Command::new("/bin/kill")
                    .args(["-0", pid.trim()])
                    .output()
                    .unwrap()
                    .status
                    .success();
                if !alive {
                    break;
                }
                if Instant::now() >= deadline {
                    let _ = std::process::Command::new("/bin/kill")
                        .arg(pid.trim())
                        .output();
                    panic!("command runner left its descendant alive");
                }
                sleep(Duration::from_millis(20)).await;
            }
        }
        fs::remove_dir_all(root).expect("test project should be cleaned up");
    }

    #[test]
    fn matches_error_summary_prefixes_case_insensitively() {
        for line in [
            "ERROR: failed build",
            "error[E0308]: mismatched types",
            "[ERROR] build failed",
            "src/main.ts(1,1): error TS2304: Cannot find name",
            "thread 'main' panicked at src/main.rs:1:1",
            "fail test case",
            "Failed assertion",
            "panic: abort",
            "fatal linker error",
            "Traceback (most recent call last):",
            "Uncaught TypeError: Cannot read properties of undefined",
            "Unhandled Runtime Error",
            "Exception in thread main",
            "Internal server error: module failed",
            "Build failed to compile",
        ] {
            assert!(is_error_line(line), "{line} should match");
        }

        assert!(!is_error_line("warning: not fatal yet"));
        assert!(!is_error_line("this line mentions error later"));
    }

    #[tokio::test]
    async fn application_shutdown_finishes_task_command_before_exiting() {
        const CHILD: &str = "LOOM_TASK_SHUTDOWN_TEST";
        if std::env::var(CHILD).as_deref() != Ok("1") {
            let output=std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact","command_runner::tests::application_shutdown_finishes_task_command_before_exiting","--nocapture"])
                .env(CHILD,"1").output().unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        let root = std::env::temp_dir().join(IdGenerator::default().next("loom-task-shutdown"));
        fs::create_dir_all(&root).unwrap();
        let app = CapturingCommandEmitter::default();
        let registry = CommandRegistry::default();
        let ids = IdGenerator::default();
        let task = command_smoke_task(&root);
        tasks::save_task(&task).unwrap();
        let run = start_command_run_inner(
            app,
            &registry,
            &ids,
            shell_spec(
                &root,
                &task.id,
                "trap '' TERM; printf 'SHUTDOWN READY\\n'; while :; do sleep 0.05; done",
            ),
            0,
        )
        .await
        .unwrap();
        let result = crate::chat::shutdown::drain(crate::chat::shutdown::Budget {
            force_after: Duration::from_millis(80),
            deadline: Duration::from_secs(3),
        })
        .await;
        if result.is_err() {
            let _ = process_supervisor::supervisor().force_stop(&run.id);
        }
        result.unwrap();
        let saved = tasks::load_task(&root, &task.id).unwrap();
        let saved = saved
            .command_runs
            .iter()
            .find(|item| item.id == run.id)
            .unwrap();
        assert_eq!(saved.status, CommandRunStatus::Cancelled);
        assert_eq!(saved.termination_reason.as_deref(), Some("app_shutdown"));
        assert!(saved.ended_at_ms.is_some());
        assert!(process_supervisor::supervisor()
            .shutdown_snapshot()
            .unwrap()
            .1
            .is_empty());
        assert!(start_command_run_inner(
            CapturingCommandEmitter::default(),
            &registry,
            &ids,
            shell_spec(&root, &task.id, "printf forbidden"),
            0
        )
        .await
        .is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn keeps_stderr_tail_to_twenty_lines() {
        let lines = (0..25)
            .map(|index| format!("error: line {index}"))
            .collect::<Vec<_>>();
        let summary = analyze_output(Some(1), &[], &lines);

        assert_eq!(summary.stderr_tail.len(), 20);
        assert_eq!(
            summary.stderr_tail.first().map(String::as_str),
            Some("error: line 5")
        );
        assert_eq!(summary.matched_lines.len(), 25);
        assert!(summary.failed);
    }

    #[test]
    fn redacts_command_text_before_persistence() {
        let marker = "marker-value";
        let auth_header = format!("Authorization: {} {}", "Bearer", marker);
        let text = redact_sensitive_text(&command_text("curl", &["-H".to_string(), auth_header]));

        assert!(text.contains("Bearer [REDACTED]"));
        assert!(!text.contains(marker));
    }

    #[test]
    fn analyzes_redacted_stderr_without_leaking_credentials() {
        let marker = "marker-value";
        let password_marker = "marker-password";
        let auth_header = format!("Authorization: {} {}", "Bearer", marker);
        let lines = vec![
            redact_sensitive_text("error: build failed"),
            redact_sensitive_text(&format!("password={password_marker}")),
            redact_sensitive_text(&auth_header),
        ];
        let summary = analyze_output(Some(1), &[], &lines);

        assert!(summary.failed);
        assert!(summary
            .matched_lines
            .contains(&"error: build failed".to_string()));
        assert!(summary
            .stderr_tail
            .iter()
            .any(|line| line.contains("[REDACTED]")));
        assert!(!summary
            .stderr_tail
            .iter()
            .any(|line| line.contains(password_marker)));
        assert!(!summary.stderr_tail.iter().any(|line| line.contains(marker)));
    }

    #[test]
    fn extracts_cross_stack_log_insights() {
        let stdout = vec![
            "VITE ready at http://localhost:1420/".to_string(),
            "warning: unused import in src/main.rs".to_string(),
            "--- FAIL: TestCreateUser (0.02s)".to_string(),
            "FAILED tests/test_api.py::test_create_user".to_string(),
            "00:03 +2 -1: Some Flutter widget test failed".to_string(),
        ];
        let stderr = vec![
            "test result: FAILED. 8 passed; 1 failed".to_string(),
            "Traceback (most recent call last):".to_string(),
            "    at renderApp (/repo/src/app.ts:12:3)".to_string(),
            "Server listening on port 8080".to_string(),
        ];

        let summary = analyze_output(Some(1), &stdout, &stderr);

        assert_eq!(summary.urls, vec!["http://localhost:1420/"]);
        assert!(summary.ports.contains(&1420));
        assert!(summary.ports.contains(&8080));
        assert_eq!(summary.warnings.len(), 1);
        assert!(summary.test_failures.len() >= 4);
        assert_eq!(summary.stack_trace_lines.len(), 2);
    }

    #[test]
    fn records_non_failing_warnings_and_urls_as_findings() {
        let summary = analyze_output(
            Some(0),
            &[
                "Local: https://127.0.0.1:5173/".to_string(),
                "WARN: deprecated option".to_string(),
            ],
            &[],
        );

        assert!(!summary.failed);
        assert!(analysis_has_findings(&summary));
        assert_eq!(summary.urls, vec!["https://127.0.0.1:5173/"]);
        assert_eq!(summary.ports, vec![5173]);
        assert_eq!(summary.warnings.len(), 1);
    }
}
