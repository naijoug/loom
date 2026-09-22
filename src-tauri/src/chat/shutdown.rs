//! Graceful application exit. Only current in-memory ownership may be signalled.
use super::repository::ChatRepository;
use crate::process_supervisor::supervisor;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{
        atomic::{AtomicU8, Ordering},
        Mutex, OnceLock,
    },
    time::Duration,
};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

type RecoveryKey = (PathBuf, String);
fn recovery() -> &'static Mutex<HashMap<RecoveryKey, ChatRepository>> {
    static QUEUE: OnceLock<Mutex<HashMap<RecoveryKey, ChatRepository>>> = OnceLock::new();
    QUEUE.get_or_init(Mutex::default)
}

pub(super) fn remember(repository: ChatRepository, session_id: String) {
    if let Ok(mut queue) = recovery().lock() {
        queue.insert((repository.project_path().into(), session_id), repository);
    }
}

#[derive(Default)]
struct ExitGate {
    phase: AtomicU8,
}
#[derive(Debug, PartialEq, Eq)]
enum ExitAction {
    Start,
    Wait,
    Allow,
}
impl ExitGate {
    fn request(&self) -> ExitAction {
        if self.phase.load(Ordering::Acquire) == 2 {
            return ExitAction::Allow;
        }
        if self
            .phase
            .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            ExitAction::Start
        } else {
            ExitAction::Wait
        }
    }
    fn ready(&self) {
        self.phase.store(2, Ordering::Release);
    }
    fn retry(&self) {
        self.phase.store(0, Ordering::Release);
    }
}
fn gate() -> &'static ExitGate {
    static GATE: OnceLock<ExitGate> = OnceLock::new();
    GATE.get_or_init(ExitGate::default)
}

#[derive(Clone, Copy)]
pub(crate) struct Budget {
    pub force_after: Duration,
    pub deadline: Duration,
}
impl Default for Budget {
    fn default() -> Self {
        Self {
            force_after: Duration::from_secs(2),
            deadline: Duration::from_secs(8),
        }
    }
}

fn capture_and_cancel() -> Result<usize, String> {
    let mut active = 0;
    for repository in ChatRepository::live_repositories()? {
        for id in repository.active_session_ids()? {
            active += 1;
            remember(repository.clone(), id.clone());
            // The lease is signalled before its optional persistence; the final
            // recovery pass will detect/retry any storage failure.
            let _ = repository.cancel_for_shutdown(&id);
        }
    }
    Ok(active)
}

fn flush_recovery() -> Result<(), String> {
    let entries = recovery()
        .lock()
        .map_err(|_| "shutdown recovery queue unavailable")?
        .clone();
    for (key, repository) in entries {
        let session = repository
            .load(&key.1)
            .map_err(|e| format!("会话 {} 无法保存或读取：{e}", key.1))?;
        if session.turn_status == "streaming"
            || session.turns.iter().any(|turn| !turn.status.is_terminal())
        {
            return Err(format!("会话 {} 仍未完成收口", key.1));
        }
        recovery()
            .lock()
            .map_err(|_| "shutdown recovery queue unavailable")?
            .remove(&key);
    }
    Ok(())
}

pub(crate) async fn drain(budget: Budget) -> Result<(), String> {
    supervisor().begin_shutdown()?;
    let started = tokio::time::Instant::now();
    let mut signalled = HashSet::new();
    loop {
        let active_chats = capture_and_cancel()?;
        let (operations, processes) = supervisor().shutdown_snapshot()?;
        let mut failures = Vec::new();
        for process in &processes {
            if signalled.insert(process.run_id.clone()) {
                if let Err(error) =
                    supervisor().request_stop(&process.run_id, super::STOP_REASON_SHUTDOWN)
                {
                    failures.push(error);
                }
            }
            if started.elapsed() >= budget.force_after {
                if let Err(error) = supervisor().force_stop(&process.run_id) {
                    failures.push(error);
                }
            }
        }
        if operations == 0 && processes.is_empty() && active_chats == 0 {
            match flush_recovery() {
                Ok(()) => return Ok(()),
                Err(error) => failures.push(error),
            }
        }
        if started.elapsed() >= budget.deadline {
            return Err(if failures.is_empty() {
                format!(
                    "仍有 {operations} 个执行操作、{} 个进程和 {active_chats} 个聊天回合未结束。",
                    processes.len()
                )
            } else {
                failures.join("\n")
            });
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

fn emergency_stop() {
    let _ = supervisor().begin_shutdown();
    if let Ok((_, processes)) = supervisor().shutdown_snapshot() {
        for process in processes {
            let _ = supervisor().force_stop(&process.run_id);
        }
    }
}

#[cfg(unix)]
struct ExitSignals {
    terminate: tokio::signal::unix::Signal,
    interrupt: tokio::signal::unix::Signal,
}
#[cfg(unix)]
impl ExitSignals {
    fn new() -> std::io::Result<Self> {
        use tokio::signal::unix::{signal, SignalKind};
        Ok(Self {
            terminate: signal(SignalKind::terminate())?,
            interrupt: signal(SignalKind::interrupt())?,
        })
    }
    async fn next(&mut self) -> i32 {
        tokio::select! {_ = self.terminate.recv()=>143,_ = self.interrupt.recv()=>130}
    }
}

#[cfg(unix)]
pub(crate) fn install_signal_handlers(app: &tauri::AppHandle) {
    // Register before entering the GUI event loop; SIGTERM/Ctrl+C then request
    // the same deferred exit as the window/menu, rather than killing it early.
    match tauri::async_runtime::block_on(async { ExitSignals::new() }) {
        Ok(mut signals) => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    app.exit(signals.next().await);
                }
            });
        }
        Err(error) => eprintln!("Loom cannot install graceful signal shutdown: {error}"),
    }
}
#[cfg(not(unix))]
pub(crate) fn install_signal_handlers(_app: &tauri::AppHandle) {}

