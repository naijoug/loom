//! Domain mutations shared by the IPC boundary and behavior tests.
use super::{
    repository::{ChatRepository, TurnLease},
    ChatMessagePart, ChatSession, ChatTurnFinishedEvent, ChatTurnOutcome,
};

pub(super) fn require_idle(session: &ChatSession) -> Result<(), String> {
    if session.turn_status == "streaming" {
        return Err("回合运行中，请先停止后再修改会话。".into());
    }
    Ok(())
}

/// Production persistence/runner boundary, also used by local subprocess tests.
pub(super) async fn run_chat_turn(
    repository: &ChatRepository,
    lease: &TurnLease,
    request: super::runtime::Request,
    publish: impl Fn(super::ChatEvent),
) -> Result<ChatTurnOutcome, String> {
    let mut logs = repository.open_run_logs(lease)?;
    super::runtime::run_observed(
        request,
        || lease.stop_reason(),
        |event| {
            publish(repository.append_stream(lease, event)?);
            Ok(())
        },
        |observation| match observation {
            super::runtime::Observation::Started(pid) => {
                let snapshot = repository.mark_running(lease, pid)?;
                if let Ok(event) = repository.event_at(&snapshot) {
                    publish(event);
                }
                Ok(())
            }
            super::runtime::Observation::Log { stderr, line } => logs.append(stderr, &line),
        },
    )
    .await
}

