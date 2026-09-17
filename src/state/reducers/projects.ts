import type { AppAction, AppState } from "../model";
import { cacheProjectTasks, removeCachedProjectTasks } from "./taskCollections";

export function reduceProjects(state: AppState, action: AppAction): AppState {
  switch (action.type) {
    case "projects/loadStarted":
      return {
        ...state,
        app: { ...state.app, isLoadingProjects: true, projectError: null },
      };
    case "projects/loadFailed":
      return {
        ...state,
        app: { ...state.app, isLoadingProjects: false, projectError: action.error },
      };
    case "projects/recentLoaded": {
      const recent = action.projects;
      const currentStillValid =
        state.projects.current != null &&
        recent.some((project) => project.id === state.projects.current?.id);
      const nextCurrent = currentStillValid
        ? state.projects.current
        : (recent[0] ?? null);
      return {
        ...state,
        app: {
          ...state.app,
          isLoadingProjects: false,
          projectError: null,
          activeProjectId: nextCurrent?.id ?? null,
        },
        projects: { ...state.projects, recent, current: nextCurrent },
      };
    }
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
          currentView: "chat",
          selectedTaskId: null,
          selectedTodoId: null,
          viewedStage: null,
          isCreatingTask: false,
          isLoadingProjects: false,
          projectError: null,
        },
        projects: { current: action.project, recent },
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
          currentView: "chat",
          selectedTaskId: null,
          selectedTodoId: null,
          viewedStage: null,
          isCreatingTask: false,
          projectError: null,
        },
        projects: { ...state.projects, current: project ?? state.projects.current },
        tasks,
        commandRuns: tasks.flatMap((task) => task.commandRuns),
        commandLogs: {},
        planningLogs: {},
        planningProgress: {},
      };
    }
    case "projects/removed": {
      const removedProject =
        state.projects.recent.find((candidate) => candidate.id === action.projectId) ??
        (state.projects.current?.id === action.projectId ? state.projects.current : null);
      const removedProjectPath = action.projectPath ?? removedProject?.path ?? null;
      const taskCache = removedProjectPath
        ? removeCachedProjectTasks(state.taskCache, removedProjectPath)
        : state.taskCache;
      const removesCurrentProject =
        state.app.activeProjectId === action.projectId || state.projects.current?.id === action.projectId;

      if (!removesCurrentProject) {
        return {
          ...state,
          app: { ...state.app, isLoadingProjects: false, projectError: null },
          projects: {
            ...state.projects,
            recent: state.projects.recent.filter((project) => project.id !== action.projectId),
          },
          taskCache,
        };
      }
      return {
        ...state,
        app: {
          ...state.app,
          activeProjectId: null,
          selectedTaskId: null,
          selectedTodoId: null,
          viewedStage: null,
          currentView: "chat",
          isCreatingTask: false,
          isLoadingProjects: false,
          projectError: null,
        },
        projects: {
          current: null,
          recent: state.projects.recent.filter((project) => project.id !== action.projectId),
        },
        tasks: [],
        taskCache,
        commandRuns: [],
        commandLogs: {},
        planningLogs: {},
        planningProgress: {},
      };
    }
    default:
      return state;
  }
}
