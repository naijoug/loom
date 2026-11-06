//! Versioned target-research contracts, separate from legacy turns.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const TARGET_SCHEMA_VERSION: u32 = 2;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Target {
    Github {
        url: String,
        #[serde(default)]
        git_ref: Option<String>,
    },
    Website {
        url: String,
        /// Resolved browser executable. Callers omit it; preflight fills it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        browser: Option<String>,
    },
    App {
        path: String,
    },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResearchMode {
    Single,
    Iterative,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Budget {
    pub max_rounds: u32,
    pub max_calls: u32,
    pub max_collections: u32,
    pub max_active_ms: u64,
    pub max_disk_bytes: u64,
}
impl Budget {
    pub fn defaults(mode: ResearchMode) -> Self {
        Self {
            max_rounds: if mode == ResearchMode::Single { 1 } else { 5 },
            max_calls: 12,
            max_collections: if mode == ResearchMode::Single { 8 } else { 20 },
            max_active_ms: if mode == ResearchMode::Single {
                1_200_000
            } else {
                1_800_000
            },
            max_disk_bytes: 512 * 1024 * 1024,
        }
    }
    pub fn validate(&self, mode: ResearchMode) -> Result<(), String> {
        let max = Self::defaults(mode);
        if self.max_rounds == 0
            || self.max_rounds > max.max_rounds
            || self.max_calls == 0
            || self.max_calls > max.max_calls
            || self.max_collections > max.max_collections
            || self.max_active_ms == 0
            || self.max_active_ms > max.max_active_ms
            || self.max_disk_bytes < 64 * 1024
            || self.max_disk_bytes > max.max_disk_bytes
        {
            return Err("invalid_budget".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub calls: u32,
    pub collections: u32,
    pub rounds: u32,
    pub active_ms: u64,
    pub disk_bytes: u64,
    #[serde(default)]
    pub disk_reserved_bytes: u64,
    pub reservation_ms: u64,
}
impl Usage {
    pub fn reserve(&mut self, budget: &Budget, kind: &str, max_ms: u64) -> Result<(), String> {
        if self.reservation_ms != 0 {
            return Err("step_already_reserved".into());
        }
        if max_ms == 0 || self.active_ms.saturating_add(max_ms) > budget.max_active_ms {
            return Err("budget_exhausted".into());
        }
        match kind {
            "agent" if self.calls < budget.max_calls => self.calls += 1,
            "collection" if self.collections < budget.max_collections => self.collections += 1,
            "material_read" | "report" => {}
            _ => return Err("budget_exhausted".into()),
        }
        self.reservation_ms = max_ms;
        Ok(())
    }
    pub fn settle(&mut self, elapsed_ms: u64) {
        self.active_ms = self.active_ms.saturating_add(elapsed_ms);
        self.reservation_ms = 0;
    }
    pub fn recover(&mut self) {
        self.active_ms = self.active_ms.saturating_add(self.reservation_ms);
        self.reservation_ms = 0;
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TargetInput {
    pub target: Target,
    pub question: String,
    pub agent_id: String,
    pub model: Option<String>,
    pub language: String,
    pub mode: ResearchMode,
    #[serde(default)]
    pub budget: Option<Budget>,
    #[serde(default)]
    pub tools: Option<crate::rea::Config>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetConfig {
    pub target: Target,
    pub run_ids: Vec<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Queued,
    Preflight,
    Collecting,
    Analyzing,
    Reporting,
    Cancelling,
    Completed,
    Cancelled,
    Failed,
    Interrupted,
    AwaitingUser,
}
impl RunStatus {
    pub fn active(self) -> bool {
        matches!(
            self,
            Self::Queued
                | Self::Preflight
                | Self::Collecting
                | Self::Analyzing
                | Self::Reporting
                | Self::Cancelling
        )
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Preflight => "preflight",
            Self::Collecting => "collecting",
            Self::Analyzing => "analyzing",
            Self::Reporting => "reporting",
            Self::Cancelling => "cancelling",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
            Self::AwaitingUser => "awaiting_user",
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunKind {
    Initial,
    Refresh,
    EvidenceFollowup,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSpec {
    pub schema_version: u32,
    pub research_id: String,
    pub run_id: String,
    pub kind: RunKind,
    pub input: TargetInput,
    pub budget: Budget,
    pub config_digest: String,
    pub agent_config_digest: String,
    pub target_identity: String,
    pub parent_report_id: Option<String>,
    pub source_snapshots: Vec<String>,
    pub source_evidence: Vec<String>,
    pub created_at_ms: u128,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunState {
    pub run_id: String,
    pub status: RunStatus,
    pub attempt: u32,
    pub sequence: u64,
    pub usage: Usage,
    pub snapshot_ids: Vec<String>,
    pub evidence_ids: Vec<String>,
    pub report_ids: Vec<String>,
    pub stop_reason: Option<String>,
    pub error: Option<String>,
    pub updated_at_ms: u128,
    #[serde(default)]
    pub exchange: super::materials::ExchangeBudget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material_context: Option<super::materials::ContextRef>,
}
impl RunState {
    pub fn new(spec: &RunSpec) -> Self {
        Self {
            run_id: spec.run_id.clone(),
            status: RunStatus::Queued,
            attempt: 1,
            sequence: 0,
            usage: Usage::default(),
            snapshot_ids: spec.source_snapshots.clone(),
            evidence_ids: spec.source_evidence.clone(),
            report_ids: Vec::new(),
            stop_reason: None,
            error: None,
            updated_at_ms: spec.created_at_ms,
            exchange: Default::default(),
            material_context: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunView {
    pub spec: RunSpec,
    pub state: RunState,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetEvent {
    pub research_id: String,
    pub run_id: String,
    pub step_id: Option<String>,
    pub status: RunStatus,
    pub phase: RunStatus,
    pub progress: Usage,
    pub stop_reason: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotFile {
    pub sha256: String,
    pub bytes: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceSnapshot {
    pub snapshot_id: String,
    pub identity: String,
    pub files: BTreeMap<String, SnapshotFile>,
    pub total_bytes: u64,
    pub created_at_ms: u128,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub report_id: String,
    pub run_id: String,
    pub revision: u32,
    pub snapshot_ids: Vec<String>,
    pub evidence_ids: Vec<String>,
    pub default_source_snapshot_id: Option<String>,
    pub markdown_sha256: String,
    pub created_at_ms: u128,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportContent {
    pub report: Report,
    pub markdown: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceRecord {
    pub evidence_id: String,
    pub run_id: String,
    pub source_id: Option<String>,
    pub tool: String,
    pub tool_version: String,
    pub raw_sha256: String,
    pub model_sha256: String,
    pub bytes: u64,
    pub coverage: String,
    pub limitations: Vec<String>,
    pub created_at_ms: u128,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum MaterialRequest {
    EvidenceIndex {
        offset: usize,
        limit: usize,
    },
    Source {
        snapshot_id: String,
        path: String,
        start_line: u32,
        end_line: u32,
    },
    Tree {
        snapshot_id: String,
        offset: usize,
        limit: usize,
    },
    Evidence {
        evidence_id: String,
        offset: usize,
        limit: usize,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Material {
    pub text: String,
    pub next_cursor: Option<usize>,
    pub truncated: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExportInput {
    pub report_id: String,
    #[serde(default)]
    pub evidence_ids: Vec<String>,
    #[serde(default)]
    pub include_raw: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub directory: String,
    pub files: Vec<String>,
    pub includes_raw: bool,
    pub warning: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn budget_failure_is_atomic_and_recovery_charges_unknown_work() {
        let mut b = Budget::defaults(ResearchMode::Single);
        b.max_calls = 1;
        let mut u = Usage::default();
        u.reserve(&b, "agent", 100).unwrap();
        assert!(u.reserve(&b, "agent", 100).is_err());
        assert_eq!(u.calls, 1);
        u.recover();
        assert_eq!(u.active_ms, 100);
        assert_eq!(u.reservation_ms, 0);
        assert!(u.reserve(&b, "agent", 100).is_err());
        assert_eq!(u.active_ms, 100);
        assert!(u.reserve(&b, "collection", b.max_active_ms).is_err());
        assert_eq!(u.collections, 0);
    }
}
