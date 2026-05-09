use serde::Serialize;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter, State};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
    sync::Mutex,
};

#[cfg(unix)]
unsafe extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
    fn setpgid(pid: i32, pgid: i32) -> i32;
}

#[cfg(unix)]
const SIGTERM: i32 = 15;

pub struct SpikeRegistry {
    next_id: AtomicU64,
    runs: Mutex<HashMap<String, ManagedSpikeRun>>,
}

impl Default for SpikeRegistry {
    fn default() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            runs: Mutex::new(HashMap::new()),
        }
    }
}

struct ManagedSpikeRun {
    child: Child,
    #[cfg(unix)]
    process_group_id: i32,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpikeRun {
    run_id: String,
    command: String,
    cwd: String,
    pid: Option<u32>,
    started_at_ms: u128,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SpikeLogEvent {
    run_id: String,
    stream: &'static str,
    line: String,
    timestamp_ms: u128,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpikeStopResult {
    run_id: String,
    stopped: bool,
    exit_code: Option<i32>,
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

#[tauri::command]
pub async fn start_spike_run(
    app: AppHandle,
    registry: State<'_, SpikeRegistry>,
) -> Result<SpikeRun, String> {
    let id = registry.next_id.fetch_add(1, Ordering::Relaxed);
    let run_id = format!("spike-{id}");
    let command_text =
        "i=0; while true; do echo \"[loom-spike] tick $i\"; i=$((i+1)); sleep 1; done";
    let cwd = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .display()
        .to_string();

    let mut command = Command::new("sh");
    command
        .arg("-c")
        .arg(command_text)
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
        .map_err(|error| format!("failed to start spike command: {error}"))?;
    let pid = child.id();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    if let Some(stdout) = stdout {
        spawn_log_reader(app.clone(), run_id.clone(), "stdout", stdout);
    }

    if let Some(stderr) = stderr {
        spawn_log_reader(app.clone(), run_id.clone(), "stderr", stderr);
    }

    let run = SpikeRun {
        run_id: run_id.clone(),
        command: command_text.to_string(),
        cwd,
        pid,
        started_at_ms: now_ms(),
    };

    let managed = ManagedSpikeRun {
        child,
        #[cfg(unix)]
        process_group_id: pid.unwrap_or_default() as i32,
    };
    registry.runs.lock().await.insert(run_id, managed);

    Ok(run)
}

#[tauri::command]
pub async fn stop_spike_run(
    run_id: String,
    registry: State<'_, SpikeRegistry>,
) -> Result<SpikeStopResult, String> {
    let Some(mut managed) = registry.runs.lock().await.remove(&run_id) else {
        return Ok(SpikeStopResult {
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

    let _ = managed.child.kill().await;
    let status = managed
        .child
        .wait()
        .await
        .map_err(|error| format!("failed to wait for spike command: {error}"))?;

    Ok(SpikeStopResult {
        run_id,
        stopped: true,
        exit_code: status.code(),
    })
}

fn spawn_log_reader<R>(app: AppHandle, run_id: String, stream: &'static str, reader: R)
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tauri::async_runtime::spawn(async move {
        let mut lines = BufReader::new(reader).lines();

        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    let _ = app.emit(
                        "loom://spike-log",
                        SpikeLogEvent {
                            run_id: run_id.clone(),
                            stream,
                            line,
                            timestamp_ms: now_ms(),
                        },
                    );
                }
                Ok(None) => break,
                Err(error) => {
                    let _ = app.emit(
                        "loom://spike-log",
                        SpikeLogEvent {
                            run_id: run_id.clone(),
                            stream: "stderr",
                            line: format!("log reader failed: {error}"),
                            timestamp_ms: now_ms(),
                        },
                    );
                    break;
                }
            }
        }
    });
}
