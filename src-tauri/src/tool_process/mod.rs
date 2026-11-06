//! Bounded non-Agent stdio sessions. Uses the application's sole supervisor.
//! Callers supply argv, never shell text, and retain the session until cleanup.
use crate::process_supervisor::{self, OperationGuard, ProcessMetadata};
pub use crate::process_supervisor::{ProcessKind, ProcessOwner};
use std::{collections::BTreeMap, path::PathBuf, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::mpsc,
    task::JoinHandle,
    time::Instant,
};

/// Bounded accounting for a host-owned file or directory. Symlinks are counted
/// as their own nodes and never followed. A missing path consumes zero bytes;
/// permission or shape failures never look like zero.
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
        if depth > 64 || *count > 100_000 {
            return Err("unsafe_or_excessive_disk_tree".into());
        }
        if meta.file_type().is_symlink() {
            *bytes = bytes
                .checked_add(meta.len())
                .ok_or("disk_budget_overflow")?;
            if *bytes > limit {
                return Err("disk_budget_exhausted".into());
            }
            return Ok(());
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

pub type RegisteredHook = Box<dyn Fn(&ProcessMetadata) + Send + Sync>;

pub struct BoundedCommand {
    pub operation_id: String,
    pub owner: ProcessOwner,
    pub kind: ProcessKind,
    pub program: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub timeout: Duration,
    pub max_stdout: usize,
    pub max_stderr: usize,
    pub disk_root: Option<(PathBuf, u64)>,
    pub env: BTreeMap<String, String>,
    pub remove_env: Vec<String>,
    pub on_registered: Option<RegisteredHook>,
}

pub struct BoundedOutput {
    pub success: bool,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// One-shot argv execution. The caller owns the program name, arguments and
/// process owner; this only applies the supervisor, limits and cleanup.
pub async fn run_bounded(
    spec: BoundedCommand,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<BoundedOutput, String> {
    if spec.timeout.is_zero()
        || spec.timeout > Duration::from_secs(600)
        || spec.max_stdout == 0
        || spec.max_stdout > 16 * 1024 * 1024
        || spec.max_stderr == 0
        || spec.max_stderr > 64 * 1024
        || spec.operation_id.is_empty()
        || spec.program != "git"
        || !spec.cwd.is_absolute()
        || matches!(spec.owner, ProcessOwner::Unscoped)
    {
        return Err("invalid_tool_process_spec".into());
    }
    if cancelled() {
        return Err("tool_cancelled".into());
    }
    if let Some((root, limit)) = &spec.disk_root {
        directory_bytes(root, *limit)?;
    }
    let mut operation = process_supervisor::supervisor().begin_operation()?;
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .current_dir(&spec.cwd)
        .envs(&spec.env)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    for key in &spec.remove_env {
        command.env_remove(key);
    }
    process_supervisor::configure_process_group(&mut command);
    let mut child = command
        .spawn()
        .map_err(|error| format!("tool_spawn_failed: {error}"))?;
    let pid = child.id().ok_or("tool_pid_missing")?;
    if let Err(error) = operation.register(ProcessMetadata::owned(
        &spec.operation_id,
        spec.owner.clone(),
        spec.kind,
        pid,
        spec.timeout.as_millis().min(u64::MAX as u128) as u64,
    )) {
        let _ = child.kill().await;
        let _ = process_supervisor::force_terminate_process_group(pid);
        return Err(error);
    }
    if let Some(callback) = &spec.on_registered {
        if let Some(metadata) = process_supervisor::supervisor().metadata(&spec.operation_id) {
            callback(&metadata);
        }
    }
    let stdout_task = tokio::spawn(drain_capped(
        child.stdout.take().ok_or("tool_stdout_missing")?,
        spec.max_stdout,
    ));
    let stderr_task = tokio::spawn(drain_capped(
        child.stderr.take().ok_or("tool_stderr_missing")?,
        spec.max_stderr,
    ));
    let began = Instant::now();
    let status = loop {
        tokio::select! {
            result = child.wait() => break result.map_err(|error| error.to_string()),
            _ = tokio::time::sleep(Duration::from_millis(50)) => {
                if cancelled() { break Err("tool_cancelled".into()); }
                if process_supervisor::supervisor().is_shutting_down() { break Err("tool_stopped".into()); }
                if began.elapsed() > spec.timeout { break Err("tool_timeout".into()); }
                if let Some((root, limit)) = &spec.disk_root {
                    if let Err(error) = directory_bytes(root, *limit) { break Err(error); }
                }
            }
        }
    };
    if status.is_err() {
        let _ = child.start_kill();
    }
    let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
    let cleanup = process_supervisor::cleanup_reaped_group(pid).await;
    process_supervisor::supervisor().complete(&spec.operation_id);
    let stdout = tokio::time::timeout(Duration::from_secs(2), stdout_task)
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or_default();
    let stderr = tokio::time::timeout(Duration::from_secs(2), stderr_task)
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or_default();
    cleanup?;
    let status = status?;
    Ok(BoundedOutput {
        success: status.success(),
        stdout,
        stderr,
    })
}

async fn drain_capped(mut reader: impl tokio::io::AsyncRead + Unpin, cap: usize) -> Vec<u8> {
    let mut captured = Vec::new();
    let mut buffer = [0; 8192];
    while let Ok(n) = reader.read(&mut buffer).await {
        if n == 0 {
            break;
        }
        let keep = n.min(cap.saturating_sub(captured.len()));
        captured.extend_from_slice(&buffer[..keep]);
    }
    captured
}

pub struct DetachedSpec {
    pub operation_id: String,
    pub owner: ProcessOwner,
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
}

/// A long-lived process with no stdio protocol. Closing it signals only this
/// process group.
pub struct Detached {
    child: Child,
    operation: OperationGuard<'static>,
    id: String,
    pid: u32,
}

impl Detached {
    pub fn spawn(spec: DetachedSpec) -> Result<Self, String> {
        if spec.operation_id.is_empty()
            || !spec.program.is_absolute()
            || !spec.cwd.is_absolute()
            || matches!(spec.owner, ProcessOwner::Unscoped)
        {
            return Err("invalid_tool_process_spec".into());
        }
        let mut operation = process_supervisor::supervisor().begin_operation()?;
        let mut command = Command::new(&spec.program);
        command
            .args(&spec.args)
            .current_dir(&spec.cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        process_supervisor::configure_process_group(&mut command);
        let mut child = command
            .spawn()
            .map_err(|error| format!("tool_spawn_failed: {error}"))?;
        let pid = child.id().ok_or("tool_pid_missing")?;
        if let Err(error) = operation.register(ProcessMetadata::owned(
            &spec.operation_id,
            spec.owner,
            ProcessKind::Tool,
            pid,
            120_000,
        )) {
            let _ = child.start_kill();
            let _ = process_supervisor::force_terminate_process_group(pid);
            return Err(error);
        }
        Ok(Self {
            child,
            operation,
            id: spec.operation_id,
            pid,
        })
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    pub fn exited(&mut self) -> Result<Option<i32>, String> {
        Ok(self
            .child
            .try_wait()
            .map_err(|error| error.to_string())?
            .and_then(|status| status.code()))
    }

    pub async fn close(mut self) -> Result<(), String> {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.start_kill();
        }
        let _ = tokio::time::timeout(Duration::from_secs(2), self.child.wait()).await;
        let cleanup = process_supervisor::cleanup_reaped_group(self.pid).await;
        process_supervisor::supervisor().complete(&self.id);
        drop(self.operation);
        cleanup
    }
}

/// The listening socket for `port` must be loopback and belong to `root` or
/// one of its descendants. Anything else is refused.
#[cfg(target_os = "linux")]
pub fn require_owned_loopback_port(root: u32, port: u16) -> Result<(), String> {
    require_owned_loopback_port_linux(root, port)
}

#[cfg(target_os = "macos")]
pub fn require_owned_loopback_port(root: u32, port: u16) -> Result<(), String> {
    require_owned_loopback_port_macos(root, port)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn require_owned_loopback_port(root: u32, port: u16) -> Result<(), String> {
    let _ = (root, port);
    Err("browser_port_probe_unsupported".into())
}

#[cfg(target_os = "linux")]
fn require_owned_loopback_port_linux(root: u32, port: u16) -> Result<(), String> {
    let inodes = loopback_listen_inodes(port)?;
    let tree = process_tree(root)?;
    let mut owned = false;
    for pid in &tree {
        for inode in socket_inodes(*pid)? {
            if inodes.contains(&inode) {
                owned = true;
            }
        }
    }
    if owned {
        Ok(())
    } else {
        Err("browser_port_not_owned".into())
    }
}

#[cfg(target_os = "macos")]
fn require_owned_loopback_port_macos(root: u32, port: u16) -> Result<(), String> {
    let output = std::process::Command::new("lsof")
        .args(["-nP", &format!("-iTCP:{port}"), "-sTCP:LISTEN"])
        .output()
        .map_err(|_| "browser_port_probe_unavailable")?;
    if !output.status.success() && output.stdout.is_empty() {
        return Err("browser_port_not_listening".into());
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let tree = process_tree(root)?;
    let mut owned = false;
    let mut saw = false;
    for line in text.lines().skip(1) {
        let lower = line.to_ascii_lowercase();
        if !lower.contains("(listen)") {
            continue;
        }
        saw = true;
        if !lower.contains("127.0.0.1:") && !lower.contains("[::1]:") {
            return Err("browser_port_not_loopback".into());
        }
        if let Some(pid) = line
            .split_whitespace()
            .nth(1)
            .and_then(|pid| pid.parse().ok())
        {
            if tree.contains(&pid) {
                owned = true;
            }
        }
    }
    if !saw {
        return Err("browser_port_not_listening".into());
    }
    if owned {
        Ok(())
    } else {
        Err("browser_port_not_owned".into())
    }
}

#[cfg(target_os = "macos")]
fn process_tree(root: u32) -> Result<Vec<u32>, String> {
    let output = std::process::Command::new("ps")
        .args(["-ax", "-o", "pid=,ppid="])
        .output()
        .map_err(|_| "browser_port_probe_unavailable")?;
    if !output.status.success() {
        return Err("browser_port_probe_unavailable".into());
    }
    let mut parent = std::collections::HashMap::<u32, u32>::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let mut parts = line.split_whitespace();
        let Some(pid) = parts.next().and_then(|pid| pid.parse().ok()) else {
            continue;
        };
        let Some(ppid) = parts.next().and_then(|pid| pid.parse().ok()) else {
            continue;
        };
        parent.insert(pid, ppid);
    }
    let mut tree = vec![root];
    let mut changed = true;
    while changed {
        changed = false;
        for (&pid, &ppid) in &parent {
            if tree.contains(&ppid) && !tree.contains(&pid) {
                tree.push(pid);
                changed = true;
            }
        }
    }
    Ok(tree)
}

#[cfg(target_os = "linux")]
fn process_tree(root: u32) -> Result<Vec<u32>, String> {
    let mut parent = std::collections::HashMap::<u32, u32>::new();
    for entry in std::fs::read_dir("/proc").map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        let Ok(status) = std::fs::read_to_string(format!("/proc/{pid}/status")) else {
            continue;
        };
        if let Some(rest) = status.lines().find_map(|line| line.strip_prefix("PPid:")) {
            if let Ok(ppid) = rest.trim().parse::<u32>() {
                parent.insert(pid, ppid);
            }
        }
    }
    let mut tree = vec![root];
    let mut changed = true;
    while changed {
        changed = false;
        for (&pid, &ppid) in &parent {
            if tree.contains(&ppid) && !tree.contains(&pid) {
                tree.push(pid);
                changed = true;
            }
        }
    }
    Ok(tree)
}

#[cfg(target_os = "linux")]
fn socket_inodes(pid: u32) -> Result<Vec<u64>, String> {
    let mut inodes = Vec::new();
    let directory = std::path::PathBuf::from(format!("/proc/{pid}/fd"));
    let entries = match std::fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(inodes),
        Err(error) => return Err(error.to_string()),
    };
    for entry in entries {
        let Ok(entry) = entry else { continue };
        let Ok(target) = std::fs::read_link(entry.path()) else {
            continue;
        };
        let text = target.to_string_lossy();
        if let Some(rest) = text.strip_prefix("socket:[") {
            if let Some(inode) = rest.strip_suffix(']') {
                if let Ok(inode) = inode.parse() {
                    inodes.push(inode);
                }
            }
        }
    }
    Ok(inodes)
}

#[cfg(target_os = "linux")]
fn loopback_listen_inodes(port: u16) -> Result<std::collections::HashSet<u64>, String> {
    let mut inodes = std::collections::HashSet::new();
    let mut saw_other = false;
    for path in ["/proc/net/tcp", "/proc/net/tcp6"] {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        for line in text.lines().skip(1) {
            let fields: Vec<_> = line.split_whitespace().collect();
            if fields.len() < 10 || fields[3] != "0A" {
                continue;
            }
            let Some((address, listed_port)) = fields[1].split_once(':') else {
                continue;
            };
            let Ok(listed_port) = u16::from_str_radix(listed_port, 16) else {
                continue;
            };
            if listed_port != port {
                continue;
            }
            if !loopback_address(path.ends_with('6'), address) {
                saw_other = true;
                continue;
            }
            if let Ok(inode) = fields[9].parse::<u64>() {
                inodes.insert(inode);
            }
        }
    }
    if saw_other {
        return Err("browser_port_not_loopback".into());
    }
    if inodes.is_empty() {
        return Err("browser_port_not_listening".into());
    }
    Ok(inodes)
}

#[cfg(target_os = "linux")]
fn loopback_address(ipv6: bool, hex: &str) -> bool {
    if ipv6 {
        return hex.eq_ignore_ascii_case("00000000000000000000000001000000")
            || hex.eq_ignore_ascii_case("0000000000000000FFFF00000100007F");
    }
    let Ok(value) = u32::from_str_radix(hex, 16) else {
        return false;
    };
    let bytes = value.to_be_bytes();
    bytes[3] == 127
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
            let deadline = Instant::now() + Duration::from_secs(2);
            while process_supervisor::process_group_exists(metadata.process_id)? {
                if Instant::now() >= deadline {
                    return Err("tool_cleanup_timeout: descendants remain".into());
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
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

    #[tokio::test]
    async fn bounded_git_records_the_caller_owner_and_reaps() {
        let root = std::env::temp_dir().join(format!(
            "loom-bounded-git-{}-{}",
            std::process::id(),
            crate::models::now_ms()
        ));
        std::fs::create_dir(&root).unwrap();
        let seen = std::sync::Arc::new(std::sync::Mutex::new(None));
        let owner = ProcessOwner::Learn {
            study_id: "study-t".into(),
            job_id: "job-t".into(),
        };
        let output = super::run_bounded(
            super::BoundedCommand {
                operation_id: "bounded-git-version".into(),
                owner: owner.clone(),
                kind: ProcessKind::Learn,
                program: "git".into(),
                args: vec!["--version".into()],
                cwd: root.clone(),
                timeout: Duration::from_secs(30),
                max_stdout: 4096,
                max_stderr: 1024,
                disk_root: None,
                env: BTreeMap::new(),
                remove_env: vec!["GIT_DIR".into()],
                on_registered: Some(Box::new({
                    let seen = seen.clone();
                    move |metadata| *seen.lock().unwrap() = Some(metadata.owner.clone())
                })),
            },
            &|| false,
        )
        .await
        .unwrap();
        assert!(
            output.success,
            "{:?}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout)
            .to_ascii_lowercase()
            .contains("git"));
        assert_eq!(seen.lock().unwrap().clone(), Some(owner));
        assert!(process_supervisor::supervisor()
            .metadata("bounded-git-version")
            .is_none());
        std::fs::remove_dir_all(root).unwrap();
    }
}
