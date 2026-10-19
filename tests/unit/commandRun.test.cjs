const test = require("node:test");
const assert = require("node:assert/strict");
const {
  commandRunIntent,
  isFailedValidationRun,
  isSuccessfulValidationRun,
  isValidationRun,
} = require("../../.tmp/test-build/src/utils/commandRun.js");

function run(overrides = {}) {
  return {
    id: "run-1",
    taskId: "task-1",
    command: "pnpm test",
    cwd: "/repo",
    status: "failed",
    startedAtMs: 1,
    ...overrides,
  };
}

test("commandRunIntent treats absent intent as legacy", () => {
  assert.equal(commandRunIntent(run({ intent: undefined })), "legacy");
});

test("validation intent wins even when command text is not configured", () => {
  const validationCommands = new Set(["pnpm build"]);

  assert.equal(isValidationRun(run({ intent: "validation", command: "custom check" }), validationCommands), true);
});

test("agent action failure does not count as validation failure", () => {
  const validationCommands = new Set(["pnpm test"]);

  assert.equal(isFailedValidationRun(run({ intent: "agent_action" }), validationCommands), false);
});

test("legacy runs fall back to configured validation commands", () => {
  const validationCommands = new Set(["pnpm test"]);

  assert.equal(isValidationRun(run({ intent: undefined }), validationCommands), true);
  assert.equal(isFailedValidationRun(run({ intent: undefined }), validationCommands), true);
  assert.equal(
    isSuccessfulValidationRun(run({ intent: undefined, status: "succeeded" }), validationCommands),
    true,
  );
});
