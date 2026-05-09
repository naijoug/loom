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
