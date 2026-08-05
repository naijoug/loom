const test = require("node:test");
const assert = require("node:assert/strict");
const {
  defaultAgentDraft,
  draftFromAgent,
  isBuiltInAgent,
  profileSummary,
} = require("../../.tmp/test-build/src/features/settings/agentDraft.js");

test("default Agent draft is implementation-capable and safe to edit", () => {
  const draft = defaultAgentDraft();
  assert.equal(draft.adapterType, "cli");
  assert.ok(draft.capabilities.includes("implementation"));
});

test("Agent draft round-trips editable fields", () => {
  const agent = {
    id: "custom",
    name: "Reviewer",
    command: "reviewer",
    args: ["--json"],
    workingDirectoryPolicy: "project_root",
    capabilities: ["review"],
    adapterType: "cli",
    canWriteFiles: false,
    canRunCommands: true,
    enabled: true,
    available: true,
  };
  assert.deepEqual(draftFromAgent(agent).args, ["--json"]);
  assert.equal(isBuiltInAgent(agent), false);
  assert.equal(profileSummary(agent), "reviewer --json");
});
