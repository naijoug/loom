const test = require("node:test");
const assert = require("node:assert/strict");

const {
  normalizeChatPermissionMode,
} = require("../../.tmp/test-build/src/domain/chat.js");

test("normalizeChatPermissionMode maps legacy aliases", () => {
  assert.equal(normalizeChatPermissionMode("read_only"), "explore");
  assert.equal(normalizeChatPermissionMode("read_write"), "ask");
  assert.equal(normalizeChatPermissionMode("explore"), "explore");
  assert.equal(normalizeChatPermissionMode("ask"), "ask");
  assert.equal(normalizeChatPermissionMode("auto"), "auto");
  assert.equal(normalizeChatPermissionMode(undefined), "explore");
  assert.equal(normalizeChatPermissionMode("weird"), "explore");
});
