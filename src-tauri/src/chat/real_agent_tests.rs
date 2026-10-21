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
    three_turns("grok", ADAPTER_GROK, false).await;
}

#[tokio::test]
#[ignore = "requires LOOM_REAL_CHAT=1 and a configured Claude account"]
async fn claude_three_turns_resume_after_repository_reopen() {
    three_turns("claude", crate::agent_adapter::ADAPTER_CLAUDE, false).await;
}

#[tokio::test]
#[ignore = "requires LOOM_REAL_CHAT=1 and a configured Codex account"]
async fn codex_three_turns_resume_after_repository_reopen() {
    three_turns("codex", crate::agent_adapter::ADAPTER_CODEX, false).await;
}

#[tokio::test]
#[ignore = "requires LOOM_REAL_CHAT=1; real cross-Agent history replay"]
async fn claude_to_codex_history_replay() {
    three_turns("claude", crate::agent_adapter::ADAPTER_CLAUDE, true).await;
}
async fn three_turns(command: &str, adapter: &str, switch_agent: bool) {
    assert_eq!(
        std::env::var("LOOM_REAL_CHAT").as_deref(),
        Ok("1"),
        "real model calls require explicit opt-in"
    );
    let ids = IdGenerator::default();
    let project = std::env::temp_dir().join(ids.next("loom-real-chat"));
    std::fs::create_dir_all(&project).unwrap();
    let mut agent = AgentConfig {
        id: "real-grok-canary".into(),
        name: "Grok canary".into(),
        command: command.into(),
        args: Vec::new(),
        working_directory_policy: "project_root".into(),
        capabilities: vec!["planning".into()],
        adapter_type: adapter.into(),
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
        if switch_agent && index == 2 {
            agent.command = "codex".into();
            agent.adapter_type = crate::agent_adapter::ADAPTER_CODEX.into();
            agent.id = "switched-codex".into();
        }

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
                crate::chat_context::update_execution_config(session, Some(&agent.id), None)?;
                let transport =
                    prepare_chat_invocation(&agent, session, &prompt, AgentStage::Planning, false)?;
                assert_eq!(
                    transport.invocation.resumed,
                    index > 0 && !(switch_agent && index == 2)
                );
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
        assert_eq!(handle.adapter_type, agent.adapter_type);
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
    println!("REAL_CHAT evidence={}", project.display());
}

#[tokio::test]
#[ignore = "requires LOOM_REAL_CHAT=1; real model file-tool probes in synthetic temporary projects"]
async fn grok_file_permissions_in_new_and_resumed_chat() {
    file_permissions("grok", ADAPTER_GROK, false).await;
}
#[tokio::test]
#[ignore = "requires LOOM_REAL_CHAT=1; real Claude file tools"]
async fn claude_file_permissions_in_new_and_resumed_chat() {
    file_permissions("claude", crate::agent_adapter::ADAPTER_CLAUDE, false).await;
}
#[tokio::test]
#[ignore = "requires LOOM_REAL_CHAT=1; real Codex file tools"]
async fn codex_file_permissions_in_new_and_resumed_chat() {
    file_permissions("codex", crate::agent_adapter::ADAPTER_CODEX, false).await;
}
#[tokio::test]
#[ignore = "requires LOOM_REAL_CHAT=1; real Research to Code mode switch"]
async fn claude_mode_switch() {
    file_permissions("claude", crate::agent_adapter::ADAPTER_CLAUDE, true).await;
}
#[tokio::test]
#[ignore = "requires LOOM_REAL_CHAT=1; real Research to Code mode switch"]
async fn codex_mode_switch() {
    file_permissions("codex", crate::agent_adapter::ADAPTER_CODEX, true).await;
}
async fn file_permissions(command: &str, adapter: &str, switch_mode: bool) {
    assert_eq!(std::env::var("LOOM_REAL_CHAT").as_deref(), Ok("1"));
    let ids = IdGenerator::default();
    let project = std::env::temp_dir().join(ids.next("loom-permission-canary"));
    std::fs::create_dir_all(&project).unwrap();
    let agent = AgentConfig {
        id: "cli-permission-canary".into(),
        name: "CLI canary".into(),
        command: command.into(),
        args: vec![],
        working_directory_policy: "project_root".into(),
        capabilities: vec![],
        adapter_type: adapter.into(),
        can_write_files: true,
        can_run_commands: true,
        enabled: true,
        available: true,
    };
    println!("PERMISSION_CANARY project={}", project.display());
    for mode in ["explore", "ask"].into_iter().filter(|mode| {
        std::env::var("LOOM_REAL_MODES")
            .ok()
            .is_none_or(|requested| requested == *mode)
    }) {
        if switch_mode && mode == "ask" {
            continue;
        }
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
        let file_mode = mode;
        for round in 0..2 {
            let mode = if switch_mode && round == 1 {
                "ask"
            } else {
                file_mode
            };
            let repo = ChatRepository::open(&project).unwrap();
            let changed = ids.next("WRITE-MARKER");
            let reading = if command == "codex" {
                "Use a read-only cat command to read the file if no file-reading tool is available. Use apply_patch for the edit; never use a shell command to write."
            } else {
                "Use the file reading and editing tools. Do not use shell commands."
            };
            let prompt = if switch_mode && mode == "explore" {
                if command == "claude" {
                    format!("Discover your built-in Read tool with ToolSearch query select:Read, then Read sentinel-{file_mode}.txt. Reply only with its exact contents. Do not use Monitor, background tasks, MCP tools, or change files or permission modes.")
                } else {
                    format!("Read sentinel-{file_mode}.txt and report its exact contents. Use a read-only command if no file reading tool is available. Do not edit any files or change permission modes.")
                }
            } else {
                format!("Read sentinel-{file_mode}.txt and report its exact contents. Then attempt to replace its entire contents with {changed}. {reading} This is a disposable test file. Do not change permission modes or modify any other file. If editing is unavailable or denied, report that; do not work around it.")
            };
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
                    crate::chat_context::update_execution_config(session, None, Some(mode))?;
                    let stage = super::permission_to_stage(mode);
                    let prepared = prepare_chat_invocation(&agent, session, &prompt, stage, false)?;
                    assert_eq!(
                        prepared.invocation.resumed,
                        round > 0 && !(switch_mode && round == 1)
                    );
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
                if command == "codex" {
                    assert!(answer.parts.iter().any(|part| matches!(part, super::ChatMessagePart::Tool { metadata, .. } if metadata.diff_stats.as_ref().is_some_and(|stats| stats.added > 0 && stats.removed > 0) && metadata.diff_preview.as_ref().is_some_and(|preview| preview.contains(&changed)))), "Codex edit card is missing the actual workspace diff");
                }
            }
        }
    }
    // Keep only this synthetic evidence directory for independent review.
}

