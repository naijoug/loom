const test = require("node:test");
const assert = require("node:assert/strict");
const {
  DEFAULT_LOOP_BUDGET,
  decideAfterValidation,
  decideAfterRepair,
  failureFingerprint,
  isTimedOut,
} = require("../../.tmp/test-build/src/utils/loopPolicy.js");

function progress(overrides = {}) {
  return {
    repairAttempts: 0,
    repeatedFailureCount: 0,
    lastFailureFingerprint: undefined,
    startedAtMs: 0,
    ...overrides,
  };
}

function failedRun(overrides = {}) {
  return {
    status: "failed",
    command: "pnpm test",
    exitCode: 1,
    errorSummary: { matchedLines: ["error: boom"], stderrTail: ["error: boom"] },
    ...overrides,
  };
}

test("passes when the validation run succeeds", () => {
  const decision = decideAfterValidation(
    progress(),
    { status: "succeeded", command: "pnpm test" },
    DEFAULT_LOOP_BUDGET,
    100,
  );
  assert.equal(decision.kind, "pass");
});

test("repairs on the first failure within budget", () => {
  const decision = decideAfterValidation(progress(), failedRun(), DEFAULT_LOOP_BUDGET, 100);
  assert.equal(decision.kind, "repair");
  assert.equal(decision.attempt, 1);
  assert.equal(decision.repeatedFailureCount, 1);
  assert.equal(decision.fingerprint, "error: boom");
});

test("escalates after the max repair attempts are spent", () => {
  // Distinct fingerprints each round so no-progress does not fire first.
  const decision = decideAfterValidation(
    progress({ repairAttempts: 3, lastFailureFingerprint: "other" }),
    failedRun(),
    DEFAULT_LOOP_BUDGET,
    100,
  );
  assert.equal(decision.kind, "escalate");
  assert.equal(decision.reason, "max_attempts");
});

test("escalates on repeated identical failures (no progress)", () => {
  const decision = decideAfterValidation(
    progress({ repairAttempts: 1, repeatedFailureCount: 2, lastFailureFingerprint: "error: boom" }),
    failedRun(),
    DEFAULT_LOOP_BUDGET,
    100,
  );
  assert.equal(decision.kind, "escalate");
  assert.equal(decision.reason, "no_progress");
  assert.equal(decision.repeatedFailureCount, 3);
});

test("escalates when the wall-clock budget is exceeded (hung run)", () => {
  const decision = decideAfterValidation(
    progress({ startedAtMs: 0 }),
    failedRun(),
    DEFAULT_LOOP_BUDGET,
    DEFAULT_LOOP_BUDGET.wallClockMs + 1,
  );
  assert.equal(decision.kind, "escalate");
  assert.equal(decision.reason, "timeout");
});

test("decideAfterRepair re-validates on success and escalates on agent failure", () => {
  assert.deepEqual(
    decideAfterRepair(progress(), { status: "succeeded" }, DEFAULT_LOOP_BUDGET, 10),
    { kind: "validate" },
  );
  const failed = decideAfterRepair(progress(), { status: "failed" }, DEFAULT_LOOP_BUDGET, 10);
  assert.equal(failed.kind, "escalate");
  assert.equal(failed.reason, "agent_failed");
});

test("decideAfterRepair times out even when the agent run reports success", () => {
  const decision = decideAfterRepair(
    progress({ startedAtMs: 0 }),
    { status: "succeeded" },
    DEFAULT_LOOP_BUDGET,
    DEFAULT_LOOP_BUDGET.wallClockMs + 1,
  );
  assert.equal(decision.kind, "escalate");
  assert.equal(decision.reason, "timeout");
});

test("failureFingerprint normalizes evidence and falls back to command+exit", () => {
  assert.equal(
    failureFingerprint({ status: "failed", command: "pnpm test", errorSummary: { matchedLines: ["  ERROR: X "], stderrTail: [] } }),
    "error: x",
  );
  assert.equal(
    failureFingerprint({ status: "failed", command: "pnpm dev", exitCode: 7 }),
    "pnpm dev:7",
  );
});

test("isTimedOut respects a disabled (zero) budget", () => {
  assert.equal(isTimedOut(progress({ startedAtMs: 0 }), { ...DEFAULT_LOOP_BUDGET, wallClockMs: 0 }, 1_000_000), false);
});
