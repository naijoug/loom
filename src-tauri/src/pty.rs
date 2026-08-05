//! PTY-backed terminal runs for the Testing cockpit.
//!
//! Unlike `command_runner` (which pipes stdout/stderr line-by-line for agent
//! runs), dev-server slots run inside a pseudo-terminal so tools like vite /
//! cargo keep their ANSI colors and progress bars (they check `isatty`). Raw
//! bytes stream to the frontend `loom://pty-output` event for xterm.js; only
//! the *output* bypasses `commandLogs` — the run is still recorded as a normal
//! `CommandRun` (for stop/status/cycles).
//!
//! PTY runs do NOT produce an `errorSummary`: a PTY merges stdout+stderr into a
//! single stream, so the stderr-based `analyze_error` cannot apply. Dev-server
//! errors are surfaced by the human selecting log text (see plan M2), not by
//! exit-code analysis.

use crate::{
    agents::redact_sensitive_text,
    command_runner::CommandRunStopResult,
    execution_policy::{self, ExecutionApprovalRegistry, ExecutionRequest},
    models::{
        now_ms, CommandFinishedEvent, CommandRun, CommandRunIntent, CommandRunStatus, IdGenerator,
    },
    tasks,
};
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    io::{Read, Write},
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::{AppHandle, Emitter, Runtime, State};

#[cfg(unix)]
unsafe extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
}

#[cfg(unix)]
const SIGTERM: i32 = 15;

fn default_rows() -> u16 {
    24
}

fn default_cols() -> u16 {
    80
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PtySpec {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: String,
    #[serde(default)]
    pub project_path: Option<String>,
    pub task_id: Option<String>,
    #[serde(default)]
    pub approval_id: Option<String>,
    #[serde(default = "default_rows")]
    pub rows: u16,
    #[serde(default = "default_cols")]
    pub cols: u16,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PtyOutputEvent {
    pub run_id: String,
    pub task_id: String,
    pub bytes: Vec<u8>,
    pub timestamp_ms: u128,
}

#[derive(Default)]
pub struct PtyRegistry {
    sessions: Arc<Mutex<HashMap<String, PtySession>>>,
}

struct PtySession {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Arc<Mutex<Box<dyn portable_pty::Child + Send + Sync>>>,
    task_id: Option<String>,
    project_path: PathBuf,
    #[cfg(unix)]
    pid: Option<u32>,
}

trait PtyEventEmitter: Clone + Send + Sync + 'static {
    fn emit_output(&self, event: PtyOutputEvent);
    fn emit_finished(&self, event: CommandFinishedEvent);
}

impl<R: Runtime> PtyEventEmitter for AppHandle<R> {
    fn emit_output(&self, event: PtyOutputEvent) {
        let _ = self.emit("loom://pty-output", event);
    }

    fn emit_finished(&self, event: CommandFinishedEvent) {
        let _ = self.emit("loom://command-finished", event);
    }
}

fn command_text(program: &str, args: &[String]) -> String {
    std::iter::once(program.to_string())
        .chain(args.iter().cloned())
        .collect::<Vec<_>>()
        .join(" ")
}

#[tauri::command]
pub async fn start_pty_run(
    app: AppHandle,
    registry: State<'_, PtyRegistry>,
    approvals: State<'_, ExecutionApprovalRegistry>,
    ids: State<'_, IdGenerator>,
    mut spec: PtySpec,
) -> Result<CommandRun, String> {
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
            agent_id: None,
        },
        spec.approval_id.as_deref(),
    )?;
    spec.project_path = Some(authorization.project_path.display().to_string());
    spec.cwd = authorization.cwd.display().to_string();
    start_pty_run_inner(app, registry.inner(), ids.inner(), spec)
}

