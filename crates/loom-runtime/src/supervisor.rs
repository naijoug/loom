use crate::protocol::*;
use loom_core::{ConnectionAuth, Id};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs,
    io::{Read, Write},
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender, TrySendError},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

#[derive(Clone, Debug)]
pub struct WorkerCommand {
    pub executable: PathBuf,
    pub script: PathBuf,
    pub timeout: Duration,
    pub exit_grace: Duration,
}
struct PendingRequest {
    tool_call_id: Id,
    name: String,
    input: serde_json::Value,
    external: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeError {
    pub code: &'static str,
    pub message: String,
}
impl RuntimeError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}
impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for RuntimeError {}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunOutcome {
    Completed,
    Cancelled,
}
/// Callbacks must return promptly. Tool requests contain exact raw arguments;
/// handlers must redact their audit preview before persistence.
pub trait Handler {
    fn event(&mut self, event: &WorkerEvent) -> Result<(), RuntimeError>;
    fn decisions(&mut self) -> Result<Vec<ToolDecision>, RuntimeError> {
        Ok(vec![])
    }
    fn cancelled(&self) -> bool {
        false
    }
    fn external_tool(
        &mut self,
        _request: &ExternalToolRequest<'_>,
    ) -> Result<serde_json::Value, RuntimeError> {
        Err(RuntimeError::new(
            "external_tool",
            "external tool execution is unavailable",
        ))
    }
}

pub struct ExternalToolRequest<'a> {
    pub name: &'a str,
    pub input: &'a serde_json::Value,
}
enum ReadEvent {
    Line(Vec<u8>),
    Eof,
    Invalid,
}
fn enqueue(sender: &SyncSender<ReadEvent>, mut value: ReadEvent, stop: &AtomicBool) {
    while !stop.load(Ordering::Acquire) {
        match sender.try_send(value) {
            Ok(()) => return,
            Err(TrySendError::Disconnected(_)) => return,
            Err(TrySendError::Full(pending)) => {
                value = pending;
                thread::sleep(Duration::from_millis(2));
            }
        }
    }
}
fn read_output(mut reader: impl Read, sender: SyncSender<ReadEvent>, stop: Arc<AtomicBool>) {
    let mut pending = Vec::new();
    let mut buffer = [0; 8192];
    while !stop.load(Ordering::Acquire) {
        match reader.read(&mut buffer) {
            Ok(0) => {
                enqueue(
                    &sender,
                    if pending.is_empty() {
                        ReadEvent::Eof
                    } else {
                        ReadEvent::Invalid
                    },
                    &stop,
                );
                return;
            }
            Ok(n) => {
                for byte in &buffer[..n] {
                    if pending.len() >= MAX_FRAME {
                        enqueue(&sender, ReadEvent::Invalid, &stop);
                        return;
                    }
                    pending.push(*byte);
                    if *byte == b'\n' {
                        enqueue(
                            &sender,
                            ReadEvent::Line(std::mem::take(&mut pending)),
                            &stop,
                        );
                    }
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) =>
            {
                thread::sleep(Duration::from_millis(2))
            }
            Err(_) => {
                enqueue(&sender, ReadEvent::Invalid, &stop);
                return;
            }
        }
    }
}
fn discard_errors(mut reader: impl Read, stop: Arc<AtomicBool>) {
    // SDK stderr can include headers and prompts. Drain it without retaining or
    // echoing it; only normalized Failed events may become user-visible errors.
    let mut bytes = [0; 8192];
    while !stop.load(Ordering::Acquire) {
        match reader.read(&mut bytes) {
            Ok(0) => return,
            Ok(_) => bytes.fill(0),
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) =>
            {
                thread::sleep(Duration::from_millis(2))
            }
            Err(_) => return,
        }
    }
}
struct Process {
    child: Child,
    stop: Arc<AtomicBool>,
    readers: Vec<thread::JoinHandle<()>>,
    reaped: bool,
    live: Arc<Mutex<bool>>,
}
impl Process {
    fn signal(&mut self, force: bool) {
        let live = self.live.lock().unwrap_or_else(|error| error.into_inner());
        if self.reaped || !*live {
            return;
        }
        // The leader is deliberately NOT reaped until group cleanup. Its PID
        // therefore cannot be recycled while we signal this owned process group.
        #[cfg(unix)]
        unsafe {
            libc::kill(
                -(self.child.id() as i32),
                if force { libc::SIGKILL } else { libc::SIGTERM },
            );
        }
        #[cfg(not(unix))]
        {
            let _ = force;
            let _ = self.child.kill();
        }
    }
    fn finish(&mut self) -> Result<bool, RuntimeError> {
        self.signal(true);
        *self.live.lock().unwrap_or_else(|error| error.into_inner()) = false;
        self.stop.store(true, Ordering::Release);
        let status = self
            .child
            .wait()
            .map_err(|_| RuntimeError::new("process_wait", "worker exit could not be collected"))?;
        self.reaped = true;
        for reader in self.readers.drain(..) {
            let _ = reader.join();
        }
        Ok(status.success())
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = self.finish();
        }
    }
}

