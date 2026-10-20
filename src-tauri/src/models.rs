use serde::{Deserialize, Serialize};
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

pub const CURRENT_SCHEMA_VERSION: u32 = 1;

pub struct IdGenerator {
    next_id: &'static AtomicU64,
}

impl Default for IdGenerator {
    fn default() -> Self {
        // ProcessSupervisor is process-wide. Separate app/test registries must
        // not generate the same run id when constructed in the same millisecond.
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        Self { next_id: &NEXT_ID }
    }
}

impl IdGenerator {
    pub fn next(&self, prefix: &str) -> String {
        let sequence = self.next_id.fetch_add(1, Ordering::Relaxed);
        format!("{prefix}-{}-{sequence}", now_ms())
    }
}

#[cfg(test)]
mod id_tests {
    use super::*;

    #[test]
    fn independent_generators_never_reuse_a_process_registry_id() {
        let mut seen = std::collections::HashSet::new();
        for _ in 0..1024 {
            let id = IdGenerator::default().next("run");
            assert!(
                seen.insert(id.clone()),
                "duplicate process registry id: {id}"
            );
        }
    }
}

pub fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ThemeMode {
    #[serde(rename = "light")]
    Light,
    #[serde(rename = "dark")]
    Dark,
    #[serde(rename = "system")]
    System,
}

fn default_theme_mode() -> ThemeMode {
    ThemeMode::System
}

fn default_confirm_before_commands() -> bool {
    true
}

fn default_command_timeout_seconds() -> u64 {
    600
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Locale {
    #[default]
    #[serde(rename = "en")]
    En,
    #[serde(rename = "zh-CN")]
    ZhCn,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    #[serde(default)]
    pub locale: Locale,
    #[serde(default = "default_theme_mode")]
    pub theme_mode: ThemeMode,
    #[serde(default = "default_confirm_before_commands")]
    pub confirm_before_commands: bool,
    #[serde(default = "default_command_timeout_seconds")]
    pub command_timeout_seconds: u64,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            locale: Locale::default(),
            theme_mode: default_theme_mode(),
            confirm_before_commands: default_confirm_before_commands(),
            command_timeout_seconds: default_command_timeout_seconds(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub id: String,
    pub path: String,
    pub name: String,
    pub detected_stacks: Vec<String>,
    pub suggested_commands: Vec<String>,
    pub is_git_repository: bool,
    pub git_branch: Option<String>,
    pub has_uncommitted_changes: bool,
    pub loom_dir_ready: bool,
    pub schema_version: u32,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMetadata {
    pub schema_version: u32,
    pub project_id: String,
    pub project_path: String,
    pub project_name: String,
    pub updated_at_ms: u128,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentConfig {
    pub id: String,
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub working_directory_policy: String,
    pub capabilities: Vec<String>,
    pub adapter_type: String,
    pub can_write_files: bool,
    pub can_run_commands: bool,
    pub enabled: bool,
    pub available: bool,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentConfigInput {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub working_directory_policy: String,
    pub capabilities: Vec<String>,
    pub adapter_type: String,
    pub can_write_files: bool,
    pub can_run_commands: bool,
    pub enabled: bool,
}
