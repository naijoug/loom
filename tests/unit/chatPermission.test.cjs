const test = require("node:test");
const assert = require("node:assert/strict");

/** Mirror of src/domain/chat.ts normalizeChatPermissionMode (contract note). */
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

/** Phase 2: ask and auto map to write-capable adapter stage. */
function chatPermissionAllowsWrite(mode) {
  const normalized = normalizeChatPermissionMode(mode);
  return normalized === "ask" || normalized === "auto";
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

test("ask and auto allow write-capable adapter stage (Phase 2)", () => {
  assert.equal(chatPermissionAllowsWrite("explore"), false);
  assert.equal(chatPermissionAllowsWrite("ask"), true);
  assert.equal(chatPermissionAllowsWrite("read_write"), true);
  assert.equal(chatPermissionAllowsWrite("auto"), true);
  assert.equal(chatPermissionAllowsWrite("read_only"), false);
});
