//! Opt-in real CLI acceptance. Never runs in the ordinary/offline test suite.
use super::{
    repository::{BeginTurn, ChatRepository},
    runtime,
    service::finish_chat_turn,
    ChatMessage, ChatSendInput, ChatSession,
};
use crate::{
    agent_adapter::{AgentStage, ADAPTER_GROK},
    chat_context::prepare_chat_invocation,
    models::{now_ms, AgentConfig, IdGenerator},
};

#[tokio::test]
#[ignore = "requires explicit LOOM_REAL_CHAT=1 and a configured Grok account; consumes model usage"]
async fn grok_three_turns_resume_after_repository_reopen() {
    assert_eq!(
        std::env::var("LOOM_REAL_CHAT").as_deref(),
        Ok("1"),
        "real model calls require explicit opt-in"
    );
    let ids = IdGenerator::default();
    let project = std::env::temp_dir().join(ids.next("loom-real-chat"));
    std::fs::create_dir_all(&project).unwrap();
    let agent = AgentConfig {
        id: "real-grok-canary".into(),
        name: "Grok canary".into(),
        command: "grok".into(),
        args: Vec::new(),
        working_directory_policy: "project_root".into(),
        capabilities: vec!["planning".into()],
        adapter_type: ADAPTER_GROK.into(),
        can_write_files: false,
        can_run_commands: false,
        enabled: true,
        available: true,
    };
    let session_id = ids.next("session");
    let marker = ids.next("LOOM-CANARY");
    {
        let repo = ChatRepository::open(&project).unwrap();
        let session: ChatSession = serde_json::from_value(serde_json::json!({
            "id": session_id, "projectPath": repo.project_path(), "agentId": agent.id,
            "title": "Real CLI canary", "permissionMode": "explore", "messages": [],
            "createdAtMs": now_ms(), "updatedAtMs": now_ms(), "turnStatus": "idle", "schemaVersion": 2
        })).unwrap();
        repo.create(&session).unwrap();
    }
    for index in 0..3 {
        // No prior repository instance survives: later turns must read their
        // native resume handle from disk, just as after an application restart.
        let repo = ChatRepository::open(&project).unwrap();
        let prompt = if index == 0 {
            format!("Remember this exact marker for later: {marker}. Reply with only that marker. Do not use tools, inspect files, or execute commands.")
        } else {
            "What exact marker did I ask you to remember? Reply only with that marker. Do not use any tools.".into()
        };
        let message_id = ids.next("assistant");
        let turn_id = ids.next("turn");
        let input = ChatSendInput {
            project_path: project.to_string_lossy().into_owned(),
            session_id: session_id.clone(),
            client_request_id: ids.next("request"),
            text: prompt.clone(),
            permission_mode: Some("explore".into()),
        };
        let accepted = repo
            .begin_once(&input, &turn_id, |session| {
                let transport =
                    prepare_chat_invocation(&agent, session, &prompt, AgentStage::Planning)?;
                assert_eq!(transport.invocation.resumed, index > 0);
                session.messages.push(ChatMessage {
                    id: ids.next("user"),
                    role: "user".into(),
                    content: prompt,
                    status: "complete".into(),
                    created_at_ms: now_ms(),
                    error_summary: None,
                    parts: Vec::new(),
                });
                session.messages.push(ChatMessage {
                    id: message_id.clone(),
                    role: "assistant".into(),
                    content: String::new(),
                    status: "streaming".into(),
                    created_at_ms: now_ms(),
                    error_summary: None,
                    parts: Vec::new(),
                });
                session.turns.push(super::turns::new_turn(
                    &turn_id,
                    &input.client_request_id,
                    &session.messages[session.messages.len() - 2].id,
                    &message_id,
                    transport.logged_invocation.clone(),
                ));
                Ok(transport)
            })
            .unwrap();
        let BeginTurn::Started(_, transport, lease) = accepted else {
            panic!("new request was deduplicated")
        };
        let retry = repo
            .begin_once::<()>(&input, "unused-active-retry", |_| {
                panic!("active duplicate prepared a real invocation")
            })
            .unwrap();
        assert!(matches!(retry, BeginTurn::Existing(ref result) if result.turn_id == turn_id));
        let resume_binding = transport.resume_binding;
        let request = runtime::Request {
            project_key: repo.project_path().to_string_lossy().into_owned(),
            session_id: session_id.clone(),
            turn_id: turn_id.clone(),
            message_id: message_id.clone(),
            prepared: transport.invocation,
            stdin_text: transport.stdin_text,
            limits: runtime::Limits {
                timeout: std::time::Duration::from_secs(90),
                ..runtime::Limits::default()
            },
        };
        let streamed_bytes = std::cell::Cell::new(0);
        let result = super::service::run_chat_turn(&repo, &lease, request, |event| {
            if let super::ChatEventPayload::Stream(stream) = event.body {
                streamed_bytes.set(streamed_bytes.get() + stream.delta.len());
            }
        })
        .await;
        let event = finish_chat_turn(&repo, &lease, &message_id, Some(&resume_binding), result);
        let snapshot = repo.load(&session_id).unwrap();
        let answer = snapshot.messages.last().unwrap();
        println!(
            "REAL_CHAT turn={} status={} stream_bytes={} native_resume={} marker_matches={}",
            index + 1,
            event.status,
            streamed_bytes.get(),
            snapshot.resume_command.is_some(),
            answer.content.matches(&marker).count()
        );
        assert_eq!(
            event.status,
            "complete",
            "{}",
            event.error_summary.unwrap_or_default()
        );
        assert!(streamed_bytes.get() > 0, "missing real streaming text");
        assert_eq!(
            snapshot.turns.last().unwrap().status,
            super::ChatTurnStatus::Completed
        );
        assert_eq!(snapshot.turns.last().unwrap().exit_code, Some(0));
        assert_eq!(
            answer.content.trim(),
            marker,
            "real CLI response was lost or duplicated"
        );
        assert!(
            snapshot.resume_command.is_some(),
            "native resume was not captured"
        );
        let handle = snapshot
            .resume_handle
            .as_ref()
            .expect("structured native resume missing");
        assert_eq!(handle.config_fingerprint, resume_binding.config_fingerprint);
        assert_eq!(handle.adapter_type, ADAPTER_GROK);
        assert_eq!(snapshot.send_receipts.len(), index + 1);
        drop(lease);
        drop(repo);
        let reopened = ChatRepository::open(&project).unwrap();
        let retry = reopened
            .begin_once::<()>(&input, "unused-completed-retry", |_| {
                panic!("completed duplicate prepared a real invocation")
            })
            .unwrap();
        let BeginTurn::Existing(receipt) = retry else {
            panic!("duplicate spawned")
        };
        assert_eq!(receipt.turn_id, turn_id);
        assert_eq!(receipt.session.last_seq, snapshot.last_seq);
        assert_eq!(receipt.session.messages.len(), (index + 1) * 2);
        println!(
            "REAL_CHAT turn={} duplicate_active=deduplicated duplicate_reopened=deduplicated",
            index + 1
        );
    }
    std::fs::remove_dir_all(project).unwrap();
}

