const test = require("node:test");
const assert = require("node:assert/strict");
const { deriveTaskStages, stageOf } = require("../../.tmp/test-build/src/state/selectors.js");

function statusById(status) {
  return Object.fromEntries(deriveTaskStages(status).map((stage) => [stage.id, stage.status]));
}

function labelById(status) {
  return Object.fromEntries(deriveTaskStages(status).map((stage) => [stage.id, stage.label]));
}

test("completed tasks show Done as the active stage", () => {
  const stages = statusById("completed");

  assert.equal(stages.planning, "done");
  assert.equal(stages.implementing, "done");
  assert.equal(stages.testing, "done");
  assert.equal(stages.done, "active");
});

test("verifying tasks remain in the running Testing stage before manual accept", () => {
  const stages = statusById("verifying");

  assert.equal(stages.planning, "done");
  assert.equal(stages.implementing, "done");
  assert.equal(stages.testing, "running");
  assert.equal(stages.done, "pending");
});

test("cleared stages use past-tense labels, in-progress stages stay present-tense", () => {
  const labels = labelById("verifying");

  assert.equal(labels.planning, "Planned");
  assert.equal(labels.implementing, "Implemented");
  assert.equal(labels.testing, "Testing");
  assert.equal(labels.done, "Done");
});

test("a fully completed task labels every cleared stage in past tense", () => {
  const labels = labelById("completed");

  assert.equal(labels.planning, "Planned");
  assert.equal(labels.implementing, "Implemented");
  assert.equal(labels.testing, "Tested");
  assert.equal(labels.done, "Done");
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

test("stageOf folds task statuses into the four public workflow stages", () => {
  assert.equal(stageOf("drafting_requirements"), "planning");
  assert.equal(stageOf("plan_review"), "planning");
  assert.equal(stageOf("ready_to_implement"), "implementing");
  assert.equal(stageOf("reviewing"), "implementing");
  assert.equal(stageOf("debugging"), "testing");
  assert.equal(stageOf("fixing"), "testing");
  assert.equal(stageOf("verifying"), "testing");
  assert.equal(stageOf("completed"), "done");
  assert.equal(stageOf(null), "planning");
});

test("deriveTaskStages highlights the viewed stage and locks unreached stages", () => {
  const byId = Object.fromEntries(
    deriveTaskStages("verifying", "planning").map((stage) => [stage.id, stage]),
  );

  // Viewing an earlier stage highlights only that stage.
  assert.equal(byId.planning.viewing, true);
  assert.equal(byId.testing.viewing, false);

  // Reached stages (current + completed) are clickable; future ones are not.
  assert.equal(byId.planning.clickable, true);
  assert.equal(byId.implementing.clickable, true);
  assert.equal(byId.testing.clickable, true);
  assert.equal(byId.done.clickable, false);
});

test("deriveTaskStages without a viewed stage highlights the task's current stage", () => {
  const byId = Object.fromEntries(
    deriveTaskStages("implementing").map((stage) => [stage.id, stage]),
  );

  assert.equal(byId.implementing.viewing, true);
  assert.equal(byId.planning.viewing, false);
  assert.equal(byId.testing.viewing, false);
});

test("deriveTaskStages exposes testing as a stage id (not the legacy debugging id)", () => {
  const ids = deriveTaskStages("debugging").map((stage) => stage.id);
  assert.deepEqual(ids, ["planning", "implementing", "testing", "done"]);
});
