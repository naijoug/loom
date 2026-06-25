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
  // Which workflow stage the user is reviewing. null = follow the task's real
  // stage (default). Purely a UI/view concern — never persisted, never changes
  // task.status. See selectors.WorkflowStageId.
  viewedStage: WorkflowStageId | null;
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

function normalizeAppView(view: AppView): AppView {
  return view;
}

export type AppAction =
  | { type: "app/viewSelected"; view: AppSlice["currentView"] }
  | { type: "app/stageViewed"; stage: WorkflowStageId | null }
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
  | { type: "tasks/loaded"; projectPath: string; tasks: Task[] }
  | { type: "tasks/upserted"; task: Task }
  | { type: "tasks/selected"; taskId: string }
  | { type: "tasks/removed"; taskId: string; projectPath?: string }
  | { type: "tasks/new" }
  | { type: "tasks/todoSelected"; taskId: string; todoId: string }
  | { type: "tasks/todoCompleted"; taskId: string; todoId: string }
  | { type: "commands/started"; run: CommandRun }
  | { type: "commands/logReceived"; event: CommandLogEvent }
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
  taskCache: {},
  commandRuns: [],
  commandLogs: {},
  planningLogs: {},
  planningProgress: {},
};

function planningProgressKey(event: PlanningAgentStatusEvent) {
  return `${event.planningRunId}:${event.phase}:${event.agentId}`;
}

export function planningLogKey(event: Pick<PlanningAgentLogEvent, "planningRunId" | "phase" | "agentId">) {
  return `${event.planningRunId}:${event.phase}:${event.agentId}`;
}

function removePendingPlanningProgress(
  progress: Record<string, PlanningAgentStatusEvent>,
  taskId: string,
) {
  return Object.fromEntries(
    Object.entries(progress).filter(
      ([, event]) => !(event.taskId === taskId && event.planningRunId === "pending"),
    ),
  );
}

function selectedTaskAfterLoad(tasks: Task[], currentTaskId: string | null) {
  return tasks.find((task) => task.id === currentTaskId) ?? tasks[tasks.length - 1] ?? null;
}

function selectedTodoIdForTask(task: Task | null, currentTodoId: string | null) {
  if (!task) {
    return null;
  }

  return task.planTodos.some((todo) => todo.id === currentTodoId)
    ? currentTodoId
    : task.planTodos[0]?.id ?? null;
}

function taskProjectPath(state: AppState, taskId: string) {
  const currentTask = state.tasks.find((task) => task.id === taskId);
  if (currentTask) {
    return currentTask.projectPath;
  }

  for (const tasks of Object.values(state.taskCache)) {
    const cachedTask = tasks.find((task) => task.id === taskId);
    if (cachedTask) {
      return cachedTask.projectPath;
    }
  }

  return null;
}

function upsertTask(tasks: Task[], task: Task) {
  return [...tasks.filter((candidate) => candidate.id !== task.id), task].sort(
    (left, right) => left.createdAtMs - right.createdAtMs,
  );
}

function replaceTask(tasks: Task[], taskId: string, update: (task: Task) => Task) {
  return tasks.map((task) => (task.id === taskId ? update(task) : task));
}

function removeTask(tasks: Task[], taskId: string) {
  return tasks.filter((task) => task.id !== taskId);
}

function cacheProjectTasks(
  cache: Record<string, Task[]>,
  projectPath: string,
  tasks: Task[],
) {
  return {
    ...cache,
    [projectPath]: tasks,
  };
}