fn start_pty_run_inner<E: PtyEventEmitter>(
    app: E,
    registry: &PtyRegistry,
    ids: &IdGenerator,
    spec: PtySpec,
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

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: spec.rows,
            cols: spec.cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|error| format!("failed to open pty: {error}"))?;

    let mut builder = CommandBuilder::new(&spec.program);
    builder.args(&spec.args);
    builder.cwd(&spec.cwd);
    // CommandBuilder starts with an empty environment; without this the child
    // would have no PATH and dev-server commands (pnpm/cargo) would not resolve.
    for (key, value) in std::env::vars() {
        builder.env(key, value);
    }

    let child = pair
        .slave
        .spawn_command(builder)
        .map_err(|error| format!("failed to start command: {error}"))?;
    #[cfg(unix)]
    let pid = child.process_id();

    let reader = pair
        .master
        .try_clone_reader()
        .map_err(|error| format!("failed to read pty: {error}"))?;
    let writer = pair
        .master
        .take_writer()
        .map_err(|error| format!("failed to open pty writer: {error}"))?;
    // Drop the slave handle so the master sees EOF once the child exits.
    drop(pair.slave);

    let run = CommandRun {
        id: run_id.clone(),
        task_id: task_id.clone(),
        command: command_text,
        cwd: spec.cwd.clone(),
        intent: CommandRunIntent::Preview,
        loop_id: None,
        iteration: None,
        attempt: None,
        termination_reason: None,
        session_id: None,
        resume_command: None,
        started_at_ms: now_ms(),
        ended_at_ms: None,
        status: CommandRunStatus::Running,
        exit_code: None,
        stdout_log_ref: None,
        stderr_log_ref: None,
        error_summary: None,
    };
    tasks::add_command_run(&project_path, &task_id, run.clone())?;

    let child = Arc::new(Mutex::new(child));
    registry.sessions.lock().unwrap().insert(
        run_id.clone(),
        PtySession {
            master: pair.master,
            writer,
            child: child.clone(),
            task_id: Some(task_id.clone()),
            project_path: project_path.clone(),
            #[cfg(unix)]
            pid,
        },
    );

    spawn_pty_reader(app.clone(), task_id.clone(), run_id.clone(), reader);
    spawn_pty_monitor(
        app,
        registry.sessions.clone(),
        child,
        project_path,
        task_id,
        run_id,
    );

    Ok(run)
}

#[tauri::command]
pub async fn write_pty(
    registry: State<'_, PtyRegistry>,
    run_id: String,
    data: String,
) -> Result<(), String> {
    let mut sessions = registry.sessions.lock().unwrap();
    if let Some(session) = sessions.get_mut(&run_id) {
        session
            .writer
            .write_all(data.as_bytes())
            .map_err(|error| format!("failed to write to pty: {error}"))?;
        let _ = session.writer.flush();
    }
    Ok(())
}

#[tauri::command]
pub async fn resize_pty(
    registry: State<'_, PtyRegistry>,
    run_id: String,
    rows: u16,
    cols: u16,
) -> Result<(), String> {
    let sessions = registry.sessions.lock().unwrap();
    if let Some(session) = sessions.get(&run_id) {
        session
            .master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| format!("failed to resize pty: {error}"))?;
    }
    Ok(())
}

#[tauri::command]
pub async fn stop_pty_run(
    registry: State<'_, PtyRegistry>,
    run_id: String,
) -> Result<CommandRunStopResult, String> {
    stop_pty_run_inner(registry.inner(), run_id, None)
}

