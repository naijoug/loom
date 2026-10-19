use crate::*;
use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportOrigin {
    pub project_directory: String,
    pub kind: String,
    pub source_id: Id,
    pub source_version: u32,
    pub content_digest: String,
    pub imported_at_ms: u64,
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportedHistoryRef {
    pub origin: ImportOrigin,
    pub manifest_digest: String,
    pub message_count: usize,
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoricalMessage {
    pub id: Id,
    pub role: Role,
    pub content: String,
    pub status: String,
    pub created_at_ms: u64,
    #[serde(default)]
    pub error_summary: Option<String>,
    #[serde(default)]
    pub parts: Vec<HistoricalPart>,
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum HistoricalPart {
    Text {
        text: String,
    },
    Tool {
        name: String,
        #[serde(default, alias = "input_summary")]
        input_summary: Option<String>,
        #[serde(default, alias = "output_summary")]
        output_summary: Option<String>,
        #[serde(default)]
        status: Option<String>,
    },
    Error {
        message: String,
        #[serde(default)]
        code: Option<String>,
    },
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportedResource {
    pub id: Id,
    pub source_path: String,
    pub bytes: u64,
    pub sha256: String,
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportHistoryPage {
    pub history: ImportedHistoryRef,
    pub messages: Vec<HistoricalMessage>,
    pub offset: usize,
    pub has_more: bool,
    pub resources: Vec<ImportedResource>,
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportResourceChunk {
    pub resource: ImportedResource,
    pub offset: u64,
    pub bytes: Vec<u8>,
    pub has_more: bool,
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LegacyImportPlan {
    pub id: Id,
    pub workspace_id: Id,
    pub project_id: Id,
    pub created_at_ms: u64,
    pub preview: LegacyPreview,
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LegacyImportResult {
    pub kind: String,
    pub source_id: String,
    pub session_key: Option<SessionKey>,
    pub status: String,
    pub error: Option<String>,
    pub issues: Vec<LegacyIssue>,
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LegacyImportReport {
    pub plan_id: Id,
    pub workspace_id: Id,
    pub completed_at_ms: u64,
    pub results: Vec<LegacyImportResult>,
}
