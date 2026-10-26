use super::*;
use crate::skills::tests::Fixture;
use serde_json::json;
#[test]
fn local_commands_are_isolated_and_never_embed_document_text() {
    let (_, codex) = command(
        ProviderId::Codex,
        Some("fixture-model"),
        Path::new("/tmp/result.md"),
    )
    .unwrap();
    assert!(codex.iter().any(|a| a == "read-only"));
    assert!(codex.iter().any(|a| a == "--ephemeral"));
    assert!(codex.iter().any(|a| a == "--ignore-user-config"));
    assert_eq!(codex.last().unwrap(), "-");
    assert!(!codex.iter().any(|a| a == "--ignore-rules"));
    let (_, claude) = command(ProviderId::ClaudeCode, None, Path::new("/tmp/result.md")).unwrap();
    for flag in [
        "--no-session-persistence",
        "--safe-mode",
        "--restricted",
        "--strict-mcp-config",
    ] {
        assert!(claude.contains(&flag.into()));
    }
    let index = claude.iter().position(|a| a == "--tools").unwrap();
    assert_eq!(claude[index + 1], "");
    assert_eq!(
        parse_claude(
            &serde_json::to_vec(
                &json!({"type":"result","subtype":"success","is_error":false,"result":"# 翻译"})
            )
            .unwrap()
        )
        .unwrap(),
        "# 翻译"
    );
    assert!(parse_claude(
        br#"{"type":"result","subtype":"error_max_turns","is_error":true,"result":"secret"}"#
    )
    .is_err());
}
#[cfg(unix)]
#[tokio::test]
async fn supervised_process_bounds_io_and_timeout_without_recording_text() {
    let f = Fixture::new();
    let r = Repository::open(&f.0.join("data"), &f.0.join("coord")).unwrap();
    for (name, script, timeout, expected) in [
        ("success", "cat", Duration::from_secs(3), None),
        (
            "error",
            "cat >/dev/null; echo private-token >&2; exit 2",
            Duration::from_secs(3),
            Some(ErrorCode::SourceUnavailable),
        ),
        (
            "timeout",
            "cat >/dev/null; sleep 3",
            Duration::from_millis(30),
            Some(ErrorCode::Timeout),
        ),
        (
            "overflow",
            "cat >/dev/null; dd if=/dev/zero bs=1048576 count=3 2>/dev/null",
            Duration::from_secs(3),
            Some(ErrorCode::InvalidInput),
        ),
    ] {
        let mut op = Operation::begin(&r, "fixture", "translate").unwrap();
        op.record.executions.push(ExecutionRecord {
            program: "fixture".into(),
            args: vec![],
            cwd: f.0.clone(),
            started_at_ms: 0,
            finished_at_ms: None,
            exit_code: None,
            error: None,
        });
        let actual = execute(
            &mut op,
            "/bin/sh",
            &["-c".into(), script.into()],
            &f.0,
            "document-secret",
            timeout,
        )
        .await;
        match expected {
            None => assert_eq!(actual.unwrap(), b"document-secret"),
            Some(code) => {
                let error = actual.unwrap_err();
                assert_eq!(error.code, code, "{name}");
                assert!(!error.message.contains("private-token"));
            }
        }
        assert!(!serde_json::to_string(&op.record)
            .unwrap()
            .contains("document-secret"));
        assert!(!serde_json::to_string(&op.record)
            .unwrap()
            .contains("private-token"));
    }
}

#[cfg(unix)]
#[tokio::test]
async fn local_process_cancels_inflight_without_waiting_for_its_normal_exit() {
    let f = Fixture::new();
    let repo = Repository::open(&f.0.join("data"), &f.0.join("coord")).unwrap();
    let mut op = Operation::begin(&repo, "fixture", "translate").unwrap();
    let id = op.record.operation_id.clone();
    op.record.executions.push(ExecutionRecord {
        program: "fixture".into(),
        args: vec![],
        cwd: f.0.clone(),
        started_at_ms: 0,
        finished_at_ms: None,
        exit_code: None,
        error: None,
    });
    let start = tokio::time::Instant::now();
    let args = ["-c".into(), "cat >/dev/null; sleep 30".into()];
    let work = execute(
        &mut op,
        "/bin/sh",
        &args,
        &f.0,
        "synthetic text",
        Duration::from_secs(10),
    );
    let cancel = async {
        tokio::time::sleep(Duration::from_millis(80)).await;
        crate::skills::operations::cancel(&repo, &id).unwrap();
    };
    let (outcome, ()) = tokio::join!(work, cancel);
    assert_eq!(outcome.unwrap_err().code, ErrorCode::Cancelled);
    assert!(start.elapsed() < Duration::from_secs(5));
    assert!(!serde_json::to_string(&op.record)
        .unwrap()
        .contains("synthetic text"));
}
