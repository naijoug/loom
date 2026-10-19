import type { AgentConfig } from "../../domain";

export function extractMentionNames(value: string) {
  return Array.from(value.matchAll(/@([\w-]+)/g), (match) => match[1].toLowerCase());
}

export function mentionAliases(agent: AgentConfig) {
  const normalizedName = agent.name.toLowerCase().replace(/\s+/g, "-");
  const aliases = new Set([
    agent.id.toLowerCase(),
    agent.name.toLowerCase(),
    normalizedName,
    agent.command.toLowerCase(),
  ]);

  if (agent.adapterType === "codex_cli") {
    aliases.add("codex");
  }
  if (agent.adapterType === "claude_code_cli") {
    aliases.add("claude");
    aliases.add("claude-code");
  }
  return aliases;
}

export function canonicalMention(agent: AgentConfig) {
  if (agent.adapterType === "codex_cli") {
    return "codex";
  }
  if (agent.adapterType === "claude_code_cli") {
    return "claude";
  }
  return agent.name.toLowerCase().replace(/\s+/g, "-");
}

export function agentIdsForMentions(value: string, agents: AgentConfig[]) {
  const mentions = extractMentionNames(value);
  if (mentions.length === 0) {
    return [];
  }

  return agents
    .filter((agent) => {
      const aliases = mentionAliases(agent);
      return mentions.some((mention) => aliases.has(mention));
    })
    .map((agent) => agent.id);
}

export function unknownMentionNames(value: string, agents: AgentConfig[]) {
  const aliases = new Set(agents.flatMap((agent) => Array.from(mentionAliases(agent))));
  return Array.from(
    new Set(extractMentionNames(value).filter((mention) => !aliases.has(mention))),
  );
}

export function defaultPlanningAgentIds(
  availableAgents: AgentConfig[],
  preferredAgentIds: string[] = [],
) {
  const availableIds = new Set(availableAgents.map((agent) => agent.id));
  const preferred = preferredAgentIds.filter((agentId) => availableIds.has(agentId));
  return preferred.length > 0 ? preferred : availableAgents.map((agent) => agent.id);
}