#[tokio::test]
#[ignore = "requires LOOM_REAL_CHAT=1; real Claude cancellation and next turn"]
async fn claude_stop_then_send() {
    stop_then_send("claude", crate::agent_adapter::ADAPTER_CLAUDE).await;
}
#[tokio::test]
#[ignore = "requires LOOM_REAL_CHAT=1; real Codex cancellation and next turn"]
async fn codex_stop_then_send() {
    stop_then_send("codex", crate::agent_adapter::ADAPTER_CODEX).await;
}

async fn stop_then_send(command: &str, adapter: &str) {
    assert_eq!(std::env::var("LOOM_REAL_CHAT").as_deref(), Ok("1"));
    let ids = IdGenerator::default();
    let project = std::env::temp_dir().join(ids.next("loom-stop-canary"));
    std::fs::create_dir_all(&project).unwrap();
    let repo = ChatRepository::open(&project).unwrap();
    let agent: AgentConfig = serde_json::from_value(serde_json::json!({
        "id":"stop-canary","name":"Stop canary","command":command,"args":[],"workingDirectoryPolicy":"project_root","capabilities":[],"adapterType":adapter,"canWriteFiles":false,"canRunCommands":false,"enabled":true,"available":true
    })).unwrap();
    let session_id = ids.next("session");
    let session: ChatSession = serde_json::from_value(serde_json::json!({
        "id":session_id,"projectPath":repo.project_path(),"agentId":agent.id,"title":"Stop canary","permissionMode":"explore","messages":[],"createdAtMs":now_ms(),"updatedAtMs":now_ms(),"turnStatus":"idle","schemaVersion":2
    })).unwrap();
    repo.create(&session).unwrap();
    for round in 0..2 {
        let prompt = if round == 0 {
            "Explain prime numbers in detail. Do not use tools."
        } else {
            "The previous generation was stopped deliberately. Reply with only STOP-RECOVERED. Do not use tools."
        };
        let input = ChatSendInput {
            project_path: project.to_string_lossy().into_owned(),
            session_id: session_id.clone(),
            client_request_id: ids.next("request"),
            text: prompt.into(),
            permission_mode: Some("explore".into()),
        };
        let turn_id = ids.next("turn");
        let assistant = ids.next("assistant");
        let accepted = repo
            .begin_once(&input, &turn_id, |session| {
                let prepared =
                    prepare_chat_invocation(&agent, session, prompt, AgentStage::Planning, false)?;
                let user = ids.next("user");
                for (id, role, content, status) in [
                    (user.clone(), "user", prompt.to_string(), "complete"),
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
                    &turn_id,
                    &input.client_request_id,
                    &user,
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
        let cancel_task = if round == 0 {
            let repository = repo.clone();
            let session_id = session_id.clone();
            let turn_id = turn_id.clone();
            Some(tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                let cancelled = repository
                    .cancel(&session_id, Some(&turn_id))
                    .unwrap()
                    .is_some();
                if cancelled {
                    crate::process_supervisor::supervisor()
                        .request_stop(&turn_id, super::STOP_REASON_ABORT)
                        .unwrap();
                }
                cancelled
            }))
        } else {
            None
        };
        let result = super::service::run_chat_turn(
            &repo,
            &lease,
            runtime::Request {
                project_key: repo.project_path().to_string_lossy().into_owned(),
                session_id: session_id.clone(),
                turn_id: turn_id.clone(),
                message_id: assistant.clone(),
                prepared: prepared.invocation,
                stdin_text: prepared.stdin_text,
                limits: runtime::Limits {
                    timeout: std::time::Duration::from_secs(120),
                    ..Default::default()
                },
            },
            |_| {},
        )
        .await;
        let cancelled = if let Some(task) = cancel_task {
            task.await.unwrap()
        } else {
            false
        };
        let finished = finish_chat_turn(&repo, &lease, &assistant, Some(&binding), result);
        let snapshot = repo.load(&session_id).unwrap();
        if round == 0 {
            assert!(cancelled);
            assert_eq!(
                snapshot.turns.last().unwrap().termination_reason.as_deref(),
                Some(super::STOP_REASON_ABORT)
            );
            assert_eq!(finished.status, "aborted");
        } else {
            assert_eq!(finished.status, "complete", "{:?}", finished.error_summary);
            assert_eq!(
                snapshot.messages.last().unwrap().content.trim(),
                "STOP-RECOVERED"
            );
        }
        println!(
            "STOP_CANARY agent={command} round={} status={} persisted_messages={}",
            round + 1,
            finished.status,
            snapshot.messages.len()
        );
    }
    println!("STOP_CANARY evidence={}", project.display());
}

#[tokio::test]
#[ignore = "requires LOOM_REAL_CHAT=1 and a working OpenCode CLI"]
async fn opencode_three_turns_resume_after_repository_reopen() {
    let command = std::env::var("LOOM_REAL_OPENCODE_COMMAND").unwrap_or_else(|_| "opencode".into());
    three_turns(&command, crate::agent_adapter::ADAPTER_OPENCODE, false).await;
}
#[tokio::test]
#[ignore = "requires LOOM_REAL_CHAT=1 and a working OpenCode CLI"]
async fn opencode_stop_then_send() {
    let command = std::env::var("LOOM_REAL_OPENCODE_COMMAND").unwrap_or_else(|_| "opencode".into());
    stop_then_send(&command, crate::agent_adapter::ADAPTER_OPENCODE).await;
}
#[tokio::test]
#[ignore = "requires LOOM_REAL_CHAT=1 and a working OpenCode CLI"]
async fn opencode_file_permissions_in_new_and_resumed_chat() {
    let command = std::env::var("LOOM_REAL_OPENCODE_COMMAND").unwrap_or_else(|_| "opencode".into());
    file_permissions(&command, crate::agent_adapter::ADAPTER_OPENCODE, false).await;
}

/// OpenUI inline cards acceptance (plan O5): fixed prompts through the real Loom path with the
/// experiment on. Each answer is written as JSON to `LOOM_REAL_CARDS_OUT`; card parsing happens
/// in `scripts/openui-acceptance.mjs` with the frontend parser. `LOOM_REAL_MODES` picks
/// `explore` (Research) or `ask` (Code); `LOOM_REAL_CARDS_ONLY` runs one case id.
#[tokio::test]
#[ignore = "requires LOOM_REAL_CHAT=1; real Claude inline card acceptance"]
async fn claude_inline_cards_acceptance() {
    assert_eq!(std::env::var("LOOM_REAL_CHAT").as_deref(), Ok("1"));
    let out = std::path::PathBuf::from(
        std::env::var("LOOM_REAL_CARDS_OUT").expect("LOOM_REAL_CARDS_OUT"),
    );
    let mode = std::env::var("LOOM_REAL_MODES").unwrap_or_else(|_| "explore".into());
    let only = std::env::var("LOOM_REAL_CARDS_ONLY").ok();
    std::fs::create_dir_all(&out).unwrap();
    // id, prompt, expects a card, resumes the q1 session
    let cases = [
        (
            "q01",
            "列出这个项目的入口文件，并说明每个文件的作用。",
            true,
            false,
        ),
        ("q02", "总结最近一次提交改了什么。", true, false),
        (
            "q03",
            "对比 src/add.js 和 src/sub.js 的实现，用表格说明差异。",
            true,
            false,
        ),
        ("q04", "给出这个项目发布前的检查清单。", true, false),
        ("q05", "哪些文件包含测试？分别测了什么？", true, false),
        (
            "q06",
            "package.json 里的每个 script 分别做什么？请对比说明。",
            true,
            false,
        ),
        ("q07", "总结工作区当前还没提交的改动。", true, false),
        ("q08", "add 函数返回什么类型？一句话回答。", false, false),
        ("q09", "用一两句话说明这个项目是做什么的。", false, false),
        ("q10", "把测试文件也加进刚才的入口文件列表里。", true, true),
    ];
    let ids = IdGenerator::default();
    let agent = AgentConfig {
        id: "claude-cards".into(),
        name: "Claude cards".into(),
        command: "claude".into(),
        args: vec![],
        working_directory_policy: "project_root".into(),
        capabilities: vec![],
        adapter_type: crate::agent_adapter::ADAPTER_CLAUDE.into(),
        can_write_files: true,
        can_run_commands: true,
        enabled: true,
        available: true,
    };
    for (id, text, expects_card, resumes) in cases {
        if only
            .as_deref()
            .is_some_and(|only| only != id && !(resumes && only == "q01"))
        {
            continue;
        }
        if resumes && only.as_deref() == Some("q10") {
            continue; // q10 always runs right after q01 in the same session
        }
        let project = if resumes {
            out.join("project-q01")
        } else {
            let project = out.join(format!("project-{id}"));
            card_fixture(&project);
            project
        };
        let git_before = git(&project, &["status", "--porcelain"]);
        let session_id = if resumes {
            std::fs::read_to_string(out.join("q01.session")).unwrap()
        } else {
            let session_id = ids.next("session");
            let repo = ChatRepository::open(&project).unwrap();
            let session: ChatSession = serde_json::from_value(serde_json::json!({
                "id": session_id, "projectPath": repo.project_path(), "agentId": agent.id,
                "title": id, "permissionMode": mode, "messages": [],
                "createdAtMs": now_ms(), "updatedAtMs": now_ms(), "turnStatus": "idle", "schemaVersion": 2
            }))
            .unwrap();
            repo.create(&session).unwrap();
            std::fs::write(out.join(format!("{id}.session")), &session_id).unwrap();
            session_id
        };
        let repo = ChatRepository::open(&project).unwrap();
        let input = ChatSendInput {
            project_path: project.to_string_lossy().into_owned(),
            session_id: session_id.clone(),
            client_request_id: ids.next("request"),
            text: text.into(),
            permission_mode: Some(mode.clone()),
        };
        let assistant = ids.next("assistant");
        let turn = ids.next("turn");
        let accepted = repo
            .begin_once(&input, &turn, |session| {
                let stage = super::permission_to_stage(&mode);
                let prepared = prepare_chat_invocation(&agent, session, text, stage, true)?;
                assert_eq!(prepared.invocation.resumed, resumes);
                assert!(prepared
                    .invocation
                    .args
                    .iter()
                    .any(|arg| arg == "--append-system-prompt"));
                for (id, role, content, status) in [
                    (ids.next("user"), "user", text.to_string(), "complete"),
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
        let started = std::time::Instant::now();
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
                    timeout: std::time::Duration::from_secs(600),
                    ..runtime::Limits::default()
                },
            },
            |_| {},
        )
        .await;
        let finished = finish_chat_turn(&repo, &lease, &assistant, Some(&binding), result);
        drop(lease);
        let session = repo.load(&session_id).unwrap();
        let answer = session.messages.last().unwrap();
        let logged = serde_json::to_string(&session.turns.last().unwrap().invocation).unwrap();
        let record = serde_json::json!({
            "id": id, "mode": mode, "prompt": text, "expectsCard": expects_card, "resumed": resumes,
            "status": finished.status, "error": finished.error_summary, "seconds": started.elapsed().as_secs(),
            "content": answer.content,
            "tools": answer.parts.iter().filter_map(|p| match p { super::ChatMessagePart::Tool { name, .. } => Some(name.clone()), _ => None }).collect::<Vec<_>>(),
            "projectWrites": git(&project, &["status", "--porcelain"]) != git_before,
            "promptRedacted": logged.contains("[OPENUI_PROMPT sha256:") && !logged.contains("## Inline Mode"),
            "nativeResume": session.resume_handle.is_some(),
        });
        println!("REAL_CARDS {}", record);
        std::fs::write(
            out.join(format!("{id}-{mode}.json")),
            serde_json::to_string_pretty(&record).unwrap(),
        )
        .unwrap();
    }
}

fn git(project: &std::path::Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(project)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// A small synthetic JS project with two commits and one uncommitted change.
fn card_fixture(project: &std::path::Path) {
    let _ = std::fs::remove_dir_all(project);
    std::fs::create_dir_all(project.join("src")).unwrap();
    std::fs::create_dir_all(project.join("test")).unwrap();
    let write = |path: &str, text: &str| std::fs::write(project.join(path), text).unwrap();
    write(
        "package.json",
        r#"{"name":"fixture-calc","version":"0.2.0","type":"module","main":"src/index.js","bin":{"calc":"src/cli.js"},"scripts":{"test":"node --test","lint":"node --check src/index.js","start":"node src/cli.js"}}"#,
    );
    write(
        "README.md",
        "# fixture-calc\n\nTiny calculator library with a CLI.\n",
    );
    write(
        "src/index.js",
        "export { add } from './add.js';\nexport { sub } from './sub.js';\n",
    );
    write(
        "src/add.js",
        "/** @returns {number} */\nexport function add(a, b) {\n  return a + b;\n}\n",
    );
    write("src/sub.js", "export const sub = (a, b) => a - b;\n");
    write("src/cli.js", "#!/usr/bin/env node\nimport { add } from './index.js';\nconst [a, b] = process.argv.slice(2).map(Number);\nconsole.log(add(a, b));\n");
    write("test/add.test.js", "import test from 'node:test';\nimport assert from 'node:assert';\nimport { add } from '../src/add.js';\ntest('add', () => assert.equal(add(1, 2), 3));\n");
    git(project, &["init", "-q"]);
    // Loom keeps chat history in `.loom/`; only changes outside it count as project writes.
    std::fs::write(project.join(".git/info/exclude"), ".loom/\n").unwrap();
    git(
        project,
        &[
            "-c",
            "user.name=fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "add",
            ".",
        ],
    );
    git(
        project,
        &[
            "-c",
            "user.name=fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "feat: add and sub",
        ],
    );
    write("test/sub.test.js", "import test from 'node:test';\nimport assert from 'node:assert';\nimport { sub } from '../src/sub.js';\ntest('sub', () => assert.equal(sub(3, 2), 1));\n");
    write("src/cli.js", "#!/usr/bin/env node\nimport { add, sub } from './index.js';\nconst [op, a, b] = process.argv.slice(2);\nconsole.log(op === 'sub' ? sub(+a, +b) : add(+a, +b));\n");
    git(
        project,
        &[
            "-c",
            "user.name=fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "add",
            ".",
        ],
    );
    git(
        project,
        &[
            "-c",
            "user.name=fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "feat(cli): support sub and test it",
        ],
    );
    write(
        "README.md",
        "# fixture-calc\n\nTiny calculator library with a CLI.\n\nUsage: `calc sub 3 2`\n",
    );
}
