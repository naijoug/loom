use crate::{
    agents::redact_sensitive_text,
    models::{
        now_ms, CommandFinishedEvent, CommandLogEvent, CommandRun, CommandSpec, ErrorSummary,
        IdGenerator,
    },
    storage, tasks,
};
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tauri::{AppHandle, Emitter, Runtime, State};
use tokio::{
    fs::OpenOptions,
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, Command},
    sync::Mutex,
    time::{sleep, Duration},
};

#[cfg(unix)]
unsafe extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
    fn setpgid(pid: i32, pgid: i32) -> i32;
}

#[cfg(unix)]
const SIGTERM: i32 = 15;

#[derive(Default)]
pub struct CommandRegistry {
    runs: Arc<Mutex<HashMap<String, ManagedCommandRun>>>,
}

#[derive(Clone)]
struct ManagedCommandRun {
    child: Arc<Mutex<Child>>,
    task_id: Option<String>,
    project_path: PathBuf,
    #[cfg(unix)]
    process_group_id: i32,
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

#[tauri::command]
pub async fn start_command_run(
    app: AppHandle,
    registry: State<'_, CommandRegistry>,
    ids: State<'_, IdGenerator>,
    spec: CommandSpec,
) -> Result<CommandRun, String> {
    start_command_run_inner(app, registry.inner(), ids.inner(), spec).await
}

async fn start_command_run_inner<E: CommandEventEmitter>(
    app: E,
    registry: &CommandRegistry,
    ids: &IdGenerator,
    spec: CommandSpec,
) -> Result<CommandRun, String> {
    if spec.program.trim().is_empty() {
        return Err("command program is required".to_string());
    }

    let task_id = spec
        .task_id
        .clone()
        .ok_or_else(|| "taskId is required for command runs".to_string())?;
    let project_path = PathBuf::from(&spec.cwd);
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

    #[cfg(unix)]
    unsafe {
        command.pre_exec(|| {
            if setpgid(0, 0) == 0 {
                Ok(())
            } else {
                Err(std::io::Error::last_os_error())
            }
        });
    }

    let mut child = command
        .spawn()
        .map_err(|error| format!("failed to start command: {error}"))?;
    let pid = child.id();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stderr_lines = Arc::new(Mutex::new(Vec::new()));

    if let Some(stdout) = stdout {
        spawn_log_reader(
            app.clone(),
            task_id.clone(),
            run_id.clone(),
            "stdout",
            stdout,
            stdout_log.clone(),
            None,
        );
    }

    if let Some(stderr) = stderr {
        spawn_log_reader(
            app.clone(),
            task_id.clone(),
            run_id.clone(),
            "stderr",
            stderr,
            stderr_log.clone(),
            Some(stderr_lines.clone()),
        );
    }

    let run = CommandRun {
        id: run_id.clone(),
        task_id: task_id.clone(),
        command: command_text,
        cwd: spec.cwd.clone(),
        started_at_ms: now_ms(),
        ended_at_ms: None,
        status: "running".to_string(),
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
            #[cfg(unix)]
            process_group_id: pid.unwrap_or_default() as i32,
        },
    );
    spawn_command_monitor(
        app,
        registry.runs.clone(),
        child,
        project_path,
        run.task_id.clone(),
        run.id.clone(),
        stderr_lines,
    );

    Ok(run)
}

#[tauri::command]
pub async fn stop_command_run(
    registry: State<'_, CommandRegistry>,
    run_id: String,
) -> Result<CommandRunStopResult, String> {
    stop_command_run_inner(registry.inner(), run_id).await
}

async fn stop_command_run_inner(
    registry: &CommandRegistry,
    run_id: String,
) -> Result<CommandRunStopResult, String> {
    let Some(managed) = registry.runs.lock().await.remove(&run_id) else {
        return Ok(CommandRunStopResult {
            run_id,
            stopped: false,
            exit_code: None,
        });
    };

    #[cfg(unix)]
    if managed.process_group_id > 0 {
        unsafe {
            kill(-managed.process_group_id, SIGTERM);
        }
    }

    let mut child = managed.child.lock().await;
    let _ = child.kill().await;
    let status = child
        .wait()
        .await
        .map_err(|error| format!("failed to wait for command: {error}"))?;
    let exit_code = status.code();

    if let Some(task_id) = managed.task_id {
        let _ = tasks::finish_command_run(
            &managed.project_path,
            &task_id,
            &run_id,
            "cancelled",
            exit_code,
            None,
        );
    }

    Ok(CommandRunStopResult {
        run_id,
        stopped: true,
        exit_code,
    })
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandRunStopResult {
    run_id: String,
    stopped: bool,
    exit_code: Option<i32>,
}

fn spawn_log_reader<E, Reader>(
    app: E,
    task_id: String,
    run_id: String,
    stream: &'static str,
    reader: Reader,
    log_path: PathBuf,
    captured_lines: Option<Arc<Mutex<Vec<String>>>>,
) where
    E: CommandEventEmitter,
    Reader: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tauri::async_runtime::spawn(async move {
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

            if let Some(file) = log_file.as_mut() {
                let _ = file.write_all(redacted_line.as_bytes()).await;
                let _ = file.write_all(b"\n").await;
            }

            if let Some(captured) = captured_lines.as_ref() {
                captured.lock().await.push(redacted_line.clone());
            }

            app.emit_log(CommandLogEvent {
                task_id: Some(task_id.clone()),
                run_id: run_id.clone(),
                stream: stream.to_string(),
                line: redacted_line,
                timestamp_ms: now_ms(),
            });
        }
    });
}

