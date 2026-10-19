//! M0 integration experiment, not production authorization policy.
//! Owns a real SDK worker and permits only its single synthetic probe.txt write.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::io::{BufRead, BufReader, Read, Write};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Context {
    workspace_id: String,
    session_id: String,
    turn_id: String,
}
fn digest(value: &Value) -> String {
    // serde_json's default map is sorted recursively, matching worker canonicalization.
    format!("{:x}", Sha256::digest(serde_json::to_vec(value).unwrap()))
}
fn approved_binding(event: &Value, scope: &Context, cwd: &Path) -> Result<String, String> {
    let received: Context =
        serde_json::from_value(event["context"].clone()).map_err(|e| e.to_string())?;
    if &received != scope || event["protocolVersion"] != 1 {
        return Err("approval scope/protocol mismatch".into());
    }
    let name = event["toolName"].as_str().ok_or("missing tool name")?;
    let input = &event["input"];
    let path = match name {
        "Write" => input["file_path"].as_str(),
        "write" => input["path"].as_str(),
        _ => return Err("probe only allows Write/write".into()),
    }
    .ok_or("missing write path")?;
    let candidate = if Path::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        cwd.join(path)
    };
    if candidate != cwd.join("probe.txt") || input["content"] != "loom-sdk-probe\n" {
        return Err("tool arguments are outside the synthetic fixture".into());
    }
    if candidate.symlink_metadata().is_ok() {
        return Err("probe destination must not exist".into());
    }
    if event["cwd"].as_str() != cwd.to_str()
        || event["toolCallId"].as_str().is_none_or(str::is_empty)
    {
        return Err("tool cwd or identity mismatch".into());
    }
    let binding = json!({"context": scope, "toolCallId":event["toolCallId"], "toolName":name, "input":input, "cwd":cwd});
    let hash = digest(&binding);
    if event["digest"] != hash {
        return Err("tool binding digest mismatch".into());
    }
    Ok(hash)
}
fn send(pipe: &mut impl Write, value: Value) -> Result<(), Box<dyn std::error::Error>> {
    serde_json::to_writer(&mut *pipe, &value)?;
    pipe.write_all(b"\n")?;
    pipe.flush()?;
    Ok(())
}
fn terminate(child: &mut std::process::Child) {
    if !matches!(child.try_wait(), Ok(None)) {
        return;
    }
    #[cfg(unix)]
    // This process group was created specifically for the probe worker. Never
    // signal a persisted PID or an unrelated user's SDK process.
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    #[cfg(not(unix))]
    let _ = child.kill();
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 6 {
        return Err("Usage: probe <bun> <worker.ts> <claude|pi> <loopback endpoint> <fixture cwd> <allow|deny|cancel|stale|disconnect>".into());
    }
    let cwd = Path::new(&args[4]).canonicalize()?;
    let decision = args[5].as_str();
    if !["allow", "deny", "cancel", "stale", "disconnect"].contains(&decision) {
        return Err("Unknown probe decision".into());
    }
    let scope = Context {
        workspace_id: "m0-workspace".into(),
        session_id: "m0-session".into(),
        turn_id: "m0-turn".into(),
    };
    let mut command = Command::new(&args[0]);
    command.arg(&args[1]).current_dir(&cwd).env_clear();
    for key in ["PATH", "HOME", "USER", "SHELL", "TMPDIR", "LANG"] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    #[cfg(unix)]
    command.process_group(0);
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut input = Some(child.stdin.take().ok_or("missing stdin")?);
    let output = child.stdout.take().ok_or("missing stdout")?;
    let child = Arc::new(Mutex::new(child));
    let watchdog = Arc::clone(&child);
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(45));
        terminate(&mut watchdog.lock().unwrap());
    });
    send(
        input.as_mut().unwrap(),
        json!({"protocolVersion":1, "kind":"start", "context":scope,
        "provider":args[2], "endpoint":args[3], "cwd":cwd, "runtimeDir":cwd.join("sdk-state"),
        "prompt":"Write exactly loom-sdk-probe followed by a newline to probe.txt using the write tool, then report the result."}),
    )?;
    let mut reader = BufReader::new(output);
    let mut seen = HashSet::new();
    let mut terminal = false;
    let run = (|| -> Result<(), Box<dyn std::error::Error>> {
        loop {
            let mut bytes = Vec::new();
            let count = reader
                .by_ref()
                .take(1024 * 1024 + 1)
                .read_until(b'\n', &mut bytes)?;
            if count == 0 {
                break;
            }
            if count > 1024 * 1024 || !bytes.ends_with(b"\n") {
                return Err("Invalid or oversized worker frame".into());
            }
            let event: Value = serde_json::from_slice(&bytes)?;
            if event["kind"] != "stream" {
                println!("{}", event);
            }
            if event["kind"] == "approvalRequested" {
                let hash = approved_binding(&event, &scope, &cwd)?;
                let id = event["requestId"].as_str().ok_or("missing request ID")?;
                if !seen.insert(id.to_string()) {
                    return Err("Replayed approval request".into());
                }
                std::thread::sleep(Duration::from_millis(100));
                if cwd.join("probe.txt").exists() {
                    return Err("Side effect occurred before Rust approval".into());
                }
                let response = json!({"protocolVersion":1, "kind":"approvalResponse", "context":scope, "requestId":id, "digest":hash, "allowed": decision == "allow" || decision == "stale"});
                if decision == "disconnect" {
                    drop(input.take());
                    continue;
                }
                let pipe = input.as_mut().ok_or("worker disconnected")?;
                if decision == "cancel" {
                    send(
                        pipe,
                        json!({"protocolVersion":1,"kind":"cancel","context":scope}),
                    )?;
                } else if decision == "stale" {
                    let mut stale = response.clone();
                    stale["context"]["turnId"] = json!("expired-turn");
                    send(pipe, stale)?;
                    std::thread::sleep(Duration::from_millis(100));
                    if cwd.join("probe.txt").exists() {
                        return Err("Stale approval executed a tool".into());
                    }
                    send(pipe, response)?;
                } else {
                    send(pipe, response)?;
                }
            }
            if event["kind"] == "completed" || event["kind"] == "error" {
                terminal = true;
            }
        }
        if !terminal || seen.is_empty() {
            return Err("SDK never reached a tool approval and terminal result".into());
        }
        Ok(())
    })();
    drop(input);
    if run.is_err() {
        terminate(&mut child.lock().unwrap());
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(status) = child.lock().unwrap().try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            terminate(&mut child.lock().unwrap());
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    run?;
    if !status.success() {
        return Err(format!("worker failed: {status}").into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn binding_rejects_changed_scope_arguments_and_digest() {
        let scope = Context {
            workspace_id: "w".into(),
            session_id: "s".into(),
            turn_id: "t".into(),
        };
        let cwd = std::env::temp_dir().join(format!("loom-binding-{}", std::process::id()));
        std::fs::create_dir_all(&cwd).unwrap();
        let mut event = json!({"protocolVersion":1,"context":scope,"toolCallId":"tool-1","toolName":"Write","input":{"file_path":cwd.join("probe.txt"),"content":"loom-sdk-probe\n"},"cwd":cwd});
        let binding = json!({"context":scope,"toolCallId":event["toolCallId"],"toolName":event["toolName"],"input":event["input"],"cwd":cwd});
        event["digest"] = json!(digest(&binding));
        assert!(approved_binding(&event, &scope, &cwd).is_ok());
        for (key, value) in [
            (
                "context",
                json!({"workspaceId":"w","sessionId":"s","turnId":"other"}),
            ),
            (
                "input",
                json!({"file_path":"/tmp/other","content":"loom-sdk-probe\n"}),
            ),
            ("digest", json!("forged")),
        ] {
            let mut changed = event.clone();
            changed[key] = value;
            assert!(approved_binding(&changed, &scope, &cwd).is_err());
        }
        std::fs::remove_dir(&cwd).unwrap();
    }
}
