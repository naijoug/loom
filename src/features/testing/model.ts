import type { CommandRun, Task, TerminalSlot } from "../../domain";
import {
  isFailedValidationRun,
  isSuccessfulValidationRun,
  isValidationRun,
} from "../../utils/commandRun";

export interface QuoteDraft {
  text: string;
  command: string;
  failedRunId?: string;
}

export interface ConversationTurn {
  id: string;
  role: "human" | "agent";
  timestampMs: number;
  content: string;
  run?: CommandRun;
}

export interface AutoTestingLoop {
  loopId: string;
  sourceFailureRunId: string;
  validationSlotId?: string;
  validationCommand: string;
  validationCwd: string;
  repairAttempts: number;
  lastFailureFingerprint?: string;
  repeatedFailureCount: number;
  startedAtMs: number;
  status: "repairing" | "validating" | "passed" | "escalated";
}

export function feedbackConversationContent(feedback: Task["feedback"][number]) {
  return [
    feedback.content,
    feedback.reproductionSteps ? `复现步骤:\n${feedback.reproductionSteps}` : "",
    feedback.expectedBehavior ? `期望行为:\n${feedback.expectedBehavior}` : "",
    feedback.quotedLog ? `引用日志:\n${feedback.quotedLog}` : "",
    feedback.attachments?.length
      ? `附件:\n${feedback.attachments.map((attachment) => `- ${attachment.name}`).join("\n")}`
      : "",
  ].filter(Boolean).join("\n\n");
}

export function slotEndpoint(slot: TerminalSlot, run?: CommandRun) {
  return run?.errorSummary?.urls?.[0]
    ?? run?.errorSummary?.ports?.[0]?.toString()
    ?? (slot.kind === "preview" ? "localhost:1420" : "exit code required");
}

export function slotEmptyMessage(slot: TerminalSlot) {
  return slot.kind === "preview"
    ? "Start the preview command to stream runtime logs and keep a live surface for manual checks."
    : "Run this check to create acceptance evidence: command, cwd, stdout/stderr logs, and exit status.";
}

export function latestRunForCommand(runs: CommandRun[], taskId: string, command: string) {
  return runs
    .filter((run) => run.taskId === taskId && run.command === command)
    .sort((left, right) => right.startedAtMs - left.startedAtMs)[0];
}

export function deriveValidationEvidence(
  runs: CommandRun[],
  taskId: string,
  slots: TerminalSlot[],
) {
  const validationCommands = new Set(
    slots.filter((slot) => slot.kind === "validation").map((slot) => slot.command),
  );
  const successfulRun = runs
    .filter((run) => run.taskId === taskId && isSuccessfulValidationRun(run, validationCommands))
    .sort((left, right) => right.startedAtMs - left.startedAtMs)[0];
  const failedRun = runs
    .filter((run) => run.taskId === taskId && isFailedValidationRun(run, validationCommands))
    .sort((left, right) => right.startedAtMs - left.startedAtMs)[0];
  const blockingFailure =
    failedRun && (!successfulRun || failedRun.startedAtMs > successfulRun.startedAtMs)
      ? failedRun
      : undefined;
  return {
    validationCommands,
    successfulRun,
    failedRun,
    blockingFailure,
    hasEvidence: runs.some((run) => run.taskId === taskId && isValidationRun(run, validationCommands)),
    hasPassingEvidence: Boolean(successfulRun && !blockingFailure),
  };
}

export function latestTaskRun(runs: CommandRun[], taskId: string) {
  return runs
    .filter((run) => run.taskId === taskId)
    .sort((left, right) => right.startedAtMs - left.startedAtMs)[0];
}

export function cycleLabel(run: CommandRun, index: number) {
  const started = new Date(run.startedAtMs).toLocaleTimeString("en-US", {
    hour: "2-digit",
    minute: "2-digit",
  });
  return `Cycle ${index + 1} · ${run.status} · ${started}`;
}

export function shortTime(timestampMs?: number) {
  if (!timestampMs) return "not captured";
  return new Date(timestampMs).toLocaleTimeString("en-US", {
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function statusLabel(run?: CommandRun) {
  if (!run) return "Not run";
  return typeof run.exitCode === "number" ? `${run.status} · exit ${run.exitCode}` : run.status;
}

export function errorFinding(run?: CommandRun) {
  if (!run?.errorSummary) return "No failing command has been captured for this task yet.";
  return run.errorSummary.matchedLines[0]
    ?? run.errorSummary.stderrTail[run.errorSummary.stderrTail.length - 1]
    ?? `Command exited with ${run.errorSummary.exitCode ?? "an error"}`;
}

export function gateStatus({
  hasPassingEvidence,
  hasRunningValidationRun,
  latestBlockingFailure,
  hasValidationEvidence,
}: {
  hasPassingEvidence: boolean;
  hasRunningValidationRun: boolean;
  latestBlockingFailure?: CommandRun;
  hasValidationEvidence: boolean;
}) {
  if (latestBlockingFailure) {
    return { tone: "err", title: "需要修复", copy: "有新的失败验证阻塞验收，需要在其后重新跑通验证命令。" } as const;
  }
  if (hasPassingEvidence) {
    return { tone: "ok", title: "验证已通过", copy: "已有成功验证命令，且没有更新的失败运行阻塞验收。" } as const;
  }
  if (hasRunningValidationRun) {
    return { tone: "running", title: "检查运行中", copy: "等待验证命令结束后再验收任务。" } as const;
  }
  if (hasValidationEvidence) {
    return { tone: "idle", title: "验证未通过", copy: "重新运行验证命令，并捕获成功退出后再验收。" } as const;
  }
  return { tone: "idle", title: "暂无验证证据", copy: "运行验证命令以捕获 stdout、stderr、退出码和日志引用。" } as const;
}
