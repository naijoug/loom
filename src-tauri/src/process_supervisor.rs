use crate::models::now_ms;
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};
use tokio::process::Command as TokioCommand;

#[cfg(unix)]
unsafe extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
    fn setpgid(pid: i32, pgid: i32) -> i32;
}

#[cfg(unix)]
const SIGTERM: i32 = 15;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessKind {
    Agent,
    Command,
    ImplementationReview,
    Pty,
    /// Chat turns have their own project/session/turn owner, not a Task id.
    Chat,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ProcessOwner {
    Unscoped,
    Task {
        task_id: String,
    },
    Chat {
        project_key: String,
        session_id: String,
        turn_id: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessMetadata {
    pub run_id: String,
    pub owner: ProcessOwner,
    pub kind: ProcessKind,
    pub process_id: u32,
    pub started_at_ms: u128,
    pub timeout_ms: Option<u64>,
    pub termination_reason: Option<String>,
}

impl ProcessMetadata {
    pub fn unscoped(
        run_id: impl Into<String>,
        kind: ProcessKind,
        process_id: u32,
        timeout_ms: Option<u64>,
    ) -> Self {
        Self {
            run_id: run_id.into(),
            owner: ProcessOwner::Unscoped,
            kind,
            process_id,
            started_at_ms: now_ms(),
            timeout_ms,
            termination_reason: None,
        }
    }
    pub fn new(
        run_id: impl Into<String>,
        task_id: impl Into<String>,
        kind: ProcessKind,
        process_id: u32,
        timeout_ms: Option<u64>,
    ) -> Self {
        Self {
            run_id: run_id.into(),
            owner: ProcessOwner::Task {
                task_id: task_id.into(),
            },
            kind,
            process_id,
            started_at_ms: now_ms(),
            timeout_ms,
            termination_reason: None,
        }
    }

    pub fn chat(
        run_id: impl Into<String>,
        project_key: impl Into<String>,
        session_id: impl Into<String>,
        process_id: u32,
        timeout_ms: u64,
    ) -> Self {
        let run_id = run_id.into();
        Self {
            owner: ProcessOwner::Chat {
                project_key: project_key.into(),
                session_id: session_id.into(),
                turn_id: run_id.clone(),
            },
            run_id,
            kind: ProcessKind::Chat,
            process_id,
            started_at_ms: now_ms(),
            timeout_ms: Some(timeout_ms),
            termination_reason: None,
        }
    }
}

#[derive(Default)]
pub struct ProcessSupervisor {
    processes: Mutex<HashMap<String, ProcessMetadata>>,
    lifecycle: Mutex<Lifecycle>,
}

#[derive(Default)]
struct Lifecycle {
    closing: bool,
    operations: usize,
}

/// Admitted work remains visible across the spawn/register/final-save gaps.
pub struct OperationGuard<'a> {
    supervisor: &'a ProcessSupervisor,
    run_id: Option<String>,
}
impl OperationGuard<'_> {
    pub fn register(&mut self, metadata: ProcessMetadata) -> Result<(), String> {
        if self.run_id.is_some() {
            return Err("execution operation already owns a process".into());
        }
        let id = metadata.run_id.clone();
        self.supervisor.register_inner(metadata, true)?;
        self.run_id = Some(id);
        Ok(())
    }
}
impl Drop for OperationGuard<'_> {
    fn drop(&mut self) {
        if let Some(id) = &self.run_id {
            // A completed registration is already absent: never signal its old PID.
            if self.supervisor.force_stop(id).is_ok() {
                self.supervisor.complete(id);
            }
        }
        if let Ok(mut state) = self.supervisor.lifecycle.lock() {
            state.operations = state.operations.saturating_sub(1);
        }
    }
}

