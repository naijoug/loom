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

export type AgentStage = AgentCapability;

export interface PrepareAgentInvocationInput {
  projectPath: string;
  taskId: string;
  agentId: string;
  stage: AgentStage;
  prompt: string;
  resumeCommand?: string;
}

export interface PreparedAgentInvocation {
  program: string;
  args: string[];
  cwd: string;
  stdinPrompt: boolean;
  outputMode: "plain" | "codex_json" | "claude_stream_json";
  resumed: boolean;
}

export interface ProjectAgentPreferences {
  planningAgentIds: string[];
  implementationAgentId?: string;
  reviewAgentIds: string[];
  debuggingAgentId?: string;
  testingAgentId?: string;
  documentationAgentId?: string;
  updatedAtMs: number;
}

export const EMPTY_PROJECT_AGENT_PREFERENCES: ProjectAgentPreferences = {
  planningAgentIds: [],
  reviewAgentIds: [],
  updatedAtMs: 0,
};

export interface AgentDiagnostic {
  agentId: string;
  status: "ready" | "disabled" | "missing";
  resolvedPath?: string;
  version?: string;
  authStatus: "unknown" | "not_applicable";
  detail: string;
  checkedAtMs: number;
}
