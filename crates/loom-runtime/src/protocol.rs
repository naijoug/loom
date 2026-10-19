use loom_core::{ConnectionConfig, Id};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const VERSION: u32 = 2;
pub const MAX_FRAME: usize = 1024 * 1024;

#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Scope {
    pub workspace_id: Id,
    pub session_id: Id,
    pub turn_id: Id,
    pub run_id: Id,
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HistoryMessage {
    pub role: String,
    pub text: String,
}

/// Contains no secret; credentials are borrowed into a private stdin frame.
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunConfig {
    pub scope: Scope,
    pub connection: ConnectionConfig,
    pub model: String,
    pub thinking: Option<String>,
    pub cwd: String,
    pub runtime_directory: String,
    pub prompt: String,
    pub history: Vec<HistoryMessage>,
    pub tools: Vec<String>,
    pub external_tools: Vec<ExternalTool>,
    pub max_turns: u32,
}

#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExternalTool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Start<'a> {
    pub protocol_version: u32,
    #[cfg_attr(feature = "bindings", ts(type = "\"start\""))]
    pub kind: &'static str,
    pub config: &'a RunConfig,
    pub credential: Option<&'a str>,
}

#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum WorkerEvent {
    Ready,
    Text {
        part_id: Id,
        text_kind: String,
        delta: String,
    },
    ToolRequest {
        request_id: Id,
        tool_call_id: Id,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_call_id: Id,
        failed: bool,
        output: Value,
    },
    Usage {
        input_tokens: u64,
        output_tokens: u64,
    },
    Completed {
        cancelled: bool,
    },
    Failed {
        code: String,
        message: String,
    },
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Frame {
    pub protocol_version: u32,
    pub scope: Scope,
    pub seq: u64,
    pub event: WorkerEvent,
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolDecision {
    pub request_id: Id,
    pub tool_call_id: Id,
    pub allowed: bool,
    pub output: Option<Value>,
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DecisionFrame<'a> {
    pub protocol_version: u32,
    pub kind: &'static str,
    pub scope: &'a Scope,
    pub decision: &'a ToolDecision,
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CancelFrame<'a> {
    pub protocol_version: u32,
    pub kind: &'static str,
    pub scope: &'a Scope,
}
