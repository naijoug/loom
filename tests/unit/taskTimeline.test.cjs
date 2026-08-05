const test = require("node:test");
const assert = require("node:assert/strict");
const { buildTaskTimelineRows } = require("../../.tmp/test-build/src/utils/taskTimeline.js");

function task(overrides = {}) {
  return {
    id: "task-1",
    projectPath: "/tmp/project",
    title: "Task",
    rawRequirement: "Requirement",
    status: "debugging",
    selectedPlanningAgentIds: [],
    reviewAgentIds: [],
    planningRuns: [],
    agentInvocations: [],
    planReviews: [],
    planningDecisions: [],
    planTodos: [],
    loopTrace: [],
    events: [],
    commandRuns: [],
    feedback: [],
    createdAtMs: 1,
    updatedAtMs: 1,
    ...overrides,
  };
}

test("buildTaskTimelineRows merges planning, loop trace, and task events by newest first", () => {
  const rows = buildTaskTimelineRows(
    task({
      planningRuns: [
        {
          id: "plan-1",
          taskId: "task-1",
          requirement: "Plan it",
          selectedAgentIds: ["agent-1"],
          status: "succeeded",
          summary: "Planning succeeded",
          startedAtMs: 10,
          endedAtMs: 20,
        },
      ],
      agentInvocations: [
        {
          id: "invoke-1",
          planningRunId: "plan-1",
          taskId: "task-1",
          agentId: "agent-1",
          agentName: "Codex",
          status: "succeeded",
          promptSummary: "Planning discussion",
          rawOutput: "raw",
          outputSummary: "Codex proposed a plan",
          stderrTail: [],
          timedOut: false,
          attempt: 1,
          startedAtMs: 21,
          endedAtMs: 30,
        },
      ],
      loopTrace: [
        {
          id: "trace-1",
          taskId: "task-1",
          loopId: "loop-1",
          stage: "testing",
          entryType: "command_finished",
          iteration: 1,
          attempt: 0,
          contextSummary: "ctx",
          actionSummary: "pnpm test",
          verificationSummary: "Command failed",
          commandRunId: "run-1",
          terminationReason: "failed",
          timestampMs: 50,
        },
      ],
      events: [
        {
          id: "event-1",
          taskId: "task-1",
          timestampMs: 40,
          actor: "system",
          status: "implementing",
          outputSummary: "Todo started",
        },
      ],
    }),
  );

  assert.deepEqual(
    rows.map((row) => row.id),
    [
      "loop-trace-trace-1",
      "task-event-event-1",
      "agent-invocation-invoke-1",
      "planning-run-plan-1",
    ],
  );
  assert.equal(rows[0].stage, "testing");
  assert.equal(rows[0].kind, "command finished");
  assert.equal(rows[2].summary, "Codex proposed a plan");
});

test("buildTaskTimelineRows omits command runs already represented by loop trace", () => {
  const rows = buildTaskTimelineRows(
    task({
      loopTrace: [
        {
          id: "trace-1",
          taskId: "task-1",
          loopId: "loop-1",
          stage: "testing",
          entryType: "command_finished",
          contextSummary: "ctx",
          actionSummary: "pnpm test",
          verificationSummary: "Command succeeded",
          commandRunId: "run-1",
          timestampMs: 20,
        },
      ],
      commandRuns: [
        {
          id: "run-1",
          taskId: "task-1",
          command: "pnpm test",
          cwd: "/tmp/project",
          intent: "validation",
          startedAtMs: 10,
          endedAtMs: 20,
          status: "succeeded",
          exitCode: 0,
        },
        {
          id: "run-2",
          taskId: "task-1",
          command: "pnpm build",
          cwd: "/tmp/project",
          intent: "validation",
          startedAtMs: 30,
          endedAtMs: 40,
          status: "failed",
          exitCode: 1,
          errorSummary: {
            exitCode: 1,
            stderrTail: ["error: build failed"],
            matchedLines: [],
            failed: true,
          },
        },
      ],
    }),
  );

  assert.deepEqual(
    rows.map((row) => row.id),
    ["command-run-run-2", "loop-trace-trace-1"],
  );
  assert.equal(rows[0].summary, "error: build failed");
});

test("buildTaskTimelineRows includes implementation review, feedback, and summary artifacts", () => {
  const rows = buildTaskTimelineRows(
    task({
      implementationReviews: [
        {
          id: "review-1",
          runId: "review-run-1",
          taskId: "task-1",
          reviewerAgentId: "agent-2",
          reviewerAgentName: "Claude",
          status: "succeeded",
          summary: "Implementation is ready",
          rawOutput: "{}",
          findings: [],
          startedAtMs: 50,
          endedAtMs: 60,
        },
      ],
      implementationReviewDecisions: [],
      feedback: [
        {
          id: "feedback-1",
          taskId: "task-1",
          content: "Button does not respond",
          attachments: [],
          timestampMs: 70,
        },
      ],
      summary: {
        taskId: "task-1",
        title: "Task",
        requirement: "Requirement",
        completedTodos: [],
        changedFiles: [{ path: "src/app.ts", status: "M ", attribution: "task_introduced" }],
        totalAdditions: 1,
        totalDeletions: 0,
        decisions: [],
        reviews: [],
        validationEvidence: [{ runId: "run-1", command: "pnpm test", status: "succeeded", startedAtMs: 80 }],
        remainingRisks: [],
        recommendations: [],
        generatedAtMs: 90,
        jsonPath: "/tmp/summary.json",
        markdownPath: "/tmp/summary.md",
      },
    }),
  );

  assert.deepEqual(rows.map((row) => row.kind), [
    "delivery summary",
    "human feedback",
    "Claude implementation review",
  ]);
});
