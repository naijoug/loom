const test = require("node:test");
const assert = require("node:assert/strict");
const { WORKFLOW_COPY } = require("../../.tmp/test-build/src/copy/workflow.js");

test("core workflow copy uses Chinese actions with stable domain terms", () => {
  assert.equal(WORKFLOW_COPY.locale, "zh-CN");
  assert.deepEqual(Object.values(WORKFLOW_COPY.stages), ["规划", "实施", "测试验收", "完成"]);
  assert.deepEqual(WORKFLOW_COPY.terms, {
    agent: "Agent",
    review: "Review",
    testing: "测试验收",
    evidence: "证据",
  });
  assert.equal(new Set(Object.values(WORKFLOW_COPY.actions)).size, 4);
  assert.match(WORKFLOW_COPY.blockers.prefix, /不能继续/);
});
