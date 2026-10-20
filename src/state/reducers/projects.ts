import type { AppAction, AppState } from "../model";
export function reduceProjects(state: AppState, action: AppAction): AppState {
  switch (action.type) {
    case "projects/loadStarted": return { ...state, app: { ...state.app, isLoadingProjects: true, projectError: null } };
    case "projects/loadFailed": return { ...state, app: { ...state.app, isLoadingProjects: false, projectError: action.error } };
    case "projects/recentLoaded": {
      const current = action.projects.find(project => project.id === state.projects.current?.id) ?? action.projects[0] ?? null;
      return { ...state, projects: { current, recent: action.projects }, app: { ...state.app, activeProjectId: current?.id ?? null, isLoadingProjects: false, projectError: null } };
    }
    case "projects/registered": return { ...state, projects: { current: action.project, recent: [action.project, ...state.projects.recent.filter(project => project.path !== action.project.path)].slice(0, 10) },
      app: { ...state.app, currentView: "chat", activeProjectId: action.project.id, isLoadingProjects: false, projectError: null } };
    case "projects/selected": {
      const current = state.projects.recent.find(project => project.id === action.projectId);
      return current ? { ...state, projects: { ...state.projects, current }, app: { ...state.app, activeProjectId: current.id, currentView: "chat", projectError: null } } : state;
    }
    case "projects/removed": {
      const recent = state.projects.recent.filter(project => project.id !== action.projectId);
      const current = state.projects.current?.id === action.projectId ? recent[0] ?? null : state.projects.current;
      return { ...state, projects: { current, recent }, app: { ...state.app, activeProjectId: current?.id ?? null, isLoadingProjects: false, projectError: null } };
    }
    default: return state;
  }
}