fn stop_pty_run_inner(
    registry: &PtyRegistry,
    run_id: String,
    termination_reason: Option<String>,
) -> Result<CommandRunStopResult, String> {
    let Some(session) = registry.sessions.lock().unwrap().remove(&run_id) else {
        return Ok(CommandRunStopResult {
            run_id,
            stopped: false,
            exit_code: None,
        });
    };

    // Kill the whole process group so dev-server grandchildren (e.g. vite →
    // esbuild) don't leak; the child is its own session leader under the pty.
    #[cfg(unix)]
    if let Some(pid) = session.pid {
        unsafe {
            kill(-(pid as i32), SIGTERM);
        }
    }

    let exit_code = {
        let mut child = session.child.lock().unwrap();
        let _ = child.kill();
        child.wait().ok().map(|status| status.exit_code() as i32)
    };

    if let Some(task_id) = session.task_id {
        let _ = tasks::finish_command_run(
            &session.project_path,
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

pub(crate) fn stop_task_runs(
    registry: &PtyRegistry,
    task_id: &str,
    termination_reason: &str,
) -> Result<usize, String> {
    let run_ids = registry
        .sessions
        .lock()
        .map_err(|_| "pty registry is unavailable".to_string())?
        .iter()
        .filter(|(_, session)| session.task_id.as_deref() == Some(task_id))
        .map(|(run_id, _)| run_id.clone())
        .collect::<Vec<_>>();
    let mut stopped = 0;
    for run_id in run_ids {
        let result = stop_pty_run_inner(registry, run_id, Some(termination_reason.to_string()))?;
        stopped += usize::from(result.stopped);
    }
    Ok(stopped)
}

fn spawn_pty_reader<E: PtyEventEmitter>(
    app: E,
    task_id: String,
    run_id: String,
    mut reader: Box<dyn Read + Send>,
) {
    // PTY reads are blocking; run on a dedicated OS thread.
    std::thread::spawn(move || {
        let mut buffer = [0u8; 8192];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => app.emit_output(PtyOutputEvent {
                    run_id: run_id.clone(),
                    task_id: task_id.clone(),
                    bytes: buffer[..count].to_vec(),
                    timestamp_ms: now_ms(),
                }),
                Err(_) => break,
            }
        }
    });
}

fn spawn_pty_monitor<E: PtyEventEmitter>(
    app: E,
    sessions: Arc<Mutex<HashMap<String, PtySession>>>,
    child: Arc<Mutex<Box<dyn portable_pty::Child + Send + Sync>>>,
    project_path: PathBuf,
    task_id: String,
    run_id: String,
) {
    std::thread::spawn(move || {
        let status = { child.lock().unwrap().wait() };

        // If `stop_pty_run` already removed the session, it owns the finish.
        if sessions.lock().unwrap().remove(&run_id).is_none() {
            return;
        }

        let (command_status, exit_code) = match status {
            Ok(status) if status.success() => {
                (CommandRunStatus::Succeeded, Some(status.exit_code() as i32))
            }
            Ok(status) => (CommandRunStatus::Failed, Some(status.exit_code() as i32)),
            Err(_) => (CommandRunStatus::Failed, None),
        };

        let _ = tasks::finish_command_run(
            &project_path,
            &task_id,
            &run_id,
            tasks::CommandRunCompletion {
                status: command_status,
                exit_code,
                error_summary: None,
                session_id: None,
                resume_command: None,
                termination_reason: None,
            },
        );
        app.emit_finished(CommandFinishedEvent {
            task_id,
            run_id,
            status: command_status,
            exit_code,
            error_summary: None,
            session_id: None,
            resume_command: None,
            termination_reason: None,
            timestamp_ms: now_ms(),
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{PlanTodoItem, PlanTodoStatus, Task, TaskStatus};
    use std::{fs, path::Path, time::Duration};

    #[derive(Clone, Default)]
    struct CapturingEmitter {
        output: Arc<Mutex<Vec<u8>>>,
    }

    impl PtyEventEmitter for CapturingEmitter {
        fn emit_output(&self, event: PtyOutputEvent) {
            self.output.lock().unwrap().extend_from_slice(&event.bytes);
        }

        fn emit_finished(&self, _event: CommandFinishedEvent) {}
    }

    fn pty_task(root: &Path) -> Task {
        Task {
            id: "task-pty-smoke".to_string(),
            project_path: root.display().to_string(),
            title: "PTY smoke".to_string(),
            raw_requirement: "Run a pty command.".to_string(),
            status: TaskStatus::Debugging,
            lifecycle: Default::default(),
            selected_planning_agent_ids: Vec::new(),
            primary_agent_id: None,
            review_agent_ids: Vec::new(),
            implementation_review_runs: Vec::new(),
            implementation_reviews: Vec::new(),
            implementation_review_decisions: Vec::new(),
            final_plan: None,
            final_plan_path: None,
            final_plan_html_path: None,
            discussion_summary: None,
            planning_runs: Vec::new(),
            agent_invocations: Vec::new(),
            plan_reviews: Vec::new(),
            planning_decisions: Vec::new(),
            plan_todos: vec![PlanTodoItem {
                id: "todo-pty".to_string(),
                task_id: "task-pty-smoke".to_string(),
                title: "pty".to_string(),
                description: "pty".to_string(),
                status: PlanTodoStatus::Done,
                order: 0,
                plan_ref: None,
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

    fn pty_spec(root: &Path, task_id: &str, script: &str) -> PtySpec {
        PtySpec {
            program: "/bin/sh".to_string(),
            args: vec!["-c".to_string(), script.to_string()],
            cwd: root.display().to_string(),
            project_path: Some(root.display().to_string()),
            task_id: Some(task_id.to_string()),
            approval_id: None,
            rows: 24,
            cols: 80,
        }
    }

    fn run_status(task: &Task, run_id: &str) -> String {
        task.command_runs
            .iter()
            .find(|run| run.id == run_id)
            .map(|run| run.status)
            .expect("run should be recorded")
            .to_string()
    }

    #[test]
    fn pty_run_records_command_and_finishes_successfully() {
        let root = std::env::temp_dir().join(format!("loom-pty-smoke-{}", now_ms()));
        fs::create_dir_all(&root).expect("test project should be created");
        let task = pty_task(&root);
        tasks::save_task(&task).expect("task should be persisted");

        let app = CapturingEmitter::default();
        let registry = PtyRegistry::default();
        let ids = IdGenerator::default();

        let run = start_pty_run_inner(
            app.clone(),
            &registry,
            &ids,
            pty_spec(&root, &task.id, "printf 'hello-pty\\n'; exit 0"),
        )
        .expect("pty run should start");

        let mut finished = None;
        for _ in 0..100 {
            let task = tasks::load_task(&root, &task.id).expect("task readable");
            if run_status(&task, &run.id) != "running" {
                finished = Some(task);
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }

        let task = finished.expect("pty run should finish");
        assert_eq!(run_status(&task, &run.id), "succeeded");
        let output = String::from_utf8_lossy(&app.output.lock().unwrap()).to_string();
        assert!(output.contains("hello-pty"), "captured output: {output:?}");

        fs::remove_dir_all(root).ok();
    }

    #[cfg(unix)]
    #[test]
    fn stop_pty_run_kills_the_whole_process_group() {
        let root = std::env::temp_dir().join(format!("loom-pty-killpg-{}", now_ms()));
        fs::create_dir_all(&root).expect("test project should be created");
        let task = pty_task(&root);
        tasks::save_task(&task).expect("task should be persisted");

        let app = CapturingEmitter::default();
        let registry = PtyRegistry::default();
        let ids = IdGenerator::default();

        // The shell spawns a long-lived grandchild and prints its pid, then waits.
        let run = start_pty_run_inner(
            app.clone(),
            &registry,
            &ids,
            pty_spec(
                &root,
                &task.id,
                "sleep 300 & printf 'GRANDCHILD=%s\\n' \"$!\"; wait",
            ),
        )
        .expect("pty run should start");

        // Wait until the grandchild pid is printed.
        let mut grandchild_pid = None;
        for _ in 0..100 {
            let captured = String::from_utf8_lossy(&app.output.lock().unwrap()).to_string();
            if let Some(rest) = captured.split("GRANDCHILD=").nth(1) {
                if let Some(pid) = rest
                    .split_whitespace()
                    .next()
                    .and_then(|v| v.parse::<i32>().ok())
                {
                    grandchild_pid = Some(pid);
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let grandchild_pid = grandchild_pid.expect("grandchild pid should be printed");

        // The grandchild is alive before stopping.
        assert_eq!(
            unsafe { kill(grandchild_pid, 0) },
            0,
            "grandchild should be alive"
        );

        let result =
            stop_pty_run_inner(&registry, run.id.clone(), None).expect("stop should succeed");
        assert!(result.stopped);

        // After a process-group kill, the grandchild is gone (kill(pid,0) → ESRCH).
        let mut dead = false;
        for _ in 0..40 {
            if unsafe { kill(grandchild_pid, 0) } != 0 {
                dead = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(dead, "grandchild process group should be killed");

        fs::remove_dir_all(root).ok();
    }
}
