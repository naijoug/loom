export type CommandRunStatus = "queued" | "running" | "succeeded" | "failed" | "cancelled";

export interface CommandRun {
  id: string;
  taskId: string;
  command: string;
  cwd: string;
  startedAt: string;
  endedAt?: string;
  status: CommandRunStatus;
  exitCode?: number;
  stdoutLogRef?: string;
  stderrLogRef?: string;
  errorSummary?: string;
}

export interface SpikeRun {
  runId: string;
  command: string;
  cwd: string;
  pid?: number;
  startedAtMs: number;
}

export interface SpikeStopResult {
  runId: string;
  stopped: boolean;
  exitCode?: number;
}

export interface SpikeLogEvent {
  runId: string;
  stream: "stdout" | "stderr";
  line: string;
  timestampMs: number;
}
