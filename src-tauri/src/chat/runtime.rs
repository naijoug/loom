//! Chat's view of the agent runner: registers the process as a Chat turn,
//! maps `AgentEvent` to `ChatStreamEvent` and adds file diffs to writable
//! Codex turns. Process handling lives in `crate::agent::run`.
use super::{ChatMessagePart, ChatStreamEvent, ChatTurnOutcome};
pub(crate) use crate::agent::{Limits, Observation};
use crate::{
    agent::{self, AgentEvent, PreparedAgentInvocation, RunSpec},
    agent::{ProcessKind, ProcessOwner},
};

pub(crate) struct Request {
    pub prepared: PreparedAgentInvocation,
    pub project_key: String,
    pub session_id: String,
    pub turn_id: String,
    pub message_id: String,
    pub stdin_text: Option<String>,
    pub limits: Limits,
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::{
        models::IdGenerator,
        process_supervisor::{self, ProcessOwner},
    };
    use std::time::Duration;
    use std::{
        fs,
        path::PathBuf,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
    };
    use tokio::time::Instant;

    struct Fixture {
        path: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(IdGenerator::default().next("loom-chat-runtime"));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }
        fn request(&self, mode: &str) -> Request {
            Request {
                prepared: PreparedAgentInvocation {
                    environment: Default::default(),
                    program: "/bin/sh".into(),
                    args: vec![
                        format!(
                            "{}/../tests/fixtures/chat/fake-agent.sh",
                            env!("CARGO_MANIFEST_DIR")
                        ),
                        mode.into(),
                    ],
                    cwd: self.path.to_string_lossy().into_owned(),
                    stdin_prompt: false,
                    output_mode: "plain".into(),
                    resumed: false,
                },
                project_key: self.path.to_string_lossy().into_owned(),
                session_id: IdGenerator::default().next("session"),
                turn_id: IdGenerator::default().next("turn"),
                message_id: "message-fixture".into(),
                stdin_text: None,
                limits: Limits {
                    timeout: Duration::from_secs(5),
                    terminate_grace: Duration::from_millis(80),
                    drain_grace: Duration::from_millis(50),
                    reap_grace: Duration::from_millis(400),
                    ..Limits::default()
                },
            }
        }
        async fn assert_descendant_gone(&self) {
            let pid: i32 = fs::read_to_string(self.path.join("child.pid"))
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            unsafe extern "C" {
                fn kill(pid: i32, signal: i32) -> i32;
            }
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                let result = unsafe { kill(pid, 0) };
                if result == -1 && std::io::Error::last_os_error().raw_os_error() == Some(3) {
                    break;
                }
                assert!(Instant::now() < deadline, "descendant {pid} was not reaped");
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[tokio::test]
    async fn batched_tool_results_all_reach_the_stream_and_final_transcript() {
        let f = Fixture::new();
        let mut request = f.request("batched-tools");
        request.prepared.output_mode = "streaming_json".into();
        let mut events = Vec::new();
        let result = run(
            request,
            || false,
            |event| {
                events.push(event);
                Ok(())
            },
        )
        .await
        .unwrap();
        assert_eq!(result.parts.len(), 3);
        assert_eq!(
            events
                .iter()
                .filter_map(|event| event.part.clone())
                .collect::<Vec<_>>(),
            result.parts
        );
        assert!(
            matches!(&result.parts[1], ChatMessagePart::Tool { name, output_summary: Some(output), .. }
            if name == "read-2" && output.contains("SENTINEL-SECOND"))
        );
        assert!(
            matches!(&result.parts[2], ChatMessagePart::Tool { status: Some(status), .. } if status == "error")
        );
    }

    #[tokio::test]
    async fn failed_log_observer_stops_the_process_and_releases_ownership() {
        let f = Fixture::new();
        let mut request = f.request("ignore-term");
        request.limits.terminate_grace = Duration::from_millis(40);
        let turn = request.turn_id.clone();
        let result = run_observed(
            request,
            || None,
            |_| Ok(()),
            |event| match event {
                Observation::Started(_) => Ok(()),
                Observation::Log { .. } => {
                    Err("chat run log write failed: synthetic disk failure".into())
                }
            },
        )
        .await
        .unwrap();
        assert_eq!(result.status, "error");
        assert!(result.error_summary.unwrap().contains("log write failed"));
        assert!(process_supervisor::supervisor().metadata(&turn).is_none());
    }

    #[tokio::test]
    async fn stderr_pressure_does_not_deadlock_stdout_and_owner_is_chat() {
        let f = Fixture::new();
        let request = f.request("stderr-flood");
        let turn = request.turn_id.clone();
        let project = request.project_key.clone();
        let mut seen = String::new();
        let result = run(request, || false, |event| {
            if !event.done {
                let meta = process_supervisor::supervisor().metadata(&event.turn_id).unwrap();
                assert!(matches!(meta.owner, ProcessOwner::Chat { project_key, .. } if project_key == project));
            }
            seen.push_str(&event.delta); Ok(())
        }).await.unwrap();
        assert_eq!(result.status, "complete");
        assert_eq!(seen, "answer after stderr\n");
        assert_eq!(result.content, seen);
        assert!(process_supervisor::supervisor().metadata(&turn).is_none());
    }

    #[tokio::test]
    async fn nonzero_exit_stays_failed_even_with_partial_output() {
        let f = Fixture::new();
        let result = run(f.request("partial-failure"), || false, |_| Ok(()))
            .await
            .unwrap();
        assert_eq!(result.status, "error");
        assert!(result.content.contains("partial answer"));
        assert!(result.error_summary.unwrap().contains("fixture failed"));
        assert!(result.resume_command.is_none());
    }

    #[tokio::test]
    async fn cancellation_escalates_past_ignored_term_and_keeps_partial() {
        let f = Fixture::new();
        let cancelled = Arc::new(AtomicBool::new(false));
        let stop = cancelled.clone();
        let started = Instant::now();
        let result = run(
            f.request("ignore-term"),
            || cancelled.load(Ordering::Acquire),
            |event| {
                if event.delta.contains("READY") {
                    stop.store(true, Ordering::Release);
                }
                Ok(())
            },
        )
        .await
        .unwrap();
        assert!(started.elapsed() < Duration::from_secs(3));
        assert_eq!(result.status, "aborted");
        assert!(result.content.contains("READY"));
        assert!(result.error_summary.is_none());
    }

    #[tokio::test]
    async fn timeout_escalates_past_ignored_term_and_cleans_registry() {
        let f = Fixture::new();
        let mut request = f.request("ignore-term");
        request.limits.timeout = Duration::from_millis(200);
        let turn = request.turn_id.clone();
        let result = run(request, || false, |_| Ok(())).await.unwrap();
        assert_eq!(result.status, "aborted");
        assert!(result.error_summary.unwrap().contains("超时"));
        assert!(process_supervisor::supervisor().metadata(&turn).is_none());
    }

    #[tokio::test]
    async fn exited_parent_with_inherited_pipes_does_not_hang_or_leave_child() {
        // Repetition exercises the short interval between KILL and OS reaping.
        for _ in 0..20 {
            let f = Fixture::new();
            let started = Instant::now();
            let result = run(f.request("descendant-pipes"), || false, |_| Ok(()))
                .await
                .unwrap();
            assert_eq!(result.status, "complete");
            assert!(result.content.contains("parent finished"));
            assert!(started.elapsed() < Duration::from_secs(3));
            f.assert_descendant_gone().await;
        }
    }

    #[tokio::test]
    async fn descendants_are_stopped_even_when_they_close_output_pipes() {
        let f = Fixture::new();
        let result = run(f.request("descendant-closed"), || false, |_| Ok(()))
            .await
            .unwrap();
        assert_eq!(result.status, "complete");
        f.assert_descendant_gone().await;
    }

    #[tokio::test]
    async fn stdin_is_written_concurrently_and_closed_at_end() {
        let f = Fixture::new();
        let mut request = f.request("stdin");
        request.prepared.stdin_prompt = true;
        let input = "中文 context\n".repeat(20_000);
        request.limits.max_lines = 30_000;
        request.stdin_text = Some(input.clone());
        let result = run(request, || false, |_| Ok(())).await.unwrap();
        assert_eq!(result.status, "complete");
        assert_eq!(result.content, input);
    }

    #[tokio::test]
    async fn utf8_across_reads_and_final_structured_messages_are_preserved() {
        let f = Fixture::new();
        let result = run(f.request("utf8"), || false, |_| Ok(())).await.unwrap();
        assert_eq!(result.content, "中\n");
        let mut request = f.request("unknown-json");
        request.prepared.output_mode = "codex_json".into();
        let result = run(request, || false, |_| Ok(())).await.unwrap();
        assert_eq!(result.content, "structured answer");
    }

    #[tokio::test]
    async fn output_budget_and_callback_failure_terminate_the_process() {
        let f = Fixture::new();
        let mut request = f.request("huge-line");
        request.limits.max_line_bytes = 100;
        let turn = request.turn_id.clone();
        let result = run(request, || false, |_| Ok(())).await.unwrap();
        assert_eq!(result.status, "error");
        assert!(result.error_summary.unwrap().contains("output_limit"));
        assert!(process_supervisor::supervisor().metadata(&turn).is_none());
        for bytes_budget in [true, false] {
            let mut request = f.request("stderr-flood");
            if bytes_budget {
                request.limits.max_bytes = 128;
            } else {
                request.limits.max_lines = 3;
            }
            let result = run(request, || false, |_| Ok(())).await.unwrap();
            assert_eq!(result.status, "error");
            assert!(result.error_summary.unwrap().starts_with("output_limit:"));
        }
        let result = run(
            f.request("ignore-term"),
            || false,
            |_| Err("consumer failed".into()),
        )
        .await
        .unwrap();
        assert_eq!(result.status, "error");
        assert!(result.content.contains("READY"));
        assert_eq!(result.error_summary.as_deref(), Some("consumer failed"));
        let result = run(
            f.request("utf8"),
            || false,
            |event| {
                if event.done {
                    Err("terminal consumer failed".into())
                } else {
                    Ok(())
                }
            },
        )
        .await
        .unwrap();
        assert_eq!(result.status, "error");
        assert_eq!(result.content, "中\n");
        assert_eq!(
            result.error_summary.as_deref(),
            Some("terminal consumer failed")
        );
    }

    #[tokio::test]
    async fn cancellation_before_spawn_and_spawn_errors_leave_no_registry_entry() {
        let f = Fixture::new();
        let mut request = f.request("stdin");
        request.prepared.program = "/nonexistent/loom-test-agent".into();
        let turn = request.turn_id.clone();
        let result = run(request, || true, |_| Ok(())).await.unwrap();
        assert_eq!(result.status, "aborted");
        assert!(process_supervisor::supervisor().metadata(&turn).is_none());
        let mut request = f.request("stdin");
        request.prepared.program = "/nonexistent/loom-test-agent".into();
        assert!(run(request, || false, |_| Ok(()))
            .await
            .unwrap_err()
            .contains("failed to start agent"));
    }

    #[tokio::test]
    async fn dropping_runtime_future_kills_its_process_and_releases_ownership() {
        let f = Fixture::new();
        let request = f.request("ignore-term");
        let turn = request.turn_id.clone();
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let mut sender = Some(sender);
            run(
                request,
                || false,
                |event| {
                    if event.delta.contains("READY") {
                        let pid = process_supervisor::supervisor()
                            .metadata(&event.turn_id)
                            .unwrap()
                            .process_id;
                        if let Some(sender) = sender.take() {
                            let _ = sender.send(pid);
                        }
                    }
                    Ok(())
                },
            )
            .await
        });
        let pid = tokio::time::timeout(Duration::from_secs(3), receiver)
            .await
            .unwrap()
            .unwrap();
        fs::write(f.path.join("child.pid"), pid.to_string()).unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert!(process_supervisor::supervisor().metadata(&turn).is_none());
        f.assert_descendant_gone().await;
    }
}

