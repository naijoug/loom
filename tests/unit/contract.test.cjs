const test = require("node:test");
const assert = require("node:assert/strict");
const { readFileSync, readdirSync, statSync } = require("node:fs");
const { join } = require("node:path");

const {
  MODEL_CONTRACT_SAMPLES,
  STORE_SCHEMA_VERSIONS,
  TAURI_COMMANDS,
  TAURI_EVENTS,
} = require("../../.tmp/test-build/src/api/contract.js");
const { createTauriClient } = require("../../.tmp/test-build/src/api/tauriClient.js");

const fixture = JSON.parse(
  readFileSync(join(__dirname, "../../contracts/tauri-contract.json"), "utf8"),
);

test("frontend command event and schema contract matches the canonical fixture", () => {
  assert.deepEqual(Object.values(TAURI_COMMANDS), fixture.commands);
  assert.deepEqual(Object.values(TAURI_EVENTS), fixture.events);
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

test("typed Tauri client forwards invoke payloads and unwraps event payloads", async () => {
  const invocations = [];
  const listeners = [];
  let unlistened = false;
  const client = createTauriClient(
    async (command, args) => {
      invocations.push({ command, args });
      return { status: "ok" };
    },
    async (event, handler) => {
      listeners.push({ event, handler });
      return () => { unlistened = true; };
    },
  );

  const response = await client.invoke(TAURI_COMMANDS.healthCheck, { projectPath: "/tmp/project" });
  assert.deepEqual(response, { status: "ok" });
  assert.deepEqual(invocations, [{ command: "health_check", args: { projectPath: "/tmp/project" } }]);

  const payloads = [];
  const unlisten = await client.listen(TAURI_EVENTS.chatEvent, (payload) => payloads.push(payload));
  assert.equal(listeners[0].event, "loom://chat-event");
  listeners[0].handler({ payload: { runId: "run-1" } });
  assert.deepEqual(payloads, [{ runId: "run-1" }]);
  unlisten();
  assert.equal(unlistened, true);
});
