export type TaskStatus =
  | "drafting_requirements"
  | "planning"
  | "plan_review"
  | "ready_to_implement"
  | "implementing"
  | "reviewing"
  | "debugging"
  | "fixing"
  | "verifying"
  | "completed"
  | "blocked"
  | "cancelled";

export interface TaskEvent {
  id: string;
  taskId: string;
  timestampMs: number;
  actor: "user" | "system" | "agent";
  status: TaskStatus;
  inputSummary?: string;
  outputSummary?: string;
  evidenceRef?: string;
}

export interface UserFeedback {
  id: string;
  taskId: string;
  commandRunId?: string;
  content: string;
  reproductionSteps?: string;
  expectedBehavior?: string;
  quotedLog?: string;
  attachments: FeedbackAttachment[];
  timestampMs: number;
}

export interface FeedbackAttachment {
  id: string;
  name: string;
  mimeType: string;
  sizeBytes: number;
  storedPath: string;
  createdAtMs: number;
}

export interface StructuredFeedbackInput {
  content: string;
  reproductionSteps?: string;
  expectedBehavior?: string;
  quotedLog?: string;
  attachmentPaths?: string[];
}

export interface GitBaselineFile {
  path: string;
  status: string;
}

export interface GitBaseline {
  capturedAtMs: number;
  available: boolean;
  head?: string;
  files: GitBaselineFile[];
  failureDetail?: string;
}

export interface ChangedFileSummary {
  path: string;
  status: string;
  additions?: number;
  deletions?: number;
  attribution: "pre_existing" | "task_introduced" | "unknown" | string;
}

export interface TaskSummaryDecision {
  kind: string;
  title: string;
  detail: string;
}

export interface TaskSummaryReview {
  reviewer: string;
  status: string;
  summary: string;
  findingCount: number;
  evidenceRef?: string;
}

export interface TaskSummaryValidation {
  runId: string;
  command: string;
  status: import("./command").CommandRunStatus;
  exitCode?: number;
  stdoutLogRef?: string;
  stderrLogRef?: string;
  startedAtMs: number;
  endedAtMs?: number;
}

export interface TaskSummary {
  taskId: string;
  title: string;
  requirement: string;
  completedTodos: string[];
  changedFiles: ChangedFileSummary[];
  totalAdditions: number;
  totalDeletions: number;
  decisions: TaskSummaryDecision[];
  reviews: TaskSummaryReview[];
  validationEvidence: TaskSummaryValidation[];
  remainingRisks: string[];
  recommendations: string[];
  generatedAtMs: number;
  jsonPath: string;
  markdownPath: string;
}

export type AgentInvocationStatus = "pending" | "running" | "succeeded" | "failed";

export type PlanningFailureKind =
  | "timeout"
  | "empty_output"
  | "nonzero_exit"
  | "not_retryable";

export interface AgentInvocation {
  id: string;
  planningRunId: string;
  taskId: string;
  agentId: string;
  agentName: string;
  status: AgentInvocationStatus;
  promptSummary: string;
  rawOutput: string;
  outputSummary: string;
  evidenceRef?: string;
  planPath?: string;
  stderrTail: string[];
  exitCode?: number;
  timedOut: boolean;
  attempt: number;
  failureKind?: PlanningFailureKind;
  failureDetail?: string;
  errorLines?: string[];
  stderrRef?: string;
  sessionId?: string;
  resumeCommand?: string;
  startedAtMs: number;
  endedAtMs?: number;
}

export type PlanningAgentPhase = "planning" | "review" | "synthesis";

export type PlanningAgentRuntimeStatus = AgentInvocationStatus | "retrying";

export interface PlanningAgentStatusEvent {
  taskId: string;
  planningRunId: string;
  agentId: string;
  agentName: string;
  phase: PlanningAgentPhase;
  status: PlanningAgentRuntimeStatus;
  attempt: number;
  startedAtMs: number;
  endedAtMs?: number;
  elapsedMs?: number;
}

export interface PlanningAgentLogEvent {
  taskId: string;
  planningRunId: string;
  agentId: string;
  agentName: string;
  phase: PlanningAgentPhase;
  attempt: number;
  stream: "stdout" | "stderr";
  lines: string[];
  timestampMs: number;
}

export interface PlanReview {
  id: string;
  planningRunId: string;
  taskId: string;
  reviewerAgentId: string;
  reviewerAgentName: string;
  targetAgentId: string;
  targetAgentName: string;
  status: AgentInvocationStatus;
  finding: string;
  severity: "info" | "risk" | "blocker";
  accepted: boolean;
  rawOutput: string;
  evidenceRef?: string;
  stderrRef?: string;
  sessionId?: string;
  resumeCommand?: string;
  startedAtMs: number;
  endedAtMs?: number;
}

