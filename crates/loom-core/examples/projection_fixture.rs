use loom_core::*;
use serde_json::json;
fn id(value: &str) -> Id {
    Id::new(value).unwrap()
}
fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let root = std::env::temp_dir().join(format!(
        "loom-projection-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    std::fs::create_dir(&root)?;
    let result = (|| -> std::result::Result<serde_json::Value, Box<dyn std::error::Error>> {
        let core = Core::open(&root)?;
        core.create_workspace(id("workspace"), "Workspace".into())?;
        let key = SessionKey {
            workspace_id: id("workspace"),
            session_id: id("session"),
        };
        let initial = core.create_session(CreateSession {
            key: key.clone(),
            title: None,
            project_id: None,
            permission_mode: PermissionMode::Ask,
            connection: Default::default(),
        })?;
        let send = |request: &str| {
            core.send_message(SendMessage {
                key: key.clone(),
                client_request_id: id(request),
                text: "真实协议 中文🌍".into(),
                attachment_ids: vec![],
            })
        };
        let start = || {
            core.claim_next_turn(
                &key,
                RuntimeBinding {
                    adapter: "projection-fixture".into(),
                    connection: Default::default(),
                    permission_mode: PermissionMode::Ask,
                    policy_version: 1,
                    working_directory: None,
                    connection_snapshot_digest: None,
                    policy_snapshot_digest: None,
                },
            )
        };
        let first = send("first")?;
        let lease = start()?;
        core.apply_runtime(
            &lease,
            id("thinking"),
            RuntimeUpdate::TextDelta {
                part_id: id("thinking"),
                text_kind: TextKind::Thinking,
                delta: "thought".into(),
            },
        )?;
        core.apply_runtime(
            &lease,
            id("text1"),
            RuntimeUpdate::TextDelta {
                part_id: id("reply"),
                text_kind: TextKind::Text,
                delta: "第一段".into(),
            },
        )?;
        core.apply_runtime(
            &lease,
            id("tool"),
            RuntimeUpdate::ToolStarted {
                tool_call_id: id("__proto__"),
                name: "Read".into(),
                input: json!({"file":"readme"}),
            },
        )?;
        core.apply_runtime(
            &lease,
            id("tool-result"),
            RuntimeUpdate::ToolFinished {
                tool_call_id: id("__proto__"),
                status: ToolStatus::Succeeded,
                output: json!("content"),
            },
        )?;
        core.apply_runtime(
            &lease,
            id("text2"),
            RuntimeUpdate::TextDelta {
                part_id: id("reply"),
                text_kind: TextKind::Text,
                delta: "第二段".into(),
            },
        )?;
        let spec = ToolApprovalSpec {
            request_id: id("__proto__"),
            tool_call_id: id("approved-write"),
            tool_name: "Write".into(),
            input: json!({"file_path":"fixture.txt","content":"private fixture body"}),
            ttl_ms: 30_000,
            remember: Some(RememberOffer {
                keys: vec![RememberKey::new(
                    RememberKind::Directory,
                    "fixture-directory",
                    "测试目录写入".into(),
                )?],
            }),
        };
        let approval = core.request_tool_approval(&lease, &spec)?;
        core.respond_tool_approval_with_memory(
            &key,
            &spec.request_id,
            &approval.binding_digest,
            true,
            true,
        )?;
        let _permit = core.consume_tool_approval(&lease, &spec)?;
        core.apply_runtime(
            &lease,
            id("approved-result"),
            RuntimeUpdate::ToolFinished {
                tool_call_id: spec.tool_call_id,
                status: ToolStatus::Succeeded,
                output: json!({"ok":true}),
            },
        )?;
        for allowed in [true, false] {
            let name = if allowed {
                "automatic-allow"
            } else {
                "automatic-deny"
            };
            let spec = ToolApprovalSpec {
                request_id: id(name),
                tool_call_id: id(name),
                tool_name: "read".into(),
                input: json!({"path":"fixture"}),
                ttl_ms: 30_000,
                remember: None,
            };
            core.record_tool_policy(
                &lease,
                id(name),
                &spec,
                ToolPolicyDecision {
                    allowed,
                    reason: "fixture policy".into(),
                },
                &[],
            )?;
            if allowed {
                core.apply_runtime(
                    &lease,
                    id("automatic-result"),
                    RuntimeUpdate::ToolFinished {
                        tool_call_id: id(name),
                        status: ToolStatus::Succeeded,
                        output: json!("content"),
                    },
                )?;
            }
        }
        core.apply_runtime(
            &lease,
            id("usage"),
            RuntimeUpdate::Usage {
                usage: TokenUsage {
                    input_tokens: 100,
                    output_tokens: 20,
                },
            },
        )?;
        core.finish_turn(&lease, FinishStatus::Completed, None)?;
        drop(lease);
        core.mark_read(&key, first.receipt.assistant_message_id)?;
        let intermediate = core.get_session(&key)?;
        core.clear_remembered_grants(&key)?;
        core.patch_session(
            &key,
            intermediate.last_seq,
            SessionPatch {
                title: Change::Set("标题".into()),
                notes: Some("笔记".into()),
                flagged: Some(true),
                labels: Some(vec!["test".into()]),
                ..Default::default()
            },
        )?;
        let second = send("second")?;
        let lease = start()?;
        let waiting = ToolApprovalSpec {
            request_id: id("cancel-pending"),
            tool_call_id: id("pending-write"),
            tool_name: "Write".into(),
            input: json!({"path":"pending.txt"}),
            ttl_ms: 30_000,
            remember: None,
        };
        core.request_tool_approval(&lease, &waiting)?;
        let allowed = ToolApprovalSpec {
            request_id: id("cancel-allowed"),
            tool_call_id: id("allowed-write"),
            tool_name: "Write".into(),
            input: json!({"path":"allowed.txt"}),
            ttl_ms: 30_000,
            remember: None,
        };
        let approval = core.request_tool_approval(&lease, &allowed)?;
        core.respond_tool_approval(&key, &allowed.request_id, &approval.binding_digest, true)?;
        core.apply_runtime(
            &lease,
            id("text"),
            RuntimeUpdate::TextDelta {
                part_id: id("reply"),
                text_kind: TextKind::Text,
                delta: "partial".into(),
            },
        )?;
        core.cancel_turn(&key, &second.receipt.turn_id)?;
        core.finish_turn(&lease, FinishStatus::Cancelled, None)?;
        drop(lease);
        let third = send("third")?;
        core.cancel_turn(&key, &third.receipt.turn_id)?;
        let fourth = send("preparation-failure")?;
        core.fail_queued_turn(
            &key,
            &fourth.receipt.turn_id,
            "Fixture runtime is unavailable".into(),
        )?;
        let foreign_key = SessionKey {
            workspace_id: id("workspace"),
            session_id: id("foreign"),
        };
        core.create_session(CreateSession {
            key: foreign_key.clone(),
            title: None,
            project_id: None,
            permission_mode: PermissionMode::Ask,
            connection: Default::default(),
        })?;
        Ok(
            json!({"initial":initial,"intermediate":intermediate,"events":core.read_events(&key,initial.last_seq,500)?.events,"expected":core.get_session(&key)?,"foreign":core.read_events(&foreign_key,0,1)?.events[0]}),
        )
    })();
    let _ = std::fs::remove_dir_all(&root);
    println!("{}", result?);
    Ok(())
}
