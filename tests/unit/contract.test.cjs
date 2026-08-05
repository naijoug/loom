const test = require("node:test");
const assert = require("node:assert/strict");
const { readFileSync, readdirSync, statSync } = require("node:fs");
const { join } = require("node:path");

const {
  COMMAND_RUN_STATUSES,
  MODEL_CONTRACT_SAMPLES,
  PLAN_TODO_STATUSES,
  STORE_SCHEMA_VERSIONS,
  TASK_STATUSES,
  TAURI_COMMANDS,
  TAURI_EVENTS,
} = require("../../.tmp/test-build/src/api/contract.js");

const fixture = JSON.parse(
  readFileSync(join(__dirname, "../../contracts/tauri-contract.json"), "utf8"),
);

test("frontend command event and schema contract matches the canonical fixture", () => {
  assert.deepEqual(Object.values(TAURI_COMMANDS), fixture.commands);
  assert.deepEqual(Object.values(TAURI_EVENTS), fixture.events);
  assert.deepEqual([...TASK_STATUSES], fixture.taskStatuses);
  assert.deepEqual([...COMMAND_RUN_STATUSES], fixture.commandRunStatuses);
  assert.deepEqual([...PLAN_TODO_STATUSES], fixture.planTodoStatuses);
  assert.deepEqual(STORE_SCHEMA_VERSIONS, fixture.storeSchemaVersions);
  assert.deepEqual(MODEL_CONTRACT_SAMPLES, fixture.modelSamples);
});

function sourceFiles(directory) {
  return readdirSync(directory).flatMap((entry) => {
    const file = join(directory, entry);
    return statSync(file).isDirectory() ? sourceFiles(file) : [file];
  });
}

test("Tauri core and event access stays behind the typed client", () => {
  const sourceRoot = join(__dirname, "../../src");
  const violations = sourceFiles(sourceRoot)
    .filter((file) => /\.(ts|tsx)$/.test(file))
    .filter((file) => !file.endsWith(join("api", "tauriClient.ts")))
    .filter((file) => /@tauri-apps\/api\/(core|event)/.test(readFileSync(file, "utf8")));

  assert.deepEqual(violations, []);
});
