const test = require("node:test");
const assert = require("node:assert/strict");
const {
  deriveValidationEvidence,
  feedbackConversationContent,
  gateStatus,
} = require("../../.tmp/test-build/src/features/testing/model.js");

function run(id, status, startedAtMs) {
  return {
    id,
    taskId: "task-1",
    command: "pnpm test",
    cwd: "/repo",
    intent: "validation",
    status,
    startedAtMs,
  };
}

const slots = [{ id: "validation", name: "Tests", command: "pnpm test", cwd: "", kind: "validation" }];

test("validation evidence requires a passing run newer than the latest failure", () => {
  const blocked = deriveValidationEvidence(
    [run("pass", "succeeded", 1), run("fail", "failed", 2)],
    "task-1",
    slots,
  );
  assert.equal(blocked.hasPassingEvidence, false);
  assert.equal(blocked.blockingFailure.id, "fail");

  const recovered = deriveValidationEvidence(
    [run("fail", "failed", 2), run("pass", "succeeded", 3)],
    "task-1",
    slots,
  );
  assert.equal(recovered.hasPassingEvidence, true);
  assert.equal(recovered.blockingFailure, undefined);
});

test("gate status explains running and blocking conditions", () => {
  assert.equal(gateStatus({ hasPassingEvidence: false, hasRunningValidationRun: true, hasValidationEvidence: true }).title, "检查运行中");
  assert.equal(gateStatus({ hasPassingEvidence: false, hasRunningValidationRun: false, hasValidationEvidence: true, latestBlockingFailure: run("fail", "failed", 2) }).title, "需要修复");
});

test("feedback conversation keeps structured evidence visible", () => {
  const content = feedbackConversationContent({
    id: "feedback-1",
    taskId: "task-1",
    content: "Button is stuck",
    reproductionSteps: "Click Save",
    expectedBehavior: "Dialog closes",
    quotedLog: "timeout",
    attachments: [{ name: "screen.png", path: "evidence/screen.png" }],
    createdAtMs: 1,
  });
  assert.match(content, /复现步骤/);
  assert.match(content, /screen\.png/);
});
