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
  timestampMs: number;
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

export interface Task {
  id: string;
  projectPath: string;
  title: string;
  rawRequirement: string;
  status: TaskStatus;
  selectedPlanningAgentIds: string[];
  primaryAgentId?: string;
  reviewAgentIds: string[];
  finalPlan?: string;
  finalPlanPath?: string;
  finalPlanHtmlPath?: string;
  discussionSummary?: string;
  planningRuns: PlanningRun[];
  agentInvocations: AgentInvocation[];
  planReviews: PlanReview[];
  planningDecisions: PlanningDecision[];
  planTodos: PlanTodoItem[];
  events: TaskEvent[];
  commandRuns: import("./command").CommandRun[];
  feedback: UserFeedback[];
  repairContextPreview?: string;
  createdAtMs: number;
  updatedAtMs: number;
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
