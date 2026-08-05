import type {
  AgentConfig,
  CommandFinishedEvent,
  CommandLogEvent,
  CommandRun,
  PlanningAgentLogEvent,
  PlanningAgentStatusEvent,
  ProjectSummary,
  Task,
} from "../domain";
import type { WorkflowStageId } from "./selectors";

export type AppView = "workspace" | "board" | "planning" | "task-detail" | "settings";

export interface AppSlice {
  currentView: AppView;
  activeProjectId: string | null;
  selectedTaskId: string | null;
  selectedTodoId: string | null;
  // Purely a UI review state. It is never persisted and never changes task.status.
  viewedStage: WorkflowStageId | null;
  isCreatingTask: boolean;
  activeCommandRunId: string | null;
  isLoadingProjects: boolean;
  isLoadingAgents: boolean;
  isLoadingTasks: boolean;
  projectError: string | null;
  agentError: string | null;
  taskError: string | null;
  commandError: string | null;
}

export interface ProjectsSlice {
  recent: ProjectSummary[];
  current: ProjectSummary | null;
}

export interface AppState {
  app: AppSlice;
  projects: ProjectsSlice;
  agents: AgentConfig[];
  tasks: Task[];
  taskCache: Record<string, Task[]>;
  commandRuns: CommandRun[];
  commandLogs: Record<string, CommandLogEvent[]>;
  planningLogs: Record<string, PlanningAgentLogEvent[]>;
  planningProgress: Record<string, PlanningAgentStatusEvent>;
}

export type AppAction =
  | { type: "app/viewSelected"; view: AppSlice["currentView"] }
  | { type: "app/stageViewed"; stage: WorkflowStageId | null }
  | { type: "projects/loadStarted" }
  | { type: "projects/loadFailed"; error: string }
  | { type: "projects/recentLoaded"; projects: ProjectSummary[] }
  | { type: "projects/registered"; project: ProjectSummary }
  | { type: "projects/selected"; projectId: string }
  | { type: "projects/removed"; projectId: string; projectPath?: string }
  | { type: "agents/loadStarted" }
  | { type: "agents/loadFailed"; error: string }
  | { type: "agents/loaded"; agents: AgentConfig[] }
  | { type: "agents/created"; agent: AgentConfig }
  | { type: "tasks/loadStarted" }
  | { type: "tasks/loadFailed"; error: string }
  | { type: "tasks/loaded"; projectPath: string; tasks: Task[] }
  | { type: "tasks/upserted"; task: Task }
  | { type: "tasks/selected"; taskId: string }
  | { type: "tasks/removed"; taskId: string; projectPath?: string }
  | { type: "tasks/new" }
  | { type: "tasks/newClosed" }
  | { type: "tasks/todoSelected"; taskId: string; todoId: string }
  | { type: "tasks/todoCompleted"; taskId: string; todoId: string }
  | { type: "commands/started"; run: CommandRun }
  | { type: "commands/logReceived"; event: CommandLogEvent }
  | { type: "commands/logHistoryLoaded"; runId: string; events: CommandLogEvent[] }
  | { type: "commands/finished"; event: CommandFinishedEvent }
  | { type: "commands/failed"; error: string }
  | { type: "commands/cleared" }
  | {
      type: "planning/progressQueued";
      taskId: string;
      agents: Array<{ id: string; name: string }>;
    }
  | { type: "planning/progressUpdated"; event: PlanningAgentStatusEvent }
  | { type: "planning/logReceived"; event: PlanningAgentLogEvent }
  | { type: "planning/progressCleared"; taskId: string };

export const initialAppState: AppState = {
  app: {
    currentView: "workspace",
    activeProjectId: null,
    selectedTaskId: null,
    selectedTodoId: null,
    viewedStage: null,
    isCreatingTask: false,
    activeCommandRunId: null,
    isLoadingProjects: false,
    isLoadingAgents: false,
    isLoadingTasks: false,
    projectError: null,
    agentError: null,
    taskError: null,
    commandError: null,
  },
  projects: { recent: [], current: null },
  agents: [],
  tasks: [],
  taskCache: {},
  commandRuns: [],
  commandLogs: {},
  planningLogs: {},
  planningProgress: {},
};
