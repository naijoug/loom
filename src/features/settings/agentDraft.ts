import type { AgentConfig, AgentConfigInput, ModelProvider } from "../../domain";

export function defaultAgentDraft(): AgentConfigInput {
  return {
    name: "Custom Agent",
    command: "",
    args: [],
    workingDirectoryPolicy: "project_root",
    capabilities: ["planning", "implementation", "review"],
    adapterType: "cli",
    canWriteFiles: true,
    canRunCommands: true,
    enabled: true,
  };
}

/** A direct-API executor: no executable, no tools; the backend clears both again on save. */
export function apiAgentDraft(provider?: ModelProvider): AgentConfigInput {
  return {
    name: provider ? `API · ${provider.name}` : "API",
    command: "",
    args: [],
    workingDirectoryPolicy: "project_root",
    capabilities: ["planning", "review", "documentation"],
    adapterType: "api",
    canWriteFiles: false,
    canRunCommands: false,
    enabled: true,
    providerId: provider?.id ?? null,
  };
}

export function draftFromAgent(agent: AgentConfig): AgentConfigInput {
  return {
    name: agent.name,
    command: agent.command,
    args: agent.args,
    workingDirectoryPolicy: agent.workingDirectoryPolicy,
    capabilities: agent.capabilities,
    adapterType: agent.adapterType,
    canWriteFiles: agent.canWriteFiles,
    canRunCommands: agent.canRunCommands,
    enabled: agent.enabled,
    providerId: agent.providerId ?? null,
  };
}

export function isBuiltInAgent(agent: AgentConfig) {
  return ["agent-codex", "agent-claude", "agent-grok", "agent-opencode", "agent-dummy"].includes(agent.id);
}
