export type CommandRunStatus =
  | "queued"
  | "running"
  | "succeeded"
  | "failed"
  | "cancelled"
  | "interrupted";
export type CommandRunIntent = "agent_action" | "validation" | "preview" | "loop_step" | "legacy";

export interface ErrorSummary {
  exitCode?: number;
  stderrTail: string[];
  matchedLines: string[];
  urls?: string[];
  ports?: number[];
  warnings?: string[];
  testFailures?: string[];
  stackTraceLines?: string[];
  failed: boolean;
}

export interface CommandRun {
  id: string;
  taskId: string;
  command: string;
  cwd: string;
  intent?: CommandRunIntent;
  loopId?: string;
  iteration?: number;
  attempt?: number;
  terminationReason?: string;
  sessionId?: string;
  resumeCommand?: string;
  startedAtMs: number;
  endedAtMs?: number;
  status: CommandRunStatus;
  exitCode?: number;
  stdoutLogRef?: string;
  stderrLogRef?: string;
  errorSummary?: ErrorSummary;
}

export interface CommandSpec {
  program: string;
  args: string[];
  cwd: string;
  projectPath?: string;
  taskId?: string;
  agentId?: string;
  approvalId?: string;
  intent?: Exclude<CommandRunIntent, "legacy">;
  loopId?: string;
  iteration?: number;
  attempt?: number;
  terminationReason?: string;
}

export interface PtySpec {
  program: string;
  args: string[];
  cwd: string;
  projectPath?: string;
  taskId?: string;
  approvalId?: string;
  rows: number;
  cols: number;
}

export interface ExecutionRequest {
  program: string;
  args: string[];
  cwd: string;
  projectPath: string;
  agentId?: string;
}

export interface ExecutionAssessment {
  decision: "allowed" | "approval_required" | "denied";
  riskLevel: "safe" | "medium" | "high";
  category:
    | "none"
    | "dependency_install"
    | "destructive_filesystem"
    | "destructive_git"
    | "production_external"
    | "project_boundary";
  detail: string;
  normalizedProjectPath?: string;
  normalizedCwd?: string;
}

export interface ExecutionApproval {
  id: string;
  expiresAtMs: number;
}

export interface PtyOutputEvent {
  runId: string;
  taskId: string;
  bytes: number[];
  timestampMs: number;
}

export type TerminalSlotKind = "preview" | "validation";

export interface TerminalSlot {
  id: string;
  name: string;
  command: string;
  kind: TerminalSlotKind;
  /** Working directory relative to the project root (monorepo subdir apps). */
  cwd?: string;
}

export interface CommandRunStopResult {
  runId: string;
  stopped: boolean;
  exitCode?: number;
}

export interface CommandLogEvent {
  taskId?: string;
  runId: string;
  stream: "stdout" | "stderr";
  line: string;
  timestampMs: number;
}

export interface CommandFinishedEvent {
  taskId: string;
  runId: string;
  status: CommandRunStatus;
  exitCode?: number;
  errorSummary?: ErrorSummary;
  sessionId?: string;
  resumeCommand?: string;
  terminationReason?: string;
  timestampMs: number;
}
