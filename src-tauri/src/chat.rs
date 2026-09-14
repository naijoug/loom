//! Chat-first sessions (M2). ChatSession ≠ Task — does not use task_state.
use crate::agent_adapter::{
    self, AdapterInvocationRequest, AgentStage, PreparedAgentInvocation,
};
use crate::agents::{self, load_agents};
use crate::models::{now_ms, IdGenerator};
use crate::session_capture;
use crate::storage;
use crate::tasks;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex;

const CHAT_SCHEMA_VERSION: u32 = 1;

#[derive(Clone)]
pub struct ChatTurnRegistry {
    inner: Arc<Mutex<HashMap<String, u32>>>, // session_id -> pid
}

impl Default for ChatTurnRegistry {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }
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
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSession {
    pub id: String,
    pub project_path: String,
    pub agent_id: String,
    pub title: String,
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
    serde_json::from_str(&raw).map_err(|error| format!("invalid chat session: {error}"))
}

fn save_session(root: &Path, session: &ChatSession) -> Result<(), String> {
    let raw = serde_json::to_string_pretty(session)
        .map_err(|error| format!("failed to serialize chat session: {error}"))?;
    std::fs::write(session_path(root, &session.id), raw)
        .map_err(|error| format!("failed to write chat session: {error}"))
}

fn permission_to_stage(mode: &str) -> AgentStage {
    if mode == "read_write" {
        AgentStage::Debugging
    } else {
        AgentStage::Planning
    }
}

