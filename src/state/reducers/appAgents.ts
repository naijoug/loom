import type { AppAction, AppState } from "../model";

export function reduceAppAndAgents(state: AppState, action: AppAction): AppState {
  switch (action.type) {
    case "app/viewSelected":
      return {
        ...state,
        app: { ...state.app, currentView: action.view },
      };
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
    case "agents/diagnosed": {
      // A single-agent diagnosis replaces only that agent's entry.
      const fresh = new Set(action.diagnostics.map((item) => item.agentId));
      return { ...state, agentDiagnostics: [...state.agentDiagnostics.filter((item) => !fresh.has(item.agentId)), ...action.diagnostics] };
    }
    default:
      return state;
  }
}