pub(crate) fn handle_event(app: &tauri::AppHandle, event: tauri::RunEvent) {
    match event {
        tauri::RunEvent::WindowEvent {
            label,
            event: tauri::WindowEvent::CloseRequested { api, .. },
            ..
        } if label == "main" => {
            // Keep the workbench visible until all owned work is drained.
            api.prevent_close();
            app.exit(0);
        }
        tauri::RunEvent::ExitRequested { code, api, .. } => {
            // Tauri explicitly cannot defer its restart path. No restart action
            // is exposed by Loom; retain a process cleanup fallback for it.
            if code == Some(tauri::RESTART_EXIT_CODE) {
                emergency_stop();
                return;
            }
            match gate().request() {
                ExitAction::Allow => {}
                ExitAction::Wait => api.prevent_exit(),
                ExitAction::Start => {
                    api.prevent_exit();
                    // Close admission synchronously, before scheduling cleanup.
                    let admission = supervisor().begin_shutdown();
                    let app = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let result = match admission {
                            Ok(()) => drain(Budget::default()).await,
                            Err(error) => Err(error),
                        };
                        match result {
                            Ok(()) => {
                                gate().ready();
                                app.exit(code.unwrap_or(0));
                            }
                            Err(error) => {
                                gate().retry();
                                app.dialog().message(format!("未能完成退出：{error}\n\n已停止接收新的执行请求，窗口保持打开。请处理原因后再次退出。"))
                                    .title("Loom").kind(MessageDialogKind::Error).show(|_|{});
                            }
                        }
                    });
                }
            }
        }
        tauri::RunEvent::Exit => emergency_stop(),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        chat::{
            service::{self, tests::Fixture},
            ChatEventPayload, ChatLogStream, ChatTurnStatus,
        },
        process_supervisor::{self, ProcessKind, ProcessMetadata},
    };

    #[test]
    fn exit_gate_defers_once_allows_only_after_ready_and_can_retry_failure() {
        let gate = ExitGate::default();
        assert_eq!(gate.request(), ExitAction::Start);
        assert_eq!(gate.request(), ExitAction::Wait);
        gate.retry();
        assert_eq!(gate.request(), ExitAction::Start);
        gate.ready();
        assert_eq!(gate.request(), ExitAction::Allow);
    }

    // Closing the global admission gate is intentionally irreversible in a
    // process. Run these production paths in a separate test process so other
    // parallel tests and the user's desktop application remain untouched.
    #[tokio::test]
    async fn shutdown_cases_in_isolated_processes() {
        const CHILD: &str = "LOOM_SHUTDOWN_TEST_CASE";
        let Ok(case) = std::env::var(CHILD) else {
            for case in ["active", "pending", "late", "storage", "signal"] {
                let output = std::process::Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "chat::shutdown::tests::shutdown_cases_in_isolated_processes",
                        "--nocapture",
                    ])
                    .env(CHILD, case)
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "case {case}:\n{}\n{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            return;
        };
        let budget = Budget {
            force_after: Duration::from_millis(40),
            deadline: Duration::from_secs(3),
        };
        match case.as_str() {
            "signal" => {
                #[cfg(unix)]
                {
                    let mut signals = ExitSignals::new().unwrap();
                    let operation = supervisor().begin_operation().unwrap();
                    let status = std::process::Command::new("/bin/kill")
                        .args(["-TERM", &std::process::id().to_string()])
                        .status()
                        .unwrap();
                    assert!(status.success());
                    let code = tokio::time::timeout(Duration::from_secs(2), signals.next())
                        .await
                        .unwrap();
                    assert_eq!(code, 143);
                    drop(operation);
                    drain(budget).await.unwrap();
                }
                #[cfg(not(unix))]
                {
                    supervisor().begin_shutdown().unwrap();
                }
            }
            "active" => {
                let mut fixtures = Vec::new();
                let mut workers = Vec::new();
                let mut repositories = Vec::new();
                for _ in 0..2 {
                    let fixture = Fixture::new();
                    let (repo, lease, request) = fixture.start("ignore-term");
                    repositories.push(repo.clone());
                    fixtures.push(fixture);
                    let (sender, receiver) = tokio::sync::oneshot::channel();
                    let sender = Mutex::new(Some(sender));
                    workers.push(tokio::spawn(async move {
                        let result=service::run_chat_turn(&repo,&lease,request,|event| {
                            if matches!(event.body,ChatEventPayload::Stream(ref stream) if stream.delta.contains("READY")) {
                                if let Some(sender)=sender.lock().unwrap().take(){let _=sender.send(());}
                            }
                        }).await;
                        service::finish_chat_turn(&repo,&lease,"assistant",None,result);
                    }));
                    tokio::time::timeout(Duration::from_secs(2), receiver)
                        .await
                        .unwrap()
                        .unwrap();
                }
                drain(budget).await.unwrap();
                for worker in workers {
                    worker.await.unwrap();
                }
                for repo in repositories {
                    let session = repo.load("session").unwrap();
                    let turn = &session.turns[0];
                    assert_eq!(turn.status, ChatTurnStatus::Cancelled);
                    assert_eq!(turn.termination_reason.as_deref(), Some("app_shutdown"));
                    assert!(session.messages[1].content.contains("READY"));
                    assert!(repo
                        .read_run_logs("session", &turn.id, ChatLogStream::Stdout, 0, 32768)
                        .unwrap()
                        .text
                        .contains("READY"));
                    assert!(repo.begin("session", "new-turn", |_| Ok(())).is_err());
                }
                assert_eq!(supervisor().shutdown_snapshot().unwrap(), (0, Vec::new()));
                drop(fixtures);
            }
            "pending" => {
                let operation = supervisor().begin_operation().unwrap();
                assert!(drain(Budget {
                    force_after: Duration::from_millis(10),
                    deadline: Duration::from_millis(60)
                })
                .await
                .is_err());
                assert!(supervisor().begin_operation().is_err());
                drop(operation);
                drain(budget).await.unwrap();
            }
            "late" => {
                let mut operation = supervisor().begin_operation().unwrap();
                let worker = tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    let mut command = tokio::process::Command::new("sh");
                    command.args(["-c", "sleep 30"]).kill_on_drop(true);
                    process_supervisor::configure_process_group(&mut command);
                    let mut child = command.spawn().unwrap();
                    operation
                        .register(ProcessMetadata::unscoped(
                            "late-worker",
                            ProcessKind::Command,
                            child.id().unwrap(),
                            None,
                        ))
                        .unwrap();
                    assert_eq!(
                        supervisor()
                            .metadata("late-worker")
                            .unwrap()
                            .termination_reason
                            .as_deref(),
                        Some("app_shutdown")
                    );
                    assert!(!child.wait().await.unwrap().success());
                    supervisor().complete("late-worker");
                });
                let started = tokio::time::Instant::now();
                drain(budget).await.unwrap();
                assert!(started.elapsed() >= Duration::from_millis(100));
                worker.await.unwrap();
                assert_eq!(supervisor().shutdown_snapshot().unwrap(), (0, Vec::new()));
            }
            "storage" => {
                let fixture = Fixture::new();
                let (repo, lease, request) = fixture.start("audit-logs");
                let result = service::run_chat_turn(&repo, &lease, request, |_| {}).await;
                let path = fixture
                    .path
                    .join(".loom/chat/v2/sessions/session/snapshot.json");
                let original = std::fs::read_to_string(&path).unwrap();
                let mut future: serde_json::Value = serde_json::from_str(&original).unwrap();
                future["schemaVersion"] = serde_json::json!(99);
                let future = serde_json::to_string(&future).unwrap();
                std::fs::write(&path, &future).unwrap();
                supervisor().begin_shutdown().unwrap();
                assert!(
                    service::finish_chat_turn(&repo, &lease, "assistant", None, result)
                        .journal_event
                        .is_none()
                );
                drop(lease);
                assert!(drain(Budget {
                    force_after: Duration::from_millis(10),
                    deadline: Duration::from_millis(60)
                })
                .await
                .is_err());
                assert_eq!(std::fs::read_to_string(&path).unwrap(), future);
                std::fs::write(&path, original).unwrap();
                drain(budget).await.unwrap();
                let session = repo.load("session").unwrap();
                assert_eq!(session.turns[0].status, ChatTurnStatus::Interrupted);
                assert!(session.messages[1].content.contains("first 中 output"));
            }
            _ => panic!("unexpected shutdown case"),
        }
        assert!(supervisor().is_shutting_down());
    }
}
