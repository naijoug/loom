use serde::{Deserialize, Serialize};
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

pub const CURRENT_SCHEMA_VERSION: u32 = 1;

pub struct IdGenerator {
    next_id: AtomicU64,
}

impl Default for IdGenerator {
    fn default() -> Self {
        Self {
            next_id: AtomicU64::new(1),
        }
    }
}

impl IdGenerator {
    pub fn next(&self, prefix: &str) -> String {
        let sequence = self.next_id.fetch_add(1, Ordering::Relaxed);
        format!("{prefix}-{}-{sequence}", now_ms())
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

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub task_id: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandRun {
    pub id: String,
    pub task_id: String,
    pub command: String,
    pub cwd: String,
    pub started_at_ms: u128,
    pub ended_at_ms: Option<u128>,
    pub status: String,
    pub exit_code: Option<i32>,
    pub stdout_log_ref: Option<String>,
    pub stderr_log_ref: Option<String>,
    pub error_summary: Option<ErrorSummary>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandLogEvent {
    pub task_id: Option<String>,
    pub run_id: String,
    pub stream: String,
    pub line: String,
    pub timestamp_ms: u128,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandFinishedEvent {
    pub task_id: String,
    pub run_id: String,
    pub status: String,
    pub exit_code: Option<i32>,
    pub timestamp_ms: u128,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorSummary {
    pub exit_code: Option<i32>,
    pub stderr_tail: Vec<String>,
    pub matched_lines: Vec<String>,
    pub failed: bool,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub project_path: String,
    pub title: String,
    pub raw_requirement: String,
    pub status: String,
    pub selected_planning_agent_ids: Vec<String>,
    pub primary_agent_id: Option<String>,
    pub review_agent_ids: Vec<String>,
    pub final_plan: Option<String>,
    #[serde(default)]
    pub final_plan_path: Option<String>,
    #[serde(default)]
    pub discussion_summary: Option<String>,
    #[serde(default)]
    pub planning_runs: Vec<PlanningRun>,
    #[serde(default)]
    pub agent_invocations: Vec<AgentInvocation>,
    #[serde(default)]
    pub plan_todos: Vec<PlanTodoItem>,
    pub events: Vec<TaskEvent>,
    pub command_runs: Vec<CommandRun>,
    pub feedback: Vec<UserFeedback>,
    pub repair_context_preview: Option<String>,
    pub created_at_ms: u128,
    pub updated_at_ms: u128,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanningRun {
    pub id: String,
    pub task_id: String,
    pub requirement: String,
    pub selected_agent_ids: Vec<String>,
    pub status: String,
    pub summary: String,
    pub started_at_ms: u128,
    pub ended_at_ms: Option<u128>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentInvocation {
    pub id: String,
    pub planning_run_id: String,
    pub task_id: String,
    pub agent_id: String,
    pub agent_name: String,
    pub status: String,
    pub prompt_summary: String,
    pub raw_output: String,
    pub output_summary: String,
    pub evidence_ref: Option<String>,
    #[serde(default)]
    pub stderr_tail: Vec<String>,
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub timed_out: bool,
    pub started_at_ms: u128,
    pub ended_at_ms: Option<u128>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanTodoItem {
    pub id: String,
    pub task_id: String,
    pub title: String,
    pub description: String,
    pub status: String,
    pub order: u32,
    pub plan_ref: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskEvent {
    pub id: String,
    pub task_id: String,
    pub timestamp_ms: u128,
    pub actor: String,
    pub status: String,
    pub input_summary: Option<String>,
    pub output_summary: Option<String>,
    pub evidence_ref: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserFeedback {
    pub id: String,
    pub task_id: String,
    pub command_run_id: Option<String>,
    pub content: String,
    pub timestamp_ms: u128,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTaskInput {
    pub project_path: String,
    pub title: String,
    pub raw_requirement: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanningDiscussionInput {
    pub project_path: String,
    pub task_id: String,
    pub requirement: String,
    pub agent_ids: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedbackInput {
    pub project_path: String,
    pub task_id: String,
    pub command_run_id: Option<String>,
    pub content: String,
}
