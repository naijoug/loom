const test = require("node:test");
const assert = require("node:assert/strict");
const {
  implementationReviewGate,
} = require("../../.tmp/test-build/src/utils/implementationReview.js");

function task(overrides = {}) {
  return {
    primaryAgentId: "builder",
    implementationReviewRuns: [],
    implementationReviews: [],
    ...overrides,
  };
}

test("implementation Review gate requires a completed run", () => {
  assert.equal(implementationReviewGate(task()).ready, false);
});

test("implementation Review gate rejects self-review and blockers", () => {
  const base = task({
    implementationReviewRuns: [{ id: "run-1", status: "succeeded" }],
    implementationReviews: [{
      runId: "run-1",
      status: "succeeded",
      reviewerAgentId: "builder",
      findings: [],
    }],
  });
  assert.equal(implementationReviewGate(base).ready, false);

  base.implementationReviews[0].reviewerAgentId = "reviewer";
  base.implementationReviews[0].findings = [{ severity: "blocker", status: "open" }];
  assert.equal(implementationReviewGate(base).ready, false);
});

test("implementation Review gate allows a different reviewer with accepted blockers", () => {
  const result = implementationReviewGate(task({
    implementationReviewRuns: [{ id: "run-1", status: "succeeded" }],
    implementationReviews: [{
      runId: "run-1",
      status: "succeeded",
      reviewerAgentId: "reviewer",
      findings: [{ severity: "blocker", status: "accepted_risk" }],
    }],
  }));
  assert.equal(result.ready, true);
});
