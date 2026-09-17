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
    /// Chat-first turns (not Task stage). task_id uses `chat:{sessionId}`.
    Chat,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessMetadata {
    pub run_id: String,
    pub task_id: String,
    pub kind: ProcessKind,
    pub process_id: u32,
    pub started_at_ms: u128,
    pub timeout_ms: Option<u64>,
    pub termination_reason: Option<String>,
}

impl ProcessMetadata {
    pub fn new(
        run_id: impl Into<String>,
        task_id: impl Into<String>,
        kind: ProcessKind,
        process_id: u32,
        timeout_ms: Option<u64>,
    ) -> Self {
        Self {
            run_id: run_id.into(),
            task_id: task_id.into(),
            kind,
            process_id,
            started_at_ms: now_ms(),
            timeout_ms,
            termination_reason: None,
        }
    }
}

#[derive(Default)]
pub struct ProcessSupervisor {
    processes: Mutex<HashMap<String, ProcessMetadata>>,
}

impl ProcessSupervisor {
    pub fn register(&self, metadata: ProcessMetadata) -> Result<(), String> {
        self.processes
            .lock()
            .map_err(|_| "process supervisor registry is unavailable".to_string())?
            .insert(metadata.run_id.clone(), metadata);
        Ok(())
    }

    pub fn complete(&self, run_id: &str) -> Option<ProcessMetadata> {
        self.processes.lock().ok()?.remove(run_id)
    }

    #[cfg(test)]
    fn metadata(&self, run_id: &str) -> Option<ProcessMetadata> {
        self.processes.lock().ok()?.get(run_id).cloned()
    }

    pub fn task_runs(&self, task_id: &str, kinds: &[ProcessKind]) -> Vec<ProcessMetadata> {
        self.processes
            .lock()
            .map(|processes| {
                processes
                    .values()
                    .filter(|process| process.task_id == task_id && kinds.contains(&process.kind))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn request_stop(&self, run_id: &str, reason: &str) -> Result<bool, String> {
        let process_id = {
            let mut processes = self
                .processes
                .lock()
                .map_err(|_| "process supervisor registry is unavailable".to_string())?;
            let Some(process) = processes.get_mut(run_id) else {
                return Ok(false);
            };
            process.termination_reason = Some(reason.to_string());
            process.process_id
        };
        terminate_process_group(process_id)?;
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
    #[cfg(unix)]
    unsafe {
        if kill(-(process_id as i32), SIGTERM) != 0 {
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
    fn registry_tracks_task_kind_and_timeout_metadata() {
        let registry = ProcessSupervisor::default();
        registry
            .register(ProcessMetadata::new(
                "run-contract",
                "task-contract",
                ProcessKind::Command,
                u32::MAX,
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
                    u32::MAX,
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