export function appReducer(state: AppState, action: AppAction): AppState {
  switch (action.type) {
    case "app/viewSelected":
      return {
        ...state,
        app: {
          ...state.app,
          currentView: normalizeAppView(action.view),
        },
      };

    case "app/stageViewed":
      return {
        ...state,
        app: {
          ...state.app,
          viewedStage: action.stage,
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
          currentView: "board",
          selectedTaskId: null,
          selectedTodoId: null,
          viewedStage: null,
          isLoadingProjects: false,
          projectError: null,
        },
        projects: {
          current: action.project,
          recent,
        },
        tasks: [],
        taskCache: cacheProjectTasks(state.taskCache, action.project.path, []),
        commandRuns: [],
        commandLogs: {},
        planningLogs: {},
        planningProgress: {},
      };
    }

    case "projects/selected": {
      const project = state.projects.recent.find((candidate) => candidate.id === action.projectId);
      const tasks = project ? state.taskCache[project.path] ?? [] : [];

      return {
        ...state,
        app: {
          ...state.app,
          activeProjectId: action.projectId,
          currentView: "board",
          selectedTaskId: null,
          selectedTodoId: null,
          viewedStage: null,
          projectError: null,
        },
        projects: {
          ...state.projects,
          current: project ?? state.projects.current,
        },
        tasks,
        commandRuns: tasks.flatMap((task) => task.commandRuns),
        commandLogs: {},
        planningLogs: {},
        planningProgress: {},
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

    case "tasks/loaded": {
      const taskCache = cacheProjectTasks(state.taskCache, action.projectPath, action.tasks);
      const loadingCurrentProject =
        !state.projects.current || state.projects.current.path === action.projectPath;

      if (!loadingCurrentProject) {
        return {
          ...state,
          app: {
            ...state.app,
            isLoadingTasks: false,
            taskError: null,
          },
          taskCache,
        };
      }

      const selectedTask = selectedTaskAfterLoad(action.tasks, state.app.selectedTaskId);

      return {
        ...state,
        app: {
          ...state.app,
          isLoadingTasks: false,
          taskError: null,
          selectedTaskId: selectedTask?.id ?? null,
          selectedTodoId: selectedTodoIdForTask(selectedTask, state.app.selectedTodoId),
          // Keep the review state only while the selected task is unchanged; a
          // reload that lands on a different task drops any stale review.
          viewedStage:
            selectedTask?.id === state.app.selectedTaskId ? state.app.viewedStage : null,
        },
        tasks: action.tasks,
        taskCache,
        commandRuns: action.tasks.flatMap((task) => task.commandRuns),
      };
    }

    case "tasks/upserted": {
      const isCurrentProject = state.projects.current?.path === action.task.projectPath;
      const sourceTasks = isCurrentProject
        ? state.tasks
        : state.taskCache[action.task.projectPath] ?? [];
      const projectTasks = upsertTask(sourceTasks, action.task);
      // A live update to the task already under review must not interrupt the
      // review; only an upsert that switches the selected task resets it.
      const sameSelectedTask = state.app.selectedTaskId === action.task.id;

      if (!isCurrentProject && state.projects.current) {
        return {
          ...state,
          app: {
            ...state.app,
            isLoadingTasks: false,
            taskError: null,
          },
          taskCache: cacheProjectTasks(state.taskCache, action.task.projectPath, projectTasks),
          planningProgress: removePendingPlanningProgress(state.planningProgress, action.task.id),
        };
      }

      return {
        ...state,
        app: {
          ...state.app,
          isLoadingTasks: false,
          taskError: null,
          selectedTaskId: action.task.id,
          selectedTodoId: selectedTodoIdForTask(action.task, state.app.selectedTodoId),
          viewedStage: sameSelectedTask ? state.app.viewedStage : null,
        },
        tasks: projectTasks,
        taskCache: cacheProjectTasks(state.taskCache, action.task.projectPath, projectTasks),
        commandRuns: projectTasks.flatMap((task) => task.commandRuns),
        planningProgress: removePendingPlanningProgress(state.planningProgress, action.task.id),
      };
    }

    case "tasks/selected": {
      const task = state.tasks.find((candidate) => candidate.id === action.taskId) ?? null;

      return {
        ...state,
        app: {
          ...state.app,
          currentView: "task-detail",
          selectedTaskId: action.taskId,
          selectedTodoId: selectedTodoIdForTask(task, state.app.selectedTodoId),
          viewedStage: null,
        },
      };
    }

    case "tasks/new":
      return {
        ...state,
        app: {
          ...state.app,
          currentView: "planning",
          selectedTaskId: null,
          selectedTodoId: null,
          viewedStage: null,
        },
      };

    case "tasks/removed": {
      const projectPath = action.projectPath ?? taskProjectPath(state, action.taskId);
      const currentProjectPath = state.projects.current?.path ?? null;
      const removesCurrentProject = !currentProjectPath || projectPath === currentProjectPath;
      const sourceTasks =
        projectPath && !removesCurrentProject ? state.taskCache[projectPath] ?? [] : state.tasks;
      const projectTasks = removeTask(sourceTasks, action.taskId);
      const tasks = removesCurrentProject ? projectTasks : state.tasks;
      const wasSelected = state.app.selectedTaskId === action.taskId;

      return {
        ...state,
        app: {
          ...state.app,
          selectedTaskId: wasSelected ? null : state.app.selectedTaskId,
          selectedTodoId: wasSelected ? null : state.app.selectedTodoId,
          viewedStage: wasSelected ? null : state.app.viewedStage,
          currentView: wasSelected ? "board" : state.app.currentView,
        },
        tasks,
        taskCache: projectPath
          ? cacheProjectTasks(state.taskCache, projectPath, projectTasks)
          : state.taskCache,
        commandRuns: state.commandRuns.filter((run) => run.taskId !== action.taskId),
      };
    }

    case "tasks/todoSelected": {
      const tasks = replaceTask(state.tasks, action.taskId, (task) => ({
        ...task,
        planTodos: task.planTodos.map((todo) =>
          todo.id === action.todoId
            ? { ...todo, status: "implementing" }
            : todo.status === "implementing"
              ? { ...todo, status: "pending" }
              : todo,
        ),
      }));
      const projectPath = state.projects.current?.path;

      return {
        ...state,
        app: {
          ...state.app,
          selectedTodoId: action.todoId,
        },
        tasks,
        taskCache: projectPath ? cacheProjectTasks(state.taskCache, projectPath, tasks) : state.taskCache,
      };
    }

    case "tasks/todoCompleted": {
      const tasks = replaceTask(state.tasks, action.taskId, (task) => {
        const planTodos = task.planTodos.map((todo) =>
          todo.id === action.todoId ? { ...todo, status: "done" as const } : todo,
        );

        return {
          ...task,
          status: planTodos.every((todo) => todo.status === "done") ? "reviewing" : task.status,
          planTodos,
        };
      });
      const projectPath = state.projects.current?.path;

      return {
        ...state,
        app: {
          ...state.app,
          selectedTodoId: action.todoId,
        },
        tasks,
        taskCache: projectPath ? cacheProjectTasks(state.taskCache, projectPath, tasks) : state.taskCache,
      };
    }

    case "commands/started":
      return {
        ...state,
        app: {
          ...state.app,
          activeCommandRunId: action.run.id,
          commandError: null,
        },
        commandRuns: [...state.commandRuns.filter((run) => run.id !== action.run.id), action.run],
        commandLogs: {
          ...state.commandLogs,
          [action.run.id]: state.commandLogs[action.run.id] ?? [],
        },
      };

    case "commands/logReceived": {
      const runLogs = state.commandLogs[action.event.runId] ?? [];

      return {
        ...state,
        commandLogs: {
          ...state.commandLogs,
          [action.event.runId]: [...runLogs.slice(-299), action.event],
        },
      };
    }

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
                errorSummary: action.event.errorSummary,
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
        commandLogs: {},
      };

    case "planning/progressQueued": {
      const existing = Object.fromEntries(
        Object.entries(state.planningProgress).filter(
          ([, event]) => event.taskId !== action.taskId,
        ),
      );
      const existingLogs = Object.fromEntries(
        Object.entries(state.planningLogs).filter(
          ([, logs]) => logs[0]?.taskId !== action.taskId,
        ),
      );
      const queued = Object.fromEntries(
        action.agents.map((agent) => [
          `pending:planning:${agent.id}`,
          {
            taskId: action.taskId,
            planningRunId: "pending",
            agentId: agent.id,
            agentName: agent.name,
            phase: "planning" as const,
            status: "pending" as const,
            attempt: 1,
            startedAtMs: Date.now(),
          },
        ]),
      );

      return {
        ...state,
        planningLogs: existingLogs,
        planningProgress: {
          ...existing,
          ...queued,
        },
      };
    }

    case "planning/progressUpdated": {
      const progress =
        action.event.status === "running"
          ? removePendingPlanningProgress(state.planningProgress, action.event.taskId)
          : state.planningProgress;

      return {
        ...state,
        planningProgress: {
          ...progress,
          [planningProgressKey(action.event)]: action.event,
        },
      };
    }

    case "planning/logReceived": {
      const key = planningLogKey(action.event);
      const runLogs = state.planningLogs[key] ?? [];

      return {
        ...state,
        planningLogs: {
          ...state.planningLogs,
          [key]: [...runLogs, action.event].slice(-300),
        },
      };
    }

    case "planning/progressCleared":
      return {
        ...state,
        planningLogs: Object.fromEntries(
          Object.entries(state.planningLogs).filter(
            ([, logs]) => logs[0]?.taskId !== action.taskId,
          ),
        ),
        planningProgress: Object.fromEntries(
          Object.entries(state.planningProgress).filter(
            ([, event]) => event.taskId !== action.taskId,
          ),
        ),
      };

    default:
      return state;
  }
}
