export type AgentCapability =
  | "planning"
  | "implementation"
  | "review"
  | "debugging"
  | "testing"
  | "documentation";

export type AgentAdapterType =
  | "dummy"
  | "cli"
  | "codex_cli"
  | "claude_code_cli"
  | "grok_cli"
  | "opencode_cli"
  | "mcp"
  | "http";

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
  available: boolean;
}

export type AgentConfigInput = Omit<AgentConfig, "id" | "available">;

export interface AgentCapabilities {
  structuredStream: boolean;
  toolEvents: boolean;
  nativeResume: boolean;
  inlineCards: boolean;
  /** Accepts a per-invocation model across Chat, Learn, Research and Discuss. */
  modelSelection?: boolean;
  modes: Array<{ mode: "explore" | "ask" | "auto"; policy: string }>;
}

export interface AgentDiagnostic {
  capabilities?: AgentCapabilities;
  agentId: string;
  /** `broken`: the CLI starts but `--version` exits non-zero (usually a half-installed package). */
  status: "ready" | "disabled" | "missing" | "unavailable" | "broken";
  resolvedPath?: string;
  version?: string;
  authStatus: "unknown" | "not_applicable";
  detail: string;
  checkedAtMs: number;
}

