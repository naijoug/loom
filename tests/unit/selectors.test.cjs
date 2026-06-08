const test = require("node:test");
const assert = require("node:assert/strict");
const { deriveTaskStages } = require("../../.tmp/test-build/src/state/selectors.js");

function statusMap(status) {
  return Object.fromEntries(deriveTaskStages(status).map((stage) => [stage.label, stage.status]));
}

test("completed tasks show Done as the active stage", () => {
  const stages = statusMap("completed");

  assert.equal(stages.Planning, "done");
  assert.equal(stages.Implementing, "done");
  assert.equal(stages.Testing, "done");
  assert.equal(stages.Done, "active");
});

test("verifying tasks remain in the running Testing stage before manual accept", () => {
  const stages = statusMap("verifying");

  assert.equal(stages.Planning, "done");
  assert.equal(stages.Implementing, "done");
  assert.equal(stages.Testing, "running");
  assert.equal(stages.Done, "pending");
});

test("empty task state exposes the four planned stages", () => {
  const stages = deriveTaskStages(null).map((stage) => [stage.label, stage.status]);

  assert.deepEqual(stages, [
    ["Planning", "pending"],
    ["Implementing", "pending"],
    ["Testing", "pending"],
    ["Done", "pending"],
  ]);
});
