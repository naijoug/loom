import type { AgentConfig, AgentConfigInput, AgentDiagnostic } from "../domain";
import { TAURI_COMMANDS } from "./contract";
import { invokeCommand } from "./tauriClient";

/** Agent catalog commands shared by every feature; feature APIs (chat, learn, …) stay separate. */
export const agentsApi = {
  list: () => invokeCommand<AgentConfig[]>(TAURI_COMMANDS.listAgents),
  create: (input: AgentConfigInput) => invokeCommand<AgentConfig>(TAURI_COMMANDS.createAgent, { input }),
  update: (agentId: string, input: AgentConfigInput) =>
    invokeCommand<AgentConfig[]>(TAURI_COMMANDS.updateAgent, { agentId, input }),
  remove: (agentId: string) => invokeCommand<AgentConfig[]>(TAURI_COMMANDS.deleteAgent, { agentId }),
  setEnabled: (agentId: string, enabled: boolean) =>
    invokeCommand<AgentConfig[]>(TAURI_COMMANDS.setAgentEnabled, { agentId, enabled }),
  /** Probes the CLIs (and clears the backend start-check cache); `agentId` limits it to one agent. */
  diagnose: (agentId?: string) => invokeCommand<AgentDiagnostic[]>(TAURI_COMMANDS.diagnoseAgents, { agentId }),
};

/** Capabilities come from the diagnosis; an agent not diagnosed yet reports none. */
export function agentCapabilities(diagnostics: AgentDiagnostic[], agentId: string) {
  return diagnostics.find((item) => item.agentId === agentId)?.capabilities;
}
