//! Chat wire models, including backwards-compatible v1 persistence fields.
use serde::{Deserialize, Serialize};

// Tagged enum buffers do not support u128; these millisecond values fit in u64.
fn timestamp<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<u128, D::Error> {
    u64::deserialize(deserializer).map(u128::from)
}

pub(super) const DEFAULT_PERMISSION_MODE: &str = "explore";
pub(super) const DEFAULT_SESSION_STATUS: &str = "active";

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
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ChatMessagePart {
    Text {
        text: String,
    },
    Tool {
        name: String,
        #[serde(
            default,
            alias = "input_summary",
            skip_serializing_if = "Option::is_none"
        )]
        input_summary: Option<String>,
        #[serde(
            default,
            alias = "output_summary",
            skip_serializing_if = "Option::is_none"
        )]
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
    #[serde(deserialize_with = "timestamp")]
    pub created_at_ms: u128,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_summary: Option<String>,
    #[serde(
        default = "default_message_parts",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub parts: Vec<ChatMessagePart>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSendReceipt {
    pub client_request_id: String,
    pub turn_id: String,
    pub user_message_id: String,
    pub assistant_message_id: String,
    /// The supplied request value, not mutable session configuration.
    pub requested_permission_mode: Option<String>,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub send_receipts: Vec<ChatSendReceipt>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub turns: Vec<ChatTurn>,
    #[serde(deserialize_with = "timestamp")]
    pub created_at_ms: u128,
    #[serde(deserialize_with = "timestamp")]
    pub updated_at_ms: u128,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_handle: Option<ChatResumeHandle>,
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
    #[serde(default)]
    pub last_seq: u64,
    #[serde(default)]
    pub revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatResumeHandle {
    pub version: u32,
    pub adapter_type: String,
    pub native_session_id: String,
    pub config_fingerprint: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatTurnStatus {
    Starting,
    Running,
    Cancelling,
    Completed,
    Failed,
    Cancelled,
    TimedOut,
    Interrupted,
}

impl ChatTurnStatus {
    pub(super) fn is_terminal(&self) -> bool {
        !matches!(self, Self::Starting | Self::Running | Self::Cancelling)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatInvocationSnapshot {
    pub agent_id: String,
    pub adapter_type: String,
    pub program: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub permission_mode: String,
    pub stdin_prompt: bool,
    pub output_mode: String,
    pub config_fingerprint: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatTurn {
    pub id: String,
    pub client_request_id: String,
    pub user_message_id: String,
    pub assistant_message_id: String,
    pub invocation: ChatInvocationSnapshot,
    pub status: ChatTurnStatus,
    pub accepted_at_ms: u64,
    pub started_at_ms: Option<u64>,
    pub finished_at_ms: Option<u64>,
    pub process_id: Option<u32>,
    pub exit_code: Option<i32>,
    pub termination_reason: Option<String>,
    pub error_summary: Option<String>,
    pub stdout_log_ref: String,
    pub stderr_log_ref: String,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatLogStream {
    Stdout,
    Stderr,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatLogPage {
    pub text: String,
    pub next_offset: u64,
    pub has_more: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatExportInput {
    pub project_path: String,
    pub session_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatExportResult {
    pub directory: String,
    pub json_path: String,
    pub markdown_path: String,
    pub snapshot_seq: u64,
    pub in_progress: bool,
    pub warnings: Vec<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_error: Option<String>,
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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSendInput {
    pub project_path: String,
    pub session_id: String,
    pub client_request_id: String,
    pub text: String,
    #[serde(default)]
    pub permission_mode: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatAbortInput {
    pub project_path: String,
    pub session_id: String,
    pub turn_id: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSendResult {
    pub turn_id: String,
    pub session: ChatSession,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatStreamEvent {
    pub(super) session_id: String,
    pub(super) turn_id: String,
    pub(super) message_id: String,
    pub(super) delta: String,
    pub(super) done: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) part: Option<ChatMessagePart>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSessionPatch {
    pub fields: serde_json::Map<String, serde_json::Value>,
    pub changed_messages: Vec<ChatMessage>,
    pub appended_messages: Vec<ChatMessage>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
pub enum ChatEventPayload {
    SessionCreated(Box<ChatSession>),
    SessionPatch(ChatSessionPatch),
    Stream(ChatStreamEvent),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatEvent {
    pub schema_version: u32,
    pub project_key: String,
    pub session_id: String,
    pub seq: u64,
    #[serde(deserialize_with = "timestamp")]
    pub timestamp_ms: u128,
    #[serde(flatten)]
    pub body: ChatEventPayload,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatEventPage {
    pub events: Vec<ChatEvent>,
    pub has_more: bool,
    pub last_seq: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ChatTurnFinishedEvent {
    pub(super) project_key: String,
    pub(super) session_id: String,
    pub(super) turn_id: String,
    pub(super) message_id: String,
    pub(super) status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error_summary: Option<String>,
    #[serde(skip)]
    pub(super) journal_event: Option<ChatEvent>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ChatIndex {
    pub(super) schema_version: u32,
    pub(super) session_ids: Vec<String>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_parts_emit_frontend_camel_case_and_read_legacy_snake_case() {
        let legacy = serde_json::json!({
            "type": "tool", "name": "shell", "input_summary": "pwd",
            "output_summary": "/project", "status": "done",
        });
        let part: ChatMessagePart = serde_json::from_value(legacy).unwrap();
        let wire = serde_json::to_value(&part).unwrap();
        assert_eq!(
            wire,
            serde_json::json!({
                "type": "tool", "name": "shell", "inputSummary": "pwd",
                "outputSummary": "/project", "status": "done",
            })
        );
        assert_eq!(
            serde_json::from_value::<ChatMessagePart>(wire).unwrap(),
            part
        );
    }
}
