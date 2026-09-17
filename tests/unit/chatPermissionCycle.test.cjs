const test = require("node:test");
const assert = require("node:assert/strict");

/** Mirror of src/features/chat/chatPermission.ts cyclePermissionMode. */
const CHAT_PERMISSION_CYCLE = ["explore", "ask", "auto"];

function normalizeChatPermissionMode(value) {
  switch (value) {
    case "explore":
    case "ask":
    case "auto":
      return value;
    case "read_only":
      return "explore";
    case "read_write":
      return "ask";
    default:
      return "explore";
  }
}

function cyclePermissionMode(mode) {
  const current = normalizeChatPermissionMode(mode);
  const index = CHAT_PERMISSION_CYCLE.indexOf(current);
  return CHAT_PERMISSION_CYCLE[(index + 1) % CHAT_PERMISSION_CYCLE.length];
}

const LABELS = {
  explore: "探索",
  ask: "询问编辑",
  auto: "自动",
};

function permissionModeLabel(mode) {
  return LABELS[normalizeChatPermissionMode(mode)];
}

test("cyclePermissionMode walks explore→ask→auto→explore", () => {
  assert.equal(cyclePermissionMode("explore"), "ask");
  assert.equal(cyclePermissionMode("ask"), "auto");
  assert.equal(cyclePermissionMode("auto"), "explore");
  assert.equal(cyclePermissionMode("read_only"), "ask");
  assert.equal(cyclePermissionMode("read_write"), "auto");
});

test("permissionModeLabel uses Chinese Craft-style copy", () => {
  assert.equal(permissionModeLabel("explore"), "探索");
  assert.equal(permissionModeLabel("ask"), "询问编辑");
  assert.equal(permissionModeLabel("auto"), "自动");
});