fn write_frame(
    pipe: &mut ChildStdin,
    value: &impl Serialize,
    deadline: Instant,
    stop: &AtomicBool,
) -> Result<(), RuntimeError> {
    let bytes = Zeroizing::new(
        serde_json::to_vec(value)
            .map_err(|_| RuntimeError::new("protocol", "could not encode worker request"))?,
    );
    if bytes.len() + 1 > MAX_FRAME {
        return Err(RuntimeError::new(
            "frame_limit",
            "worker request exceeds 1 MiB",
        ));
    }
    for slice in [bytes.as_slice(), b"\n"] {
        let mut after = 0;
        while after < slice.len() {
            if stop.load(Ordering::Acquire) {
                return Err(RuntimeError::new("cancelled", "worker request cancelled"));
            }
            if Instant::now() >= deadline {
                return Err(RuntimeError::new(
                    "timeout",
                    "worker request exceeded its deadline",
                ));
            }
            match pipe.write(&slice[after..]) {
                Ok(0) => {
                    return Err(RuntimeError::new(
                        "worker_disconnected",
                        "worker input closed",
                    ))
                }
                Ok(n) => after += n,
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                    ) =>
                {
                    thread::sleep(Duration::from_millis(2))
                }
                Err(_) => {
                    return Err(RuntimeError::new(
                        "worker_disconnected",
                        "worker input unavailable",
                    ))
                }
            }
        }
    }
    Ok(())
}
#[cfg(unix)]
fn nonblocking(fd: std::os::fd::RawFd) -> Result<(), RuntimeError> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(RuntimeError::new(
            "pipe_setup",
            "could not configure worker pipe",
        ));
    }
    Ok(())
}
pub(crate) fn credential_forms(secret: &str) -> Vec<String> {
    if secret.is_empty() {
        return vec![];
    }
    let quoted = serde_json::to_string(secret).expect("string serialization");
    let escaped = quoted[1..quoted.len() - 1].to_string();
    let encoded = secret
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect::<String>();
    let mut forms = vec![secret.to_string()];
    for value in [escaped, encoded] {
        if !forms.contains(&value) {
            forms.push(value);
        }
    }
    forms
}
pub(crate) fn clean_text(value: &str, secret: Option<&str>) -> String {
    credential_forms(secret.unwrap_or_default())
        .into_iter()
        .fold(value.to_string(), |text, secret| {
            text.replace(&secret, "[credential redacted]")
        })
}
fn clean_value(value: &mut serde_json::Value, secret: Option<&str>) {
    match value {
        serde_json::Value::String(text) => *text = clean_text(text, secret),
        serde_json::Value::Array(values) => {
            for value in values {
                clean_value(value, secret)
            }
        }
        serde_json::Value::Object(values) => {
            let original = std::mem::take(values);
            for (key, mut value) in original {
                let key = clean_text(&key, secret);
                let mut unique = key.clone();
                let mut suffix = 1;
                while values.contains_key(&unique) {
                    unique = format!("{key}-{suffix}");
                    suffix += 1;
                }
                clean_value(&mut value, secret);
                values.insert(unique, value);
            }
        }
        _ => {}
    }
}

