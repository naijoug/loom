const test = require("node:test");
const assert = require("node:assert/strict");

/** Mirror of src/domain/chat.ts normalizeChatPermissionMode (contract note for M0). */
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

function chatPermissionAllowsWrite(mode) {
  return normalizeChatPermissionMode(mode) === "auto";
}

test("chat permission enum aliases map read_only→explore and read_write→ask", () => {
  assert.equal(normalizeChatPermissionMode("read_only"), "explore");
  assert.equal(normalizeChatPermissionMode("read_write"), "ask");
  assert.equal(normalizeChatPermissionMode("explore"), "explore");
  assert.equal(normalizeChatPermissionMode("ask"), "ask");
  assert.equal(normalizeChatPermissionMode("auto"), "auto");
  assert.equal(normalizeChatPermissionMode(undefined), "explore");
  assert.equal(normalizeChatPermissionMode("weird"), "explore");
});

test("only auto allows write-capable adapter stage in Phase 1", () => {
  assert.equal(chatPermissionAllowsWrite("explore"), false);
  assert.equal(chatPermissionAllowsWrite("ask"), false);
  assert.equal(chatPermissionAllowsWrite("read_write"), false);
  assert.equal(chatPermissionAllowsWrite("auto"), true);
});
