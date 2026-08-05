import type { CommandRun, Task } from "../domain";

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
  return value.split("_").join(" ");
}

function statusDetail(status: string, exitCode?: number) {
  return typeof exitCode === "number" ? `${status} · exit ${exitCode}` : status;
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
      kind: "planning run",
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
      kind: `${review.reviewerAgentName} review`,
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
      kind: "decision",
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
      kind: `${review.reviewerAgentName} implementation review`,
      summary: review.summary || `${review.findings.length} finding(s)`,
      detail: `${review.status} · ${review.findings.length} finding(s)`,
      evidenceRef: review.evidenceRef,
    });
  }

  for (const decision of task.implementationReviewDecisions ?? []) {
    rows.push({
      id: `implementation-review-decision-${decision.id}`,
      timestampMs: decision.createdAtMs,
      stage: "reviewing",
      kind: "finding decision",
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
      kind: "human feedback",
      summary:
        feedback.content ||
        feedback.expectedBehavior ||
        feedback.reproductionSteps ||
        `${feedback.attachments?.length ?? 0} attachment(s)`,
      detail: feedback.commandRunId,
      evidenceRef: feedback.attachments?.[0]?.storedPath ?? feedback.commandRunId,
    });
  }

  if (task.summary) {
    rows.push({
      id: `task-summary-${task.id}-${task.summary.generatedAtMs}`,
      timestampMs: task.summary.generatedAtMs,
      stage: "completed",
      kind: "delivery summary",
      summary: `${task.summary.changedFiles.length} changed file(s), ${task.summary.validationEvidence.length} validation record(s)`,
      detail: task.summary.markdownPath,
      evidenceRef: task.summary.markdownPath,
    });
  }

  for (const entry of task.loopTrace ?? []) {
    rows.push({
      id: `loop-trace-${entry.id}`,
      timestampMs: entry.timestampMs,
      stage: stageLabel(entry.stage),
      kind: entry.entryType.split("_").join(" "),
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
      kind: "command",
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