/// KMP prefix buffering prevents a credential split across SDK text deltas from
/// escaping a per-frame replace. Ordinary text is emitted immediately.
struct TextFilter {
    needle: Vec<u8>,
    prefix: Vec<usize>,
    held: VecDeque<u8>,
}
struct TextFilters(Vec<TextFilter>);
impl TextFilters {
    fn new(secret: &str) -> Self {
        Self(
            credential_forms(secret)
                .iter()
                .map(|value| TextFilter::new(value))
                .collect(),
        )
    }
    fn push(&mut self, text: &str) -> String {
        self.0
            .iter_mut()
            .fold(text.to_string(), |text, filter| filter.push(&text))
    }
    fn finish(&mut self) -> String {
        let mut output = String::new();
        for index in 0..self.0.len() {
            let tail = self.0[index].finish();
            let tail = self
                .0
                .iter_mut()
                .skip(index + 1)
                .fold(tail, |text, filter| filter.push(&text));
            output.push_str(&tail);
        }
        output
    }
}
impl TextFilter {
    fn new(secret: &str) -> Self {
        let needle = secret.as_bytes().to_vec();
        let mut prefix = vec![0; needle.len()];
        for index in 1..needle.len() {
            let mut n = prefix[index - 1];
            while n > 0 && needle[index] != needle[n] {
                n = prefix[n - 1];
            }
            if needle[index] == needle[n] {
                n += 1;
            }
            prefix[index] = n;
        }
        Self {
            needle,
            prefix,
            held: VecDeque::new(),
        }
    }
    fn push(&mut self, text: &str) -> String {
        if self.needle.is_empty() {
            return text.into();
        }
        let mut output = Vec::new();
        for byte in text.bytes() {
            while !self.held.is_empty() && self.needle[self.held.len()] != byte {
                let next = self.prefix[self.held.len() - 1];
                while self.held.len() > next {
                    output.push(self.held.pop_front().unwrap());
                }
            }
            if self.needle[self.held.len()] == byte {
                self.held.push_back(byte);
                if self.held.len() == self.needle.len() {
                    output.extend_from_slice(b"[credential redacted]");
                    self.held.clear();
                }
            } else {
                output.push(byte);
            }
        }
        String::from_utf8(output)
            .expect("UTF-8 input and secret prefixes preserve code-point boundaries")
    }
    fn finish(&mut self) -> String {
        String::from_utf8(self.held.drain(..).collect())
            .expect("held suffix ends at a UTF-8 boundary")
    }
}

