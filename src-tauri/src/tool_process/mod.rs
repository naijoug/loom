//! Bounded non-Agent stdio sessions. Uses the application's sole supervisor.
//! Callers supply argv, never shell text, and retain the session until cleanup.
pub use crate::process_supervisor::ProcessOwner;
use crate::process_supervisor::{self, OperationGuard, ProcessKind, ProcessMetadata};
use std::{collections::BTreeMap, path::PathBuf, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::mpsc,
    task::JoinHandle,
    time::Instant,
};

/// Bounded, no-symlink accounting for a host-owned file or directory. A missing
/// path consumes zero bytes; permission/shape failures never look like zero.
pub fn directory_bytes(path: &std::path::Path, limit: u64) -> Result<u64, String> {
    fn visit(
        path: &std::path::Path,
        depth: usize,
        count: &mut usize,
        bytes: &mut u64,
        limit: u64,
    ) -> Result<(), String> {
        let meta = match std::fs::symlink_metadata(path) {
            Ok(meta) => meta,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(format!("disk_accounting_failed: {e}")),
        };
        *count += 1;
        if depth > 64 || *count > 100_000 || meta.file_type().is_symlink() {
            return Err("unsafe_or_excessive_disk_tree".into());
        }
        if meta.is_file() {
            *bytes = bytes
                .checked_add(meta.len())
                .ok_or("disk_budget_overflow")?;
            if *bytes > limit {
                return Err("disk_budget_exhausted".into());
            }
        } else if meta.is_dir() {
            let entries = match std::fs::read_dir(path) {
                Ok(entries) => entries,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                Err(error) => return Err(error.to_string()),
            };
            for entry in entries {
                visit(
                    &entry.map_err(|e| e.to_string())?.path(),
                    depth + 1,
                    count,
                    bytes,
                    limit,
                )?;
            }
        } else {
            return Err("unsafe_disk_entry".into());
        }
        Ok(())
    }
    let mut bytes = 0;
    visit(path, 0, &mut 0, &mut bytes, limit)?;
    Ok(bytes)
}

