use crate::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum PermissionScope {
    Application,
    Workspace { workspace_id: Id },
    Source { workspace_id: Id, source_id: Id },
}
impl PermissionScope {
    pub fn workspace_id(&self) -> Option<&Id> {
        match self {
            Self::Application => None,
            Self::Workspace { workspace_id } | Self::Source { workspace_id, .. } => {
                Some(workspace_id)
            }
        }
    }
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PermissionDocument {
    pub scope: PermissionScope,
    pub revision: u64,
    pub operation_id: Id,
    // None resets this layer to bundled defaults (application) or no additions.
    pub config: Option<Value>,
    pub updated_at_ms: u64,
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SavePermissionRequest {
    pub scope: PermissionScope,
    pub operation_id: Id,
    pub expected_revision: Option<u64>,
    pub config: Option<Value>,
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PermissionSettings {
    pub application_defaults: Value,
    pub documents: Vec<PermissionDocument>,
}
pub(crate) fn validate_permission_document(config: &Option<Value>) -> Result<()> {
    if let Some(value) = config {
        if !value.is_object()
            || serde_json::to_vec(value)
                .map_err(|_| CoreError::invalid("invalid permission JSON"))?
                .len()
                > 256 * 1024
        {
            return Err(CoreError::invalid(
                "permission configuration must be an object of at most 256 KiB",
            ));
        }
    }
    Ok(())
}
