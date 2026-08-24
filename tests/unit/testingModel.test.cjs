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

test("validation evidence ignores preview and unrelated task runs", () => {
  const previewRun = run("preview", "succeeded", 4);
  previewRun.intent = "preview";

  const otherTaskValidation = run("other-task-validation", "succeeded", 5);
  otherTaskValidation.taskId = "task-2";

  const evidence = deriveValidationEvidence(
    [previewRun, otherTaskValidation, run("legacy-pass", "succeeded", 3)],
    "task-1",
    slots,
  );

  assert.equal(evidence.hasEvidence, true);
  assert.equal(evidence.hasPassingEvidence, true);
  assert.equal(evidence.successfulRun.id, "legacy-pass");
});

test("explicit validation intent counts even when command text is not configured", () => {
  const customValidation = run("custom-validation", "succeeded", 6);
  customValidation.command = "npm run verify:release";
  customValidation.intent = "validation";

  const evidence = deriveValidationEvidence([customValidation], "task-1", slots);

  assert.equal(evidence.hasEvidence, true);
  assert.equal(evidence.hasPassingEvidence, true);
  assert.equal(evidence.successfulRun.id, "custom-validation");
});

test("running validation evidence includes explicit custom commands", () => {
  const legacyPreview = run("legacy-preview", "running", 7);
  legacyPreview.command = "pnpm dev";
  legacyPreview.intent = "preview";

  const customValidation = run("custom-validation", "running", 8);
  customValidation.command = "npm run verify:release";
  customValidation.intent = "validation";

  const evidence = deriveValidationEvidence([legacyPreview, customValidation], "task-1", slots);

  assert.equal(evidence.hasEvidence, true);
  assert.equal(evidence.hasRunningEvidence, true);
  assert.equal(evidence.runningRun.id, "custom-validation");
  assert.equal(evidence.hasPassingEvidence, false);
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
