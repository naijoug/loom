import { reduceAppAndAgents } from "./reducers/appAgents";
import { reduceProjects } from "./reducers/projects";
import type { AppAction, AppState } from "./model";
export type { AppAction, AppSlice, AppState, AppView, ProjectsSlice } from "./model";
export { initialAppState } from "./model";
export function appReducer(state: AppState, action: AppAction): AppState {
  return action.type.startsWith("projects/") ? reduceProjects(state, action) : reduceAppAndAgents(state, action);
}
