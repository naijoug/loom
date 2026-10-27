//! What a run produces: message parts while it streams, and the final outcome.
//! Chat persists parts (as `ChatMessagePart`) and returns them over IPC, so
//! their serde shape is part of the contract.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff_stats: Option<DiffStats>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff_preview: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffStats {
    pub added: usize,
    pub removed: usize,
}

/// text | tool | error parts (M0 sketch). Flexible fields for forward-compat JSON.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum MessagePart {
    Text {
        text: String,
    },
    Tool {
        #[serde(flatten)]
        metadata: ToolMetadata,
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

/// Streamed while the agent runs, in output order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentEvent {
    /// Visible assistant text, already separated from the previous block.
    Text(String),
    /// A tool call, tool result or reported error.
    Part(MessagePart),
    /// The process and its output pipes are finished.
    Done,
}

#[derive(Debug, Default)]
pub struct AgentOutcome {
    pub(crate) content: String,
    pub(crate) parts: Vec<MessagePart>,
    pub(crate) resume_command: Option<String>,
    pub(crate) status: String,
    pub(crate) error_summary: Option<String>,
    pub(crate) exit_code: Option<i32>,
    pub(crate) termination_reason: Option<String>,
}
