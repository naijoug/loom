//! Persisted Research shapes. All JSON is camelCase and versioned.
use serde::{Deserialize, Serialize};

pub use crate::source_files::{RefStats, SourceRef, SourceText, TreeEntry};

pub const RESEARCH_SCHEMA_VERSION: u32 = 2;

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Workflow {
    #[default]
    Legacy,
    Target,
}
impl Workflow {
    pub fn is_legacy(&self) -> bool {
        *self == Self::Legacy
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TurnStatus {
    Running,
    Done,
    Failed,
    Cancelled,
    Interrupted,
}

/// One question and the report answering it; the report is `turns/<index>.md`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Turn {
    pub index: u32,
    pub question: String,
    pub status: TurnStatus,
    pub job_id: String,
    pub started_at_ms: u128,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at_ms: Option<u128>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refs: Option<RefStats>,
    /// `native` when the agent resumed its own session, `replay` when Loom
    /// replayed earlier questions and reports; absent for the first question.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Research {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_owner: Option<super::admission::Owner>,
    #[serde(default, skip_serializing_if = "Workflow::is_legacy")]
    pub workflow: Workflow,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<super::target_models::TargetConfig>,
    pub schema_version: u32,
    pub research_id: String,
    pub title: String,
    pub agent_id: String,
    pub agent_name: String,
    pub adapter_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Registered project the agent reads; `None` runs in an empty Loom-managed directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
    pub language: String,
    pub prompt_version: u32,
    pub created_at_ms: u128,
    pub updated_at_ms: u128,
    pub turns: Vec<Turn>,
    /// Native resume handle from the last completed run and the agent command it
    /// belongs to; follow-ups replay history when either no longer matches.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_agent_command: Option<String>,
}

impl Research {
    pub fn running_turn(&self) -> Option<&Turn> {
        self.turns.iter().find(|t| t.status == TurnStatus::Running)
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchSummary {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_owner: Option<super::admission::Owner>,
    #[serde(default, skip_serializing_if = "Workflow::is_legacy")]
    pub workflow: Workflow,
    pub research_id: String,
    pub title: String,
    pub agent_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
    /// `running`, `failed` (last turn did not finish) or `ready`.
    pub status: String,
    pub turns: usize,
    pub updated_at_ms: u128,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchList {
    pub items: Vec<ResearchSummary>,
    pub storage_errors: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateResearchInput {
    pub question: String,
    pub agent_id: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub project_path: Option<String>,
    pub language: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnContent {
    pub index: u32,
    pub markdown: String,
    pub refs: Vec<SourceRef>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobRecord {
    pub job_id: String,
    pub turn: u32,
    pub status: TurnStatus,
    pub started_at_ms: u128,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at_ms: Option<u128>,
    /// Redacted program and arguments of each attempt (a replay fallback included).
    #[serde(default)]
    pub invocations: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default)]
    pub tool_calls: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobLogChunk {
    pub text: String,
    pub next_offset: u64,
    pub has_more: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchEvent {
    pub research_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn: Option<u32>,
    /// A `TurnStatus` value, or `deleted`.
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
