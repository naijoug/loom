import type { CommandRun, LoopTraceEntry, Task } from "../../domain";
import { replaceTask } from "./taskCollections";

function commandRunIntent(run: CommandRun) {
  return run.intent ?? "legacy";
}

function isImplementationLoopValidation(task: Task, run: CommandRun) {
  return (
    commandRunIntent(run) === "validation" &&
    Boolean(run.loopId) &&
    (task.status === "implementing" || task.status === "reviewing")
  );
}

function applyCommandStartStatus(task: Task, run: CommandRun): Task["status"] {
  if (isImplementationLoopValidation(task, run)) return task.status;
  const intent = commandRunIntent(run);
  return intent === "validation" || intent === "legacy" ? "debugging" : task.status;
}

function applyCommandFinishStatus(task: Task, run: CommandRun): Task["status"] {
  if (isImplementationLoopValidation(task, run)) {
    return run.status === "succeeded" ? "verifying" : task.status;
  }
  const intent = commandRunIntent(run);
  if (intent !== "validation" && intent !== "legacy") return task.status;
  return run.status === "succeeded" ? "verifying" : "debugging";
}

function traceStageForRun(run: CommandRun) {
  switch (commandRunIntent(run)) {
    case "agent_action": return "implement";
    case "validation": return "testing";
    case "preview": return "preview";
    case "loop_step": return "loop";
    default: return "legacy";
  }
}

function compactTraceText(value: string, limit = 600) {
  const compacted = value.split(/\s+/).filter(Boolean).join(" ");
  return compacted.length > limit ? `${compacted.slice(0, Math.max(0, limit - 3))}...` : compacted;
}

function commandErrorFingerprint(run: CommandRun) {
  const summary = run.errorSummary;
  if (!summary) return undefined;
  const evidence = summary.matchedLines.length > 0 ? summary.matchedLines : summary.stderrTail;
  const fingerprint = evidence.map((line) => line.trim().toLowerCase()).filter(Boolean).join("\n");
  return fingerprint ? compactTraceText(fingerprint) : undefined;
}

function commandTraceVerification(run: CommandRun, entryType: "command_started" | "command_finished") {
  if (entryType === "command_started") return "Command started; awaiting process exit.";
  const stderrTail = run.errorSummary?.stderrTail ?? [];
  const error = run.errorSummary?.matchedLines[0] ?? stderrTail[stderrTail.length - 1] ?? "(none)";
  return `Command ${run.status}; exitCode=${run.exitCode ?? "(none)"}; error=${error}`;
}

function commandTraceEntry(
  run: CommandRun,
  entryType: "command_started" | "command_finished",
  timestampMs: number,
): LoopTraceEntry {
  return {
    id: `trace-${run.id}-${entryType}`,
    taskId: run.taskId,
    loopId: run.loopId ?? "",
    stage: traceStageForRun(run),
    entryType,
    iteration: run.iteration,
    attempt: run.attempt,
    contextSummary: `loop=${run.loopId ?? "(none)"} iteration=${run.iteration ?? "(none)"} attempt=${run.attempt ?? "(none)"}`,
    actionSummary: run.command,
    verificationSummary: commandTraceVerification(run, entryType),
    commandRunId: run.id,
    fingerprint: commandErrorFingerprint(run),
    terminationReason: run.terminationReason ?? (entryType === "command_finished" ? run.status : undefined),
    tokenUsage: undefined,
    timestampMs,
  };
}

function upsertTraceEntry(trace: LoopTraceEntry[] | undefined, entry: LoopTraceEntry) {
  const existing = trace ?? [];
  return [...existing.filter((candidate) => candidate.id !== entry.id), entry].sort(
    (left, right) => left.timestampMs - right.timestampMs,
  );
}

export function syncCommandRunStarted(tasks: Task[], run: CommandRun) {
  return replaceTask(tasks, run.taskId, (task) => ({
    ...task,
    status: applyCommandStartStatus(task, run),
    commandRuns: [...task.commandRuns.filter((candidate) => candidate.id !== run.id), run],
    loopTrace: run.loopId
      ? upsertTraceEntry(task.loopTrace, commandTraceEntry(run, "command_started", run.startedAtMs))
      : task.loopTrace,
  }));
}

export function syncCommandRunFinished(tasks: Task[], run: CommandRun) {
  return replaceTask(tasks, run.taskId, (task) => ({
    ...task,
    status: applyCommandFinishStatus(task, run),
    commandRuns: task.commandRuns.map((candidate) => (candidate.id === run.id ? run : candidate)),
    loopTrace: run.loopId
      ? upsertTraceEntry(task.loopTrace, commandTraceEntry(run, "command_finished", run.endedAtMs ?? Date.now()))
      : task.loopTrace,
  }));
}