#[tokio::test]
#[ignore = "requires LOOM_REAL_CHAT=1; real model file-tool probes in synthetic temporary projects"]
async fn grok_file_permissions_in_new_and_resumed_chat() {
    assert_eq!(std::env::var("LOOM_REAL_CHAT").as_deref(), Ok("1"));
    let ids = IdGenerator::default();
    let project = std::env::temp_dir().join(ids.next("loom-permission-canary"));
    std::fs::create_dir_all(&project).unwrap();
    let agent = AgentConfig {
        id: "grok-permission-canary".into(),
        name: "Grok canary".into(),
        command: "grok".into(),
        args: vec![],
        working_directory_policy: "project_root".into(),
        capabilities: vec![],
        adapter_type: ADAPTER_GROK.into(),
        can_write_files: true,
        can_run_commands: true,
        enabled: true,
        available: true,
    };
    println!("PERMISSION_CANARY project={}", project.display());
    for mode in ["explore", "ask"] {
        let session_id = ids.next("session");
        let path = project.join(format!("sentinel-{mode}.txt"));
        let original = ids.next("READ-MARKER");
        std::fs::write(&path, &original).unwrap();
        {
            let repo = ChatRepository::open(&project).unwrap();
            let session: ChatSession = serde_json::from_value(serde_json::json!({
                "id": session_id, "projectPath": repo.project_path(), "agentId": agent.id,
                "title": "File tool canary", "permissionMode": mode, "messages": [],
                "createdAtMs": now_ms(), "updatedAtMs": now_ms(), "turnStatus": "idle", "schemaVersion": 2
            })).unwrap();
            repo.create(&session).unwrap();
        }
        for round in 0..2 {
            let repo = ChatRepository::open(&project).unwrap();
            let changed = ids.next("WRITE-MARKER");
            let prompt = format!("Read sentinel-{mode}.txt using the file reading tool and report its exact contents. Then attempt to replace its entire contents with {changed} using the file editing tool. This is a disposable test file. Do not change permission modes, use shell commands, or modify any other file. If editing is unavailable or denied, report that; do not work around it.");
            let input = ChatSendInput {
                project_path: project.to_string_lossy().into_owned(),
                session_id: session_id.clone(),
                client_request_id: ids.next("request"),
                text: prompt.clone(),
                permission_mode: Some(mode.into()),
            };
            let assistant = ids.next("assistant");
            let turn = ids.next("turn");
            let accepted = repo
                .begin_once(&input, &turn, |session| {
                    let stage = super::permission_to_stage(mode);
                    let prepared = prepare_chat_invocation(&agent, session, &prompt, stage)?;
                    assert_eq!(prepared.invocation.resumed, round > 0);
                    for (id, role, content, status) in [
                        (ids.next("user"), "user", prompt.clone(), "complete"),
                        (assistant.clone(), "assistant", String::new(), "streaming"),
                    ] {
                        session.messages.push(ChatMessage {
                            id,
                            role: role.into(),
                            content,
                            status: status.into(),
                            created_at_ms: now_ms(),
                            error_summary: None,
                            parts: vec![],
                        });
                    }
                    session.turns.push(super::turns::new_turn(
                        &turn,
                        &input.client_request_id,
                        &session.messages[session.messages.len() - 2].id,
                        &assistant,
                        prepared.logged_invocation.clone(),
                    ));
                    Ok(prepared)
                })
                .unwrap();
            let BeginTurn::Started(_, prepared, lease) = accepted else {
                panic!()
            };
            let binding = prepared.resume_binding;
            let result = super::service::run_chat_turn(
                &repo,
                &lease,
                runtime::Request {
                    project_key: repo.project_path().to_string_lossy().into_owned(),
                    session_id: session_id.clone(),
                    turn_id: turn,
                    message_id: assistant.clone(),
                    prepared: prepared.invocation,
                    stdin_text: prepared.stdin_text,
                    limits: runtime::Limits {
                        timeout: std::time::Duration::from_secs(120),
                        ..runtime::Limits::default()
                    },
                },
                |_| {},
            )
            .await;
            let finished = finish_chat_turn(&repo, &lease, &assistant, Some(&binding), result);
            let session = repo.load(&session_id).unwrap();
            let answer = session.messages.last().unwrap();
            let actual = std::fs::read_to_string(&path).unwrap();
            println!("PERMISSION_CANARY mode={mode} round={} status={} native={} file_changed={} parts={:?}", round+1,
                finished.status, session.resume_handle.is_some(), actual.trim() != original,
                answer.parts.iter().filter_map(|p| match p { super::ChatMessagePart::Tool { name, .. } => Some(name), _ => None }).collect::<Vec<_>>());
            assert_eq!(finished.status, "complete", "{:?}", finished.error_summary);
            assert!(session.resume_handle.is_some());
            if mode == "explore" {
                assert_eq!(actual, original, "explore modified a project file");
                assert!(answer.content.contains(&original) || answer.parts.iter().any(|part| matches!(
                    part, super::ChatMessagePart::Tool { output_summary: Some(output), .. } if output.contains(&original)
                )), "the real CLI did not return the sentinel in text or a tool result");
            } else {
                assert_eq!(
                    actual.trim(),
                    changed,
                    "writable chat did not edit the sentinel"
                );
            }
        }
    }
    // Keep only this synthetic evidence directory for independent review.
}
