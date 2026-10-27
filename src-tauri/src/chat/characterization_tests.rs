//! Characterization tests that pin the current agent runner behaviour before it
//! moves out of Chat (agent layer plan, P1). Recorded CLI streams are replayed
//! through the real runner and compared with checked-in golden snapshots; any
//! change to events, outcome or logs must be deliberate.
//!
//! Regenerate the snapshots with `LOOM_UPDATE_GOLDEN=1` and review the diff.
use super::runtime::{run_observed, run_owned, Limits, Observation, Request};
use crate::{
    agent_adapter::PreparedAgentInvocation,
    process_supervisor::{self, ProcessKind, ProcessOwner},
};
use serde_json::{json, Value};
use std::path::PathBuf;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cli-streams")
}

fn request(turn_id: &str, output_mode: &str, program: &str, args: Vec<String>) -> Request {
    Request {
        prepared: PreparedAgentInvocation {
            program: program.into(),
            args,
            cwd: std::env::temp_dir().to_string_lossy().into_owned(),
            stdin_prompt: false,
            output_mode: output_mode.into(),
            resumed: false,
            environment: Default::default(),
        },
        project_key: "/fixture".into(),
        session_id: "session".into(),
        turn_id: turn_id.into(),
        message_id: "message".into(),
        stdin_text: None,
        limits: Limits::default(),
    }
}

fn shell(turn_id: &str, output_mode: &str, script: &str) -> Request {
    request(
        turn_id,
        output_mode,
        "/bin/sh",
        vec!["-c".into(), script.into()],
    )
}

/// FNV-1a, stable across toolchains, so large log streams are pinned without
/// copying them into the snapshot.
fn fnv1a(text: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// Replays `request` and returns everything a caller can observe, minus the
/// process id and timing.
async fn observe(request: Request) -> Value {
    let mut events = Vec::new();
    let mut logs = Vec::new();
    let mut started = 0;
    let outcome = run_observed(
        request,
        || None,
        |event| {
            let mut value = serde_json::to_value(&event).unwrap();
            let fields = value.as_object_mut().unwrap();
            for constant in ["sessionId", "turnId", "messageId"] {
                fields.remove(constant);
            }
            events.push(value);
            Ok(())
        },
        |observation| {
            match observation {
                Observation::Started(_) => started += 1,
                Observation::Log { stderr, line } => logs.push((stderr, line)),
            }
            Ok(())
        },
    )
    .await
    .unwrap();
    let joined = logs
        .iter()
        .map(|(stderr, line)| format!("{}{line}", if *stderr { "E " } else { "O " }))
        .collect::<Vec<_>>()
        .join("\n");
    json!({
        "started": started,
        "events": events,
        "logs": { "lines": logs.len(), "stderrLines": logs.iter().filter(|(e, _)| *e).count(), "fnv1a": fnv1a(&joined) },
        "outcome": {
            "content": outcome.content,
            "parts": outcome.parts,
            "resumeCommand": outcome.resume_command,
            "status": outcome.status,
            "errorSummary": outcome.error_summary,
            "exitCode": outcome.exit_code,
            "terminationReason": outcome.termination_reason,
        },
    })
}

fn assert_golden(path: PathBuf, actual: &Value) {
    let rendered = serde_json::to_string_pretty(actual).unwrap() + "\n";
    if std::env::var("LOOM_UPDATE_GOLDEN").as_deref() == Ok("1") {
        std::fs::write(&path, &rendered).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "missing golden {}; run with LOOM_UPDATE_GOLDEN=1",
            path.display()
        )
    });
    assert!(
        expected == rendered,
        "runner output changed for {}; review and regenerate with LOOM_UPDATE_GOLDEN=1",
        path.display()
    );
}