fn build_prompt(session: &ChatSession, user_text: &str) -> String {
    let mut parts = Vec::new();
    parts.push("You are a coding assistant running inside Loom Chat.".to_string());
    parts.push("Answer helpfully. Prefer concise, actionable replies.".to_string());
    parts.push(format!("Project path: {}", session.project_path));
    if !session.messages.is_empty() {
        parts.push("Conversation so far:".to_string());
        for message in session.messages.iter().rev().take(12).collect::<Vec<_>>().into_iter().rev() {
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
            if let Some(text) = value
                .pointer("/msg/text")
                .or_else(|| value.pointer("/message/content/0/text"))
                .or_else(|| value.get("text"))
                .and_then(|v| v.as_str())
            {
                chunks.push(text.to_string());
                continue;
            }
            if let Some(delta) = value
                .pointer("/delta")
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
            summaries.push(ChatSessionSummary {
                id: session.id,
                title: session.title,
                agent_id: session.agent_id,
                updated_at_ms: session.updated_at_ms,
                preview,
            });
        }
    }
    summaries.sort_by(|a, b| b.updated_at_ms.cmp(&a.updated_at_ms));
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
        permission_mode: input
            .permission_mode
            .unwrap_or_else(|| "read_only".to_string()),
        messages: Vec::new(),
        created_at_ms,
        updated_at_ms: created_at_ms,
        resume_command: None,
        active_turn_id: None,
        turn_status: "idle".to_string(),
        promoted_task_id: None,
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
pub struct ChatPromoteInput {
    pub project_path: String,
    pub session_id: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatPromoteResult {
    pub task_id: String,
    pub session: ChatSession,
}

#[tauri::command]
pub fn chat_promote_to_task(
    app: AppHandle,
    ids: State<'_, IdGenerator>,
    input: ChatPromoteInput,
) -> Result<ChatPromoteResult, String> {
    let root = ensure_chat_dirs(Path::new(&input.project_path))?;
    let mut session = load_session(&root, &input.session_id)?;
    let description = session
        .messages
        .iter()
        .rev()
        .find(|message| message.role == "user")
        .map(|message| message.content.clone())
        .unwrap_or_else(|| session.title.clone());
    let title = description.chars().take(48).collect::<String>();
    // Stub: create a draft task only — do not advance the task state machine.
    let task = tasks::create_task(
        app,
        ids,
        crate::models::CreateTaskInput {
            project_path: input.project_path.clone(),
            title,
            raw_requirement: description,
            selected_planning_agent_ids: Vec::new(),
            primary_agent_id: Some(session.agent_id.clone()),
        },
    )?;
    session.promoted_task_id = Some(task.id.clone());
    session.updated_at_ms = now_ms();
    save_session(&root, &session)?;
    Ok(ChatPromoteResult {
        task_id: task.id,
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
pub async fn chat_abort(
    registry: State<'_, ChatTurnRegistry>,
    input: ChatAbortInput,
) -> Result<(), String> {
    let _ = input.project_path;
    let _ = input.turn_id;
    let mut guard = registry.inner.lock().await;
    if let Some(pid) = guard.remove(&input.session_id) {
        let _ = Command::new("kill").arg(pid.to_string()).status().await;
    }
    Ok(())
}

#[tauri::command]
pub async fn chat_send(
    app: AppHandle,
    ids: State<'_, IdGenerator>,
    registry: State<'_, ChatTurnRegistry>,
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
        session.permission_mode = mode;
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
    };
    if session.messages.is_empty() && session.title == "新对话" {
        session.title = text.chars().take(32).collect();
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
    let registry_inner = registry.inner.clone();

    let turn_id_for_result = turn_id.clone();
    tauri::async_runtime::spawn(async move {
        let result = run_chat_turn(
            app_handle.clone(),
            registry_inner,
            project_path.clone(),
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
                Ok((content, resume_command)) => {
                    message.content = content;
                    message.status = "complete".to_string();
                    if resume_command.is_some() {
                        session.resume_command = resume_command;
                    }
                    let _ = app_handle.emit(
                        "loom://chat-turn-finished",
                        ChatTurnFinishedEvent {
                            session_id: session_id.clone(),
                            turn_id: turn_id.clone(),
                            message_id: assistant_id.clone(),
                            status: "complete".to_string(),
                            error_summary: None,
                        },
                    );
                }
                Err(error) => {
                    if message.content.trim().is_empty() {
                        message.content = format!("（调用失败）{error}");
                    }
                    message.status = "error".to_string();
                    message.error_summary = Some(error.clone());
                    // Keep prior resume_command; a failed turn should not wipe a good handle.
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
    registry: Arc<Mutex<HashMap<String, u32>>>,
    _project_path: String,
    session_id: String,
    turn_id: String,
    message_id: String,
    prepared: PreparedAgentInvocation,
    output_mode: String,
) -> Result<(String, Option<String>), String> {
    let program = prepared.program.clone();
    let mut command = Command::new(&prepared.program);
    command
        .args(&prepared.args)
        .current_dir(&prepared.cwd)
        .kill_on_drop(true)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| format!("failed to start agent: {error}"))?;
    if let Some(pid) = child.id() {
        registry.lock().await.insert(session_id.clone(), pid);
    }
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "agent stdout missing".to_string())?;
    let stderr = child.stderr.take();
    let mut lines = BufReader::new(stdout).lines();
    let mut raw = String::new();
    let mut stdout_lines: Vec<String> = Vec::new();
    while let Ok(Some(line)) = lines.next_line().await {
        let redacted = agents::redact_sensitive_text(&line);
        stdout_lines.push(redacted.clone());
        raw.push_str(&redacted);
        raw.push('\n');
        let delta = if output_mode == "plain" {
            format!("{redacted}\n")
        } else {
            extract_assistant_text(&output_mode, &redacted)
        };
        if !delta.is_empty() {
            let _ = app.emit(
                "loom://chat-stream",
                ChatStreamEvent {
                    session_id: session_id.clone(),
                    turn_id: turn_id.clone(),
                    message_id: message_id.clone(),
                    delta,
                    done: false,
                },
            );
        }
    }
    let status = child
        .wait()
        .await
        .map_err(|error| format!("failed waiting for agent: {error}"))?;
    registry.lock().await.remove(&session_id);
    let mut stderr_text = String::new();
    if let Some(stderr) = stderr {
        let mut err_lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = err_lines.next_line().await {
            stderr_text.push_str(&agents::redact_sensitive_text(&line));
            stderr_text.push('\n');
        }
    }
    let content = extract_assistant_text(&output_mode, &raw);
    let stderr_lines: Vec<String> = stderr_text
        .lines()
        .map(str::to_string)
        .collect();
    let captured = session_capture::capture_session_from_lines(&program, &stdout_lines, &stderr_lines);
    if !status.success() && content.trim().is_empty() {
        return Err(format!(
            "agent exited with status {status}; stderr: {}",
            stderr_text.trim()
        ));
    }
    let _ = app.emit(
        "loom://chat-stream",
        ChatStreamEvent {
            session_id,
            turn_id,
            message_id,
            delta: String::new(),
            done: true,
        },
    );
    let content = if content.trim().is_empty() {
        if stderr_text.trim().is_empty() {
            "（Agent 没有返回可见文本）".to_string()
        } else {
            format!("（无结构化输出，stderr）\n{}", stderr_text.trim())
        }
    } else {
        content
    };
    Ok((content, captured.resume_command))
}
