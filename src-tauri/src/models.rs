use serde::{Deserialize, Serialize};
use std::{
    fmt,
    str::FromStr,
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    #[default]
    DraftingRequirements,
    Planning,
    PlanReview,
    ReadyToImplement,
    Implementing,
    Reviewing,
    Debugging,
    Fixing,
    Verifying,
    Completed,
    Blocked,
    Cancelled,
}

impl TaskStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DraftingRequirements => "drafting_requirements",
            Self::Planning => "planning",
            Self::PlanReview => "plan_review",
            Self::ReadyToImplement => "ready_to_implement",
            Self::Implementing => "implementing",
            Self::Reviewing => "reviewing",
            Self::Debugging => "debugging",
            Self::Fixing => "fixing",
            Self::Verifying => "verifying",
            Self::Completed => "completed",
            Self::Blocked => "blocked",
            Self::Cancelled => "cancelled",
        }
    }

    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled)
    }
}

impl fmt::Display for TaskStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for TaskStatus {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "drafting_requirements" => Ok(Self::DraftingRequirements),
            "planning" => Ok(Self::Planning),
            "plan_review" => Ok(Self::PlanReview),
            "ready_to_implement" => Ok(Self::ReadyToImplement),
            "implementing" => Ok(Self::Implementing),
            "reviewing" => Ok(Self::Reviewing),
            "debugging" => Ok(Self::Debugging),
            "fixing" => Ok(Self::Fixing),
            "verifying" => Ok(Self::Verifying),
            "completed" => Ok(Self::Completed),
            "blocked" => Ok(Self::Blocked),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(format!("unknown task status: {value}")),
        }
    }
}

impl PartialEq<&str> for TaskStatus {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandRunStatus {
    #[default]
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Interrupted,
}

impl CommandRunStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Interrupted => "interrupted",
        }
    }

    pub const fn is_finished(self) -> bool {
        !matches!(self, Self::Running)
    }
}

impl fmt::Display for CommandRunStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl PartialEq<&str> for CommandRunStatus {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl FromStr for CommandRunStatus {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            "interrupted" => Ok(Self::Interrupted),
            _ => Err(format!("unknown command run status: {value}")),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanTodoStatus {
    #[default]
    Pending,
    Implementing,
    Done,
    Blocked,
}

impl PlanTodoStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Implementing => "implementing",
            Self::Done => "done",
            Self::Blocked => "blocked",
        }
    }
}

