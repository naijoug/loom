const test = require("node:test");
const assert = require("node:assert/strict");
const {
  describeRoundDraftStatus,
} = require("../../.tmp/test-build/src/components/Planning/roundSummary.js");

test("round summary surfaces failed drafts instead of hiding them behind zero successes", () => {
  assert.equal(describeRoundDraftStatus([{ status: "failed" }]), "1 agent · 1 failed");
});

test("round summary reports mixed draft outcomes", () => {
  assert.equal(
    describeRoundDraftStatus([
      { status: "succeeded" },
      { status: "failed" },
      { status: "running" },
    ]),
    "3 agents · 1 succeeded · 1 failed · 1 running",
  );
});
