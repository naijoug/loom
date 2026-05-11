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
  stderrTail: string[];
  exitCode?: number;
  timedOut: boolean;
  startedAtMs: number;
  endedAtMs?: number;
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
  discussionSummary?: string;
  planningRuns: PlanningRun[];
  agentInvocations: AgentInvocation[];
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
}

export interface PlanningDiscussionInput {
  projectPath: string;
  taskId: string;
  requirement: string;
  agentIds: string[];
}
