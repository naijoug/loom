export type AgentCapability =
  | "planning"
  | "implementation"
  | "review"
  | "debugging"
  | "testing"
  | "documentation";

export type AgentAdapterType = "dummy" | "cli" | "mcp" | "http";

export type WorkingDirectoryPolicy = "project_root" | "agent_configured" | "custom";

export interface AgentConfig {
  id: string;
  name: string;
  command: string;
  args: string[];
  workingDirectoryPolicy: WorkingDirectoryPolicy;
  capabilities: AgentCapability[];
  adapterType: AgentAdapterType;
  canWriteFiles: boolean;
  canRunCommands: boolean;
  enabled: boolean;
}
