//! Chat sessions: independent of the advanced Task state machine.
mod export;
mod journal;
mod logs;
mod models;
pub use models::*;
#[cfg(test)]
mod real_agent_tests;
mod repository;
pub(crate) mod resume;
mod runtime;
mod service;
pub(crate) mod shutdown;
pub(crate) mod turns;
use crate::agent_adapter::AgentStage;
use crate::agents::load_agents;
use crate::chat_context::{prepare_chat_invocation, update_execution_config};
use crate::models::{now_ms, IdGenerator};
use crate::process_supervisor;
use crate::tasks;
use repository::ChatRepository;
use service::{finish_chat_turn, require_idle};
use std::path::Path;
use tauri::{AppHandle, Emitter, State};

pub(crate) const CHAT_SCHEMA_VERSION: u32 = 2;

/// Wall-clock budget for one chat turn. On expiry the turn is stopped via
/// `ProcessSupervisor::request_stop(..., "chat_timeout")` so the session
/// becomes sendable again with an aborted message.
pub const CHAT_TURN_TIMEOUT_MS: u64 = 10 * 60 * 1000;

/// Title from first user text: first non-empty line, collapse whitespace,
/// truncate near `max_chars` on a word boundary (not a blind byte/char slice).
pub fn title_from_user_message(text: &str, max_chars: usize) -> String {
    let first_line = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_else(|| text.trim());
    let collapsed = first_line.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return "新对话".to_string();
    }
    let chars: Vec<char> = collapsed.chars().collect();
    if chars.len() <= max_chars {
        return collapsed;
    }
    let min_cut = max_chars / 2;
    let mut cut = max_chars;
    for i in (min_cut..=max_chars).rev() {
        if chars.get(i).map(|c| c.is_whitespace()).unwrap_or(false) {
            cut = i;
            break;
        }
    }
    let mut title: String = chars.into_iter().take(cut).collect();
    while title.ends_with(char::is_whitespace) {
        title.pop();
    }
    title.push('…');
    title
}

fn session_needs_attention(session: &ChatSession) -> bool {
    if session.status == "archived" {
        return false;
    }
    if session.flagged {
        return true;
    }
    if session.turn_status == "error" {
        return true;
    }
    session
        .messages
        .iter()
        .any(|message| message.status == "error")
}

/// Lazy repair after app restart: streaming turns become aborted, partial text kept.
fn reconcile_interrupted_session(session: &mut ChatSession) -> bool {
    let mut changed = false;
    for turn in &mut session.turns {
        if !turn.status.is_terminal() {
            turn.status = ChatTurnStatus::Interrupted;
            turn.finished_at_ms = Some(now_ms() as u64);
            turn.termination_reason = Some("interrupted_by_restart".into());
            turn.error_summary = Some("Loom exited before this turn completed".into());
            changed = true;
        }
    }
    if session.turn_status == "streaming" {
        session.turn_status = "idle".to_string();
        session.active_turn_id = None;
        changed = true;
    }
    for message in &mut session.messages {
        if message.status == "streaming" {
            message.status = "aborted".to_string();
            if message.content.trim().is_empty() {
                message.content = "（已中断）".to_string();
            }
            if message.error_summary.is_none() {
                message.error_summary = Some("interrupted_by_restart".to_string());
            }
            changed = true;
        }
    }
    if changed {
        // Surface in needs_attention without inventing Craft five-state.
        session.flagged = true;
        session.updated_at_ms = now_ms();
    }
    changed
}

fn permission_to_stage(mode: &str) -> AgentStage {
    // Phase 2:
    // - explore (+ legacy read_only) → Planning (conservative CLI)
    // - ask (+ legacy read_write→ask) → Debugging (writable CLI); UI requires
    //   per-turn confirm before send (Composer gate — not enforced here)
    // - auto → Debugging without UI confirm
    match normalize_permission_mode(mode).as_str() {
        "ask" | "auto" => AgentStage::Debugging,
        _ => AgentStage::Planning,
    }
}

/// Parse all tool blocks in a stdout frame; providers may batch parallel results.
#[derive(Debug, Default, PartialEq, Eq)]
struct ParsedChatLine {
    delta: String,
    parts: Vec<ChatMessagePart>,
}

fn parse_chat_stream_line(output_mode: &str, line: &str) -> ParsedChatLine {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return ParsedChatLine::default();
    }
    if output_mode == "plain" {
        return ParsedChatLine {
            delta: format!("{line}\n"),
            parts: Vec::new(),
        };
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) else {
        if !trimmed.starts_with('{') {
            return ParsedChatLine {
                delta: trimmed.to_string(),
                parts: Vec::new(),
            };
        }
        return ParsedChatLine::default();
    };
    let event = unwrap_stream_event(&value);
    let parts = if let Some(blocks) = event
        .pointer("/message/content")
        .or_else(|| event.get("content"))
        .and_then(|v| v.as_array())
    {
        blocks
            .iter()
            .filter_map(tool_part_from_content_item)
            .collect()
    } else {
        extract_tool_or_command_part(&value).into_iter().collect()
    };
    let delta = extract_assistant_text(output_mode, trimmed);
    ParsedChatLine { delta, parts }
}

