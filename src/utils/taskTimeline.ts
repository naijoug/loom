import type { CommandRun, Task } from "../domain";
import { formatRunStatus } from "../copy/workflow";

export interface TaskTimelineRow {
  id: string;
  timestampMs: number;
  stage: string;
  kind: string;
  summary: string;
  detail?: string;
  evidenceRef?: string;
}

function stageLabel(value: string) {
  return ({
    planning: "规划",
    reviewing: "实施 Review",
    implementing: "实施",
    debugging: "测试验收",
    testing: "测试验收",
    fixing: "修复",
    verifying: "验证",
    completed: "完成",
    legacy: "历史",
  } as Record<string, string>)[value] ?? value.split("_").join(" ");
}

function statusDetail(status: string, exitCode?: number) {
  return formatRunStatus(status, exitCode);
}

function commandSummary(run: CommandRun) {
  if (run.errorSummary?.matchedLines[0]) {
    return run.errorSummary.matchedLines[0];
  }
  if (run.errorSummary?.stderrTail.length) {
    return run.errorSummary.stderrTail[run.errorSummary.stderrTail.length - 1];
  }
  return run.command;
}

export function buildTaskTimelineRows(task: Task, maxItems = 12): TaskTimelineRow[] {
  const rows: TaskTimelineRow[] = [];

  for (const run of task.planningRuns) {
    rows.push({
      id: `planning-run-${run.id}`,
      timestampMs: run.endedAtMs ?? run.startedAtMs,
      stage: "planning",
      kind: "规划运行",
      summary: run.summary || run.requirement,
      detail: statusDetail(run.status),
      evidenceRef: run.id,
    });
  }

  for (const invocation of task.agentInvocations) {
    rows.push({
      id: `agent-invocation-${invocation.id}`,
      timestampMs: invocation.endedAtMs ?? invocation.startedAtMs,
      stage: "planning",
      kind: invocation.agentName,
      summary: invocation.outputSummary || invocation.promptSummary,
      detail: statusDetail(invocation.status, invocation.exitCode),
      evidenceRef: invocation.evidenceRef ?? invocation.planPath,
    });
  }

  for (const review of task.planReviews) {
    rows.push({
      id: `plan-review-${review.id}`,
      timestampMs: review.endedAtMs ?? review.startedAtMs,
      stage: "planning",
      kind: `${review.reviewerAgentName} Review`,
      summary: review.finding,
      detail: `${review.status} · ${review.severity}`,
      evidenceRef: review.evidenceRef,
    });
  }

  for (const decision of task.planningDecisions) {
    rows.push({
      id: `planning-decision-${decision.id}`,
      timestampMs: decision.createdAtMs,
      stage: "planning",
      kind: "人工决策",
      summary: decision.title,
      detail: decision.status,
      evidenceRef: decision.id,
    });
  }

  for (const review of task.implementationReviews ?? []) {
    rows.push({
      id: `implementation-review-${review.id}`,
      timestampMs: review.endedAtMs ?? review.startedAtMs,
      stage: "reviewing",
      kind: `${review.reviewerAgentName} 实施 Review`,
      summary: review.summary || `${review.findings.length} 条发现`,
      detail: `${formatRunStatus(review.status)} · ${review.findings.length} 条发现`,
      evidenceRef: review.evidenceRef,
    });
  }

  for (const decision of task.implementationReviewDecisions ?? []) {
    rows.push({
      id: `implementation-review-decision-${decision.id}`,
      timestampMs: decision.createdAtMs,
      stage: "reviewing",
      kind: "发现项决策",
      summary: decision.reason,
      detail: `${decision.decision} · ${decision.actor}`,
      evidenceRef: decision.findingId,
    });
  }

  for (const feedback of task.feedback) {
    rows.push({
      id: `feedback-${feedback.id}`,
      timestampMs: feedback.timestampMs,
      stage: "debugging",
      kind: "人工反馈",
      summary:
        feedback.content ||
        feedback.expectedBehavior ||
        feedback.reproductionSteps ||
        `${feedback.attachments?.length ?? 0} 个附件`,
      detail: feedback.commandRunId,
      evidenceRef: feedback.attachments?.[0]?.storedPath ?? feedback.commandRunId,
    });
  }

  if (task.summary) {
    rows.push({
      id: `task-summary-${task.id}-${task.summary.generatedAtMs}`,
      timestampMs: task.summary.generatedAtMs,
      stage: "completed",
      kind: "交付总结",
      summary: `${task.summary.changedFiles.length} 个变更文件，${task.summary.validationEvidence.length} 条验证记录`,
      detail: task.summary.markdownPath,
      evidenceRef: task.summary.markdownPath,
    });
  }

  for (const entry of task.loopTrace ?? []) {
    rows.push({
      id: `loop-trace-${entry.id}`,
      timestampMs: entry.timestampMs,
      stage: stageLabel(entry.stage),
      kind: entry.entryType === "command_finished" ? "命令完成" : entry.entryType.split("_").join(" "),
      summary: entry.verificationSummary,
      detail: [
        entry.loopId,
        entry.iteration ? `i${entry.iteration}` : null,
        entry.attempt ? `a${entry.attempt}` : null,
        entry.terminationReason,
      ]
        .filter(Boolean)
        .join(" · "),
      evidenceRef: entry.commandRunId,
    });
  }

  const tracedCommandRunIds = new Set(
    (task.loopTrace ?? [])
      .map((entry) => entry.commandRunId)
      .filter((value): value is string => Boolean(value)),
  );

  for (const run of task.commandRuns) {
    if (tracedCommandRunIds.has(run.id)) {
      continue;
    }
    rows.push({
      id: `command-run-${run.id}`,
      timestampMs: run.endedAtMs ?? run.startedAtMs,
      stage: stageLabel(run.intent ?? "legacy"),
      kind: "命令",
      summary: commandSummary(run),
      detail: statusDetail(run.status, run.exitCode),
      evidenceRef: run.id,
    });
  }

  for (const event of task.events) {
    rows.push({
      id: `task-event-${event.id}`,
      timestampMs: event.timestampMs,
      stage: stageLabel(event.status),
      kind: event.actor,
      summary: event.outputSummary ?? event.inputSummary ?? event.actor,
      detail: event.evidenceRef,
      evidenceRef: event.evidenceRef,
    });
  }

  return rows
    .filter((row) => row.timestampMs > 0)
    .sort((left, right) => right.timestampMs - left.timestampMs || left.id.localeCompare(right.id))
    .slice(0, maxItems);
}