pub fn run(
    command: &WorkerCommand,
    config: &RunConfig,
    credential: Option<&str>,
    stop: Arc<AtomicBool>,
    handler: &mut impl Handler,
) -> Result<RunOutcome, RuntimeError> {
    if !cfg!(unix) {
        return Err(RuntimeError::new(
            "unsupported_platform",
            "process-group supervisor is not implemented on this platform",
        ));
    }
    config
        .connection
        .validate()
        .map_err(|_| RuntimeError::new("configuration", "invalid connection configuration"))?;
    if (config.connection.auth == ConnectionAuth::None) != credential.is_none()
        || credential.is_some_and(|s| s.is_empty() || s.len() > 65536)
    {
        return Err(RuntimeError::new(
            "credentials",
            "credential and authentication mode mismatch",
        ));
    }
    if !config.connection.models.contains(&config.model)
        || config.max_turns == 0
        || config.max_turns > 100
        || config.prompt.trim().is_empty()
    {
        return Err(RuntimeError::new(
            "configuration",
            "invalid model, prompt or turn limit",
        ));
    }
    if !PathBuf::from(&config.cwd).is_absolute()
        || !PathBuf::from(&config.runtime_directory).is_absolute()
        || command.timeout.is_zero()
        || command.exit_grace > Duration::from_secs(10)
    {
        return Err(RuntimeError::new(
            "configuration",
            "absolute directories and bounded deadlines are required",
        ));
    }
    if config
        .history
        .iter()
        .any(|m| !matches!(m.role.as_str(), "user" | "assistant"))
    {
        return Err(RuntimeError::new(
            "configuration",
            "invalid historical message role",
        ));
    }
    if stop.load(Ordering::Acquire) || handler.cancelled() {
        return Ok(RunOutcome::Cancelled);
    }
    let executable = fs::canonicalize(&command.executable)
        .map_err(|_| RuntimeError::new("worker_missing", "worker executable unavailable"))?;
    let script = fs::canonicalize(&command.script)
        .map_err(|_| RuntimeError::new("worker_missing", "worker script unavailable"))?;
    let mut process = Command::new(executable);
    process.arg(script).current_dir(&config.cwd).env_clear();
    for key in ["PATH", "HOME", "USER", "SHELL", "TMPDIR", "LANG"] {
        if let Some(value) = std::env::var_os(key) {
            process.env(key, value);
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        process.process_group(0);
    }
    let child = process
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| RuntimeError::new("spawn_failed", "could not start SDK worker"))?;
    let reader_stop = Arc::new(AtomicBool::new(false));
    let live = Arc::new(Mutex::new(true));
    let mut process = Process {
        child,
        stop: reader_stop.clone(),
        readers: vec![],
        reaped: false,
        live: live.clone(),
    };
    let mut input = process
        .child
        .stdin
        .take()
        .ok_or_else(|| RuntimeError::new("pipe_setup", "worker stdin missing"))?;
    let output = process
        .child
        .stdout
        .take()
        .ok_or_else(|| RuntimeError::new("pipe_setup", "worker stdout missing"))?;
    let errors = process
        .child
        .stderr
        .take()
        .ok_or_else(|| RuntimeError::new("pipe_setup", "worker stderr missing"))?;
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        nonblocking(input.as_raw_fd())?;
        nonblocking(output.as_raw_fd())?;
        nonblocking(errors.as_raw_fd())?;
    }
    let (sender, receiver) = sync_channel(32);
    let reader_flag = reader_stop.clone();
    process.readers.push(thread::spawn(move || {
        read_output(output, sender, reader_flag)
    }));
    process
        .readers
        .push(thread::spawn(move || discard_errors(errors, reader_stop)));
    let deadline = Instant::now() + command.timeout;
    let watchdog_stop = process.stop.clone();
    let cancel = stop.clone();
    let grace = command.exit_grace;
    let pid = process.child.id();
    process.readers.push(thread::spawn(move || {
        let mut cancelled_at = None;
        while !watchdog_stop.load(Ordering::Acquire) {
            if cancel.load(Ordering::Acquire) {
                cancelled_at.get_or_insert_with(Instant::now);
            }
            if Instant::now() >= deadline
                || cancelled_at.is_some_and(|at: Instant| at.elapsed() >= grace)
            {
                let live = live.lock().unwrap_or_else(|error| error.into_inner());
                #[cfg(unix)]
                if *live {
                    unsafe {
                        libc::kill(-(pid as i32), libc::SIGKILL);
                    }
                }
                #[cfg(not(unix))]
                let _ = (pid, *live);
                return;
            }
            thread::sleep(Duration::from_millis(5));
        }
    }));
    let result = (|| {
        write_frame(
            &mut input,
            &Start {
                protocol_version: VERSION,
                kind: "start",
                config,
                credential,
            },
            deadline,
            &stop,
        )?;
        event_loop(
            command,
            config,
            credential,
            &stop,
            handler,
            &mut process,
            &mut input,
            receiver,
            deadline,
        )
    })();
    if stop.load(Ordering::Acquire) || handler.cancelled() {
        Ok(RunOutcome::Cancelled)
    } else {
        result
    }
}
#[allow(clippy::too_many_arguments)]
fn event_loop(
    command: &WorkerCommand,
    config: &RunConfig,
    credential: Option<&str>,
    stop: &AtomicBool,
    handler: &mut impl Handler,
    process: &mut Process,
    input: &mut ChildStdin,
    receiver: Receiver<ReadEvent>,
    deadline: Instant,
) -> Result<RunOutcome, RuntimeError> {
    let mut sequence = 0;
    let mut ready = false;
    let mut terminal = None;
    let mut exit_by = None;
    let mut cancelling = false;
    let mut requests = BTreeMap::<Id, PendingRequest>::new();
    let mut seen = BTreeSet::new();
    let mut running = BTreeSet::new();
    let mut filters = BTreeMap::<Id, (String, TextFilters)>::new();
    loop {
        if !cancelling && (stop.load(Ordering::Acquire) || handler.cancelled()) {
            cancelling = true;
            exit_by = Some(Instant::now() + command.exit_grace);
            let _ = write_frame(
                input,
                &CancelFrame {
                    protocol_version: VERSION,
                    kind: "cancel",
                    scope: &config.scope,
                },
                Instant::now() + Duration::from_millis(100),
                &AtomicBool::new(false),
            );
            process.signal(false);
        }
        if Instant::now() >= deadline {
            return Err(RuntimeError::new(
                "timeout",
                "SDK worker exceeded its deadline",
            ));
        }
        if exit_by.is_some_and(|end| Instant::now() >= end) {
            return if cancelling {
                Ok(RunOutcome::Cancelled)
            } else {
                Err(RuntimeError::new(
                    "worker_hung",
                    "worker did not exit after its terminal frame",
                ))
            };
        }
        if !cancelling && terminal.is_none() {
            for decision in handler.decisions()? {
                let Some(request) = requests.remove(&decision.request_id) else {
                    return Err(RuntimeError::new(
                        "stale_decision",
                        "tool decision does not match a pending request",
                    ));
                };
                if request.tool_call_id != decision.tool_call_id {
                    return Err(RuntimeError::new(
                        "stale_decision",
                        "tool decision does not match a pending request",
                    ));
                }
                if stop.load(Ordering::Acquire) || handler.cancelled() {
                    break;
                }
                let mut decision = decision;
                if decision.allowed && request.external {
                    let output = handler.external_tool(&ExternalToolRequest {
                        name: &request.name,
                        input: &request.input,
                    })?;
                    if serde_json::to_vec(&output).map_or(true, |bytes| bytes.len() > 512 * 1024) {
                        return Err(RuntimeError::new(
                            "external_tool",
                            "REA result exceeds the inline worker budget",
                        ));
                    }
                    decision.output = Some(output);
                }
                if decision.allowed {
                    running.insert(decision.tool_call_id.clone());
                }
                write_frame(
                    input,
                    &DecisionFrame {
                        protocol_version: VERSION,
                        kind: "tool_decision",
                        scope: &config.scope,
                        decision: &decision,
                    },
                    deadline,
                    stop,
                )?;
            }
        }
        let bytes = match receiver.recv_timeout(Duration::from_millis(15)) {
            Ok(ReadEvent::Line(bytes)) => bytes,
            Ok(ReadEvent::Eof) => {
                let success = process.finish()?;
                return if cancelling || stop.load(Ordering::Acquire) || handler.cancelled() {
                    Ok(RunOutcome::Cancelled)
                } else if Instant::now() >= deadline {
                    Err(RuntimeError::new(
                        "timeout",
                        "SDK worker exceeded its deadline",
                    ))
                } else if !success {
                    Err(RuntimeError::new(
                        "worker_exit",
                        "SDK worker exited unsuccessfully",
                    ))
                } else {
                    let outcome = terminal.ok_or_else(|| {
                        RuntimeError::new(
                            "missing_terminal",
                            "SDK worker closed without a terminal result",
                        )
                    })?;
                    handler.event(&WorkerEvent::Completed {
                        cancelled: outcome == RunOutcome::Cancelled,
                    })?;
                    Ok(outcome)
                };
            }
            Ok(ReadEvent::Invalid) if cancelling => return Ok(RunOutcome::Cancelled),
            Ok(ReadEvent::Invalid) => {
                return Err(RuntimeError::new(
                    "invalid_frame",
                    "worker output is truncated, oversized or unreadable",
                ))
            }
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) if cancelling => return Ok(RunOutcome::Cancelled),
            Err(RecvTimeoutError::Disconnected) => {
                return Err(RuntimeError::new(
                    "worker_disconnected",
                    "worker output reader disconnected",
                ))
            }
        };
        if cancelling {
            continue;
        }
        let mut frame: Frame = serde_json::from_slice(&bytes).map_err(|_| {
            RuntimeError::new("invalid_frame", "worker output does not match the protocol")
        })?;
        sequence += 1;
        if frame.protocol_version != VERSION
            || frame.scope != config.scope
            || frame.seq != sequence
            || terminal.is_some()
        {
            return Err(RuntimeError::new(
                "protocol_scope",
                "worker version, scope, order or terminal boundary mismatch",
            ));
        }
        match &mut frame.event {
            WorkerEvent::Ready => {
                if ready || sequence != 1 {
                    return Err(RuntimeError::new(
                        "protocol_state",
                        "duplicate or late worker readiness",
                    ));
                }
                ready = true;
            }
            _ if !ready => {
                return Err(RuntimeError::new(
                    "protocol_state",
                    "worker event preceded readiness",
                ))
            }
            WorkerEvent::ToolRequest {
                request_id,
                tool_call_id,
                name,
                input,
            } => {
                if requests.len() >= 32
                    || !seen.insert(tool_call_id.clone())
                    || requests
                        .insert(
                            request_id.clone(),
                            PendingRequest {
                                tool_call_id: tool_call_id.clone(),
                                name: name.clone(),
                                input: input.clone(),
                                external: config
                                    .external_tools
                                    .iter()
                                    .any(|tool| &tool.name == name),
                            },
                        )
                        .is_some()
                    || (!config.tools.contains(name)
                        && !config.external_tools.iter().any(|tool| &tool.name == name))
                {
                    return Err(RuntimeError::new(
                        "tool_protocol",
                        "duplicate, excessive or undeclared tool request",
                    ));
                }
            }
            WorkerEvent::ToolResult {
                tool_call_id,
                output,
                ..
            } => {
                if !running.remove(tool_call_id) {
                    return Err(RuntimeError::new(
                        "unapproved_tool",
                        "tool completed without an allowed request",
                    ));
                }
                clean_value(output, credential);
            }
            WorkerEvent::Text {
                part_id,
                text_kind,
                delta,
            } => {
                if !matches!(text_kind.as_str(), "text" | "thinking")
                    || (!filters.contains_key(part_id) && filters.len() >= 256)
                {
                    return Err(RuntimeError::new(
                        "text_protocol",
                        "invalid or excessive text parts",
                    ));
                }
                let (kind, filter) = filters.entry(part_id.clone()).or_insert_with(|| {
                    (
                        text_kind.clone(),
                        TextFilters::new(credential.unwrap_or_default()),
                    )
                });
                if kind != text_kind {
                    return Err(RuntimeError::new("text_protocol", "text part kind changed"));
                }
                *delta = filter.push(delta);
                if delta.is_empty() {
                    continue;
                }
            }
            WorkerEvent::Usage {
                input_tokens,
                output_tokens,
            } => {
                if *input_tokens > loom_core::MAX_SAFE_SEQUENCE
                    || *output_tokens > loom_core::MAX_SAFE_SEQUENCE
                {
                    return Err(RuntimeError::new("usage_limit", "unsafe usage counter"));
                }
            }
            WorkerEvent::Completed { cancelled } => {
                if !*cancelled && (!requests.is_empty() || !running.is_empty()) {
                    return Err(RuntimeError::new(
                        "unfinished_tools",
                        "worker completed with unfinished tools",
                    ));
                }
                terminal = Some(if *cancelled {
                    RunOutcome::Cancelled
                } else {
                    RunOutcome::Completed
                });
                if !*cancelled {
                    for (id, (kind, filter)) in &mut filters {
                        let delta = filter.finish();
                        if !delta.is_empty() {
                            handler.event(&WorkerEvent::Text {
                                part_id: id.clone(),
                                text_kind: kind.clone(),
                                delta,
                            })?;
                        }
                    }
                }
                exit_by = Some(Instant::now() + command.exit_grace);
            }
            WorkerEvent::Failed { message, .. } => {
                return Err(RuntimeError::new(
                    "provider_failed",
                    clean_text(message, credential)
                        .chars()
                        .take(4096)
                        .collect::<String>(),
                ))
            }
        }
        if !cancelling && !matches!(frame.event, WorkerEvent::Completed { .. }) {
            handler.event(&frame.event)?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn streamed_redaction_handles_overlaps_unicode_and_encoded_forms() {
        for secret in ["aaaaab", "abab", "密钥🔒", "a你a", r#"key"/甲"#] {
            let forms = credential_forms(secret);
            let text = format!("prefix {} / {} / suffix aaa", secret, forms.join(" / "));
            let expected = clean_text(&text, Some(secret));
            let mut filter = TextFilters::new(secret);
            let mut actual = String::new();
            for character in text.chars() {
                actual.push_str(&filter.push(&character.to_string()));
            }
            actual.push_str(&filter.finish());
            assert_eq!(actual, expected);
        }
    }
}