#[cfg(test)]
pub(super) async fn run(
    request: Request,
    cancelled: impl Fn() -> bool,
    emit: impl FnMut(ChatStreamEvent) -> Result<(), String>,
) -> Result<ChatTurnOutcome, String> {
    run_observed(
        request,
        || cancelled().then_some(super::STOP_REASON_ABORT),
        emit,
        |_| Ok(()),
    )
    .await
}

pub(super) async fn run_observed(
    request: Request,
    cancelled: impl Fn() -> Option<&'static str>,
    mut emit: impl FnMut(ChatStreamEvent) -> Result<(), String>,
    observe: impl FnMut(Observation) -> Result<(), String>,
) -> Result<ChatTurnOutcome, String> {
    let writable_codex = request.prepared.output_mode == "codex_json"
        && request
            .prepared
            .args
            .windows(2)
            .any(|pair| pair == ["--sandbox", "workspace-write"]);
    let enrich: Option<agent::LineEnricher> = if writable_codex {
        if let Some(reason) = cancelled() {
            return Ok(agent::stop_outcome(Some(reason), String::new(), Vec::new()));
        }
        let root = std::path::PathBuf::from(&request.prepared.cwd);
        let mut snapshot =
            tokio::task::spawn_blocking(move || super::file_changes::FileChanges::capture(&root))
                .await
                .map_err(|error| error.to_string())?;
        Some(Box::new(
            move |line: &str, parts: &mut [ChatMessagePart]| snapshot.enrich(line, parts),
        ))
    } else {
        None
    };
    let Request {
        prepared,
        project_key,
        session_id,
        turn_id,
        message_id,
        stdin_text,
        limits,
    } = request;
    let spec = RunSpec {
        prepared,
        run_id: turn_id.clone(),
        owner: ProcessOwner::Chat {
            project_key,
            session_id: session_id.clone(),
            turn_id: turn_id.clone(),
        },
        kind: ProcessKind::Chat,
        stdin_text,
        limits,
        enrich,
    };
    agent::run(
        spec,
        cancelled,
        |event| {
            let (delta, part, done) = match event {
                AgentEvent::Text(delta) => (delta, None, false),
                AgentEvent::Part(part) => (String::new(), Some(part), false),
                AgentEvent::Done => (String::new(), None, true),
            };
            emit(ChatStreamEvent {
                session_id: session_id.clone(),
                turn_id: turn_id.clone(),
                message_id: message_id.clone(),
                delta,
                part,
                done,
            })
        },
        observe,
    )
    .await
}