pub async fn confirm_exit(process_id: u32) -> Result<(), String> {
    #[cfg(unix)]
    {
        let deadline = Instant::now() + Duration::from_secs(2);
        while process_supervisor::process_group_exists(process_id)? {
            if Instant::now() >= deadline {
                return Err("tool_cleanup_timeout: descendants remain".into());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = process_id;
        Err("group_confirmation_unsupported".into())
    }
}

pub struct Spec {
    pub operation_id: String,
    pub owner: ProcessOwner,
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub environment: BTreeMap<String, String>,
    pub timeout: Duration,
    pub max_frame_bytes: usize,
    pub max_output_bytes: usize,
    pub disk_limit: Option<u64>,
}

pub struct Session {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: ChildStdout,
    stderr: mpsc::Receiver<Result<Vec<u8>, String>>,
    stderr_task: JoinHandle<()>,
    operation: OperationGuard<'static>,
    id: String,
    deadline: Instant,
    frame_limit: usize,
    output_limit: usize,
    output_bytes: usize,
    pending: Vec<u8>,
    diagnostics: Vec<u8>,
    closed: bool,
    disk_limit: Option<(PathBuf, u64)>,
    last_disk_check: std::cell::Cell<Instant>,
}

impl Session {
    pub fn spawn(spec: Spec) -> Result<Self, String> {
        if spec.timeout.is_zero()
            || spec.max_frame_bytes == 0
            || spec.max_frame_bytes > 64 * 1024 * 1024
            || spec.max_output_bytes < spec.max_frame_bytes
            || spec.max_output_bytes > 128 * 1024 * 1024
            || !spec.program.is_absolute()
            || !spec.cwd.is_absolute()
            || spec.operation_id.is_empty()
            || matches!(spec.owner, ProcessOwner::Unscoped)
        {
            return Err("invalid_tool_process_spec".into());
        }
        let mut operation = process_supervisor::supervisor().begin_operation()?;
        if let Some(limit) = spec.disk_limit {
            directory_bytes(&spec.cwd, limit)?;
        }
        let mut command = Command::new(&spec.program);
        command
            .args(&spec.args)
            .envs(&spec.environment)
            .current_dir(&spec.cwd)
            .kill_on_drop(true)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        process_supervisor::configure_process_group(&mut command);
        let mut child = command
            .spawn()
            .map_err(|e| format!("tool_spawn_failed: {e}"))?;
        let pid = child.id().ok_or("tool_pid_missing")?;
        if let Err(error) = operation.register(ProcessMetadata::owned(
            &spec.operation_id,
            spec.owner,
            ProcessKind::Tool,
            pid,
            spec.timeout.as_millis().min(u64::MAX as u128) as u64,
        )) {
            process_supervisor::force_terminate_process_group(pid)?;
            return Err(error);
        }
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().ok_or("tool_stdout_missing")?;
        let mut stderr = child.stderr.take().ok_or("tool_stderr_missing")?;
        // Backpressure bounds queued diagnostics even if the caller is idle.
        let (sender, receiver) = mpsc::channel(8);
        let stderr_task = tokio::spawn(async move {
            let mut bytes = [0; 8192];
            loop {
                let result = match stderr.read(&mut bytes).await {
                    Ok(0) => break,
                    Ok(n) => Ok(bytes[..n].to_vec()),
                    Err(e) => Err(format!("tool_stderr_failed: {e}")),
                };
                let failed = result.is_err();
                if sender.send(result).await.is_err() || failed {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            stdin,
            stdout,
            stderr: receiver,
            stderr_task,
            operation,
            id: spec.operation_id,
            deadline: Instant::now() + spec.timeout,
            frame_limit: spec.max_frame_bytes,
            output_limit: spec.max_output_bytes,
            output_bytes: 0,
            pending: Vec::new(),
            diagnostics: Vec::new(),
            closed: false,
            disk_limit: spec.disk_limit.map(|limit| (spec.cwd, limit)),
            last_disk_check: std::cell::Cell::new(Instant::now()),
        })
    }

    fn check(&self, cancelled: &impl Fn() -> bool) -> Result<(), String> {
        if self.closed {
            return Err("tool_session_closed".into());
        }
        if cancelled() {
            return Err("tool_cancelled".into());
        }
        if process_supervisor::supervisor().is_shutting_down()
            || process_supervisor::supervisor()
                .metadata(&self.id)
                .is_some_and(|m| m.termination_reason.is_some())
        {
            return Err("tool_stopped".into());
        }
        if Instant::now() >= self.deadline {
            return Err("tool_timeout".into());
        }
        if self.last_disk_check.get().elapsed() >= Duration::from_millis(250) {
            self.check_disk()?;
            self.last_disk_check.set(Instant::now());
        }
        Ok(())
    }
    fn check_disk(&self) -> Result<(), String> {
        if let Some((root, limit)) = &self.disk_limit {
            directory_bytes(root, *limit)?;
        }
        Ok(())
    }

    pub async fn send(&mut self, bytes: &[u8], cancelled: impl Fn() -> bool) -> Result<(), String> {
        if bytes.len() > 1024 * 1024 {
            return Err("tool_request_limit".into());
        }
        let mut offset = 0;
        while offset < bytes.len() {
            self.check(&cancelled)?;
            tokio::select! {
                result = self.stdin.as_mut().ok_or("tool_stdin_closed")?.write(&bytes[offset..]) => {
                    let n = result.map_err(|e| format!("tool_stdin_failed: {e}"))?;
                    if n == 0 { return Err("tool_stdin_closed".into()); }
                    offset += n;
                }
                _ = tokio::time::sleep(Duration::from_millis(25)) => {}
            }
        }
        Ok(())
    }

    fn account(&mut self, n: usize) -> Result<(), String> {
        self.output_bytes = self.output_bytes.saturating_add(n);
        if self.output_bytes > self.output_limit {
            Err("tool_output_limit".into())
        } else {
            Ok(())
        }
    }

    /// A frame must end with a newline. Truncated EOF is never a successful result.
    pub async fn frame(&mut self, cancelled: impl Fn() -> bool) -> Result<Vec<u8>, String> {
        let mut bytes = [0; 8192];
        loop {
            self.check(&cancelled)?;
            if let Some(end) = self.pending.iter().position(|b| *b == b'\n') {
                if end > self.frame_limit {
                    return Err("tool_frame_limit".into());
                }
                let mut line: Vec<_> = self.pending.drain(..=end).collect();
                line.pop();
                return Ok(line);
            }
            if self.pending.len() > self.frame_limit {
                return Err("tool_frame_limit".into());
            }
            tokio::select! {
                read = self.stdout.read(&mut bytes) => {
                    let n = read.map_err(|e| format!("tool_stdout_failed: {e}"))?;
                    if n == 0 { return Err(if self.pending.is_empty() { "tool_eof" } else { "tool_truncated_frame" }.into()); }
                    self.account(n)?;
                    self.pending.extend_from_slice(&bytes[..n]);
                }
                item = self.stderr.recv(), if !self.stderr.is_closed() || !self.stderr.is_empty() => {
                    if let Some(item) = item {
                        let data = item?;
                        self.account(data.len())?;
                        let remaining = (64 * 1024usize).saturating_sub(self.diagnostics.len());
                        self.diagnostics.extend_from_slice(&data[..data.len().min(remaining)]);
                    }
                }
                _ = tokio::time::sleep(Duration::from_millis(25)) => {}
            }
        }
    }

    /// Raw diagnostics stay inside the host; callers must redact before persistence.
    pub fn diagnostics(&self) -> String {
        String::from_utf8_lossy(&self.diagnostics).into_owned()
    }
    pub async fn wait_exit(&mut self, cancelled: impl Fn() -> bool) -> Result<Option<i32>, String> {
        loop {
            self.check(&cancelled)?;
            if let Some(status) = self
                .child
                .try_wait()
                .map_err(|e| format!("tool_reap_failed: {e}"))?
            {
                return Ok(status.code());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    pub async fn close(&mut self) -> Result<Option<i32>, String> {
        if self.closed {
            return Ok(self.child.try_wait().ok().flatten().and_then(|s| s.code()));
        }
        self.stdin.take();
        let disk_result = self.check_disk();
        let registry = process_supervisor::supervisor();
        let exited = self
            .child
            .try_wait()
            .map_err(|e| format!("tool_reap_failed: {e}"))?;
        if exited.is_none() {
            if let Err(error) = registry.request_stop(&self.id, "tool_cleanup") {
                // The parent may exit between try_wait and kill. Reap before
                // retrying so a zombie-only macOS group is not a false failure.
                if !matches!(
                    tokio::time::timeout(Duration::from_millis(50), self.child.wait()).await,
                    Ok(Ok(_))
                ) {
                    return Err(error);
                }
                if let Some(metadata) = registry.metadata(&self.id) {
                    process_supervisor::cleanup_reaped_group(metadata.process_id).await?;
                }
            }
        }
        let status = if exited.is_some() {
            exited
        } else {
            match tokio::time::timeout(Duration::from_millis(250), self.child.wait()).await {
                Ok(result) => Some(result.map_err(|e| format!("tool_reap_failed: {e}"))?),
                Err(_) => None,
            }
        };
        // Always stop descendants, even after the parent exits normally.
        if status.is_some() {
            if let Some(metadata) = registry.metadata(&self.id) {
                process_supervisor::cleanup_reaped_group(metadata.process_id).await?;
            }
        } else {
            registry.force_stop(&self.id)?;
        }
        let status = match status {
            Some(status) => status,
            None => tokio::time::timeout(Duration::from_secs(2), self.child.wait())
                .await
                .map_err(|_| "tool_cleanup_timeout")?
                .map_err(|e| format!("tool_reap_failed: {e}"))?,
        };
        #[cfg(unix)]
        if let Some(metadata) = registry.metadata(&self.id) {
            confirm_exit(metadata.process_id).await?;
        }
        self.stderr_task.abort();
        registry.complete(&self.id);
        self.closed = true;
        disk_result?;
        Ok(status.code())
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.stderr_task.abort();
        // OperationGuard signals only the still-registered process on every early exit.
        let _ = &self.operation;
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    fn spec(script: &str) -> Spec {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        Spec {
            operation_id: format!(
                "tool-test-{}",
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ),
            owner: ProcessOwner::Feature {
                feature: "research".into(),
                scope_id: "fixture".into(),
                run_id: "run-fixture".into(),
            },
            program: "/bin/sh".into(),
            args: vec!["-c".into(), script.into()],
            cwd: std::env::temp_dir(),
            environment: BTreeMap::new(),
            timeout: Duration::from_secs(2),
            max_frame_bytes: 32,
            max_output_bytes: 128,
            disk_limit: None,
        }
    }
    #[tokio::test]
    async fn frames_limits_cancellation_and_eof_are_distinct() {
        for (script, expected) in [
            ("printf 'hello\\n'", "hello"),
            ("printf truncated", "tool_truncated_frame"),
            (
                "printf 'abcdefghijklmnopqrstuvwxyz0123456789\\n'",
                "tool_frame_limit",
            ),
        ] {
            let mut session = Session::spawn(spec(script)).unwrap();
            let result = session.frame(|| false).await;
            assert_eq!(
                result
                    .map(|v| String::from_utf8(v).unwrap())
                    .unwrap_or_else(|e| e),
                expected
            );
            session.close().await.unwrap();
            assert!(process_supervisor::supervisor()
                .metadata(&session.id)
                .is_none());
        }
        let mut session = Session::spawn(spec("sleep 30")).unwrap();
        assert_eq!(session.frame(|| true).await.unwrap_err(), "tool_cancelled");
        session.close().await.unwrap();
    }
    #[tokio::test]
    async fn deadlines_and_stderr_flood_are_bounded() {
        let mut config = spec("sleep 30");
        config.timeout = Duration::from_millis(40);
        let mut session = Session::spawn(config).unwrap();
        assert_eq!(session.frame(|| false).await.unwrap_err(), "tool_timeout");
        session.close().await.unwrap();
        let mut session =
            Session::spawn(spec("while :; do printf 'diagnostics' >&2; done")).unwrap();
        assert_eq!(
            session.frame(|| false).await.unwrap_err(),
            "tool_output_limit"
        );
        session.close().await.unwrap();
    }
    #[tokio::test]
    async fn cwd_budget_failure_still_closes_the_producer() {
        let root = std::env::temp_dir().join(format!(
            "loom-tool-disk-{}-{}",
            std::process::id(),
            crate::models::now_ms()
        ));
        std::fs::create_dir(&root).unwrap();
        let mut config = spec("dd if=/dev/zero of=data bs=1024 count=64 2>/dev/null; sleep 30");
        config.cwd = root.clone();
        config.disk_limit = Some(1024);
        let mut session = Session::spawn(config).unwrap();
        assert!(session
            .frame(|| false)
            .await
            .unwrap_err()
            .contains("disk_budget"));
        assert!(session.close().await.unwrap_err().contains("disk_budget"));
        assert!(process_supervisor::supervisor()
            .metadata(&session.id)
            .is_none());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn normal_parent_exit_still_reaps_its_descendants() {
        let mut session = Session::spawn(spec("sleep 30 & printf 'ready\\n'")).unwrap();
        let pid = process_supervisor::supervisor()
            .metadata(&session.id)
            .unwrap()
            .process_id;
        assert_eq!(session.frame(|| false).await.unwrap(), b"ready");
        session.close().await.unwrap();
        assert!(!process_supervisor::process_group_exists(pid).unwrap());
    }
}