pub(super) fn finish_chat_turn(
    repository: &ChatRepository,
    lease: &TurnLease,
    assistant_id: &str,
    resume_binding: Option<&super::resume::ResumeBinding>,
    result: Result<ChatTurnOutcome, String>,
) -> ChatTurnFinishedEvent {
    let mut event_status = "error".to_string();
    let mut event_error = None;
    let saved = repository.finish(lease, |session| {
        if let Some(turn) = session
            .turns
            .iter_mut()
            .find(|turn| turn.id == lease.turn_id)
        {
            if turn.status.is_terminal() {
                return Err("chat turn was already completed".into());
            }
            turn.finished_at_ms = Some(crate::models::now_ms() as u64);
            match &result {
                Ok(outcome) => {
                    turn.status = match outcome.status.as_str() {
                        "complete" => super::ChatTurnStatus::Completed,
                        "aborted"
                            if outcome.termination_reason.as_deref()
                                == Some(super::STOP_REASON_TIMEOUT) =>
                        {
                            super::ChatTurnStatus::TimedOut
                        }
                        "aborted" => super::ChatTurnStatus::Cancelled,
                        _ => super::ChatTurnStatus::Failed,
                    };
                    turn.exit_code = outcome.exit_code;
                    turn.termination_reason = outcome.termination_reason.clone();
                    turn.error_summary = outcome.error_summary.clone();
                }
                Err(error) => {
                    turn.status = super::ChatTurnStatus::Failed;
                    turn.termination_reason = Some("runtime_error".into());
                    turn.error_summary = Some(crate::agents::redact_sensitive_text(error));
                }
            }
        }
        let message = session
            .messages
            .iter_mut()
            .find(|message| message.id == assistant_id)
            .ok_or("assistant message no longer exists")?;
        match result {
            Ok(outcome) => {
                event_status = outcome.status.clone();
                event_error = outcome.error_summary.clone();
                message.content = outcome.content;
                message.parts = outcome.parts;
                message.status = outcome.status;
                message.error_summary = outcome.error_summary;
                if message.status == "complete" {
                    if let Some(resume) = outcome.resume_command {
                        session.resume_handle = resume_binding
                            .and_then(|binding| super::resume::capture(binding, &resume));
                        session.resume_command = Some(resume);
                    }
                }
                if message.status == "error" || message.error_summary.is_some() {
                    session.flagged = true;
                }
            }
            Err(error) => {
                event_error = Some(error.clone());
                if message.content.trim().is_empty() {
                    message.content = format!("（调用失败）{error}");
                }
                message.status = "error".into();
                message.error_summary = Some(error.clone());
                message.parts.push(ChatMessagePart::Error {
                    message: error,
                    code: None,
                });
                session.flagged = true;
            }
        }
        Ok(())
    });
    let saved_event = saved.and_then(|session| repository.event_at(&session));
    let journal_event = match saved_event {
        Ok(event) => Some(event),
        Err(error) => {
            if crate::process_supervisor::supervisor().is_shutting_down() {
                super::shutdown::remember(repository.clone(), lease.session_id.clone());
            }
            event_status = "error".into();
            event_error = Some(format!(
                "storage_error: failed to save chat result: {error}"
            ));
            None
        }
    };

    ChatTurnFinishedEvent {
        project_key: repository.project_path().to_string_lossy().into_owned(),
        session_id: lease.session_id.clone(),
        turn_id: lease.turn_id.clone(),
        message_id: assistant_id.to_string(),
        status: event_status,
        error_summary: event_error,
        journal_event,
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::{
        agent_adapter::PreparedAgentInvocation,
        chat::{
            repository::BeginTurn, runtime, turns, ChatInvocationSnapshot, ChatLogStream,
            ChatMessage, ChatSendInput, ChatTurnStatus,
        },
        models::{now_ms, IdGenerator},
    };
    use std::{fs, path::PathBuf, time::Duration};

    pub(in crate::chat) struct Fixture {
        pub(in crate::chat) path: PathBuf,
    }
    impl Fixture {
        pub(in crate::chat) fn new() -> Self {
            let path = std::env::temp_dir().join(IdGenerator::default().next("loom-turn-audit"));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }
        pub(in crate::chat) fn start(
            &self,
            mode: &str,
        ) -> (ChatRepository, TurnLease, runtime::Request) {
            let repo = ChatRepository::open(&self.path).unwrap();
            let session: ChatSession = serde_json::from_value(serde_json::json!({
                "id":"session", "projectPath":repo.project_path(), "agentId":"fixture", "title":"Audit",
                "permissionMode":"explore", "messages":[], "createdAtMs":now_ms(), "updatedAtMs":now_ms(),
                "turnStatus":"idle", "schemaVersion":2
            })).unwrap();
            repo.create(&session).unwrap();
            let prepared = PreparedAgentInvocation {
                program: "sh".into(),
                args: vec![
                    format!(
                        "{}/../tests/fixtures/chat/fake-agent.sh",
                        env!("CARGO_MANIFEST_DIR")
                    ),
                    mode.into(),
                ],
                cwd: repo.project_path().to_string_lossy().into_owned(),
                stdin_prompt: false,
                output_mode: "plain".into(),
                resumed: false,
            };
            let snapshot = ChatInvocationSnapshot {
                agent_id: "fixture".into(),
                adapter_type: "test-fixture".into(),
                program: prepared.program.clone(),
                args: prepared.args.clone(),
                cwd: prepared.cwd.clone(),
                permission_mode: "explore".into(),
                stdin_prompt: false,
                output_mode: "plain".into(),
                config_fingerprint: "fixture-config".into(),
            };
            let input = ChatSendInput {
                project_path: prepared.cwd.clone(),
                session_id: "session".into(),
                client_request_id: "request".into(),
                text: "fixture input".into(),
                permission_mode: Some("explore".into()),
            };
            let turn_id = IdGenerator::default().next("turn");
            let accepted = repo
                .begin_once(&input, &turn_id, |session| {
                    for role in ["user", "assistant"] {
                        session.messages.push(ChatMessage {
                            id: role.into(),
                            role: role.into(),
                            content: if role == "user" {
                                input.text.clone()
                            } else {
                                String::new()
                            },
                            status: if role == "user" {
                                "complete"
                            } else {
                                "streaming"
                            }
                            .into(),
                            created_at_ms: now_ms(),
                            error_summary: None,
                            parts: vec![],
                        });
                    }
                    session.turns.push(turns::new_turn(
                        &turn_id,
                        "request",
                        "user",
                        "assistant",
                        snapshot,
                    ));
                    Ok(())
                })
                .unwrap();
            let BeginTurn::Started(_, (), lease) = accepted else {
                panic!()
            };
            let request = runtime::Request {
                prepared,
                project_key: repo.project_path().to_string_lossy().into_owned(),
                session_id: "session".into(),
                turn_id: turn_id.clone(),
                message_id: "assistant".into(),
                stdin_text: None,
                limits: runtime::Limits::default(),
            };
            (repo, lease, request)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[tokio::test]
    async fn run_records_exit_configuration_and_separate_redacted_logs() {
        let f = Fixture::new();
        let (repo, lease, request) = f.start("audit-logs");
        let turn_id = lease.turn_id.clone();
        let starting = repo.load("session").unwrap();
        assert_eq!(starting.turns[0].status, ChatTurnStatus::Starting);
        let result = run_chat_turn(&repo, &lease, request, |_| {}).await;
        assert_eq!(
            repo.load("session").unwrap().turns[0].status,
            ChatTurnStatus::Running
        );
        repo.update("session", |s| {
            s.title = "renamed during execution".into();
            Ok(())
        })
        .unwrap();
        let event = finish_chat_turn(&repo, &lease, "assistant", None, result);
        assert_eq!(event.status, "error");
        let session = repo.load("session").unwrap();
        let turn = &session.turns[0];
        assert_eq!(turn.status, ChatTurnStatus::Failed);
        assert_eq!(turn.exit_code, Some(7));
        assert!(
            turn.started_at_ms.is_some()
                && turn.finished_at_ms.is_some()
                && turn.process_id.is_some()
        );
        assert_eq!(turn.invocation, starting.turns[0].invocation);
        assert_eq!(session.title, "renamed during execution");
        let stdout = repo
            .read_run_logs("session", &turn_id, ChatLogStream::Stdout, 0, 32768)
            .unwrap();
        let stderr = repo
            .read_run_logs("session", &turn_id, ChatLogStream::Stderr, 0, 32768)
            .unwrap();
        assert!(
            stdout.text.contains("first 中 output") && !stdout.text.contains("stderr diagnostic")
        );
        assert!(
            stderr.text.contains("stderr diagnostic") && !stderr.text.contains("first 中 output")
        );
        for text in [&stdout.text, &stderr.text] {
            assert!(!text.contains("fixture-private-value"));
            assert!(!text.contains("fixture-bearer-value"));
            assert!(text.contains("REDACTED"));
        }
        assert!(repo
            .update("session", |s| {
                s.turns[0].invocation.cwd = "/wrong".into();
                Ok(())
            })
            .is_err());
        assert!(repo
            .update("session", |s| {
                s.turns[0].exit_code = Some(0);
                Ok(())
            })
            .is_err());
        assert!(repo
            .read_run_logs("session", "unrelated", ChatLogStream::Stdout, 0, 100)
            .is_err());
        assert!(repo
            .read_run_logs("session", &turn_id, ChatLogStream::Stdout, 0, 100000)
            .is_err());
        let mut offset = 0;
        let mut text = String::new();
        loop {
            let page = repo
                .read_run_logs("session", &turn_id, ChatLogStream::Stdout, offset, 7)
                .unwrap();
            text.push_str(&page.text);
            offset = page.next_offset;
            if !page.has_more {
                break;
            }
        }
        assert_eq!(text, stdout.text);
        assert_eq!(offset, stdout.next_offset);
        drop(lease);
        drop(repo);
        let reopened = ChatRepository::open(&f.path).unwrap();
        assert_eq!(reopened.load("session").unwrap().turns[0], *turn);
        assert_eq!(
            reopened
                .read_run_logs("session", &turn_id, ChatLogStream::Stderr, 0, 32768)
                .unwrap()
                .text,
            stderr.text
        );
        #[cfg(unix)]
        {
            let log = f.path.join(format!(
                ".loom/chat/v2/sessions/session/runs/{turn_id}/stdout.log"
            ));
            fs::remove_file(&log).unwrap();
            std::os::unix::fs::symlink("/etc/hosts", &log).unwrap();
            assert!(reopened
                .read_run_logs("session", &turn_id, ChatLogStream::Stdout, 0, 100)
                .is_err());
        }
    }

    #[tokio::test]
    async fn cancellation_timeout_spawn_failure_and_success_have_distinct_terminal_records() {
        for case in [
            "cancel-before",
            "cancel-running",
            "timeout",
            "spawn-failure",
            "success",
        ] {
            let f = Fixture::new();
            let (repo, lease, mut request) =
                f.start(if matches!(case, "cancel-running" | "timeout") {
                    "ignore-term"
                } else {
                    "utf8"
                });
            let turn_id = lease.turn_id.clone();
            request.limits.terminate_grace = Duration::from_millis(40);
            if case == "timeout" {
                request.limits.timeout = Duration::from_millis(100);
            }
            if case == "spawn-failure" {
                request.prepared.program = "/no/such/loom-agent".into();
            }
            if case == "cancel-before" {
                assert!(repo.cancel("session", Some("wrong-turn")).is_err());
                repo.cancel("session", Some(&turn_id)).unwrap();
                assert_eq!(
                    repo.load("session").unwrap().turns[0].status,
                    ChatTurnStatus::Cancelling
                );
            }
            let stopped = std::cell::Cell::new(false);
            let result = run_chat_turn(&repo, &lease, request, |event| {
                if case == "cancel-running"
                    && !stopped.get()
                    && matches!(event.body, super::super::ChatEventPayload::Stream(_))
                {
                    stopped.set(true);
                    repo.cancel("session", Some(&turn_id)).unwrap();
                    assert_eq!(
                        repo.load("session").unwrap().turns[0].status,
                        ChatTurnStatus::Cancelling
                    );
                }
            })
            .await;
            finish_chat_turn(&repo, &lease, "assistant", None, result);
            let snapshot = repo.load("session").unwrap();
            let turn = &snapshot.turns[0];
            let expected = match case {
                "timeout" => ChatTurnStatus::TimedOut,
                "spawn-failure" => ChatTurnStatus::Failed,
                "success" => ChatTurnStatus::Completed,
                _ => ChatTurnStatus::Cancelled,
            };
            assert_eq!(turn.status, expected, "{case}");
            assert!(turn.finished_at_ms.is_some());
            assert_eq!(
                turn.started_at_ms.is_none(),
                matches!(case, "cancel-before" | "spawn-failure"),
                "{case}"
            );
            if case == "success" {
                assert_eq!(turn.exit_code, Some(0));
            }
            if case == "timeout" {
                assert_eq!(
                    turn.termination_reason.as_deref(),
                    Some(super::super::STOP_REASON_TIMEOUT)
                );
            }
            assert_eq!(snapshot.turn_status, "idle");
        }
    }

    #[tokio::test]
    async fn recovery_keeps_logs_and_marks_unfinished_turn_interrupted_once() {
        let f = Fixture::new();
        let (repo, lease, request) = f.start("audit-logs");
        let turn_id = lease.turn_id.clone();
        let _uncommitted_result = run_chat_turn(&repo, &lease, request, |_| {}).await;
        let before = repo
            .read_run_logs("session", &turn_id, ChatLogStream::Stdout, 0, 32768)
            .unwrap()
            .text;
        drop(lease);
        drop(repo);
        let reopened = ChatRepository::open(&f.path).unwrap();
        let snapshot = reopened.load("session").unwrap();
        assert_eq!(snapshot.turns[0].status, ChatTurnStatus::Interrupted);
        assert_eq!(snapshot.turns[0].exit_code, None);
        assert_eq!(
            reopened.load("session").unwrap().last_seq,
            snapshot.last_seq
        );
        assert_eq!(
            reopened
                .read_run_logs("session", &turn_id, ChatLogStream::Stdout, 0, 32768)
                .unwrap()
                .text,
            before
        );
    }
}
