use crate::{CoreError, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_SAFE_SEQUENCE: u64 = 9_007_199_254_740_991;
pub const MAX_MESSAGE_BYTES: usize = 1024 * 1024;

/// Validated wire identifier; it is also safe as a single storage path segment.
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
#[cfg_attr(feature = "bindings", ts(type = "string"))]
pub struct Id(String);
impl Id {
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if value.is_empty()
            || value.len() > 160
            || !value
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
        {
            return Err(CoreError::invalid(
                "ID must be 1–160 ASCII letters, digits, hyphens or underscores",
            ));
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for Id {
    type Error = CoreError;
    fn try_from(value: String) -> Result<Self> {
        Self::new(value)
    }
}
impl From<Id> for String {
    fn from(value: Id) -> Self {
        value.0
    }
}
impl std::fmt::Display for Id {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionKey {
    pub workspace_id: Id,
    pub session_id: Id,
}

#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionMode {
    Explore,
    #[default]
    Ask,
    Guarded,
    Execute,
}

#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Workspace {
    pub id: Id,
    pub name: String,
    pub created_at_ms: u64,
}

#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Project {
    pub id: Id,
    pub workspace_id: Id,
    pub name: String,
    pub directory: String,
    pub created_at_ms: u64,
}

#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectionSelection {
    pub connection_id: Option<Id>,
    pub model: Option<String>,
    pub thinking: Option<String>,
}

#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionMetadata {
    pub key: SessionKey,
    pub title: Option<String>,
    pub project_id: Option<Id>,
    pub status_id: Id,
    pub archived: bool,
    pub flagged: bool,
    pub labels: Vec<String>,
    pub permission_mode: PermissionMode,
    pub connection: ConnectionSelection,
    pub enabled_sources: Vec<Id>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    pub last_read_message_id: Option<Id>,
}

/// Tri-state optional field: omit/keep, clear, or set; avoids null/undefined ambiguity.
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", content = "value", rename_all = "snake_case")]
pub enum Change<T> {
    #[default]
    Keep,
    Clear,
    Set(T),
}

#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionPatch {
    #[serde(default)]
    pub title: Change<String>,
    pub status_id: Option<Id>,
    pub archived: Option<bool>,
    pub flagged: Option<bool>,
    pub labels: Option<Vec<String>>,
    pub permission_mode: Option<PermissionMode>,
    pub connection: Option<ConnectionSelection>,
    pub enabled_sources: Option<Vec<Id>>,
    pub notes: Option<String>,
}

#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateSession {
    pub key: SessionKey,
    pub title: Option<String>,
    pub project_id: Option<Id>,
    #[serde(default)]
    pub permission_mode: PermissionMode,
    #[serde(default)]
    pub connection: ConnectionSelection,
}

#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SendMessage {
    pub key: SessionKey,
    pub client_request_id: Id,
    pub text: String,
    #[serde(default)]
    pub attachment_ids: Vec<Id>,
}

#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SendReceipt {
    pub client_request_id: Id,
    pub turn_id: Id,
    pub user_message_id: Id,
    pub assistant_message_id: Id,
    pub accepted_seq: u64,
    pub request_digest: String,
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendResult {
    pub receipt: SendReceipt,
    pub already_accepted: bool,
    pub last_seq: u64,
}