#[cfg(test)]
mod preparing_cancellation {
    use super::*;
    #[tokio::test]
    async fn cancellation_during_file_baseline_prevents_agent_spawn() {
        let root = std::env::temp_dir()
            .join(crate::models::IdGenerator::default().next("loom-cancel-before-spawn"));
        std::fs::create_dir_all(&root).unwrap();
        let checks = std::cell::Cell::new(0);
        let request = Request {
            prepared: PreparedAgentInvocation {
                program: "loom-nonexistent-must-not-spawn".into(),
                args: vec!["--sandbox".into(), "workspace-write".into()],
                cwd: root.to_string_lossy().into_owned(),
                stdin_prompt: false,
                output_mode: "codex_json".into(),
                resumed: false,
                environment: Default::default(),
            },
            project_key: root.to_string_lossy().into_owned(),
            session_id: "s".into(),
            turn_id: "t".into(),
            message_id: "m".into(),
            stdin_text: None,
            limits: Default::default(),
        };
        let result = run_observed(
            request,
            || {
                checks.set(checks.get() + 1);
                (checks.get() > 1).then_some(super::super::STOP_REASON_ABORT)
            },
            |_| Ok(()),
            |_| panic!("agent was started after cancellation"),
        )
        .await
        .unwrap();
        assert_eq!(result.status, "aborted");
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(all(test, unix))]
mod structured_failure_tests {
    use super::*;
    #[tokio::test]
    async fn provider_error_with_zero_exit_is_not_success_or_resumable() {
        let request = Request {
            prepared: PreparedAgentInvocation { program:"/bin/sh".into(),args:vec!["-c".into(),r#"printf '%s\n' '{"type":"error","error":{"message":"provider unavailable"}}'"#.into()],cwd:std::env::temp_dir().to_string_lossy().into_owned(),stdin_prompt:false,output_mode:"opencode_json".into(),resumed:false,environment:Default::default() },
            project_key:"/fixture".into(),session_id:"session".into(),turn_id:crate::models::IdGenerator::default().next("structured-failure"),message_id:"message".into(),stdin_text:None,limits:Default::default()
        };
        let result = run_observed(request, || None, |_| Ok(()), |_| Ok(()))
            .await
            .unwrap();
        assert_eq!(result.status, "error");
        assert_eq!(result.exit_code, Some(0));
        assert_eq!(
            result.error_summary.as_deref(),
            Some("provider unavailable")
        );
        assert!(result.resume_command.is_none());
    }

    fn scripted(output_mode: &str, lines: &[&str]) -> Request {
        let script = lines
            .iter()
            .map(|line| format!("printf '%s\\n' '{line}'"))
            .collect::<Vec<_>>()
            .join("; ");
        Request {
            prepared: PreparedAgentInvocation {
                program: "/bin/sh".into(),
                args: vec!["-c".into(), script],
                cwd: std::env::temp_dir().to_string_lossy().into_owned(),
                stdin_prompt: false,
                output_mode: output_mode.into(),
                resumed: false,
                environment: Default::default(),
            },
            project_key: "/fixture".into(),
            session_id: "session".into(),
            turn_id: crate::models::IdGenerator::default().next("blocks"),
            message_id: "message".into(),
            stdin_text: None,
            limits: Default::default(),
        }
    }

    #[tokio::test]
    async fn consecutive_text_blocks_become_separate_paragraphs() {
        let codex = scripted(
            "codex_json",
            &[
                r#"{"type":"item.completed","item":{"id":"i0","type":"agent_message","text":"适合降低使用成本。"}}"#,
                r#"{"type":"item.completed","item":{"id":"i1","type":"command_execution","command":"ls","aggregated_output":"a","exit_code":0,"status":"completed"}}"#,
                r#"{"type":"item.completed","item":{"id":"i2","type":"agent_message","text":"**WeKnora** 是框架\n"}}"#,
                r#"{"type":"item.completed","item":{"id":"i3","type":"agent_message","text":"结论"}}"#,
            ],
        );
        let mut deltas = String::new();
        let result = run_observed(
            codex,
            || None,
            |event| {
                deltas.push_str(&event.delta);
                Ok(())
            },
            |_| Ok(()),
        )
        .await
        .unwrap();
        assert_eq!(
            result.content,
            "适合降低使用成本。\n\n**WeKnora** 是框架\n\n结论"
        );
        assert_eq!(
            deltas, result.content,
            "streamed deltas carry the same separators"
        );

        let claude = scripted(
            "claude_stream_json",
            &[
                r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}"#,
                r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"先看文件。"}}}"#,
                r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"马上。"}}}"#,
                r#"{"type":"assistant","message":{"content":[{"type":"text","text":"先看文件。马上。"}]}}"#,
                r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}"#,
                r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"看完了。"}}}"#,
                r#"{"type":"assistant","message":{"content":[{"type":"text","text":"看完了。"}]}}"#,
            ],
        );
        let result = run_observed(claude, || None, |_| Ok(()), |_| Ok(()))
            .await
            .unwrap();
        assert_eq!(result.content, "先看文件。马上。\n\n看完了。");
    }
}
