const test = require("node:test");
const assert = require("node:assert/strict");
const {
  describeRoundDraftStatus,
} = require("../../.tmp/test-build/src/components/Planning/roundSummary.js");

test("round summary surfaces failed drafts instead of hiding them behind zero successes", () => {
  assert.equal(describeRoundDraftStatus([{ status: "failed" }]), "1 个 Agent · 1 失败");
});

test("round summary reports mixed draft outcomes", () => {
  assert.equal(
    describeRoundDraftStatus([
      { status: "succeeded" },
      { status: "failed" },
      { status: "running" },
    ]),
    "3 个 Agent · 1 成功 · 1 失败 · 1 运行中",
  );
});
