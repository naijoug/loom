//! Chat-first sessions (M2). ChatSession ≠ Task — does not use task_state.
use crate::agent_adapter::{self, AdapterInvocationRequest, AgentStage, PreparedAgentInvocation};
use crate::agents::{self, load_agents};
use crate::models::{now_ms, IdGenerator};
use crate::process_supervisor::{self, ProcessKind, ProcessMetadata};
use crate::session_capture;
use crate::storage;
use crate::tasks;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tauri::{AppHandle, Emitter, State};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

const CHAT_SCHEMA_VERSION: u32 = 1;


const DEFAULT_PERMISSION_MODE: &str = "explore";
const DEFAULT_SESSION_STATUS: &str = "active";

/// Normalize chat permission strings for load/save compatibility.
/// `read_only` → `explore`; `read_write` → `ask` (safer than auto).
pub fn normalize_permission_mode(mode: &str) -> String {
    match mode {
        "explore" | "ask" | "auto" => mode.to_string(),
        "read_only" => "explore".to_string(),
        "read_write" => "ask".to_string(),
        _ => DEFAULT_PERMISSION_MODE.to_string(),
    }
}

fn default_permission_mode() -> String {
    DEFAULT_PERMISSION_MODE.to_string()
}

fn default_session_status() -> String {
    DEFAULT_SESSION_STATUS.to_string()
}

fn default_message_parts() -> Vec<ChatMessagePart> {
    Vec::new()
}


