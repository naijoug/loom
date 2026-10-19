import { reduceAppAndAgents } from "./reducers/appAgents";
import { reduceCommands } from "./reducers/commands";
import { reducePlanning, planningLogKey } from "./reducers/planning";
import { reduceProjects } from "./reducers/projects";
import { reduceTasks } from "./reducers/tasks";
import type { AppAction, AppState } from "./model";

export type { AppAction, AppSlice, AppState, AppView, ProjectsSlice } from "./model";
export { initialAppState } from "./model";
export { planningLogKey };

/**
 * The context remains a single state tree, while each workflow domain owns its
 * mutations. Prefix dispatch keeps the public action contract stable.
 */
export function appReducer(state: AppState, action: AppAction): AppState {
  if (action.type.startsWith("projects/")) return reduceProjects(state, action);
  if (action.type.startsWith("tasks/")) return reduceTasks(state, action);
  if (action.type.startsWith("commands/")) return reduceCommands(state, action);
  if (action.type.startsWith("planning/")) return reducePlanning(state, action);
  return reduceAppAndAgents(state, action);
}