#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnStatus {
    Queued,
    Running,
    Cancelling,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
}
impl TurnStatus {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Cancelled | Self::Interrupted
        )
    }
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishStatus {
    Completed,
    Failed,
    Cancelled,
    Interrupted,
}
impl From<FinishStatus> for TurnStatus {
    fn from(value: FinishStatus) -> Self {
        match value {
            FinishStatus::Completed => Self::Completed,
            FinishStatus::Failed => Self::Failed,
            FinishStatus::Cancelled => Self::Cancelled,
            FinishStatus::Interrupted => Self::Interrupted,
        }
    }
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeBinding {
    pub adapter: String,
    pub connection: ConnectionSelection,
    pub permission_mode: PermissionMode,
    pub policy_version: u64,
    pub working_directory: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_snapshot_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_snapshot_digest: Option<String>,
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TurnConfiguration {
    pub connection: ConnectionSelection,
    pub permission_mode: PermissionMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_connection: Option<Box<crate::ModelConnection>>,
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Turn {
    pub id: Id,
    pub receipt: SendReceipt,
    pub status: TurnStatus,
    pub configuration: TurnConfiguration,
    pub accepted_at_ms: u64,
    pub started_at_ms: Option<u64>,
    pub ended_at_ms: Option<u64>,
    pub binding: Option<RuntimeBinding>,
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<TokenUsage>,
    #[serde(skip)]
    pub runtime_receipts: BTreeMap<Id, RuntimeReceipt>,
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeReceipt {
    pub digest: String,
    pub seq: u64,
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextKind {
    Text,
    Thinking,
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum MessagePart {
    Text {
        id: Id,
        kind: TextKind,
        text: String,
    },
    Tool {
        tool_call_id: Id,
    },
    Attachment {
        attachment_id: Id,
    },
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: Id,
    pub turn_id: Id,
    pub role: Role,
    pub parts: Vec<MessagePart>,
    pub created_at_ms: u64,
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolStatus {
    Running,
    Succeeded,
    Failed,
    Denied,
    Interrupted,
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    pub id: Id,
    pub turn_id: Id,
    pub name: String,
    pub input: Value,
    pub output: Option<Value>,
    pub status: ToolStatus,
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum RuntimeUpdate {
    Usage {
        usage: TokenUsage,
    },
    ToolPolicy {
        tool_call_id: Id,
        name: String,
        input: Value,
        arguments_digest: String,
        allowed: bool,
        reason: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        authorization_ids: Vec<Id>,
    },
    TextDelta {
        part_id: Id,
        text_kind: TextKind,
        delta: String,
    },
    ToolStarted {
        tool_call_id: Id,
        name: String,
        input: Value,
    },
    ToolFinished {
        tool_call_id: Id,
        status: ToolStatus,
        output: Value,
    },
}

#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionView {
    pub metadata: SessionMetadata,
    pub notes: String,
    #[serde(default)]
    pub imported_history: Option<crate::ImportedHistoryRef>,
    #[serde(default)]
    pub approvals: BTreeMap<Id, crate::ToolApproval>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub remembered_grants: BTreeMap<Id, crate::RememberedGrant>,
    pub last_seq: u64,
    pub metadata_revision: u64,
    pub messages: Vec<Message>,
    pub turns: Vec<Turn>,
    pub tools: BTreeMap<Id, ToolCall>,
    pub active_turn_id: Option<Id>,
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub metadata: SessionMetadata,
    pub last_seq: u64,
    pub message_count: usize,
    pub preview: String,
    pub active_turn_id: Option<Id>,
    pub queued_turn_count: usize,
    pub has_unread: bool,
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionListEntry {
    pub session_id: Id,
    pub summary: Option<SessionSummary>,
    pub error: Option<String>,
}

#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum SessionEvent {
    Created {
        metadata: SessionMetadata,
    },
    HistoryImported {
        history: crate::ImportedHistoryRef,
    },
    ToolApprovalRequested {
        approval: crate::ToolApproval,
    },
    ToolApprovalResolved {
        request_id: Id,
        resolution: crate::ApprovalResolution,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        remembered_grant: Option<crate::RememberedGrant>,
    },
    ToolApprovalConsumed {
        request_id: Id,
    },
    RememberedGrantsRevoked {
        grant_ids: Vec<Id>,
        reason: crate::GrantRevocation,
    },
    MetadataChanged {
        patch: SessionPatch,
    },
    MessageAccepted {
        receipt: SendReceipt,
        text: String,
        attachment_ids: Vec<Id>,
        configuration: TurnConfiguration,
    },
    TurnStarted {
        turn_id: Id,
        binding: RuntimeBinding,
    },
    TurnPreparationFailed {
        turn_id: Id,
        error: String,
    },
    RuntimeApplied {
        turn_id: Id,
        input_id: Id,
        update: RuntimeUpdate,
    },
    TurnCancelRequested {
        turn_id: Id,
    },
    TurnFinished {
        turn_id: Id,
        status: FinishStatus,
        error: Option<String>,
    },
    ReadThrough {
        message_id: Id,
    },
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EventEnvelope<T> {
    pub schema_version: u32,
    pub seq: u64,
    pub stream: String,
    pub timestamp_ms: u64,
    pub previous_checksum: String,
    pub payload: T,
    pub checksum: String,
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventPage {
    pub events: Vec<EventEnvelope<SessionEvent>>,
    pub last_seq: u64,
    pub has_more: bool,
}

#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum WatchMessage {
    Event {
        event: Box<EventEnvelope<SessionEvent>>,
    },
    WorkspaceChanged {
        notice: WorkspaceNotice,
    },
    ResyncRequired,
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostCapabilities {
    pub protocol_version: u32,
    pub runtime: String,
    #[serde(default)]
    pub credential_store: String,
}

#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceNotice {
    pub workspace_id: Id,
    pub session_id: Option<Id>,
    pub seq: u64,
}