fn spawn_command_monitor<E: CommandEventEmitter>(
    app: E,
    runs: Arc<Mutex<HashMap<String, ManagedCommandRun>>>,
    child: Arc<Mutex<Child>>,
    project_path: PathBuf,
    task_id: String,
    run_id: String,
    stderr_lines: Arc<Mutex<Vec<String>>>,
) {
    tauri::async_runtime::spawn(async move {
        loop {
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
                let exit_code = status.code();
                let command_status = if status.success() {
                    "succeeded"
                } else {
                    "failed"
                };
                let stderr_lines = stderr_lines.lock().await.clone();
                let error_summary = if status.success() {
                    None
                } else {
                    Some(analyze_error(exit_code, &stderr_lines))
                };
                let _ = tasks::finish_command_run(
                    &project_path,
                    &task_id,
                    &run_id,
                    command_status,
                    exit_code,
                    error_summary.clone(),
                );
                app.emit_finished(CommandFinishedEvent {
                    task_id: task_id.clone(),
                    run_id: run_id.clone(),
                    status: command_status.to_string(),
                    exit_code,
                    error_summary,
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

fn analyze_error(exit_code: Option<i32>, stderr_lines: &[String]) -> ErrorSummary {
    let stderr_tail = stderr_lines
        .iter()
        .rev()
        .take(20)
        .cloned()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>();
    let matched_lines = stderr_lines
        .iter()
        .filter(|line| is_error_line(line))
        .cloned()
        .collect::<Vec<_>>();

    ErrorSummary {
        exit_code,
        stderr_tail,
        matched_lines,
        failed: exit_code.unwrap_or_default() != 0,
    }
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
    use crate::models::{PlanTodoItem, Task};
    use std::{fs, path::Path};

    #[derive(Clone)]
    struct NoopCommandEmitter;

    impl CommandEventEmitter for NoopCommandEmitter {
        fn emit_log(&self, _event: CommandLogEvent) {}

        fn emit_finished(&self, _event: CommandFinishedEvent) {}
    }

    fn command_smoke_task(root: &Path) -> Task {
        Task {
            id: "task-command-smoke".to_string(),
            project_path: root.display().to_string(),
            title: "Verify command validation loop".to_string(),
            raw_requirement: "Run failing commands, prepare repairs, rerun, then accept."
                .to_string(),
            status: "debugging".to_string(),
            selected_planning_agent_ids: Vec::new(),
            primary_agent_id: None,
            review_agent_ids: Vec::new(),
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
                status: "done".to_string(),
                order: 0,
                plan_ref: Some("docs/plans/command-smoke.md".to_string()),
            }],
            events: Vec::new(),
            command_runs: Vec::new(),
            feedback: Vec::new(),
            repair_context_preview: None,
            created_at_ms: now_ms(),
            updated_at_ms: now_ms(),
        }
    }

    fn shell_spec(root: &Path, task_id: &str, script: &str) -> CommandSpec {
        CommandSpec {
            program: "/bin/sh".to_string(),
            args: vec!["-c".to_string(), script.to_string()],
            cwd: root.display().to_string(),
            task_id: Some(task_id.to_string()),
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
        )
        .await
        .expect("cancellable command should start");
        sleep(Duration::from_millis(200)).await;
        let stop_result = stop_command_run_inner(&registry, cancellable.id.clone())
            .await
            .expect("running command should stop");
        assert!(stop_result.stopped);
        let cancelled_task = tasks::load_task(&root, &task.id).expect("task should be readable");
        assert_eq!(cancelled_task.status, "debugging");
        assert_eq!(run_status(&cancelled_task, &cancellable.id), "cancelled");

        let failed_one = start_command_run_inner(
            app.clone(),
            &registry,
            &ids,
            shell_spec(
                &root,
                &task.id,
                "printf 'error: first smoke failure\\n' >&2; sleep 0.2; exit 7",
            ),
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

        let repair_one =
            tasks::generate_repair_context(root.display().to_string(), task.id.clone())
                .expect("first repair context should be generated");
        assert_eq!(repair_one.status, "fixing");
        assert!(repair_one
            .repair_context_preview
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
        assert_eq!(
            completed
                .events
                .last()
                .and_then(|event| event.evidence_ref.as_deref()),
            Some(succeeded.id.as_str())
        );
        assert_eq!(completed.command_runs.len(), 4);

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

    #[test]
    fn keeps_stderr_tail_to_twenty_lines() {
        let lines = (0..25)
            .map(|index| format!("error: line {index}"))
            .collect::<Vec<_>>();
        let summary = analyze_error(Some(1), &lines);

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
        let summary = analyze_error(Some(1), &lines);

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
}
