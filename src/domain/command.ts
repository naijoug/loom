export type CommandRunStatus = "queued" | "running" | "succeeded" | "failed" | "cancelled";

export interface ErrorSummary {
  exitCode?: number;
  stderrTail: string[];
  matchedLines: string[];
  failed: boolean;
}

export interface CommandRun {
  id: string;
  taskId: string;
  command: string;
  cwd: string;
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
  taskId?: string;
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
  timestampMs: number;
}
