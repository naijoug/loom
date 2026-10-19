const test = require("node:test");
const assert = require("node:assert/strict");
const {
  agentIdsForMentions,
  defaultPlanningAgentIds,
  unknownMentionNames,
} = require("../../.tmp/test-build/src/components/Planning/participantSelection.js");

const agents = [
  {
    id: "agent-codex",
    name: "Codex",
    command: "codex",
    args: [],
    workingDirectoryPolicy: "project_root",
    capabilities: ["planning"],
    adapterType: "codex_cli",
    canWriteFiles: true,
    canRunCommands: true,
    enabled: true,
    available: true,
  },
  {
    id: "agent-claude",
    name: "Claude Code",
    command: "claude",
    args: [],
    workingDirectoryPolicy: "project_root",
    capabilities: ["planning"],
    adapterType: "claude_code_cli",
    canWriteFiles: true,
    canRunCommands: true,
    enabled: true,
    available: true,
  },
];

test("mentions resolve aliases without silently adding other agents", () => {
  assert.deepEqual(agentIdsForMentions("请 @claude 先起草", agents), ["agent-claude"]);
  assert.deepEqual(
    agentIdsForMentions("@codex 和 @claude-code 一起讨论", agents),
    ["agent-codex", "agent-claude"],
  );
});

test("unknown mentions are explicit instead of falling back to every agent", () => {
  assert.deepEqual(unknownMentionNames("让 @claud 和 @codex 讨论", agents), ["claud"]);
});

test("default selection preserves available project preferences", () => {
  assert.deepEqual(defaultPlanningAgentIds(agents, ["agent-claude", "missing"]), [
    "agent-claude",
  ]);
  assert.deepEqual(defaultPlanningAgentIds(agents), ["agent-codex", "agent-claude"]);
});
