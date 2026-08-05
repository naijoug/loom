import type { AgentConfig, AgentConfigInput } from "../../domain";

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
  };
}

export function isBuiltInAgent(agent: AgentConfig) {
  return ["agent-codex", "agent-claude", "agent-dummy"].includes(agent.id);
}

export function profileSummary(agent: AgentConfig) {
  switch (agent.adapterType) {
    case "codex_cli": return "codex exec --cd {projectPath} --sandbox read-only -";
    case "claude_code_cli": return "claude -p --output-format text";
    case "dummy": return "test fixture only";
    default: return agent.args.length > 0 ? `${agent.command} ${agent.args.join(" ")}` : agent.command;
  }
}
