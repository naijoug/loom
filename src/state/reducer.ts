import type {
  AgentConfig,
  CommandFinishedEvent,
  CommandLogEvent,
  CommandRun,
  ProjectSummary,
  Task,
} from "../domain";

export interface AppSlice {
  currentView: "workspace" | "settings";
  activeProjectId: string | null;
  selectedTaskId: string | null;
  selectedTodoId: string | null;
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
  commandRuns: CommandRun[];
  commandLogs: CommandLogEvent[];
}

export type AppAction =
  | { type: "app/viewSelected"; view: AppSlice["currentView"] }
  | { type: "projects/loadStarted" }
  | { type: "projects/loadFailed"; error: string }
  | { type: "projects/recentLoaded"; projects: ProjectSummary[] }
  | { type: "projects/registered"; project: ProjectSummary }
  | { type: "projects/selected"; projectId: string }
  | { type: "agents/loadStarted" }
  | { type: "agents/loadFailed"; error: string }
  | { type: "agents/loaded"; agents: AgentConfig[] }
  | { type: "agents/created"; agent: AgentConfig }
  | { type: "tasks/loadStarted" }
  | { type: "tasks/loadFailed"; error: string }
  | { type: "tasks/loaded"; tasks: Task[] }
  | { type: "tasks/upserted"; task: Task }
  | { type: "tasks/todoSelected"; todoId: string }
  | { type: "commands/started"; run: CommandRun }
  | { type: "commands/logReceived"; event: CommandLogEvent }
  | { type: "commands/finished"; event: CommandFinishedEvent }
  | { type: "commands/failed"; error: string }
  | { type: "commands/cleared" };

export const initialAppState: AppState = {
  app: {
    currentView: "workspace",
    activeProjectId: null,
    selectedTaskId: null,
    selectedTodoId: null,
    activeCommandRunId: null,
    isLoadingProjects: false,
    isLoadingAgents: false,
    isLoadingTasks: false,
    projectError: null,
    agentError: null,
    taskError: null,
    commandError: null,
  },
  projects: {
    recent: [],
    current: null,
  },
  agents: [],
  tasks: [],
  commandRuns: [],
  commandLogs: [],
};

export function appReducer(state: AppState, action: AppAction): AppState {
  switch (action.type) {
    case "app/viewSelected":
      return {
        ...state,
        app: {
          ...state.app,
          currentView: action.view,
        },
      };

    case "projects/loadStarted":
      return {
        ...state,
        app: {
          ...state.app,
          isLoadingProjects: true,
          projectError: null,
        },
      };

    case "projects/loadFailed":
      return {
        ...state,
        app: {
          ...state.app,
          isLoadingProjects: false,
          projectError: action.error,
        },
      };

    case "projects/recentLoaded":
      return {
        ...state,
        app: {
          ...state.app,
          isLoadingProjects: false,
          projectError: null,
        },
        projects: {
          ...state.projects,
          recent: action.projects,
        },
      };

    case "projects/registered": {
      const recent = [
        action.project,
        ...state.projects.recent.filter((project) => project.path !== action.project.path),
      ].slice(0, 10);

      return {
        ...state,
        app: {
          ...state.app,
          activeProjectId: action.project.id,
          currentView: "workspace",
          selectedTaskId: null,
          selectedTodoId: null,
          isLoadingProjects: false,
          projectError: null,
        },
        projects: {
          current: action.project,
          recent,
        },
        tasks: [],
        commandRuns: [],
        commandLogs: [],
      };
    }

    case "projects/selected": {
      const project = state.projects.recent.find((candidate) => candidate.id === action.projectId);

      return {
        ...state,
        app: {
          ...state.app,
          activeProjectId: action.projectId,
          selectedTaskId: null,
          selectedTodoId: null,
          projectError: null,
        },
        projects: {
          ...state.projects,
          current: project ?? state.projects.current,
        },
        tasks: [],
        commandRuns: [],
        commandLogs: [],
      };
    }

    case "agents/loadStarted":
      return {
        ...state,
        app: { ...state.app, isLoadingAgents: true, agentError: null },
      };

    case "agents/loadFailed":
      return {
        ...state,
        app: { ...state.app, isLoadingAgents: false, agentError: action.error },
      };

    case "agents/loaded":
      return {
        ...state,
        app: { ...state.app, isLoadingAgents: false, agentError: null },
        agents: action.agents,
      };

    case "agents/created":
      return {
        ...state,
        app: { ...state.app, isLoadingAgents: false, agentError: null },
        agents: [...state.agents.filter((agent) => agent.id !== action.agent.id), action.agent],
      };

    case "tasks/loadStarted":
      return {
        ...state,
        app: { ...state.app, isLoadingTasks: true, taskError: null },
      };

    case "tasks/loadFailed":
      return {
        ...state,
        app: { ...state.app, isLoadingTasks: false, taskError: action.error },
      };

    case "tasks/loaded":
      return {
        ...state,
        app: {
          ...state.app,
          isLoadingTasks: false,
          taskError: null,
          selectedTaskId: action.tasks[action.tasks.length - 1]?.id ?? state.app.selectedTaskId,
          selectedTodoId:
            action.tasks[action.tasks.length - 1]?.planTodos?.[0]?.id ?? state.app.selectedTodoId,
        },
        tasks: action.tasks,
        commandRuns: action.tasks.flatMap((task) => task.commandRuns),
      };

    case "tasks/upserted": {
      const tasks = [
        ...state.tasks.filter((task) => task.id !== action.task.id),
        action.task,
      ].sort((left, right) => left.createdAtMs - right.createdAtMs);

      return {
        ...state,
        app: {
          ...state.app,
          isLoadingTasks: false,
          taskError: null,
          selectedTaskId: action.task.id,
          selectedTodoId: action.task.planTodos[0]?.id ?? state.app.selectedTodoId,
        },
        tasks,
        commandRuns: tasks.flatMap((task) => task.commandRuns),
      };
    }

    case "tasks/todoSelected":
      return {
        ...state,
        app: {
          ...state.app,
          selectedTodoId: action.todoId,
        },
        tasks: state.tasks.map((task) => ({
          ...task,
          planTodos: task.planTodos.map((todo) =>
            todo.id === action.todoId
              ? { ...todo, status: "implementing" }
              : todo.status === "implementing"
                ? { ...todo, status: "pending" }
                : todo,
          ),
        })),
      };

    case "commands/started":
      return {
        ...state,
        app: {
          ...state.app,
          activeCommandRunId: action.run.id,
          commandError: null,
        },
        commandRuns: [...state.commandRuns.filter((run) => run.id !== action.run.id), action.run],
      };

    case "commands/logReceived":
      return {
        ...state,
        commandLogs: [...state.commandLogs.slice(-299), action.event],
      };

    case "commands/finished":
      return {
        ...state,
        app: {
          ...state.app,
          activeCommandRunId:
            state.app.activeCommandRunId === action.event.runId ? null : state.app.activeCommandRunId,
        },
        commandRuns: state.commandRuns.map((run) =>
          run.id === action.event.runId
            ? {
                ...run,
                status: action.event.status,
                exitCode: action.event.exitCode,
                endedAtMs: action.event.timestampMs,
              }
            : run,
        ),
      };

    case "commands/failed":
      return {
        ...state,
        app: { ...state.app, commandError: action.error },
      };

    case "commands/cleared":
      return {
        ...state,
        commandLogs: [],
      };

    default:
      return state;
  }
}
