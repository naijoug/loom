const test = require("node:test");
const assert = require("node:assert/strict");

const { cyclePermissionMode, askTurnRequiresConfirm, permissionCliHint } = require("../../.tmp/test-build/src/features/chat/chatPermission.js");
const { chatPermissionAllowsWrite } = require("../../.tmp/test-build/src/domain/chat.js");

test("cyclePermissionMode explore → ask → auto → explore", () => {
  assert.equal(cyclePermissionMode("explore"), "ask");
  assert.equal(cyclePermissionMode("ask"), "auto");
  assert.equal(cyclePermissionMode("auto"), "explore");
});

test("askTurnRequiresConfirm only for ask", () => {
  assert.equal(askTurnRequiresConfirm("ask"), true);
  assert.equal(askTurnRequiresConfirm("explore"), false);
  assert.equal(askTurnRequiresConfirm("auto"), false);
});

test("permissionCliHint stays truthful with write capability", () => {
  assert.match(permissionCliHint("explore"), /只读|explore|只读|只读|只读|read/i);
  assert.equal(chatPermissionAllowsWrite("ask"), true);
  assert.equal(chatPermissionAllowsWrite("auto"), true);
});