fn json_event_type(value: &serde_json::Value) -> Option<String> {
    value
        .get("type")
        .and_then(|v| v.as_str())
        .or_else(|| {
            value
                .get("msg")
                .and_then(|msg| msg.get("type"))
                .and_then(|v| v.as_str())
        })
        .or_else(|| {
            value
                .get("event")
                .and_then(|event| event.get("type"))
                .and_then(|v| v.as_str())
        })
        .or_else(|| value.get("event").and_then(|v| v.as_str()))
        .map(str::to_string)
}

fn deep_str<'a>(value: &'a serde_json::Value, keys: &[&str]) -> Option<&'a str> {
    match value {
        serde_json::Value::Object(map) => {
            for key in keys {
                if let Some(text) = map.get(*key).and_then(|v| v.as_str()) {
                    return Some(text);
                }
            }
            map.values().find_map(|child| deep_str(child, keys))
        }
        serde_json::Value::Array(items) => items.iter().find_map(|child| deep_str(child, keys)),
        _ => None,
    }
}

fn summarize_json(value: &serde_json::Value, max_chars: usize) -> Option<String> {
    let raw = match value {
        serde_json::Value::String(text) => text.clone(),
        other => serde_json::to_string(other).ok()?,
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.chars().count() <= max_chars {
        Some(trimmed.to_string())
    } else {
        Some(format!(
            "{}…",
            trimmed.chars().take(max_chars).collect::<String>()
        ))
    }
}

fn unwrap_stream_event(value: &serde_json::Value) -> &serde_json::Value {
    // Claude Code `--output-format stream-json --include-partial-messages` wraps
    // content_block_* payloads as `{ "type":"stream_event", "event": { ... } }`.
    if json_event_type(value).as_deref() == Some("stream_event") {
        value.get("event").unwrap_or(value)
    } else {
        value
    }
}

fn tool_part_from_content_item(item: &serde_json::Value) -> Option<ChatMessagePart> {
    let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
    if item_type == "tool_use" || item_type == "tool_call" || item_type == "function_call" {
        let name = item
            .get("name")
            .or_else(|| item.pointer("/function/name"))
            .and_then(|v| v.as_str())
            .unwrap_or("tool")
            .to_string();
        let input = item
            .get("input")
            .or_else(|| item.get("arguments"))
            .or_else(|| item.pointer("/function/arguments"));
        return Some(ChatMessagePart::Tool {
            name,
            input_summary: input.and_then(|v| summarize_json(v, 240)),
            output_summary: None,
            status: Some("running".to_string()),
        });
    }
    if item_type == "tool_result" {
        let name = item
            .get("name")
            .or_else(|| item.get("tool_use_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("tool_result")
            .to_string();
        let output = item
            .get("content")
            .or_else(|| item.get("output"))
            .or_else(|| item.get("result"));
        return Some(ChatMessagePart::Tool {
            name,
            input_summary: None,
            output_summary: output.and_then(|v| summarize_json(v, 240)),
            status: Some(
                if item.get("is_error").and_then(|v| v.as_bool()) == Some(true) {
                    "error"
                } else {
                    "done"
                }
                .to_string(),
            ),
        });
    }
    None
}

fn extract_tool_or_command_part(value: &serde_json::Value) -> Option<ChatMessagePart> {
    let value = unwrap_stream_event(value);
    let event = json_event_type(value).unwrap_or_default();
    let event_l = event.to_ascii_lowercase();

    // Nested content blocks (Claude / Grok streaming-json style).
    if let Some(parts) = value
        .pointer("/message/content")
        .or_else(|| value.pointer("/content"))
        .and_then(|v| v.as_array())
    {
        for item in parts {
            if let Some(part) = tool_part_from_content_item(item) {
                return Some(part);
            }
        }
    }

    // Claude content_block_start / content_block_stop (top-level or under stream_event).
    if let Some(block) = value
        .get("content_block")
        .or_else(|| value.pointer("/event/content_block"))
    {
        if let Some(part) = tool_part_from_content_item(block) {
            return Some(part);
        }
        let block_type = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if block_type == "tool_use" || block_type == "tool_call" {
            let name = block
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("tool")
                .to_string();
            return Some(ChatMessagePart::Tool {
                name,
                input_summary: block.get("input").and_then(|v| summarize_json(v, 240)),
                output_summary: None,
                status: Some("running".to_string()),
            });
        }
    }

    if event_l.contains("tool_result")
        || event_l.contains("tool_use")
        || event_l.contains("tool_call")
        || event_l.contains("function_call")
        || event_l.contains("mcp_tool")
    {
        let name = deep_str(value, &["name", "tool_name", "toolName", "function_name"])
            .unwrap_or("tool")
            .to_string();
        let input = value
            .get("input")
            .or_else(|| value.get("arguments"))
            .or_else(|| value.get("params"));
        let output = value
            .get("output")
            .or_else(|| value.get("result"))
            .or_else(|| value.get("content"));
        let status = if event_l.contains("result") {
            Some("done".to_string())
        } else {
            Some("running".to_string())
        };
        return Some(ChatMessagePart::Tool {
            name,
            input_summary: input.and_then(|v| summarize_json(v, 240)),
            output_summary: output.and_then(|v| summarize_json(v, 240)),
            status,
        });
    }

    let nested_item_type = value
        .pointer("/item/type")
        .or_else(|| value.pointer("/msg/type"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let commandish = event_l.contains("exec_command")
        || event_l.contains("command_execution")
        || event_l.contains("shell")
        || nested_item_type.contains("command_execution")
        || nested_item_type.contains("exec_command")
        || (event_l.contains("command") && !event_l.contains("permission"));
    if commandish {
        let command = deep_str(value, &["command", "cmd", "shell_command"])
            .unwrap_or("command")
            .to_string();
        let output = deep_str(value, &["output", "stdout", "text"]);
        return Some(ChatMessagePart::Tool {
            name: "command".to_string(),
            input_summary: Some(command),
            output_summary: output.map(|text| {
                if text.chars().count() > 240 {
                    format!("{}…", text.chars().take(240).collect::<String>())
                } else {
                    text.to_string()
                }
            }),
            status: Some(
                if event_l.contains("end")
                    || event_l.contains("completed")
                    || nested_item_type.contains("completed")
                {
                    "done".to_string()
                } else {
                    "running".to_string()
                },
            ),
        });
    }

    None
}

fn extract_assistant_text(output_mode: &str, raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if output_mode == "plain" {
        return trimmed.to_string();
    }
    // streaming_json / claude_stream_json / codex_json share best-effort text extraction
    // Best-effort: collect text-ish fields from JSONL / JSON event streams.
    let mut chunks = Vec::new();
    for line in trimmed.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
            let value = unwrap_stream_event(&value);
            // Codex exec emits assistant text inside an agent_message item.
            if value.pointer("/item/type").and_then(|v| v.as_str()) == Some("agent_message") {
                if let Some(text) = value.pointer("/item/text").and_then(|v| v.as_str()) {
                    chunks.push(text.to_string());
                    continue;
                }
            }
            // Claude assistant message: collect every text block in content[].
            if let Some(parts) = value
                .pointer("/message/content")
                .or_else(|| value.pointer("/content"))
                .and_then(|v| v.as_array())
            {
                let mut block_text = Vec::new();
                for item in parts {
                    if item.get("type").and_then(|v| v.as_str()) == Some("text") {
                        if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                            if !text.is_empty() {
                                block_text.push(text.to_string());
                            }
                        }
                    }
                }
                if !block_text.is_empty() {
                    chunks.extend(block_text);
                    continue;
                }
            }
            if let Some(text) = value
                .pointer("/msg/text")
                .or_else(|| value.pointer("/message/content/0/text"))
                .or_else(|| value.get("text"))
                .and_then(|v| v.as_str())
            {
                chunks.push(text.to_string());
                continue;
            }
            // Claude content_block_delta / stream_event text deltas.
            if let Some(delta) = value
                .pointer("/delta/text")
                .or_else(|| value.pointer("/delta"))
                .or_else(|| value.pointer("/message/delta"))
                .and_then(|v| v.as_str())
            {
                chunks.push(delta.to_string());
                continue;
            }
        }
        // Fallback: keep non-JSON lines
        if !line.starts_with('{') {
            chunks.push(line.to_string());
        }
    }
    chunks.join("")
}

fn open_chat_repository(app: &AppHandle, project_path: &str) -> Result<ChatRepository, String> {
    let project = crate::projects::canonical_project_path(project_path)?;
    let registered = crate::storage::load_recent_projects(app)?;
    if !registered
        .iter()
        .any(|item| Path::new(&item.path) == project)
        && !ChatRepository::owns_active_project(&project)
    {
        return Err("open this project in Loom before accessing its chats".into());
    }
    ChatRepository::open(&project)
}

fn publish_chat_session(
    app: &AppHandle,
    repository: &ChatRepository,
    session: ChatSession,
) -> Result<ChatSession, String> {
    let event = repository.event_at(&session)?;
    let _ = app.emit("loom://chat-event", event);
    Ok(session)
}

#[tauri::command]
pub async fn chat_export(
    app: AppHandle,
    ids: State<'_, IdGenerator>,
    input: ChatExportInput,
) -> Result<ChatExportResult, String> {
    let operation = process_supervisor::supervisor().begin_operation()?;
    let repository = open_chat_repository(&app, &input.project_path)?;
    let export_id = ids.next("export");
    tauri::async_runtime::spawn_blocking(move || {
        let _operation = operation;
        repository.export_session(&input.session_id, &export_id)
    })
    .await
    .map_err(|error| format!("chat export worker failed: {error}"))?
}

#[tauri::command]
pub fn chat_read_events(
    app: AppHandle,
    project_path: String,
    session_id: String,
    after_seq: u64,
    limit: Option<usize>,
) -> Result<ChatEventPage, String> {
    open_chat_repository(&app, &project_path)?.read_events(
        &session_id,
        after_seq,
        limit.unwrap_or(200),
    )
}

#[tauri::command]
pub fn chat_read_run_logs(
    app: AppHandle,
    project_path: String,
    session_id: String,
    turn_id: String,
    stream: ChatLogStream,
    offset: u64,
    limit: Option<usize>,
) -> Result<ChatLogPage, String> {
    open_chat_repository(&app, &project_path)?.read_run_logs(
        &session_id,
        &turn_id,
        stream,
        offset,
        limit.unwrap_or(32_768),
    )
}

#[tauri::command]
pub fn chat_list_sessions(
    app: AppHandle,
    project_path: String,
) -> Result<Vec<ChatSessionSummary>, String> {
    let repository = open_chat_repository(&app, &project_path)?;
    let mut summaries = Vec::new();
    for (id, entry) in repository.list_entries()? {
        let session = match entry {
            Ok(session) => session,
            Err(error) => {
                summaries.push(ChatSessionSummary {
                    id,
                    title: "无法读取的会话".into(),
                    agent_id: String::new(),
                    updated_at_ms: 0,
                    preview: Some(error.clone()),
                    status: Some("active".into()),
                    needs_attention: Some(true),
                    flagged: None,
                    storage_error: Some(error),
                });
                continue;
            }
        };
        let preview = session
            .messages
            .last()
            .map(|message| message.content.chars().take(80).collect::<String>());
        let needs_attention = session_needs_attention(&session);
        summaries.push(ChatSessionSummary {
            id: session.id,
            title: session.title,
            agent_id: session.agent_id,
            updated_at_ms: session.updated_at_ms,
            preview,
            status: Some(session.status),
            needs_attention: Some(needs_attention),
            flagged: Some(session.flagged),
            storage_error: None,
        });
    }
    summaries.sort_by_key(|a| std::cmp::Reverse(a.updated_at_ms));
    Ok(summaries)
}

#[tauri::command]
pub fn chat_create(
    app: AppHandle,
    ids: State<'_, IdGenerator>,
    input: ChatCreateInput,
) -> Result<ChatSession, String> {
    let _operation = process_supervisor::supervisor().begin_operation()?;
    let repository = open_chat_repository(&app, &input.project_path)?;
    let created_at_ms = now_ms();
    let session = ChatSession {
        id: ids.next("chat"),
        project_path: repository.project_path().to_string_lossy().into_owned(),
        agent_id: input.agent_id,
        title: input
            .title
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "新对话".to_string()),
        permission_mode: normalize_permission_mode(
            &input
                .permission_mode
                .unwrap_or_else(|| DEFAULT_PERMISSION_MODE.to_string()),
        ),
        status: DEFAULT_SESSION_STATUS.to_string(),
        messages: Vec::new(),
        send_receipts: Vec::new(),
        turns: Vec::new(),
        created_at_ms,
        updated_at_ms: created_at_ms,
        resume_command: None,
        resume_handle: None,
        active_turn_id: None,
        turn_status: "idle".to_string(),
        promoted_task_id: None,
        flagged: false,
        schema_version: CHAT_SCHEMA_VERSION,
        last_seq: 0,
        revision: 0,
    };
    repository.create(&session)?;
    publish_chat_session(&app, &repository, repository.load(&session.id)?)
}

#[tauri::command]
pub fn chat_get(
    app: AppHandle,
    project_path: String,
    session_id: String,
) -> Result<ChatSession, String> {
    open_chat_repository(&app, &project_path)?.load(&session_id)
}

#[tauri::command]
pub fn chat_set_agent(
    app: AppHandle,
    project_path: String,
    session_id: String,
    agent_id: String,
) -> Result<ChatSession, String> {
    let _operation = process_supervisor::supervisor().begin_operation()?;
    let repository = open_chat_repository(&app, &project_path)?;
    let session = repository.update(&session_id, |session| {
        update_execution_config(session, Some(&agent_id), None)
    })?;
    publish_chat_session(&app, &repository, session)
}

#[tauri::command]
pub fn chat_update_meta(app: AppHandle, input: ChatUpdateMetaInput) -> Result<ChatSession, String> {
    let _operation = process_supervisor::supervisor().begin_operation()?;
    let repository = open_chat_repository(&app, &input.project_path)?;
    let session = repository.update(&input.session_id, |session| {
        if let Some(title) = input.title {
            let trimmed = title.trim();
            if !trimmed.is_empty() {
                session.title = trimmed.to_string();
            }
        }
        if input.title_from_first_message.unwrap_or(false) {
            if let Some(first_user) = session
                .messages
                .iter()
                .find(|message| message.role == "user")
            {
                session.title = title_from_user_message(&first_user.content, 48);
            }
        }
        if let Some(mode) = input.permission_mode {
            update_execution_config(session, None, Some(&mode))?;
        }
        if let Some(status) = input.status {
            require_idle(session)?;
            session.status = match status.as_str() {
                "archived" => "archived".to_string(),
                _ => DEFAULT_SESSION_STATUS.to_string(),
            };
        }
        if let Some(flagged) = input.flagged {
            session.flagged = flagged;
        }
        Ok(())
    })?;
    publish_chat_session(&app, &repository, session)
}

fn promote_requirement_from_session(session: &ChatSession) -> (String, String) {
    let latest_user = session
        .messages
        .iter()
        .rev()
        .find(|message| message.role == "user")
        .map(|message| message.content.as_str())
        .unwrap_or(session.title.as_str());
    let title = latest_user.chars().take(48).collect::<String>();

    let mut parts = Vec::new();
    parts.push(format!(
        "Promoted from Loom chat session `{}` (agent `{}`, permission `{}`).",
        session.id, session.agent_id, session.permission_mode
    ));
    parts.push(String::new());
    parts.push("## Latest user ask".to_string());
    parts.push(latest_user.to_string());
    parts.push(String::new());
    parts.push("## Recent conversation".to_string());

    let recent: Vec<_> = session
        .messages
        .iter()
        .rev()
        .take(20)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    if recent.is_empty() {
        parts.push("(no messages yet)".to_string());
    } else {
        for message in recent {
            let body = message.content.chars().take(2000).collect::<String>();
            parts.push(format!("{}: {}", message.role, body));
        }
    }

    let mut requirement = parts.join("\n");
    const MAX_CHARS: usize = 12_000;
    if requirement.chars().count() > MAX_CHARS {
        requirement = requirement.chars().take(MAX_CHARS).collect::<String>();
        requirement.push_str("\n\n…(truncated)");
    }
    (title, requirement)
}

#[tauri::command]
pub fn chat_promote_to_task(
    app: AppHandle,
    ids: State<'_, IdGenerator>,
    input: ChatPromoteInput,
) -> Result<ChatPromoteResult, String> {
    let _operation = process_supervisor::supervisor().begin_operation()?;
    let repository = open_chat_repository(&app, &input.project_path)?;
    let mut promoted_task = None;
    let session = repository.update(&input.session_id, |session| {
        require_idle(session)?;
        let (title, raw_requirement) = promote_requirement_from_session(session);
        let task = tasks::create_task(
            app.clone(),
            ids,
            crate::models::CreateTaskInput {
                project_path: repository.project_path().to_string_lossy().into_owned(),
                title,
                raw_requirement,
                selected_planning_agent_ids: Vec::new(),
                primary_agent_id: Some(session.agent_id.clone()),
            },
        )?;
        session.promoted_task_id = Some(task.id.clone());
        promoted_task = Some(task);
        Ok(())
    })?;
    let task = promoted_task.ok_or("failed to create task")?;
    let session = publish_chat_session(&app, &repository, session)?;
    Ok(ChatPromoteResult {
        task_id: task.id.clone(),
        task,
        session,
    })
}

#[tauri::command]
pub fn chat_clear_resume(
    app: AppHandle,
    project_path: String,
    session_id: String,
) -> Result<ChatSession, String> {
    let _operation = process_supervisor::supervisor().begin_operation()?;
    let repository = open_chat_repository(&app, &project_path)?;
    let session = repository.update(&session_id, |session| {
        require_idle(session)?;
        session.resume_command = None;
        session.resume_handle = None;
        Ok(())
    })?;
    publish_chat_session(&app, &repository, session)
}

const STOP_REASON_ABORT: &str = "chat_abort";
const STOP_REASON_TIMEOUT: &str = "chat_timeout";
const STOP_REASON_SHUTDOWN: &str = "app_shutdown";

/// True when ProcessSupervisor recorded a cooperative stop (user abort or turn timeout).
pub fn is_chat_stop_reason(reason: Option<&str>) -> bool {
    matches!(
        reason,
        Some(STOP_REASON_ABORT) | Some(STOP_REASON_TIMEOUT) | Some(STOP_REASON_SHUTDOWN)
    )
}

pub fn chat_timeout_summary() -> String {
    let minutes = CHAT_TURN_TIMEOUT_MS / 60_000;
    format!("回合超时（超过 {minutes} 分钟，已自动停止）")
}

fn chat_stop_outcome(
    reason: Option<&str>,
    content: String,
    mut parts: Vec<ChatMessagePart>,
) -> ChatTurnOutcome {
    let timed_out = reason == Some(STOP_REASON_TIMEOUT);
    let error_summary = if timed_out {
        Some(chat_timeout_summary())
    } else {
        None
    };
    if let Some(summary) = error_summary.as_ref() {
        parts.push(ChatMessagePart::Error {
            message: summary.clone(),
            code: Some("chat_timeout".to_string()),
        });
    }
    let content = if content.trim().is_empty() {
        if timed_out {
            format!("（已超时）{}", error_summary.as_deref().unwrap_or(""))
        } else {
            "（已停止）".to_string()
        }
    } else {
        content
    };
    ChatTurnOutcome {
        content,
        parts,
        resume_command: None,
        status: "aborted".to_string(),
        error_summary,
        exit_code: None,
        termination_reason: Some(reason.unwrap_or(STOP_REASON_ABORT).into()),
    }
}

#[tauri::command]
pub async fn chat_abort(app: AppHandle, input: ChatAbortInput) -> Result<(), String> {
    let repository = open_chat_repository(&app, &input.project_path)?;
    if let Some(turn_id) = repository.cancel(&input.session_id, Some(&input.turn_id))? {
        // Starting turns remember cancellation in the lease, before a PID exists.
        process_supervisor::supervisor().request_stop(&turn_id, STOP_REASON_ABORT)?;
        if let Ok(session) = repository.load(&input.session_id) {
            let _ = publish_chat_session(&app, &repository, session);
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn chat_send(
    app: AppHandle,
    ids: State<'_, IdGenerator>,
    input: ChatSendInput,
) -> Result<ChatSendResult, String> {
    let operation = process_supervisor::supervisor().begin_operation()?;
    let repository = open_chat_repository(&app, &input.project_path)?;
    let turn_id = ids.next("turn");
    let assistant_id = ids.next("msg");
    let accepted = repository.begin_once(&input, &turn_id, |session| {
        let text = &input.text;
        if let Some(mode) = &input.permission_mode {
            update_execution_config(session, None, Some(mode))?;
        }
        let agents = load_agents(&app)?;
        let agent = agents
            .iter()
            .find(|agent| agent.id == session.agent_id)
            .ok_or_else(|| format!("agent '{}' was not found", session.agent_id))?;
        let stage = permission_to_stage(&session.permission_mode);
        let prepared = prepare_chat_invocation(agent, session, text, stage)?;
        if session.messages.is_empty() && session.title == "新对话" {
            session.title = title_from_user_message(text, 48);
        }
        let now = now_ms();
        session.messages.push(ChatMessage {
            id: ids.next("msg"),
            role: "user".into(),
            content: text.clone(),
            status: "complete".into(),
            created_at_ms: now,
            error_summary: None,
            parts: Vec::new(),
        });
        session.messages.push(ChatMessage {
            id: assistant_id.clone(),
            role: "assistant".into(),
            content: String::new(),
            status: "streaming".into(),
            created_at_ms: now + 1,
            error_summary: None,
            parts: Vec::new(),
        });
        let user_id = session.messages[session.messages.len() - 2].id.clone();
        session.turns.push(turns::new_turn(
            &turn_id,
            &input.client_request_id,
            &user_id,
            &assistant_id,
            prepared.logged_invocation.clone(),
        ));
        Ok(prepared)
    })?;
    let (session, prepared, lease) = match accepted {
        repository::BeginTurn::Started(session, prepared, lease) => (session, prepared, lease),
        repository::BeginTurn::Existing(result) => return Ok(result),
    };
    // Acceptance is already durable. A notification/read failure must not
    // strand it between persistence and spawn; consumers can replay the log.
    if let Ok(event) = repository.event_at(&session) {
        let _ = app.emit("loom://chat-event", event);
    }
    tauri::async_runtime::spawn(async move {
        let _operation = operation;
        let resume_binding = prepared.resume_binding;
        let request = runtime::Request {
            project_key: repository.project_path().to_string_lossy().into_owned(),
            session_id: lease.session_id.clone(),
            turn_id: lease.turn_id.clone(),
            message_id: assistant_id.clone(),
            prepared: prepared.invocation,
            stdin_text: prepared.stdin_text,
            limits: runtime::Limits::default(),
        };
        let result = service::run_chat_turn(&repository, &lease, request, |event| {
            let _ = app.emit("loom://chat-event", event);
        })
        .await;
        let event = finish_chat_turn(
            &repository,
            &lease,
            &assistant_id,
            Some(&resume_binding),
            result,
        );
        if let Some(persisted) = event.journal_event.clone() {
            let _ = app.emit("loom://chat-event", persisted);
        } else {
            // Persistence failed; this is explicitly not a durable completion.
            let _ = app.emit("loom://chat-error", event);
        }
    });
    Ok(ChatSendResult { turn_id, session })
}

#[derive(Debug, Default)]
struct ChatTurnOutcome {
    content: String,
    parts: Vec<ChatMessagePart>,
    resume_command: Option<String>,
    status: String,
    error_summary: Option<String>,
    exit_code: Option<i32>,
    termination_reason: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{
        extract_tool_or_command_part, normalize_permission_mode, parse_chat_stream_line,
        permission_to_stage, ChatMessagePart,
    };
    // chat_stop_outcome / is_chat_stop_reason / chat_timeout_summary tested via super::
    use crate::agent_adapter::{
        prepare_invocation, AdapterInvocationRequest, AgentStage, ADAPTER_CLAUDE, ADAPTER_CODEX,
        ADAPTER_GROK,
    };
    use crate::models::AgentConfig;
    use std::path::Path;

    fn sample_agent(adapter_type: &str, command: &str) -> AgentConfig {
        AgentConfig {
            id: "agent-test".to_string(),
            name: "Test".to_string(),
            command: command.to_string(),
            args: Vec::new(),
            working_directory_policy: "project_root".to_string(),
            capabilities: vec![
                "planning".to_string(),
                "implementation".to_string(),
                "review".to_string(),
                "debugging".to_string(),
            ],
            adapter_type: adapter_type.to_string(),
            can_write_files: true,
            can_run_commands: true,
            enabled: true,
            available: true,
        }
    }

    fn arg_pair<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
        args.windows(2)
            .find(|pair| pair[0] == flag)
            .map(|pair| pair[1].as_str())
    }

    #[test]
    fn normalizes_legacy_permission_modes() {
        assert_eq!(normalize_permission_mode("read_only"), "explore");
        assert_eq!(normalize_permission_mode("read_write"), "ask");
        assert_eq!(normalize_permission_mode("explore"), "explore");
        assert_eq!(normalize_permission_mode("ask"), "ask");
        assert_eq!(normalize_permission_mode("auto"), "auto");
        assert_eq!(normalize_permission_mode("nope"), "explore");
    }

    #[test]
    fn permission_modes_map_to_conservative_or_write_stages() {
        assert_eq!(permission_to_stage("explore"), AgentStage::Planning);
        assert_eq!(permission_to_stage("ask"), AgentStage::Debugging);
        assert_eq!(permission_to_stage("read_only"), AgentStage::Planning);
        assert_eq!(permission_to_stage("read_write"), AgentStage::Debugging);
        assert_eq!(permission_to_stage("auto"), AgentStage::Debugging);
    }

    #[test]
    fn permission_tiers_drive_prepare_invocation_cli_flags() {
        let root = Path::new("/repo");
        let cases = [
            ("explore", AgentStage::Planning),
            ("ask", AgentStage::Debugging),
            ("auto", AgentStage::Debugging),
        ];

        for (mode, expected_stage) in cases {
            let stage = permission_to_stage(mode);
            assert_eq!(stage, expected_stage, "mode {mode}");

            let grok = prepare_invocation(
                &sample_agent(ADAPTER_GROK, "grok"),
                &AdapterInvocationRequest {
                    project_path: root,
                    prompt: "hello",
                    prompt_file: None,
                    stage,
                    resume_command: None,
                    chat_permission_mode: Some(mode),
                    embed_prompt: true,
                },
            )
            .expect("grok");
            let expected_perm = if mode == "explore" {
                "plan"
            } else {
                "acceptEdits"
            };
            assert_eq!(
                arg_pair(&grok.args, "--permission-mode"),
                Some(expected_perm),
                "grok {mode}"
            );

            let codex = prepare_invocation(
                &sample_agent(ADAPTER_CODEX, "codex"),
                &AdapterInvocationRequest {
                    project_path: root,
                    prompt: "hello",
                    prompt_file: None,
                    stage,
                    resume_command: None,
                    chat_permission_mode: Some(mode),
                    embed_prompt: true,
                },
            )
            .expect("codex");
            let expected_sandbox = if mode == "explore" {
                "read-only"
            } else {
                "workspace-write"
            };
            assert_eq!(
                arg_pair(&codex.args, "--sandbox"),
                Some(expected_sandbox),
                "codex {mode}"
            );

            let claude = prepare_invocation(
                &sample_agent(ADAPTER_CLAUDE, "claude"),
                &AdapterInvocationRequest {
                    project_path: root,
                    prompt: "hello",
                    prompt_file: None,
                    stage,
                    resume_command: None,
                    chat_permission_mode: Some(mode),
                    embed_prompt: true,
                },
            )
            .expect("claude");
            if mode == "explore" {
                assert!(
                    arg_pair(&claude.args, "--permission-mode") == Some("plan"),
                    "claude {mode} must explicitly select plan mode"
                );
            } else {
                assert_eq!(
                    arg_pair(&claude.args, "--permission-mode"),
                    Some("acceptEdits"),
                    "claude {mode}"
                );
            }
        }
    }

    #[test]
    fn parses_grok_style_tool_use_part() {
        let line = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash","input":{"command":"ls -la"}}]}}"#;
        let parsed = parse_chat_stream_line("streaming_json", line);
        match parsed.parts.into_iter().next() {
            Some(ChatMessagePart::Tool {
                name,
                input_summary,
                status,
                ..
            }) => {
                assert_eq!(name, "Bash");
                assert!(input_summary.unwrap_or_default().contains("ls -la"));
                assert_eq!(status.as_deref(), Some("running"));
            }
            other => panic!("expected tool part, got {other:?}"),
        }
    }

    #[test]
    fn parses_codex_style_command_part() {
        let line = r#"{"type":"item.completed","item":{"type":"command_execution","command":"pwd","output":"/repo"}}"#;
        let value = serde_json::from_str(line).unwrap();
        let part = extract_tool_or_command_part(&value).expect("command part");
        match part {
            ChatMessagePart::Tool {
                name,
                input_summary,
                output_summary,
                ..
            } => {
                assert_eq!(name, "command");
                assert_eq!(input_summary.as_deref(), Some("pwd"));
                assert_eq!(output_summary.as_deref(), Some("/repo"));
            }
            other => panic!("expected tool/command part, got {other:?}"),
        }
    }

    #[test]
    fn parse_falls_back_to_text_for_non_json() {
        let parsed = parse_chat_stream_line("streaming_json", "hello plain");
        assert_eq!(parsed.delta, "hello plain");
        assert!(parsed.parts.is_empty());
    }

    #[test]
    fn parses_claude_assistant_tool_use_part() {
        let line = r#"{"type":"assistant","message":{"id":"msg_1","role":"assistant","content":[{"type":"text","text":"Looking"},{"type":"tool_use","id":"toolu_1","name":"Read","input":{"path":"src/main.rs"}}]}}"#;
        let parsed = parse_chat_stream_line("claude_stream_json", line);
        assert!(parsed.delta.contains("Looking"), "delta={}", parsed.delta);
        match parsed.parts.into_iter().next() {
            Some(ChatMessagePart::Tool {
                name,
                input_summary,
                status,
                ..
            }) => {
                assert_eq!(name, "Read");
                assert!(input_summary.unwrap_or_default().contains("main.rs"));
                assert_eq!(status.as_deref(), Some("running"));
            }
            other => panic!("expected tool part, got {other:?}"),
        }
    }

    #[test]
    fn parses_claude_stream_event_content_block_tool_use() {
        let line = r#"{"type":"stream_event","event":{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"toolu_2","name":"Bash","input":{"command":"ls -la"}}}}"#;
        let parsed = parse_chat_stream_line("claude_stream_json", line);
        match parsed.parts.into_iter().next() {
            Some(ChatMessagePart::Tool {
                name,
                input_summary,
                status,
                ..
            }) => {
                assert_eq!(name, "Bash");
                assert!(input_summary.unwrap_or_default().contains("ls -la"));
                assert_eq!(status.as_deref(), Some("running"));
            }
            other => panic!("expected tool part, got {other:?}"),
        }
    }

    #[test]
    fn parses_claude_tool_result_part() {
        let line = r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"fn main() {}"}]}}"#;
        let parsed = parse_chat_stream_line("claude_stream_json", line);
        match parsed.parts.into_iter().next() {
            Some(ChatMessagePart::Tool {
                name,
                output_summary,
                status,
                ..
            }) => {
                assert_eq!(name, "toolu_1");
                assert_eq!(output_summary.as_deref(), Some("fn main() {}"));
                assert_eq!(status.as_deref(), Some("done"));
            }
            other => panic!("expected tool_result part, got {other:?}"),
        }
    }

    #[test]
    fn title_from_user_message_truncates_on_word_boundary() {
        let title = super::title_from_user_message(
            "hello world this is a fairly long first line that should wrap",
            24,
        );
        assert!(title.ends_with('…'), "{title}");
        assert!(!title.contains("wrap"), "{title}");
        assert!(title.starts_with("hello"), "{title}");
    }

    #[test]
    fn session_needs_attention_from_flag_or_error() {
        let mut session = super::ChatSession {
            id: "s1".into(),
            project_path: "/tmp".into(),
            agent_id: "a1".into(),
            title: "t".into(),
            permission_mode: "explore".into(),
            messages: vec![],
            send_receipts: vec![],
            turns: vec![],
            created_at_ms: 1,
            updated_at_ms: 1,
            resume_command: None,
            resume_handle: None,
            active_turn_id: None,
            turn_status: "idle".into(),
            promoted_task_id: None,
            status: "active".into(),
            flagged: false,
            schema_version: 2,
            last_seq: 0,
            revision: 0,
        };
        assert!(!super::session_needs_attention(&session));
        session.flagged = true;
        assert!(super::session_needs_attention(&session));
        session.flagged = false;
        session.messages.push(super::ChatMessage {
            id: "m1".into(),
            role: "assistant".into(),
            content: "x".into(),
            status: "error".into(),
            created_at_ms: 1,
            error_summary: Some("boom".into()),
            parts: vec![],
        });
        assert!(super::session_needs_attention(&session));
        session.status = "archived".into();
        assert!(!super::session_needs_attention(&session));
    }

    #[test]
    fn reconcile_interrupted_marks_streaming_aborted() {
        let mut session = super::ChatSession {
            id: "s1".into(),
            project_path: "/tmp".into(),
            agent_id: "a1".into(),
            title: "t".into(),
            permission_mode: "explore".into(),
            messages: vec![super::ChatMessage {
                id: "m1".into(),
                role: "assistant".into(),
                content: "partial".into(),
                status: "streaming".into(),
                created_at_ms: 1,
                error_summary: None,
                parts: vec![],
            }],
            send_receipts: vec![],
            turns: vec![],
            created_at_ms: 1,
            updated_at_ms: 1,
            resume_command: Some("resume-1".into()),
            resume_handle: None,
            active_turn_id: Some("turn-1".into()),
            turn_status: "streaming".into(),
            promoted_task_id: None,
            status: "active".into(),
            flagged: false,
            schema_version: 2,
            last_seq: 0,
            revision: 0,
        };
        assert!(super::reconcile_interrupted_session(&mut session));
        assert_eq!(session.turn_status, "idle");
        assert!(session.active_turn_id.is_none());
        assert_eq!(session.messages[0].status, "aborted");
        assert_eq!(session.messages[0].content, "partial");
        assert!(session.flagged);
        assert_eq!(session.resume_command.as_deref(), Some("resume-1"));
    }

    #[test]
    fn chat_turn_timeout_constant_is_within_product_window() {
        // Plan: sensible default 5–10 minutes.
        const {
            assert!(super::CHAT_TURN_TIMEOUT_MS >= 5 * 60 * 1000);
            assert!(super::CHAT_TURN_TIMEOUT_MS <= 10 * 60 * 1000);
        }
    }

    #[test]
    fn stop_reason_covers_abort_and_timeout() {
        assert!(super::is_chat_stop_reason(Some("chat_abort")));
        assert!(super::is_chat_stop_reason(Some("chat_timeout")));
        assert!(!super::is_chat_stop_reason(Some("other")));
        assert!(!super::is_chat_stop_reason(None));
    }

    #[test]
    fn timeout_summary_is_readable() {
        let summary = super::chat_timeout_summary();
        assert!(summary.contains("超时"), "{summary}");
        assert!(summary.contains("自动停止"), "{summary}");
    }

    #[test]
    fn chat_stop_outcome_marks_timeout_aborted_with_error() {
        let outcome = super::chat_stop_outcome(Some("chat_timeout"), String::new(), Vec::new());
        assert_eq!(outcome.status, "aborted");
        assert!(outcome
            .error_summary
            .as_deref()
            .unwrap_or("")
            .contains("超时"));
        assert!(outcome.content.contains("超时"));
        assert!(matches!(
            outcome.parts.first(),
            Some(ChatMessagePart::Error { code: Some(code), .. }) if code == "chat_timeout"
        ));
    }

    #[test]
    fn chat_stop_outcome_user_abort_keeps_simple_stopped_copy() {
        let outcome = super::chat_stop_outcome(Some("chat_abort"), String::new(), Vec::new());
        assert_eq!(outcome.status, "aborted");
        assert!(outcome.error_summary.is_none());
        assert_eq!(outcome.content, "（已停止）");
    }
}
