//! Versioned discussion and per-call records. Participant IDs are internal path keys.
pub use crate::source_files::{RefStats, SourceRef, SourceText, TreeEntry};
use serde::{Deserialize, Serialize};
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Pending,
    Running,
    Done,
    Failed,
    Cancelled,
    Interrupted,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Participant {
    pub id: String,
    pub agent_id: String,
    pub name: String,
    pub adapter_type: String,
    pub model: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub participant_id: String,
    /// 1 = independent, 2 = peer review, 3 = moderator summary.
    pub round: u8,
    pub status: Status,
    pub stale: bool,
    pub attempts: Vec<String>,
    pub error: Option<String>,
    pub warnings: Vec<String>,
    pub refs: Option<RefStats>,
}
impl Task {
    pub fn retryable(&self) -> bool {
        self.status != Status::Running && (self.status != Status::Done || self.stale)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Discussion {
    pub schema_version: u32,
    pub discussion_id: String,
    pub title: String,
    pub topic: String,
    pub language: String,
    pub project_path: Option<String>,
    pub project_name: Option<String>,
    pub participants: Vec<Participant>,
    pub tasks: Vec<Task>,
    pub status: String,
    pub active_run_id: Option<String>,
    pub created_at_ms: u128,
    pub updated_at_ms: u128,
    pub prompt_version: u32,
}
impl Discussion {
    pub fn task(&self, id: &str) -> Result<&Task, String> {
        self.tasks
            .iter()
            .find(|t| t.id == id)
            .ok_or_else(|| "Discussion task not found".into())
    }
    pub fn task_mut(&mut self, id: &str) -> Result<&mut Task, String> {
        self.tasks
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or_else(|| "Discussion task not found".into())
    }
    pub fn participant(&self, id: &str) -> Result<&Participant, String> {
        self.participants
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| "Participant not found".into())
    }
}

pub fn tasks(participants: &[Participant]) -> Vec<Task> {
    let make = |p: &Participant, round| Task {
        id: if round == 3 {
            "summary".into()
        } else {
            format!("r{round}-{}", p.id)
        },
        participant_id: p.id.clone(),
        round,
        status: Status::Pending,
        stale: false,
        attempts: Vec::new(),
        error: None,
        warnings: Vec::new(),
        refs: None,
    };
    let mut result: Vec<_> = (1..=2)
        .flat_map(|r| participants.iter().map(move |p| make(p, r)))
        .collect();
    if let Some(host) = participants.first() {
        result.push(make(host, 3));
    }
    result
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobRecord {
    pub job_id: String,
    pub task_id: String,
    pub run_id: String,
    pub status: Status,
    pub started_at_ms: u128,
    pub finished_at_ms: Option<u128>,
    pub invocation: Option<String>,
    pub model: Option<String>,
    pub exit_code: Option<i32>,
    pub tool_calls: usize,
    pub error: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscussionSummary {
    pub discussion_id: String,
    pub title: String,
    pub status: String,
    pub participants: usize,
    pub project_name: Option<String>,
    pub updated_at_ms: u128,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscussionList {
    pub items: Vec<DiscussionSummary>,
    pub storage_errors: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParticipantInput {
    pub agent_id: String,
    pub model: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateDiscussionInput {
    pub topic: String,
    pub participants: Vec<ParticipantInput>,
    pub project_path: Option<String>,
    pub language: String,
    pub confirmed_calls: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub task_id: String,
    pub markdown: String,
    pub refs: Vec<SourceRef>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogChunk {
    pub text: String,
    pub next_offset: u64,
    pub has_more: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscussEvent {
    pub discussion_id: String,
    pub task_id: Option<String>,
    pub status: String,
}
