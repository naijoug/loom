use crate::*;
use serde::{Deserialize, Serialize};
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RememberKind {
    Command,
    Directory,
    Domain,
    Tool,
    Api,
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RememberKey {
    pub kind: RememberKind,
    pub digest: String,
    pub label: String,
}
impl RememberKey {
    /// Raw selectors are hashed, never persisted in an exact-command grant.
    pub fn new(kind: RememberKind, selector: &str, label: String) -> Result<Self> {
        Ok(Self {
            kind,
            digest: crate::projection::checksum(&(kind, selector))?,
            label,
        })
    }
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RememberOffer {
    pub keys: Vec<RememberKey>,
}
impl RememberOffer {
    pub(crate) fn validate(&self) -> Result<()> {
        if self.keys.is_empty()
            || self.keys.len() > 32
            || self.keys.iter().any(|k| {
                k.digest.len() != 64
                    || !k.digest.bytes().all(|b| b.is_ascii_hexdigit())
                    || k.label.is_empty()
                    || k.label.len() > 2048
            })
        {
            return Err(CoreError::invalid("invalid remembered authorization scope"));
        }
        Ok(())
    }
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RememberedGrant {
    pub id: Id,
    pub offer: RememberOffer,
    pub context_digest: String,
    pub created_at_ms: u64,
    pub revoked_at_ms: Option<u64>,
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantRevocation {
    User,
    RuntimeEnded,
}

pub(crate) fn context_digest(binding: &RuntimeBinding) -> Result<String> {
    crate::projection::checksum(&(
        &binding.adapter,
        &binding.connection.connection_id,
        &binding.working_directory,
        binding.permission_mode,
        binding.policy_version,
        &binding.policy_snapshot_digest,
    ))
}
pub(crate) fn remember_binding(base: String, offer: Option<&RememberOffer>) -> Result<String> {
    match offer {
        None => Ok(base),
        Some(o) => {
            o.validate()?;
            crate::projection::checksum(&(
                base,
                o.keys
                    .iter()
                    .map(|k| (&k.kind, &k.digest))
                    .collect::<Vec<_>>(),
            ))
        }
    }
}