#[tokio::test]
async fn recorded_cli_streams_replay_to_the_same_events_and_outcome() {
    for (dir, file, output_mode) in [
        ("claude-2.1.304", "read-command.jsonl", "claude_stream_json"),
        ("codex-0.163.0", "read-command.jsonl", "codex_json"),
        ("opencode-1.19.3", "read.jsonl", "opencode_json"),
        ("grok-1.0.62", "read-command.jsonl", "streaming_json"),
    ] {
        let source = fixtures().join(dir).join(file);
        let replay = request(
            &format!("golden-{dir}"),
            output_mode,
            "/bin/cat",
            vec![source.to_string_lossy().into_owned()],
        );
        let actual = observe(replay).await;
        assert_golden(
            fixtures().join(dir).join(format!("{file}.golden.json")),
            &actual,
        );
    }
}

#[tokio::test]
async fn non_zero_exit_keeps_partial_text_and_the_stderr_tail() {
    let long_warning = "w".repeat(2_500);
    let script = format!(
        "printf '%s\\n' '{}'; echo '{long_warning}' >&2; echo 'fatal: model missing' >&2; exit 3",
        r#"{"type":"item.completed","item":{"id":"i0","type":"agent_message","text":"partial"}}"#
    );
    let actual = observe(shell("char-exit", "codex_json", &script)).await;
    let outcome = &actual["outcome"];
    assert_eq!(outcome["status"], "error");
    assert_eq!(outcome["exitCode"], 3);
    assert_eq!(outcome["content"], "partial");
    assert_eq!(outcome["terminationReason"], "exit status: 3");
    assert!(outcome["resumeCommand"].is_null());
    let summary = outcome["errorSummary"].as_str().unwrap();
    let (head, tail) = summary.split_once("; stderr: ").unwrap();
    assert_eq!(head, "agent exited with status exit status: 3");
    // Only the last 2,000 characters of stderr are kept, ending with the fatal line.
    assert_eq!(tail.chars().count(), 2_000);
    assert!(tail.ends_with("\nfatal: model missing"));
    let parts = outcome["parts"].as_array().unwrap();
    let error = parts.last().unwrap();
    assert_eq!(error["type"], "error");
    assert_eq!(error["code"], "agent_exit_failed");
    assert_eq!(error["message"], summary);
    assert_eq!(actual["logs"]["stderrLines"], 2);
}

#[tokio::test]
async fn silent_success_and_spawn_failure_are_distinct() {
    let actual = observe(shell("char-silent", "codex_json", "exit 0")).await;
    assert_eq!(actual["outcome"]["status"], "complete");
    assert_eq!(actual["outcome"]["content"], "（Agent 没有返回可见文本）");
    assert_eq!(actual["events"], json!([{ "delta": "", "done": true }]));

    let missing = request(
        "char-missing",
        "codex_json",
        "/nonexistent/loom-agent",
        Vec::new(),
    );
    let error = run_observed(missing, || None, |_| Ok(()), |_| Ok(()))
        .await
        .unwrap_err();
    assert!(error.starts_with("failed to start agent: "), "{error}");
}

#[tokio::test]
async fn stdin_prompt_without_input_is_rejected_before_spawn() {
    let mut missing_input = shell("char-stdin", "plain", "cat");
    missing_input.prepared.stdin_prompt = true;
    let error = run_observed(
        missing_input,
        || None,
        |_| Ok(()),
        |_| panic!("agent was started"),
    )
    .await
    .unwrap_err();
    assert_eq!(
        error,
        "stdin prompt was requested but no input was supplied"
    );
}

#[tokio::test]
async fn non_chat_owner_registers_the_process_under_learn() {
    let turn = "char-learn-owner";
    let owner = ProcessOwner::Learn {
        study_id: "study".into(),
        job_id: "job".into(),
    };
    let mut seen = None;
    let outcome = run_owned(
        shell(turn, "plain", "echo hello"),
        Some(owner.clone()),
        || None,
        |_| Ok(()),
        |observation| {
            if let Observation::Started(_) = observation {
                let meta = process_supervisor::supervisor().metadata(turn).unwrap();
                seen = Some((meta.owner, meta.kind));
            }
            Ok(())
        },
    )
    .await
    .unwrap();
    assert_eq!(outcome.status, "complete");
    assert_eq!(outcome.content, "hello\n");
    assert_eq!(seen, Some((owner, ProcessKind::Learn)));
    assert!(process_supervisor::supervisor().metadata(turn).is_none());
}