/// text | tool | error parts (M0 sketch). Flexible fields for forward-compat JSON.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ChatMessagePart {
    Text {
        text: String,
    },
    Tool {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        input_summary: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output_summary: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        status: Option<String>,
    },
    Error {
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        code: Option<String>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub id: String,
    pub role: String,
    pub content: String,
    pub status: String,
    pub created_at_ms: u128,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_summary: Option<String>,
    #[serde(default = "default_message_parts", skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<ChatMessagePart>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSession {
    pub id: String,
    pub project_path: String,
    pub agent_id: String,
    pub title: String,
    #[serde(default = "default_permission_mode")]
    pub permission_mode: String,
    pub messages: Vec<ChatMessage>,
    pub created_at_ms: u128,
    pub updated_at_ms: u128,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_turn_id: Option<String>,
    pub turn_status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub promoted_task_id: Option<String>,
    /// Phase 1: `active` | `archived`. Default active for old sessions.
    #[serde(default = "default_session_status")]
    pub status: String,
    /// User flag — contributes to needs_attention inbox filter.
    #[serde(default)]
    pub flagged: bool,
    pub schema_version: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSessionSummary {
    pub id: String,
    pub title: String,
    pub agent_id: String,
    pub updated_at_ms: u128,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub needs_attention: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flagged: Option<bool>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatCreateInput {
    pub project_path: String,
    pub agent_id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub permission_mode: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSendInput {
    pub project_path: String,
    pub session_id: String,
    pub text: String,
    #[serde(default)]
    pub permission_mode: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatAbortInput {
    pub project_path: String,
    pub session_id: String,
    #[serde(default)]
    pub turn_id: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSendResult {
    pub turn_id: String,
    pub session: ChatSession,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ChatStreamEvent {
    session_id: String,
    turn_id: String,
    message_id: String,
    delta: String,
    done: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    part: Option<ChatMessagePart>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ChatTurnFinishedEvent {
    session_id: String,
    turn_id: String,
    message_id: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_summary: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChatIndex {
    schema_version: u32,
    session_ids: Vec<String>,
}

fn ensure_chat_dirs(project_path: &Path) -> Result<PathBuf, String> {
    let root = storage::project_loom_dir(project_path).join("chat");
    let sessions = root.join("sessions");
    std::fs::create_dir_all(&sessions)
        .map_err(|error| format!("failed to create chat dirs: {error}"))?;
    Ok(root)
}

fn index_path(root: &Path) -> PathBuf {
    root.join("index.json")
}

fn session_path(root: &Path, session_id: &str) -> PathBuf {
    root.join("sessions").join(format!("{session_id}.json"))
}

fn load_index(root: &Path) -> Result<ChatIndex, String> {
    let path = index_path(root);
    if !path.exists() {
        return Ok(ChatIndex {
            schema_version: CHAT_SCHEMA_VERSION,
            session_ids: Vec::new(),
        });
    }
    let raw = std::fs::read_to_string(&path)
        .map_err(|error| format!("failed to read chat index: {error}"))?;
    serde_json::from_str(&raw).map_err(|error| format!("invalid chat index: {error}"))
}

fn save_index(root: &Path, index: &ChatIndex) -> Result<(), String> {
    let raw = serde_json::to_string_pretty(index)
        .map_err(|error| format!("failed to serialize chat index: {error}"))?;
    std::fs::write(index_path(root), raw)
        .map_err(|error| format!("failed to write chat index: {error}"))
}

fn load_session(root: &Path, session_id: &str) -> Result<ChatSession, String> {
    let path = session_path(root, session_id);
    let raw = std::fs::read_to_string(&path)
        .map_err(|error| format!("failed to read chat session: {error}"))?;
    let mut session: ChatSession =
        serde_json::from_str(&raw).map_err(|error| format!("invalid chat session: {error}"))?;
    session.permission_mode = normalize_permission_mode(&session.permission_mode);
    if session.status != "active" && session.status != "archived" {
        session.status = DEFAULT_SESSION_STATUS.to_string();
    }
    if reconcile_interrupted_session(&mut session) {
        let _ = save_session(root, &session);
    }
    Ok(session)
}

fn save_session(root: &Path, session: &ChatSession) -> Result<(), String> {
    let raw = serde_json::to_string_pretty(session)
        .map_err(|error| format!("failed to serialize chat session: {error}"))?;
    std::fs::write(session_path(root, &session.id), raw)
        .map_err(|error| format!("failed to write chat session: {error}"))
}



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
    session.messages.iter().any(|message| message.status == "error")
}

/// Lazy repair after app restart: streaming turns become aborted, partial text kept.
fn reconcile_interrupted_session(session: &mut ChatSession) -> bool {
    let mut changed = false;
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
    // explore/ask (+ legacy read_only / read_write→ask) → Planning (conservative CLI).
    // auto → Debugging (workspace-write / acceptEdits).
    match normalize_permission_mode(mode).as_str() {
        "auto" => AgentStage::Debugging,
        _ => AgentStage::Planning,
    }
}


fn chat_task_id(session_id: &str) -> String {
    format!("chat:{session_id}")
}

/// Best-effort parse of one stdout line into text delta and/or a tool/command part.
#[derive(Debug, Default, PartialEq, Eq)]
struct ParsedChatLine {
    delta: String,
    part: Option<ChatMessagePart>,
}

fn parse_chat_stream_line(output_mode: &str, line: &str) -> ParsedChatLine {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return ParsedChatLine::default();
    }
    if output_mode == "plain" {
        return ParsedChatLine {
            delta: format!("{trimmed}\n"),
            part: None,
        };
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) else {
        if !trimmed.starts_with('{') {
            return ParsedChatLine {
                delta: trimmed.to_string(),
                part: None,
            };
        }
        return ParsedChatLine::default();
    };
    let part = extract_tool_or_command_part(&value);
    let delta = extract_assistant_text(output_mode, trimmed);
    ParsedChatLine { delta, part }
}

fn json_event_type(value: &serde_json::Value) -> Option<String> {
    value
        .get("type")
        .and_then(|v| v.as_str())
        .or_else(|| value.get("msg").and_then(|msg| msg.get("type")).and_then(|v| v.as_str()))
        .or_else(|| value.get("event").and_then(|event| event.get("type")).and_then(|v| v.as_str()))
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
        Some(format!("{}…", trimmed.chars().take(max_chars).collect::<String>()))
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
            status: Some("done".to_string()),
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

fn build_prompt(session: &ChatSession, user_text: &str) -> String {
    let mut parts = Vec::new();
    parts.push("You are a coding assistant running inside Loom Chat.".to_string());
    parts.push("Answer helpfully. Prefer concise, actionable replies.".to_string());
    parts.push(format!("Project path: {}", session.project_path));
    if !session.messages.is_empty() {
        parts.push("Conversation so far:".to_string());
        for message in session
            .messages
            .iter()
            .rev()
            .take(12)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
        {
            parts.push(format!("{}: {}", message.role, message.content));
        }
    }
    parts.push(format!("User: {user_text}"));
    parts.join("\n\n")
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
    if chunks.is_empty() {
        trimmed.to_string()
    } else {
        chunks.join("")
    }
}

#[tauri::command]
pub fn chat_list_sessions(project_path: String) -> Result<Vec<ChatSessionSummary>, String> {
    let project = PathBuf::from(&project_path);
    let root = ensure_chat_dirs(&project)?;
    let index = load_index(&root)?;
    let mut summaries = Vec::new();
    for session_id in index.session_ids {
        if let Ok(session) = load_session(&root, &session_id) {
            let preview = session
                .messages
                .last()
                .map(|message| message.content.chars().take(80).collect::<String>());
                        let needs_attention = session_needs_attention(&session);
            let flagged = session.flagged;
            summaries.push(ChatSessionSummary {
                id: session.id,
                title: session.title,
                agent_id: session.agent_id,
                updated_at_ms: session.updated_at_ms,
                preview,
                status: Some(session.status),
                needs_attention: Some(needs_attention),
                flagged: Some(flagged),
            });
        }
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
    let _ = app;
    let project = PathBuf::from(&input.project_path);
    let root = ensure_chat_dirs(&project)?;
    let created_at_ms = now_ms();
    let session = ChatSession {
        id: ids.next("chat"),
        project_path: input.project_path,
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
        created_at_ms,
        updated_at_ms: created_at_ms,
        resume_command: None,
        active_turn_id: None,
        turn_status: "idle".to_string(),
        promoted_task_id: None,
        flagged: false,
        schema_version: CHAT_SCHEMA_VERSION,
    };
    save_session(&root, &session)?;
    let mut index = load_index(&root)?;
    if !index.session_ids.iter().any(|id| id == &session.id) {
        index.session_ids.push(session.id.clone());
    }
    save_index(&root, &index)?;
    Ok(session)
}

#[tauri::command]
pub fn chat_get(project_path: String, session_id: String) -> Result<ChatSession, String> {
    let root = ensure_chat_dirs(Path::new(&project_path))?;
    load_session(&root, &session_id)
}

#[tauri::command]
pub fn chat_set_agent(
    project_path: String,
    session_id: String,
    agent_id: String,
) -> Result<ChatSession, String> {
    let root = ensure_chat_dirs(Path::new(&project_path))?;
    let mut session = load_session(&root, &session_id)?;
    session.agent_id = agent_id;
    session.updated_at_ms = now_ms();
    save_session(&root, &session)?;
    Ok(session)
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatUpdateMetaInput {
    pub project_path: String,
    pub session_id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub permission_mode: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub flagged: Option<bool>,
    /// When true, set title from the first user message (smart truncate).
    #[serde(default)]
    pub title_from_first_message: Option<bool>,
}

#[tauri::command]
pub fn chat_update_meta(input: ChatUpdateMetaInput) -> Result<ChatSession, String> {
    let root = ensure_chat_dirs(Path::new(&input.project_path))?;
    let mut session = load_session(&root, &input.session_id)?;
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
        session.permission_mode = normalize_permission_mode(&mode);
    }
    if let Some(status) = input.status {
        session.status = match status.as_str() {
            "archived" => "archived".to_string(),
            _ => DEFAULT_SESSION_STATUS.to_string(),
        };
    }
    if let Some(flagged) = input.flagged {
        session.flagged = flagged;
    }
    session.updated_at_ms = now_ms();
    save_session(&root, &session)?;
    Ok(session)
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatPromoteInput {
    pub project_path: String,
    pub session_id: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatPromoteResult {
    pub task_id: String,
    pub task: crate::models::Task,
    pub session: ChatSession,
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
    let root = ensure_chat_dirs(Path::new(&input.project_path))?;
    let mut session = load_session(&root, &input.session_id)?;
    let (title, raw_requirement) = promote_requirement_from_session(&session);
    // Stub: create a draft task only — do not advance the task state machine.
    let task = tasks::create_task(
        app,
        ids,
        crate::models::CreateTaskInput {
            project_path: input.project_path.clone(),
            title,
            raw_requirement,
            selected_planning_agent_ids: Vec::new(),
            primary_agent_id: Some(session.agent_id.clone()),
        },
    )?;
    session.promoted_task_id = Some(task.id.clone());
    session.updated_at_ms = now_ms();
    save_session(&root, &session)?;
    Ok(ChatPromoteResult {
        task_id: task.id.clone(),
        task,
        session,
    })
}

#[tauri::command]
pub fn chat_clear_resume(project_path: String, session_id: String) -> Result<ChatSession, String> {
    let root = ensure_chat_dirs(Path::new(&project_path))?;
    let mut session = load_session(&root, &session_id)?;
    session.resume_command = None;
    session.updated_at_ms = now_ms();
    save_session(&root, &session)?;
    Ok(session)
}

#[tauri::command]
pub async fn chat_abort(input: ChatAbortInput) -> Result<(), String> {
    let _ = input.project_path;
    let reason = "chat_abort";
    if let Some(turn_id) = input.turn_id.as_deref().filter(|value| !value.is_empty()) {
        let _ = process_supervisor::supervisor().request_stop(turn_id, reason)?;
        return Ok(());
    }
    let _ = process_supervisor::supervisor().stop_task_runs(
        &chat_task_id(&input.session_id),
        &[ProcessKind::Chat],
        reason,
    )?;
    Ok(())
}

#[tauri::command]
pub async fn chat_send(
    app: AppHandle,
    ids: State<'_, IdGenerator>,
    input: ChatSendInput,
) -> Result<ChatSendResult, String> {
    let text = input.text.trim().to_string();
    if text.is_empty() {
        return Err("message text is required".to_string());
    }
    let project = PathBuf::from(&input.project_path);
    let root = ensure_chat_dirs(&project)?;
    let mut session = load_session(&root, &input.session_id)?;
    if let Some(mode) = input.permission_mode {
        session.permission_mode = normalize_permission_mode(&mode);
    }
    if session.turn_status == "streaming" {
        return Err("a chat turn is already running for this session".to_string());
    }

    let agents = load_agents(&app)?;
    let agent = agents
        .into_iter()
        .find(|agent| agent.id == session.agent_id)
        .ok_or_else(|| format!("agent '{}' was not found", session.agent_id))?;

    let stage = permission_to_stage(&session.permission_mode);
    let prompt = build_prompt(&session, &text);
    let prepared: PreparedAgentInvocation = agent_adapter::prepare_invocation(
        &agent,
        &AdapterInvocationRequest {
            project_path: Path::new(&session.project_path),
            prompt: &prompt,
            prompt_file: None,
            stage,
            resume_command: session.resume_command.as_deref(),
            embed_prompt: true,
        },
    )?;
    if prepared.stdin_prompt {
        return Err(format!(
            "agent '{}' requires stdin prompt transport, which chat does not support yet",
            agent.name
        ));
    }

    let now = now_ms();
    let user_message = ChatMessage {
        id: ids.next("msg"),
        role: "user".to_string(),
        content: text.clone(),
        status: "complete".to_string(),
        created_at_ms: now,
        error_summary: None,
        parts: Vec::new(),
    };
    let assistant_id = ids.next("msg");
    let turn_id = ids.next("turn");
    let assistant_message = ChatMessage {
        id: assistant_id.clone(),
        role: "assistant".to_string(),
        content: String::new(),
        status: "streaming".to_string(),
        created_at_ms: now + 1,
        error_summary: None,
        parts: Vec::new(),
    };
    if session.messages.is_empty() && session.title == "新对话" {
        session.title = title_from_user_message(&text, 48);
    }
    session.messages.push(user_message);
    session.messages.push(assistant_message);
    session.active_turn_id = Some(turn_id.clone());
    session.turn_status = "streaming".to_string();
    session.updated_at_ms = now + 1;
    save_session(&root, &session)?;

    let app_handle = app.clone();
    let session_id = session.id.clone();
    let project_path = session.project_path.clone();
    let output_mode = prepared.output_mode.clone();

    let turn_id_for_result = turn_id.clone();
    tauri::async_runtime::spawn(async move {
        let result = run_chat_turn(
            app_handle.clone(),
            session_id.clone(),
            turn_id.clone(),
            assistant_id.clone(),
            prepared,
            output_mode,
        )
        .await;
        let root = match ensure_chat_dirs(Path::new(&project_path)) {
            Ok(root) => root,
            Err(_) => return,
        };
        let Ok(mut session) = load_session(&root, &session_id) else {
            return;
        };
        if let Some(message) = session
            .messages
            .iter_mut()
            .find(|message| message.id == assistant_id)
        {
            match result {
                Ok(outcome) => {
                    message.content = outcome.content;
                    message.parts = outcome.parts;
                    message.status = outcome.status.clone();
                    // Only adopt a new resume handle on a complete turn — never on abort/error.
                    if outcome.status == "complete" {
                        if let Some(resume) = outcome.resume_command {
                            session.resume_command = Some(resume);
                        }
                    }
                    if outcome.status == "error" {
                        session.flagged = true;
                    }
                    let _ = app_handle.emit(
                        "loom://chat-turn-finished",
                        ChatTurnFinishedEvent {
                            session_id: session_id.clone(),
                            turn_id: turn_id.clone(),
                            message_id: assistant_id.clone(),
                            status: outcome.status,
                            error_summary: outcome.error_summary,
                        },
                    );
                }
                Err(error) => {
                    if message.content.trim().is_empty() {
                        message.content = format!("（调用失败）{error}");
                    }
                    message.status = "error".to_string();
                    message.error_summary = Some(error.clone());
                    message.parts.push(ChatMessagePart::Error {
                        message: error.clone(),
                        code: None,
                    });
                    // Keep prior resume_command; a failed turn should not wipe a good handle.
                    session.flagged = true;
                    session.flagged = true;
                    let _ = app_handle.emit(
                        "loom://chat-turn-finished",
                        ChatTurnFinishedEvent {
                            session_id: session_id.clone(),
                            turn_id: turn_id.clone(),
                            message_id: assistant_id.clone(),
                            status: "error".to_string(),
                            error_summary: Some(error),
                        },
                    );
                }
            }
        }
        session.active_turn_id = None;
        session.turn_status = "idle".to_string();
        session.updated_at_ms = now_ms();
        let _ = save_session(&root, &session);
    });

    Ok(ChatSendResult {
        turn_id: turn_id_for_result,
        session,
    })
}

async fn run_chat_turn(
    app: AppHandle,
    session_id: String,
    turn_id: String,
    message_id: String,
    prepared: PreparedAgentInvocation,
    output_mode: String,
) -> Result<ChatTurnOutcome, String> {
    let program = prepared.program.clone();
    let mut command = Command::new(&prepared.program);
    command
        .args(&prepared.args)
        .current_dir(&prepared.cwd)
        .kill_on_drop(true)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    process_supervisor::configure_process_group(&mut command);
    let mut child = command
        .spawn()
        .map_err(|error| format!("failed to start agent: {error}"))?;
    if let Some(pid) = child.id() {
        process_supervisor::supervisor().register(ProcessMetadata::new(
            &turn_id,
            chat_task_id(&session_id),
            ProcessKind::Chat,
            pid,
            None,
        ))?;
    }
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "agent stdout missing".to_string())?;
    let stderr = child.stderr.take();
    let mut lines = BufReader::new(stdout).lines();
    let mut raw = String::new();
    let mut stdout_lines: Vec<String> = Vec::new();
    let mut parts: Vec<ChatMessagePart> = Vec::new();
    while let Ok(Some(line)) = lines.next_line().await {
        let redacted = agents::redact_sensitive_text(&line);
        stdout_lines.push(redacted.clone());
        raw.push_str(&redacted);
        raw.push('\n');
        let parsed = parse_chat_stream_line(&output_mode, &redacted);
        if let Some(part) = parsed.part.clone() {
            parts.push(part.clone());
            let _ = app.emit(
                "loom://chat-stream",
                ChatStreamEvent {
                    session_id: session_id.clone(),
                    turn_id: turn_id.clone(),
                    message_id: message_id.clone(),
                    delta: String::new(),
                    done: false,
                    part: Some(part),
                },
            );
        }
        if !parsed.delta.is_empty() {
            let _ = app.emit(
                "loom://chat-stream",
                ChatStreamEvent {
                    session_id: session_id.clone(),
                    turn_id: turn_id.clone(),
                    message_id: message_id.clone(),
                    delta: parsed.delta,
                    done: false,
                    part: None,
                },
            );
        }
    }
    let status = child
        .wait()
        .await
        .map_err(|error| format!("failed waiting for agent: {error}"))?;
    let meta = process_supervisor::supervisor().complete(&turn_id);
    let aborted = meta
        .as_ref()
        .and_then(|value| value.termination_reason.as_deref())
        == Some("chat_abort");
    let mut stderr_text = String::new();
    if let Some(stderr) = stderr {
        let mut err_lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = err_lines.next_line().await {
            stderr_text.push_str(&agents::redact_sensitive_text(&line));
            stderr_text.push('\n');
        }
    }
    let content = extract_assistant_text(&output_mode, &raw);
    let stderr_lines: Vec<String> = stderr_text.lines().map(str::to_string).collect();
    let captured =
        session_capture::capture_session_from_lines(&program, &stdout_lines, &stderr_lines);
    let _ = app.emit(
        "loom://chat-stream",
        ChatStreamEvent {
            session_id,
            turn_id,
            message_id,
            delta: String::new(),
            done: true,
            part: None,
        },
    );

    if aborted {
        let content = if content.trim().is_empty() {
            "（已停止）".to_string()
        } else {
            content
        };
        return Ok(ChatTurnOutcome {
            content,
            parts,
            resume_command: captured.resume_command,
            status: "aborted".to_string(),
            error_summary: None,
        });
    }

    if !status.success() && content.trim().is_empty() {
        return Err(format!(
            "agent exited with status {status}; stderr: {}",
            stderr_text.trim()
        ));
    }
    let content = if content.trim().is_empty() {
        if stderr_text.trim().is_empty() {
            "（Agent 没有返回可见文本）".to_string()
        } else {
            format!("（无结构化输出，stderr）\n{}", stderr_text.trim())
        }
    } else {
        content
    };
    Ok(ChatTurnOutcome {
        content,
        parts,
        resume_command: captured.resume_command,
        status: "complete".to_string(),
        error_summary: None,
    })
}

#[derive(Debug)]
struct ChatTurnOutcome {
    content: String,
    parts: Vec<ChatMessagePart>,
    resume_command: Option<String>,
    status: String,
    error_summary: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{
        extract_tool_or_command_part, normalize_permission_mode, parse_chat_stream_line,
        permission_to_stage, ChatMessagePart,
    };
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
        assert_eq!(permission_to_stage("ask"), AgentStage::Planning);
        assert_eq!(permission_to_stage("read_only"), AgentStage::Planning);
        assert_eq!(permission_to_stage("read_write"), AgentStage::Planning);
        assert_eq!(permission_to_stage("auto"), AgentStage::Debugging);
    }

    #[test]
    fn permission_tiers_drive_prepare_invocation_cli_flags() {
        let root = Path::new("/repo");
        let cases = [
            ("explore", AgentStage::Planning),
            ("ask", AgentStage::Planning),
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
                    embed_prompt: true,
                },
            )
            .expect("grok");
            let expected_perm = if mode == "auto" {
                "acceptEdits"
            } else {
                "plan"
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
                    embed_prompt: true,
                },
            )
            .expect("codex");
            let expected_sandbox = if mode == "auto" {
                "workspace-write"
            } else {
                "read-only"
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
                    embed_prompt: true,
                },
            )
            .expect("claude");
            if mode == "auto" {
                assert_eq!(
                    arg_pair(&claude.args, "--permission-mode"),
                    Some("acceptEdits"),
                    "claude auto"
                );
            } else {
                assert!(
                    arg_pair(&claude.args, "--permission-mode").is_none(),
                    "claude {mode} should omit permission-mode"
                );
            }
        }
    }

    #[test]
    fn parses_grok_style_tool_use_part() {
        let line = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash","input":{"command":"ls -la"}}]}}"#;
        let parsed = parse_chat_stream_line("streaming_json", line);
        match parsed.part {
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
        assert!(parsed.part.is_none());
    }

    #[test]
    fn parses_claude_assistant_tool_use_part() {
        let line = r#"{"type":"assistant","message":{"id":"msg_1","role":"assistant","content":[{"type":"text","text":"Looking"},{"type":"tool_use","id":"toolu_1","name":"Read","input":{"path":"src/main.rs"}}]}}"#;
        let parsed = parse_chat_stream_line("claude_stream_json", line);
        assert!(parsed.delta.contains("Looking"), "delta={}", parsed.delta);
        match parsed.part {
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
        match parsed.part {
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
        match parsed.part {
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
            created_at_ms: 1,
            updated_at_ms: 1,
            resume_command: None,
            active_turn_id: None,
            turn_status: "idle".into(),
            promoted_task_id: None,
            status: "active".into(),
            flagged: false,
            schema_version: 1,
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
            created_at_ms: 1,
            updated_at_ms: 1,
            resume_command: Some("resume-1".into()),
            active_turn_id: Some("turn-1".into()),
            turn_status: "streaming".into(),
            promoted_task_id: None,
            status: "active".into(),
            flagged: false,
            schema_version: 1,
        };
        assert!(super::reconcile_interrupted_session(&mut session));
        assert_eq!(session.turn_status, "idle");
        assert!(session.active_turn_id.is_none());
        assert_eq!(session.messages[0].status, "aborted");
        assert_eq!(session.messages[0].content, "partial");
        assert!(session.flagged);
        assert_eq!(session.resume_command.as_deref(), Some("resume-1"));
    }

}
