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
  timestamp: string;
  actor: "user" | "system" | "agent";
  status: TaskStatus;
  inputSummary?: string;
  outputSummary?: string;
  evidenceRef?: string;
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
}