export interface PlanningDecision {
  id: string;
  taskId: string;
  title: string;
  content: string;
  status: "open" | "accepted";
  createdAtMs: number;
}

export interface PlanningRun {
  id: string;
  taskId: string;
  requirement: string;
  selectedAgentIds: string[];
  status: "running" | "succeeded" | "failed";
  summary: string;
  startedAtMs: number;
  endedAtMs?: number;
}

export type PlanTodoStatus = "pending" | "implementing" | "done" | "blocked";

export interface PlanTodoItem {
  id: string;
  taskId: string;
  title: string;
  description: string;
  status: PlanTodoStatus;
  order: number;
  planRef?: string;
}

export interface LoopTraceEntry {
  id: string;
  taskId: string;
  loopId: string;
  stage: string;
  entryType: string;
  iteration?: number;
  attempt?: number;
  contextSummary: string;
  actionSummary: string;
  verificationSummary: string;
  commandRunId?: string;
  fingerprint?: string;
  terminationReason?: string;
  tokenUsage?: number;
  timestampMs: number;
}

export interface Task {
  id: string;
  projectPath: string;
  title: string;
  rawRequirement: string;
  status: TaskStatus;
  lifecycle?: {
    paused: boolean;
    resumeStatus?: TaskStatus;
    pauseReason?: string;
    blockedReason?: string;
    cancelledReason?: string;
  };
  selectedPlanningAgentIds: string[];
  primaryAgentId?: string;
  reviewAgentIds: string[];
  implementationReviewRuns: ImplementationReviewRun[];
  implementationReviews: ImplementationReview[];
  implementationReviewDecisions: ImplementationReviewDecision[];
  finalPlan?: string;
  finalPlanPath?: string;
  finalPlanHtmlPath?: string;
  discussionSummary?: string;
  planningRuns: PlanningRun[];
  agentInvocations: AgentInvocation[];
  planReviews: PlanReview[];
  planningDecisions: PlanningDecision[];
  planTodos: PlanTodoItem[];
  loopTrace?: LoopTraceEntry[];
  events: TaskEvent[];
  commandRuns: import("./command").CommandRun[];
  feedback: UserFeedback[];
  loopCompactSummary?: string;
  repairContextPreview?: string;
  gitBaseline?: GitBaseline;
  summary?: TaskSummary;
  createdAtMs: number;
  updatedAtMs: number;
}

export type ImplementationReviewFindingSeverity = "blocker" | "risk" | "suggestion" | "info";
export type ImplementationReviewFindingStatus =
  | "open"
  | "pending_re_review"
  | "accepted_risk"
  | "dismissed"
  | "superseded";

export interface ImplementationReviewFinding {
  id: string;
  reviewId: string;
  severity: ImplementationReviewFindingSeverity;
  title: string;
  detail: string;
  file?: string;
  line?: number;
  status: ImplementationReviewFindingStatus;
  createdAtMs: number;
}

export interface ImplementationReview {
  id: string;
  runId: string;
  taskId: string;
  reviewerAgentId: string;
  reviewerAgentName: string;
  status: "running" | "succeeded" | "failed" | "interrupted";
  summary: string;
  rawOutput: string;
  evidenceRef?: string;
  stderrRef?: string;
  exitCode?: number;
  failureDetail?: string;
  findings: ImplementationReviewFinding[];
  startedAtMs: number;
  endedAtMs?: number;
}

export interface ImplementationReviewRun {
  id: string;
  taskId: string;
  reviewerAgentIds: string[];
  status: "running" | "succeeded" | "failed" | "interrupted";
  contextRef: string;
  reviewIds: string[];
  startedAtMs: number;
  endedAtMs?: number;
}

export interface ImplementationReviewDecision {
  id: string;
  findingId: string;
  decision: "resolved" | "accepted_risk" | "dismissed";
  reason: string;
  actor: string;
  createdAtMs: number;
}

export interface CreateTaskInput {
  projectPath: string;
  title: string;
  rawRequirement: string;
  selectedPlanningAgentIds?: string[];
  primaryAgentId?: string;
}

export interface PlanningDiscussionInput {
  projectPath: string;
  taskId: string;
  requirement: string;
  agentIds: string[];
}

export interface PlanningDecisionInput {
  projectPath: string;
  taskId: string;
  title: string;
  content: string;
}

export interface ContextBuildOptions {
  maxPromptChars?: number;
}

export interface ContextBuildOutput {
  prompt: string;
  contextSummary: string;
  compactSummary: string;
  includedSections: string[];
  truncated: boolean;
}
