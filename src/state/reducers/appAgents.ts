import type { AppAction, AppState } from "../model";

export function reduceAppAndAgents(state: AppState, action: AppAction): AppState {
  switch (action.type) {
    case "app/viewSelected":
      return {
        ...state,
        app: { ...state.app, currentView: action.view, isCreatingTask: false },
      };
    case "app/stageViewed":
      return { ...state, app: { ...state.app, viewedStage: action.stage } };
    case "agents/loadStarted":
      return { ...state, app: { ...state.app, isLoadingAgents: true, agentError: null } };
    case "agents/loadFailed":
      return { ...state, app: { ...state.app, isLoadingAgents: false, agentError: action.error } };
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
    default:
      return state;
  }
}
