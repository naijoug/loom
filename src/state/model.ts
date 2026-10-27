import type { AgentConfig, AgentDiagnostic, ProjectSummary } from "../domain";
import type { FeatureId } from "../shell/features";
/** Registered features plus Settings, which lives at the bottom of the activity rail. */
export type AppView = FeatureId | "settings";
export interface AppSlice {
  currentView: AppView; activeProjectId: string | null;
  isLoadingProjects: boolean; isLoadingAgents: boolean;
  projectError: string | null; agentError: string | null;
}
export interface ProjectsSlice { recent: ProjectSummary[]; current: ProjectSummary | null; }
/** `agentDiagnostics`: latest `diagnose_agents` result per agent, shared so every feature's AgentPicker shows the same status. */
export interface AppState { app: AppSlice; projects: ProjectsSlice; agents: AgentConfig[]; agentDiagnostics: AgentDiagnostic[]; }
export type AppAction =
  | { type: "app/viewSelected"; view: AppView }
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
  | { type: "agents/diagnosed"; diagnostics: AgentDiagnostic[] };
export const initialAppState: AppState = {
  app: { currentView: "chat", activeProjectId: null, isLoadingProjects: false, isLoadingAgents: false, projectError: null, agentError: null },
  projects: { recent: [], current: null }, agents: [], agentDiagnostics: [],
};
