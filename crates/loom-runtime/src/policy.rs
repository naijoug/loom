//! Trusted Rust tool policy. Rules are compiled into an immutable, hashed snapshot.
//! Source activation, configuration writes and authentication remain host services.
mod config;
mod mcp;
mod paths;
pub mod remember;
use crate::{shell::ShellPolicy, Authorization, RuntimeError, ToolPolicy};
pub use config::{ApiRule, CommandHint, PermissionConfig, PolicyLayers, RulePattern};
use loom_core::{Id, PermissionMode, RuntimeContext};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};
type Result<T> = std::result::Result<T, RuntimeError>;
fn error(reason: impl Into<String>) -> RuntimeError {
    RuntimeError::new("policy", reason)
}
fn shell_error(error: crate::shell::ShellRejection) -> RuntimeError {
    RuntimeError::new("policy_config", error.to_string())
}
const POLICY_VERSION: u64 = 1;
#[derive(Clone, Debug, Serialize)]
pub struct PolicyScope {
    pub working_directory: PathBuf,
    pub home: PathBuf,
    pub plans_directory: Option<PathBuf>,
    pub data_directory: Option<PathBuf>,
    /// Host-owned tool registry metadata: true means Explore-safe.
    pub session_tools: BTreeMap<String, bool>,
    pub built_in_sources: BTreeSet<Id>,
    pub craft_cli_available: bool,
}
impl PolicyScope {
    pub fn native(working_directory: PathBuf, home: PathBuf) -> Self {
        Self {
            working_directory,
            home,
            plans_directory: None,
            data_directory: None,
            session_tools: BTreeMap::new(),
            built_in_sources: BTreeSet::new(),
            craft_cli_available: false,
        }
    }
    fn validate(&self) -> Result<()> {
        for path in [&self.working_directory, &self.home]
            .into_iter()
            .chain(self.plans_directory.iter())
            .chain(self.data_directory.iter())
        {
            if !path.is_absolute()
                || paths::absolute(
                    path.to_str()
                        .ok_or_else(|| error("Policy path is not UTF-8"))?,
                    &self.working_directory,
                    &self.home,
                )? != *path
            {
                return Err(error("Policy paths must be normalized absolute paths"));
            }
        }
        if self
            .working_directory
            .canonicalize()
            .map_err(|_| error("Working directory is unavailable"))?
            != self.working_directory
        {
            return Err(error("Working directory changed"));
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuardedRisk {
    Irreversible,
    OutsideWorkspace,
    External,
}
/// Carries raw arguments to a trusted risk provider; never log or serialize blindly.
pub struct GuardedCall<'a> {
    pub name: &'a str,
    pub input: &'a Value,
    pub working_directory: &'a Path,
}
pub trait GuardedCheck: Send {
    fn active(&self) -> bool;
    fn assess(&mut self, call: GuardedCall<'_>) -> Result<Option<Vec<GuardedRisk>>>;
}
pub struct PermissionPolicy {
    rules: config::CompiledRules,
    scope: PolicyScope,
    digest: String,
    snapshot: Value,
    guarded: Option<Box<dyn GuardedCheck>>,
    rea: crate::rea::ReaCatalog,
}
impl PermissionPolicy {
    pub fn new(mut layers: PolicyLayers, scope: PolicyScope) -> Result<Self> {
        scope.validate()?;
        if layers.application.is_none() {
            layers.application = Some(PermissionConfig::bundled()?);
        }
        let mut snapshot = serde_json::json!({"schemaVersion":1,"policyVersion":POLICY_VERSION,"layers":layers,"scope":scope});
        snapshot.sort_all_objects();
        let bytes =
            serde_json::to_vec(&snapshot).map_err(|_| error("Cannot identify policy snapshot"))?;
        if bytes.len() > 1024 * 1024 {
            return Err(error("Merged permission snapshot exceeds 1 MiB"));
        }
        let rules = config::CompiledRules::new(&layers, &scope)?;
        let digest = format!("{:x}", Sha256::digest(bytes));
        Ok(Self {
            rules,
            scope,
            digest,
            snapshot,
            guarded: None,
            rea: crate::rea::ReaCatalog::new()?,
        })
    }
    pub fn snapshot_digest(&self) -> &str {
        &self.digest
    }
    pub fn snapshot(&self) -> &Value {
        &self.snapshot
    }
    pub fn version(&self) -> u64 {
        POLICY_VERSION
    }
    pub fn set_guarded_check(&mut self, check: Option<Box<dyn GuardedCheck>>) {
        self.guarded = check;
    }
    pub fn remember_scope(
        &self,
        mode: PermissionMode,
        name: &str,
        input: &Value,
    ) -> Result<Option<loom_core::RememberOffer>> {
        remember::offer(self, mode, name, input)
    }
    pub fn decide(
        &mut self,
        mode: PermissionMode,
        name: &str,
        input: &Value,
    ) -> Result<Authorization> {
        if !input.is_object() {
            return Err(error("Tool arguments must be an object"));
        }
        let guarded =
            mode == PermissionMode::Guarded && self.guarded.as_ref().is_some_and(|c| c.active());
        let mode = if mode == PermissionMode::Guarded && !guarded {
            PermissionMode::Ask
        } else {
            mode
        };
        let effect = self.effect(name, input, mode)?;
        if let Effect::Unavailable(reason) = effect {
            return Ok(Authorization::Deny {
                reason: reason.into(),
            });
        }
        if mode == PermissionMode::Execute {
            return Ok(allow("Execute mode"));
        }
        if matches!(effect, Effect::ReadOnly) {
            return Ok(allow("Read-only or explicitly allowlisted operation"));
        }
        if mode == PermissionMode::Explore {
            return Ok(Authorization::Deny {
                reason: "Operation is not allowed in Explore; switch to Ask or Execute".into(),
            });
        }
        if mode == PermissionMode::Ask {
            return Ok(Authorization::Ask);
        }
        // Active Guarded is Execute plus risk prompts. A transient empty/error
        // answer falls back to Execute, unlike an inactive check (Ask above).
        if let Effect::Write(ref target) = effect {
            if paths::within(target, &self.scope.working_directory) || self.session_path(target) {
                return Ok(allow("Guarded write inside the project/session"));
            }
            return Ok(Authorization::Ask);
        }
        let verdict = self
            .guarded
            .as_mut()
            .expect("active check")
            .assess(GuardedCall {
                name,
                input,
                working_directory: &self.scope.working_directory,
            });
        if matches!(verdict,Ok(Some(ref risks)) if !risks.is_empty()) {
            Ok(Authorization::Ask)
        } else {
            Ok(allow("Guarded check did not request escalation"))
        }
    }
    fn session_path(&self, target: &Path) -> bool {
        self.scope
            .plans_directory
            .iter()
            .chain(self.scope.data_directory.iter())
            .any(|base| paths::within(target, base))
    }
    fn effect(&self, name: &str, input: &Value, mode: PermissionMode) -> Result<Effect> {
        match name {
            "read" | "Read" | "glob" | "Glob" | "grep" | "Grep" | "find" | "ls" => {
                return Ok(Effect::ReadOnly)
            }
            "write" | "Write" | "edit" | "Edit" | "MultiEdit" | "NotebookEdit" => {
                let raw = input
                    .get("file_path")
                    .or_else(|| input.get("path"))
                    .or_else(|| input.get("notebook_path"))
                    .and_then(Value::as_str)
                    .ok_or_else(|| error("Write tool has no valid target path"))?;
                let target = paths::absolute(raw, &self.scope.working_directory, &self.scope.home)?;
                // Ask does not inherit Explore's plans/data exception (upstream).
                if (mode == PermissionMode::Explore && self.session_path(&target))
                    || (matches!(mode, PermissionMode::Explore | PermissionMode::Ask)
                        && paths::allowlisted(&target, &self.rules.writes)?)
                {
                    return Ok(Effect::ReadOnly);
                }
                return Ok(Effect::Write(target));
            }
            "bash" | "Bash" => {
                let command = input
                    .get("command")
                    .and_then(Value::as_str)
                    .ok_or_else(|| error("Bash tool has no valid command"))?;
                return Ok(if self.rules.shell.validate(command).is_ok() {
                    Effect::ReadOnly
                } else {
                    Effect::Mutation
                });
            }
            _ => {}
        }
        if let Some(safe) = self.scope.session_tools.get(name) {
            return Ok(if *safe {
                Effect::ReadOnly
            } else {
                Effect::Mutation
            });
        }
        if self.rules.blocked.contains(name) && mode == PermissionMode::Explore {
            return Ok(Effect::Mutation);
        }
        if let Some(rest) = name.strip_prefix("mcp__") {
            let (source, action) = rest
                .split_once("__")
                .ok_or_else(|| error("Invalid MCP tool identity"))?;
            let source_id = Id::new(source).map_err(|_| error("Invalid source identity"))?;
            if !self.rules.sources.contains(&source_id)
                && !self.scope.built_in_sources.contains(&source_id)
            {
                return Ok(Effect::Unavailable("Source is not active"));
            }
            if source_id.as_str() == crate::rea::REA_SOURCE {
                let action = crate::rea::split_tool_name(name)?;
                // Session recording is additive and provider startup is possible,
                // so only the fixture-declared harmless Evidence views are read-only.
                return Ok(if self.rea.read_only(action) {
                    Effect::ReadOnly
                } else {
                    Effect::Mutation
                });
            }
            if action.starts_with("api_") {
                return self.api(input);
            }
            if mcp::read_only(source, action, &self.rules.mcp_verbs) {
                return Ok(Effect::ReadOnly);
            }
            for (scope, pattern) in &self.rules.mcp_patterns {
                // Constrain source explicitly; alternation cannot escape a textual prefix.
                if scope.as_ref().is_some_and(|s| s != &source_id) {
                    continue;
                }
                let target = if scope.is_some() { action } else { name };
                if pattern
                    .is_match(target)
                    .map_err(|_| error("MCP pattern exceeded its budget"))?
                {
                    return Ok(Effect::ReadOnly);
                }
            }
            return Ok(Effect::Mutation);
        }
        if let Some(source) = name.strip_prefix("api_") {
            let source = Id::new(source).map_err(|_| error("Invalid source identity"))?;
            if !self.rules.sources.contains(&source) {
                return Ok(Effect::Unavailable("Source is not active"));
            }
            return self.api(input);
        }
        Ok(Effect::Unavailable(
            "Tool has no registered permission metadata",
        ))
    }
    fn api(&self, input: &Value) -> Result<Effect> {
        let method = match input.get("method") {
            None => "GET",
            Some(v) => v.as_str().ok_or_else(|| error("Invalid API method"))?,
        }
        .to_ascii_uppercase();
        if method == "GET" {
            return Ok(Effect::ReadOnly);
        }
        if let Some(path) = input.get("path").and_then(Value::as_str) {
            for (allowed, pattern) in &self.rules.api {
                if allowed == &method
                    && pattern
                        .is_match(path)
                        .map_err(|_| error("API pattern exceeded its budget"))?
                {
                    return Ok(Effect::ReadOnly);
                }
            }
        }
        Ok(Effect::Mutation)
    }
}
impl ToolPolicy for PermissionPolicy {
    fn remember_offer(
        &self,
        context: &RuntimeContext,
        name: &str,
        input: &Value,
    ) -> Result<Option<loom_core::RememberOffer>> {
        self.validate_context(context)?;
        self.remember_scope(context.binding.permission_mode, name, input)
    }
    fn validate_context(&self, context: &RuntimeContext) -> Result<()> {
        if context.binding.policy_version != POLICY_VERSION
            || context.binding.policy_snapshot_digest.as_deref() != Some(self.snapshot_digest())
            || Path::new(&context.working_directory) != self.scope.working_directory
        {
            return Err(error(
                "Runtime binding differs from the compiled permission snapshot",
            ));
        }
        Ok(())
    }
    fn evaluate(
        &mut self,
        context: &RuntimeContext,
        name: &str,
        input: &Value,
    ) -> Result<Authorization> {
        self.validate_context(context)?;
        self.decide(context.binding.permission_mode, name, input)
    }
}
fn allow(reason: &str) -> Authorization {
    Authorization::Allow {
        reason: reason.into(),
    }
}
enum Effect {
    ReadOnly,
    Write(PathBuf),
    Mutation,
    Unavailable(&'static str),
}
