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

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
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

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub task_id: Option<String>,
    #[serde(default)]
    pub intent: Option<CommandRunIntent>,
    #[serde(default)]
    pub loop_id: Option<String>,
    #[serde(default)]
    pub iteration: Option<u32>,
    #[serde(default)]
    pub attempt: Option<u32>,
    #[serde(default)]
    pub termination_reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandRunIntent {
    AgentAction,
    Validation,
    Preview,
    LoopStep,
    Legacy,
}

impl Default for CommandRunIntent {
    fn default() -> Self {
        Self::Legacy
    }
}

/// A persisted terminal slot for the Testing cockpit. `kind` is "preview"
/// (long-running dev server → PTY) or "validation" (one-shot check → piped,
/// feeds the acceptance gate).
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSlot {
    pub id: String,
    pub name: String,
    pub command: String,
    pub kind: String,
    /// Working directory relative to the project root (for monorepo subdir apps).
    /// `None`/absent runs at the project root.
    #[serde(default)]
    pub cwd: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandRun {
    pub id: String,
    pub task_id: String,
    pub command: String,
    pub cwd: String,
    #[serde(default)]
    pub intent: CommandRunIntent,
    #[serde(default)]
    pub loop_id: Option<String>,
    #[serde(default)]
    pub iteration: Option<u32>,
    #[serde(default)]
    pub attempt: Option<u32>,
    #[serde(default)]
    pub termination_reason: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub resume_command: Option<String>,
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
pub struct PlanningAgentLogEvent {
    pub task_id: String,
    pub planning_run_id: String,
    pub agent_id: String,
    pub agent_name: String,
    pub phase: String,
    #[serde(default = "default_attempt")]
    pub attempt: u32,
    pub stream: String,
    pub lines: Vec<String>,
    pub timestamp_ms: u128,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandFinishedEvent {
    pub task_id: String,
    pub run_id: String,
    pub status: String,
    pub exit_code: Option<i32>,
    pub error_summary: Option<ErrorSummary>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub resume_command: Option<String>,
    #[serde(default)]
    pub termination_reason: Option<String>,
    pub timestamp_ms: u128,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoopTraceEntry {
    pub id: String,
    pub task_id: String,
    pub loop_id: String,
    pub stage: String,
    pub entry_type: String,
    #[serde(default)]
    pub iteration: Option<u32>,
    #[serde(default)]
    pub attempt: Option<u32>,
    pub context_summary: String,
    pub action_summary: String,
    pub verification_summary: String,
    #[serde(default)]
    pub command_run_id: Option<String>,
    #[serde(default)]
    pub fingerprint: Option<String>,
    #[serde(default)]
    pub termination_reason: Option<String>,
    #[serde(default)]
    pub token_usage: Option<u64>,
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
    pub final_plan_html_path: Option<String>,
    #[serde(default)]
    pub discussion_summary: Option<String>,
    #[serde(default)]
    pub planning_runs: Vec<PlanningRun>,
    #[serde(default)]
    pub agent_invocations: Vec<AgentInvocation>,
    #[serde(default)]
    pub plan_reviews: Vec<PlanReview>,
    #[serde(default)]
    pub planning_decisions: Vec<PlanningDecision>,
    #[serde(default)]
    pub plan_todos: Vec<PlanTodoItem>,
    #[serde(default)]
    pub loop_trace: Vec<LoopTraceEntry>,
    pub events: Vec<TaskEvent>,
    pub command_runs: Vec<CommandRun>,
    pub feedback: Vec<UserFeedback>,
    #[serde(default)]
    pub loop_compact_summary: Option<String>,
    #[serde(default)]
    pub repair_context_preview: Option<String>,
    pub created_at_ms: u128,
    pub updated_at_ms: u128,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanReview {
    pub id: String,
    pub planning_run_id: String,
    pub task_id: String,
    pub reviewer_agent_id: String,
    pub reviewer_agent_name: String,
    pub target_agent_id: String,
    pub target_agent_name: String,
    pub status: String,
    pub finding: String,
    pub severity: String,
    pub accepted: bool,
    pub raw_output: String,
    pub evidence_ref: Option<String>,
    #[serde(default)]
    pub stderr_ref: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub resume_command: Option<String>,
    pub started_at_ms: u128,
    pub ended_at_ms: Option<u128>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanningDecision {
    pub id: String,
    pub task_id: String,
    pub title: String,
    pub content: String,
    pub status: String,
    pub created_at_ms: u128,
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

fn default_attempt() -> u32 {
    1
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
    pub plan_path: Option<String>,
    #[serde(default)]
    pub stderr_tail: Vec<String>,
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub timed_out: bool,
    #[serde(default = "default_attempt")]
    pub attempt: u32,
    #[serde(default)]
    pub failure_kind: Option<String>,
    #[serde(default)]
    pub failure_detail: Option<String>,
    #[serde(default)]
    pub error_lines: Vec<String>,
    #[serde(default)]
    pub stderr_ref: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub resume_command: Option<String>,
    pub started_at_ms: u128,
    pub ended_at_ms: Option<u128>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanningAgentStatusEvent {
    pub task_id: String,
    pub planning_run_id: String,
    pub agent_id: String,
    pub agent_name: String,
    pub phase: String,
    pub status: String,
    #[serde(default = "default_attempt")]
    pub attempt: u32,
    pub started_at_ms: u128,
    pub ended_at_ms: Option<u128>,
    pub elapsed_ms: Option<u128>,
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
    #[serde(default)]
    pub selected_planning_agent_ids: Vec<String>,
    #[serde(default)]
    pub primary_agent_id: Option<String>,
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
pub struct PlanningDecisionInput {
    pub project_path: String,
    pub task_id: String,
    pub title: String,
    pub content: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedbackInput {
    pub project_path: String,
    pub task_id: String,
    pub command_run_id: Option<String>,
    pub content: String,
}