impl fmt::Display for PlanTodoStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl PartialEq<&str> for PlanTodoStatus {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl FromStr for PlanTodoStatus {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "pending" => Ok(Self::Pending),
            "implementing" => Ok(Self::Implementing),
            "done" => Ok(Self::Done),
            "blocked" => Ok(Self::Blocked),
            _ => Err(format!("unknown plan todo status: {value}")),
        }
    }
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
    #[serde(default)]
    pub project_path: Option<String>,
    pub task_id: Option<String>,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub approval_id: Option<String>,
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

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandRunIntent {
    AgentAction,
    Validation,
    Preview,
    LoopStep,
    #[default]
    Legacy,
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
    pub status: CommandRunStatus,
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
    pub status: CommandRunStatus,
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

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorSummary {
    pub exit_code: Option<i32>,
    pub stderr_tail: Vec<String>,
    pub matched_lines: Vec<String>,
    #[serde(default)]
    pub urls: Vec<String>,
    #[serde(default)]
    pub ports: Vec<u16>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub test_failures: Vec<String>,
    #[serde(default)]
    pub stack_trace_lines: Vec<String>,
    pub failed: bool,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskLifecycle {
    #[serde(default)]
    pub paused: bool,
    #[serde(default)]
    pub resume_status: Option<TaskStatus>,
    #[serde(default)]
    pub pause_reason: Option<String>,
    #[serde(default)]
    pub blocked_reason: Option<String>,
    #[serde(default)]
    pub cancelled_reason: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub project_path: String,
    pub title: String,
    pub raw_requirement: String,
    pub status: TaskStatus,
    #[serde(default)]
    pub lifecycle: TaskLifecycle,
    pub selected_planning_agent_ids: Vec<String>,
    pub primary_agent_id: Option<String>,
    pub review_agent_ids: Vec<String>,
    #[serde(default)]
    pub implementation_review_runs: Vec<ImplementationReviewRun>,
    #[serde(default)]
    pub implementation_reviews: Vec<ImplementationReview>,
    #[serde(default)]
    pub implementation_review_decisions: Vec<ImplementationReviewDecision>,
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
    #[serde(default)]
    pub git_baseline: Option<GitBaseline>,
    #[serde(default)]
    pub summary: Option<TaskSummary>,
    pub created_at_ms: u128,
    pub updated_at_ms: u128,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitBaseline {
    pub captured_at_ms: u128,
    pub available: bool,
    pub head: Option<String>,
    pub files: Vec<GitBaselineFile>,
    pub failure_detail: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitBaselineFile {
    pub path: String,
    pub status: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangedFileSummary {
    pub path: String,
    pub status: String,
    pub additions: Option<u64>,
    pub deletions: Option<u64>,
    pub attribution: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskSummaryDecision {
    pub kind: String,
    pub title: String,
    pub detail: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskSummaryReview {
    pub reviewer: String,
    pub status: String,
    pub summary: String,
    pub finding_count: usize,
    pub evidence_ref: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskSummaryValidation {
    pub run_id: String,
    pub command: String,
    pub status: CommandRunStatus,
    pub exit_code: Option<i32>,
    pub stdout_log_ref: Option<String>,
    pub stderr_log_ref: Option<String>,
    pub started_at_ms: u128,
    pub ended_at_ms: Option<u128>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskSummary {
    pub task_id: String,
    pub title: String,
    pub requirement: String,
    pub completed_todos: Vec<String>,
    pub changed_files: Vec<ChangedFileSummary>,
    pub total_additions: u64,
    pub total_deletions: u64,
    pub decisions: Vec<TaskSummaryDecision>,
    pub reviews: Vec<TaskSummaryReview>,
    pub validation_evidence: Vec<TaskSummaryValidation>,
    pub remaining_risks: Vec<String>,
    pub recommendations: Vec<String>,
    pub generated_at_ms: u128,
    pub json_path: String,
    pub markdown_path: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImplementationReviewRun {
    pub id: String,
    pub task_id: String,
    pub reviewer_agent_ids: Vec<String>,
    pub status: String,
    pub context_ref: String,
    pub review_ids: Vec<String>,
    pub started_at_ms: u128,
    pub ended_at_ms: Option<u128>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImplementationReview {
    pub id: String,
    pub run_id: String,
    pub task_id: String,
    pub reviewer_agent_id: String,
    pub reviewer_agent_name: String,
    pub status: String,
    pub summary: String,
    pub raw_output: String,
    pub evidence_ref: Option<String>,
    pub stderr_ref: Option<String>,
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub failure_detail: Option<String>,
    pub findings: Vec<ImplementationReviewFinding>,
    pub started_at_ms: u128,
    pub ended_at_ms: Option<u128>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImplementationReviewFinding {
    pub id: String,
    pub review_id: String,
    pub severity: String,
    pub title: String,
    pub detail: String,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub status: String,
    pub created_at_ms: u128,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImplementationReviewDecision {
    pub id: String,
    pub finding_id: String,
    pub decision: String,
    pub reason: String,
    pub actor: String,
    pub created_at_ms: u128,
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
    pub status: PlanTodoStatus,
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
    pub status: TaskStatus,
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
    #[serde(default)]
    pub reproduction_steps: Option<String>,
    #[serde(default)]
    pub expected_behavior: Option<String>,
    #[serde(default)]
    pub quoted_log: Option<String>,
    #[serde(default)]
    pub attachments: Vec<FeedbackAttachment>,
    pub timestamp_ms: u128,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedbackAttachment {
    pub id: String,
    pub name: String,
    pub mime_type: String,
    pub size_bytes: u64,
    pub stored_path: String,
    pub created_at_ms: u128,
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
    #[serde(default)]
    pub reproduction_steps: Option<String>,
    #[serde(default)]
    pub expected_behavior: Option<String>,
    #[serde(default)]
    pub quoted_log: Option<String>,
    #[serde(default)]
    pub attachment_paths: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskLifecycleInput {
    pub project_path: String,
    pub task_id: String,
    #[serde(default)]
    pub reason: Option<String>,
}
