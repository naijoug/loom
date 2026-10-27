//! Persisted Learn study shapes. All JSON is camelCase and versioned.
use serde::{Deserialize, Serialize};

pub const STUDY_SCHEMA_VERSION: u32 = 1;
pub const ARCHITECTURE_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StageStatus {
    Pending,
    Running,
    Done,
    Failed,
    Cancelled,
    Interrupted,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StageState {
    /// `fetch`, `inventory`, `architecture`, `module:<id>` or `glossary`.
    pub stage: String,
    pub status: StageStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at_ms: Option<u128>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at_ms: Option<u128>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

impl StageState {
    pub fn pending(stage: impl Into<String>) -> Self {
        Self {
            stage: stage.into(),
            status: StageStatus::Pending,
            job_id: None,
            started_at_ms: None,
            finished_at_ms: None,
            error: None,
            warnings: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageInfo {
    /// `overview`, `architecture`, `module:<id>` or `glossary`.
    pub page_id: String,
    pub title: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<u8>,
    pub ready: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refs: Option<RefStats>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RefStats {
    pub valid: usize,
    pub invalid: usize,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Coverage {
    pub source_files: usize,
    pub covered: usize,
    /// Capped sample of uncovered source files.
    pub uncovered: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Study {
    pub schema_version: u32,
    pub study_id: String,
    pub url: String,
    pub owner: String,
    pub repo: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    pub agent_id: String,
    pub agent_name: String,
    pub adapter_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub language: String,
    pub depth: String,
    pub prompt_version: u32,
    pub created_at_ms: u128,
    pub updated_at_ms: u128,
    pub stages: Vec<StageState>,
    #[serde(default)]
    pub pages: Vec<PageInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coverage: Option<Coverage>,
    /// Fingerprint of the checkout right after fetch; every agent run must keep it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkout_fingerprint: Option<String>,
}

impl Study {
    pub fn stage_mut(&mut self, stage: &str) -> &mut StageState {
        if let Some(index) = self.stages.iter().position(|s| s.stage == stage) {
            return &mut self.stages[index];
        }
        self.stages.push(StageState::pending(stage));
        self.stages.last_mut().unwrap()
    }
    pub fn stage(&self, stage: &str) -> Option<&StageState> {
        self.stages.iter().find(|s| s.stage == stage)
    }
    pub fn stage_done(&self, stage: &str) -> bool {
        self.stage(stage)
            .is_some_and(|s| s.status == StageStatus::Done)
    }
    pub fn active_job(&self) -> Option<&StageState> {
        self.stages
            .iter()
            .find(|s| s.status == StageStatus::Running)
    }
}

/// List rows omit page bodies and architecture.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StudySummary {
    pub study_id: String,
    pub owner: String,
    pub repo: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    pub agent_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// `running`, `failed`, `ready` or `idle`.
    pub status: String,
    pub pages_ready: usize,
    pub pages_total: usize,
    pub updated_at_ms: u128,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StudyList {
    pub studies: Vec<StudySummary>,
    /// Unreadable study directories are reported, not dropped silently.
    pub storage_errors: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StudyDetail {
    pub study: Study,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub architecture: Option<Architecture>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateStudyInput {
    pub url: String,
    #[serde(default)]
    pub git_ref: Option<String>,
    pub agent_id: String,
    #[serde(default)]
    pub model: Option<String>,
    pub language: String,
    pub depth: String,
}

// ---- architecture.json ----

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Architecture {
    #[serde(default = "architecture_version")]
    pub schema_version: u32,
    pub project: ProjectInfo,
    pub layers: Vec<Layer>,
    pub modules: Vec<Module>,
    pub learning_path: Vec<LearningStep>,
}

fn architecture_version() -> u32 {
    ARCHITECTURE_SCHEMA_VERSION
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInfo {
    pub name: String,
    pub summary: String,
    #[serde(default)]
    pub overview: String,
    pub tech_stack: Vec<String>,
    pub entry_points: Vec<EntryPoint>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryPoint {
    pub path: String,
    pub line: u32,
    pub why: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Layer {
    pub id: String,
    pub title: String,
    pub module_ids: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Module {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub level: u8,
    pub order: u32,
    pub paths: Vec<String>,
    pub depends_on: Vec<String>,
    pub key_files: Vec<KeyFile>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyFile {
    pub path: String,
    pub start_line: u32,
    pub end_line: u32,
    pub why: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LearningStep {
    pub level: u8,
    pub title: String,
    pub goal: String,
    pub module_ids: Vec<String>,
}

// ---- pages, sources, jobs ----

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourceRef {
    /// Raw link target after `loom-src:`.
    pub target: String,
    pub path: String,
    pub start_line: u32,
    pub end_line: u32,
    /// `valid`, `missing`, `range` or `outside`.
    pub status: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageContent {
    pub page_id: String,
    pub markdown: String,
    pub refs: Vec<SourceRef>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceText {
    pub path: String,
    pub text: String,
    pub first_line: u32,
    pub total_lines: u32,
    pub truncated: bool,
    pub language: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeEntry {
    pub name: String,
    pub path: String,
    /// `dir`, `file` or `symlink`.
    pub kind: String,
    pub size: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobRecord {
    pub job_id: String,
    pub stage: String,
    pub status: StageStatus,
    pub started_at_ms: u128,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at_ms: Option<u128>,
    /// Redacted program and arguments of each agent attempt (repairs included).
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
pub struct LearnEvent {
    pub study_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    pub stage: String,
    /// A `StageStatus` value, or `deleted`.
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
