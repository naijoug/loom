import type { AppAction, AppState } from "../model";
import {
  cacheProjectTasks,
  removeTask,
  replaceTask,
  selectedTaskAfterLoad,
  selectedTodoIdForTask,
  taskProjectPath,
  upsertTask,
} from "./taskCollections";

function removePendingPlanningProgress(state: AppState, taskId: string) {
  return Object.fromEntries(
    Object.entries(state.planningProgress).filter(
      ([, event]) => !(event.taskId === taskId && event.planningRunId === "pending"),
    ),
  );
}

export function reduceTasks(state: AppState, action: AppAction): AppState {
  switch (action.type) {
    case "tasks/loadStarted":
      return { ...state, app: { ...state.app, isLoadingTasks: true, taskError: null } };
    case "tasks/loadFailed":
      return { ...state, app: { ...state.app, isLoadingTasks: false, taskError: action.error } };
    case "tasks/loaded": {
      const taskCache = cacheProjectTasks(state.taskCache, action.projectPath, action.tasks);
      const loadingCurrentProject =
        !state.projects.current || state.projects.current.path === action.projectPath;
      if (!loadingCurrentProject) {
        return {
          ...state,
          app: { ...state.app, isLoadingTasks: false, taskError: null },
          taskCache,
        };
      }
      const selectedTask = state.app.isCreatingTask
        ? null
        : selectedTaskAfterLoad(action.tasks, state.app.selectedTaskId);
      return {
        ...state,
        app: {
          ...state.app,
          isLoadingTasks: false,
          taskError: null,
          selectedTaskId: selectedTask?.id ?? null,
          selectedTodoId: selectedTodoIdForTask(selectedTask, state.app.selectedTodoId),
          viewedStage: selectedTask?.id === state.app.selectedTaskId ? state.app.viewedStage : null,
        },
        tasks: action.tasks,
        taskCache,
        commandRuns: action.tasks.flatMap((task) => task.commandRuns),
      };
    }
    case "tasks/upserted": {
      const isCurrentProject = state.projects.current?.path === action.task.projectPath;
      const sourceTasks = isCurrentProject ? state.tasks : state.taskCache[action.task.projectPath] ?? [];
      const projectTasks = upsertTask(sourceTasks, action.task);
      const sameSelectedTask = state.app.selectedTaskId === action.task.id;
      if (!isCurrentProject && state.projects.current) {
        return {
          ...state,
          app: { ...state.app, isLoadingTasks: false, taskError: null },
          taskCache: cacheProjectTasks(state.taskCache, action.task.projectPath, projectTasks),
          planningProgress: removePendingPlanningProgress(state, action.task.id),
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
          isCreatingTask: false,
        },
        tasks: projectTasks,
        taskCache: cacheProjectTasks(state.taskCache, action.task.projectPath, projectTasks),
        commandRuns: projectTasks.flatMap((task) => task.commandRuns),
        planningProgress: removePendingPlanningProgress(state, action.task.id),
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
          isCreatingTask: false,
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
          isCreatingTask: true,
          taskError: null,
        },
      };
    case "tasks/newClosed":
      return { ...state, app: { ...state.app, isCreatingTask: false } };
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
        app: { ...state.app, selectedTodoId: action.todoId },
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
        app: { ...state.app, selectedTodoId: action.todoId },
        tasks,
        taskCache: projectPath ? cacheProjectTasks(state.taskCache, projectPath, tasks) : state.taskCache,
      };
    }
    default:
      return state;
  }
}
