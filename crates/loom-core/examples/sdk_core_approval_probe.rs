//! Integration harness: real SDK worker + production core approval journal.
//! Synthetic single-file policy only; this is not the desktop provider adapter.
use loom_core::{
    Core, CreateSession, FinishStatus, Id, PermissionMode, RuntimeBinding, RuntimeUpdate,
    SendMessage, SessionKey, TextKind, ToolApprovalSpec, ToolStatus,
};
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
    let core = Core::open(cwd.join("workbench-core"))?;
    let workspace = Id::new("sdk-workspace")?;
    let project = Id::new("sdk-project")?;
    core.create_workspace(workspace.clone(), "SDK fixture".into())?;
    core.register_project(
        workspace.clone(),
        project.clone(),
        "Synthetic project".into(),
        &cwd,
    )?;
    let key = SessionKey {
        workspace_id: workspace,
        session_id: Id::new("sdk-session")?,
    };
    core.create_session(CreateSession {
        key: key.clone(),
        title: Some("SDK approval integration".into()),
        project_id: Some(project),
        permission_mode: PermissionMode::Ask,
        connection: Default::default(),
    })?;
    let receipt = core
        .send_message(SendMessage {
            key: key.clone(),
            client_request_id: Id::new("fixture-send")?,
            text: "Run the synthetic Write/write fixture".into(),
            attachment_ids: vec![],
        })?
        .receipt;
    let lease = core.claim_next_turn(
        &key,
        RuntimeBinding {
            adapter: format!("sdk-fixture-{}", args[2]),
            connection: Default::default(),
            permission_mode: PermissionMode::Ask,
            policy_version: 1,
            working_directory: Some(cwd.to_string_lossy().into()),
            connection_snapshot_digest: None,
            policy_snapshot_digest: None,
        },
    )?;
    let scope = Context {
        workspace_id: key.workspace_id.to_string(),
        session_id: key.session_id.to_string(),
        turn_id: receipt.turn_id.to_string(),
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
    let mut approved_calls = HashSet::new();
    let mut text_index = 0;
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
            if serde_json::from_value::<Context>(event["context"].clone())? != scope {
                return Err("SDK frame scope mismatch".into());
            }
            if event["kind"] == "stream" {
                let frame = &event["event"];
                let delta = if frame["type"] == "content_block_delta"
                    && frame["delta"]["type"] == "text_delta"
                {
                    frame["delta"]["text"].as_str()
                } else if frame["type"] == "text_delta" {
                    frame["delta"].as_str()
                } else {
                    None
                };
                if let Some(delta) = delta.filter(|text| !text.is_empty()) {
                    if !lease.cancel_requested()? {
                        core.apply_runtime(
                            &lease,
                            Id::new(format!("sdk-text-{text_index}"))?,
                            RuntimeUpdate::TextDelta {
                                part_id: Id::new("sdk-reply")?,
                                text_kind: TextKind::Text,
                                delta: delta.into(),
                            },
                        )?;
                        text_index += 1;
                    }
                }
            }
            if event["kind"] == "toolResult" {
                let tool = event["toolCallId"]
                    .as_str()
                    .ok_or("missing SDK tool result ID")?;
                if !approved_calls.remove(tool) {
                    return Err("SDK tool result has no consumed core approval".into());
                }
                core.apply_runtime(
                    &lease,
                    Id::new(format!("sdk-result-{tool}"))?,
                    RuntimeUpdate::ToolFinished {
                        tool_call_id: Id::new(tool)?,
                        status: ToolStatus::Succeeded,
                        output: event["result"].clone(),
                    },
                )?;
            }
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
                let spec = ToolApprovalSpec {
                    request_id: Id::new(id)?,
                    tool_call_id: Id::new(event["toolCallId"].as_str().ok_or("missing tool ID")?)?,
                    tool_name: event["toolName"]
                        .as_str()
                        .ok_or("missing tool name")?
                        .into(),
                    input: event["input"].clone(),
                    ttl_ms: 15000,
                    remember: None,
                };
                let pending = core.request_tool_approval(&lease, &spec)?;
                if decision == "cancel" || decision == "disconnect" {
                    core.cancel_turn(&key, lease.turn_id())?;
                } else {
                    if decision == "stale"
                        && core
                            .respond_tool_approval(&key, &spec.request_id, "stale-binding", true)
                            .is_ok()
                    {
                        return Err("core accepted stale approval".into());
                    }
                    core.respond_tool_approval(
                        &key,
                        &spec.request_id,
                        &pending.binding_digest,
                        decision != "deny",
                    )?;
                    if decision != "deny" {
                        let permit = core.consume_tool_approval(&lease, &spec)?;
                        if !permit.is_live()? {
                            return Err("core permit expired before SDK response".into());
                        }
                        approved_calls.insert(spec.tool_call_id.to_string());
                    }
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
    let finish = if run.is_err() || !status.success() {
        FinishStatus::Failed
    } else if decision == "cancel" {
        FinishStatus::Cancelled
    } else if decision == "disconnect" {
        FinishStatus::Interrupted
    } else {
        FinishStatus::Completed
    };
    let view = core.finish_turn(&lease, finish, None)?;
    println!(
        "{}",
        json!({"kind":"coreAudit","approvalStatuses":view.approvals.values().map(|a|a.status).collect::<Vec<_>>(),"turnStatus":view.turns.last().map(|t|t.status),"activeTurnId":view.active_turn_id,"lastSeq":view.last_seq,"toolCount":view.tools.len()})
    );
    run?;
    if !status.success() {
        return Err(format!("worker failed: {status}").into());
    }
    Ok(())
}
