export type AgentCapability = "planning" | "implementation" | "review" | "debugging" | "testing" | "documentation";

export type AgentAdapterType =
  "dummy" | "cli" | "codex_cli" | "claude_code_cli" | "grok_cli" | "opencode_cli" | "api" | "mcp" | "http";

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
  /** Model provider the CLI runs against instead of its own login. */
  providerId?: string | null;
}

export type AgentConfigInput = Omit<AgentConfig, "id" | "available">;

export interface AgentCapabilities {
  structuredStream: boolean;
  toolEvents: boolean;
  nativeResume: boolean;
  inlineCards: boolean;
  /** Accepts a per-invocation model across Chat, Learn, Research and Discuss. */
  modelSelection?: boolean;
  /** Version/configuration has passed the no-tools execution-profile probe. */
  evidenceOnly?: boolean;
  /** Model provider kinds this CLI can run against; absent means none. */
  providerKinds?: ModelProviderKind[];
  modes: Array<{ mode: "explore" | "ask" | "auto"; policy: string }>;
}

export type ModelProviderKind = "anthropic" | "openai-responses" | "openai-chat";

/** A provider as the backend shows it: whether a key exists and its source, never the key. */
export interface ModelProvider {
  id: string;
  name: string;
  kind: ModelProviderKind;
  baseUrl: string;
  model: string | null;
  storedKey: boolean;
  keyEnv: string | null;
  keySource: "environment" | "stored" | "none";
  /** A key is available, or the base URL is local and may not need one. */
  configured: boolean;
}

export interface ModelProviderList {
  revision: number;
  providers: ModelProvider[];
}

/** `key` keeps, replaces or clears the stored key; the backend never sends it back. */
export interface ModelProviderInput {
  id: string;
  expectedRevision: number;
  name: string;
  kind: ModelProviderKind;
  baseUrl: string;
  model: string | null;
  key: { action: "keep" } | { action: "set"; value: string } | { action: "clear" };
  keyEnv: string | null;
}

export interface ModelProviderTestResult {
  outcome:
    | "ok"
    | "key_missing"
    | "auth_failed"
    | "not_found"
    | "rate_limited"
    | "http_error"
    | "invalid_response"
    | "timeout"
    | "unreachable";
  httpStatus: number | null;
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