impl ProcessSupervisor {
    pub fn begin_operation(&self) -> Result<OperationGuard<'_>, String> {
        let mut state = self
            .lifecycle
            .lock()
            .map_err(|_| "process lifecycle unavailable")?;
        if state.closing {
            return Err("Loom 正在退出，无法开始新的执行操作。".into());
        }
        state.operations += 1;
        Ok(OperationGuard {
            supervisor: self,
            run_id: None,
        })
    }
    pub fn begin_shutdown(&self) -> Result<(), String> {
        self.lifecycle
            .lock()
            .map_err(|_| "process lifecycle unavailable")?
            .closing = true;
        Ok(())
    }

    pub fn begin_control_operation(
        &self,
        run_id: &str,
    ) -> Result<Option<OperationGuard<'_>>, String> {
        let mut state = self
            .lifecycle
            .lock()
            .map_err(|_| "process lifecycle unavailable")?;
        let processes = self
            .processes
            .lock()
            .map_err(|_| "process registry unavailable")?;
        if !processes.contains_key(run_id) {
            return Ok(None);
        }
        state.operations += 1;
        Ok(Some(OperationGuard {
            supervisor: self,
            run_id: None,
        }))
    }
    pub fn is_shutting_down(&self) -> bool {
        self.lifecycle.lock().map_or(true, |state| state.closing)
    }
    pub fn shutdown_snapshot(&self) -> Result<(usize, Vec<ProcessMetadata>), String> {
        let state = self
            .lifecycle
            .lock()
            .map_err(|_| "process lifecycle unavailable")?;
        let processes = self
            .processes
            .lock()
            .map_err(|_| "process registry unavailable")?;
        Ok((state.operations, processes.values().cloned().collect()))
    }
    pub fn register(&self, metadata: ProcessMetadata) -> Result<(), String> {
        self.register_inner(metadata, false)
    }
    fn register_inner(&self, mut metadata: ProcessMetadata, admitted: bool) -> Result<(), String> {
        if metadata.process_id <= 1 || metadata.process_id > i32::MAX as u32 {
            return Err("invalid owned process group id".into());
        }
        let lifecycle = self
            .lifecycle
            .lock()
            .map_err(|_| "process lifecycle unavailable")?;
        if lifecycle.closing && !admitted {
            return Err("Loom is shutting down".into());
        }
        if lifecycle.closing {
            metadata
                .termination_reason
                .get_or_insert_with(|| "app_shutdown".into());
        }
        let mut processes = self
            .processes
            .lock()
            .map_err(|_| "process supervisor registry is unavailable".to_string())?;
        if processes.contains_key(&metadata.run_id) {
            return Err(format!(
                "process run id '{}' is already registered",
                metadata.run_id
            ));
        }
        processes.insert(metadata.run_id.clone(), metadata);
        Ok(())
    }

    pub fn complete(&self, run_id: &str) -> Option<ProcessMetadata> {
        self.processes.lock().ok()?.remove(run_id)
    }

    pub fn metadata(&self, run_id: &str) -> Option<ProcessMetadata> {
        self.processes.lock().ok()?.get(run_id).cloned()
    }

    pub fn task_runs(&self, task_id: &str, kinds: &[ProcessKind]) -> Vec<ProcessMetadata> {
        self.processes
            .lock()
            .map(|processes| {
                processes
                    .values()
                    .filter(|process| matches!(&process.owner, ProcessOwner::Task { task_id: owner } if owner == task_id) && kinds.contains(&process.kind))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn request_stop(&self, run_id: &str, reason: &str) -> Result<bool, String> {
        let mut processes = self
            .processes
            .lock()
            .map_err(|_| "process supervisor registry is unavailable".to_string())?;
        let Some(process) = processes.get_mut(run_id) else {
            return Ok(false);
        };
        process
            .termination_reason
            .get_or_insert_with(|| reason.to_string());
        terminate_process_group(process.process_id)?;
        Ok(true)
    }

    pub fn force_stop(&self, run_id: &str) -> Result<bool, String> {
        let processes = self
            .processes
            .lock()
            .map_err(|_| "process registry unavailable")?;
        let Some(process) = processes.get(run_id) else {
            return Ok(false);
        };
        force_terminate_process_group(process.process_id)?;
        Ok(true)
    }

    pub fn stop_task_runs(
        &self,
        task_id: &str,
        kinds: &[ProcessKind],
        reason: &str,
    ) -> Result<Vec<String>, String> {
        let runs = self.task_runs(task_id, kinds);
        for run in &runs {
            let _ = self.request_stop(&run.run_id, reason)?;
        }
        Ok(runs.into_iter().map(|run| run.run_id).collect())
    }
}

pub fn supervisor() -> &'static ProcessSupervisor {
    static SUPERVISOR: OnceLock<ProcessSupervisor> = OnceLock::new();
    SUPERVISOR.get_or_init(ProcessSupervisor::default)
}

pub fn configure_process_group(command: &mut TokioCommand) {
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
}

pub fn terminate_process_group(process_id: u32) -> Result<(), String> {
    signal_process_group(process_id, false)
}

pub fn force_terminate_process_group(process_id: u32) -> Result<(), String> {
    signal_process_group(process_id, true)
}

fn signal_process_group(process_id: u32, force: bool) -> Result<(), String> {
    if process_id <= 1 || process_id > i32::MAX as u32 {
        return Err("invalid process group id".into());
    }
    #[cfg(unix)]
    unsafe {
        let signal = if force { 9 } else { SIGTERM };
        if kill(-(process_id as i32), signal) != 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() != Some(3) {
                return Err(format!(
                    "failed to terminate process group {process_id}: {error}"
                ));
            }
        }
    }

    #[cfg(windows)]
    {
        let _ = force;
        let status = std::process::Command::new("taskkill")
            .args(["/PID", &process_id.to_string(), "/T", "/F"])
            .status()
            .map_err(|error| format!("failed to start taskkill for {process_id}: {error}"))?;
        if !status.success() {
            return Err(format!("taskkill failed for process {process_id}"));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_ownership_is_not_a_synthetic_task_and_duplicate_ids_do_not_overwrite() {
        let registry = ProcessSupervisor::default();
        let original = ProcessMetadata::chat("chat-run", "/project-a", "session-a", 42, 1000);
        registry.register(original.clone()).unwrap();
        assert!(registry
            .task_runs("chat:session-a", &[ProcessKind::Chat])
            .is_empty());
        assert!(
            matches!(registry.metadata("chat-run").unwrap().owner, ProcessOwner::Chat { project_key, session_id, turn_id } if project_key == "/project-a" && session_id == "session-a" && turn_id == "chat-run")
        );
        assert!(registry
            .register(ProcessMetadata::new(
                "chat-run",
                "some-task",
                ProcessKind::Command,
                99,
                None
            ))
            .is_err());
        assert_eq!(registry.complete("chat-run"), Some(original));
    }

    #[test]
    fn invalid_process_groups_are_rejected_before_signalling() {
        for pid in [0, 1, u32::MAX] {
            assert!(terminate_process_group(pid).is_err());
            assert!(force_terminate_process_group(pid).is_err());
        }
    }

    #[test]
    fn closing_admission_tracks_non_process_work_and_rejects_new_operations() {
        let registry = ProcessSupervisor::default();
        let operation = registry.begin_operation().unwrap();
        registry.begin_shutdown().unwrap();
        assert!(registry.begin_operation().is_err());
        assert!(registry
            .register(ProcessMetadata::unscoped(
                "unadmitted",
                ProcessKind::Command,
                42,
                None
            ))
            .is_err());
        assert_eq!(registry.shutdown_snapshot().unwrap().0, 1);
        drop(operation);
        assert_eq!(registry.shutdown_snapshot().unwrap().0, 0);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn admitted_late_registration_and_guard_drop_cleanup_are_owned_and_bounded() {
        let registry = ProcessSupervisor::default();
        let mut operation = registry.begin_operation().unwrap();
        registry.begin_shutdown().unwrap();
        let mut command = TokioCommand::new("sh");
        command.args(["-c", "sleep 30"]).kill_on_drop(true);
        configure_process_group(&mut command);
        let mut child = command.spawn().unwrap();
        operation
            .register(ProcessMetadata::unscoped(
                "late",
                ProcessKind::Command,
                child.id().unwrap(),
                None,
            ))
            .unwrap();
        assert_eq!(
            registry
                .metadata("late")
                .unwrap()
                .termination_reason
                .as_deref(),
            Some("app_shutdown")
        );
        drop(operation);
        let status = tokio::time::timeout(std::time::Duration::from_secs(2), child.wait())
            .await
            .unwrap()
            .unwrap();
        assert!(!status.success());
        assert_eq!(registry.shutdown_snapshot().unwrap(), (0, Vec::new()));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn completed_registration_is_not_signalled_again_when_guard_drops() {
        let registry = ProcessSupervisor::default();
        let mut operation = registry.begin_operation().unwrap();
        let mut command = TokioCommand::new("sh");
        command.args(["-c", "sleep 30"]).kill_on_drop(true);
        configure_process_group(&mut command);
        let mut child = command.spawn().unwrap();
        operation
            .register(ProcessMetadata::unscoped(
                "released",
                ProcessKind::Command,
                child.id().unwrap(),
                None,
            ))
            .unwrap();
        registry.complete("released");
        drop(operation);
        assert!(
            child.try_wait().unwrap().is_none(),
            "an unregistered PID was signalled"
        );
        force_terminate_process_group(child.id().unwrap()).unwrap();
        let _ = child.wait().await;
    }

    #[test]
    fn registry_tracks_task_kind_and_timeout_metadata() {
        let registry = ProcessSupervisor::default();
        registry
            .register(ProcessMetadata::new(
                "run-contract",
                "task-contract",
                ProcessKind::Command,
                i32::MAX as u32,
                Some(5_000),
            ))
            .expect("register process");

        let task_runs = registry.task_runs("task-contract", &[ProcessKind::Command]);
        assert_eq!(task_runs.len(), 1);
        assert_eq!(task_runs[0].timeout_ms, Some(5_000));

        let completed = registry.complete("run-contract").expect("complete process");
        assert_eq!(completed.run_id, "run-contract");
        assert!(registry.metadata("run-contract").is_none());
    }

    #[test]
    fn task_run_queries_stay_scoped_by_process_kind() {
        let registry = ProcessSupervisor::default();
        for (run_id, kind) in [
            ("agent-run", ProcessKind::Agent),
            ("review-run", ProcessKind::ImplementationReview),
            ("pty-run", ProcessKind::Pty),
        ] {
            registry
                .register(ProcessMetadata::new(
                    run_id,
                    "task-contract",
                    kind,
                    i32::MAX as u32,
                    None,
                ))
                .expect("register process");
        }

        assert_eq!(
            registry
                .task_runs(
                    "task-contract",
                    &[ProcessKind::Agent, ProcessKind::ImplementationReview],
                )
                .len(),
            2
        );
        assert_eq!(
            registry
                .task_runs("task-contract", &[ProcessKind::Pty])
                .len(),
            1
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn stop_request_records_reason_and_terminates_the_process_group() {
        let registry = ProcessSupervisor::default();
        let mut command = TokioCommand::new("sh");
        command.args(["-c", "sleep 30"]).kill_on_drop(true);
        configure_process_group(&mut command);
        let mut child = command.spawn().expect("spawn supervised process");
        let process_id = child.id().expect("child process id");
        registry
            .register(ProcessMetadata::new(
                "run-stop-contract",
                "task-stop-contract",
                ProcessKind::Command,
                process_id,
                Some(30_000),
            ))
            .unwrap();

        assert!(registry
            .request_stop("run-stop-contract", "timeout")
            .unwrap());
        let status = tokio::time::timeout(std::time::Duration::from_secs(2), child.wait())
            .await
            .expect("process should stop promptly")
            .expect("wait for stopped process");
        assert!(!status.success());
        assert_eq!(
            registry
                .metadata("run-stop-contract")
                .and_then(|metadata| metadata.termination_reason),
            Some("timeout".to_string())
        );
        registry.complete("run-stop-contract");
    }
}
