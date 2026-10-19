const test = require("node:test");
const assert = require("node:assert/strict");
const {
  CHAT_TURN_TIMEOUT_MS,
  isChatTurnRunning,
  formatTurnElapsed,
  turnRunningHint,
} = require("../../.tmp/test-build/src/features/chat/chatTurn.js");

test("CHAT_TURN_TIMEOUT_MS is 10 minutes", () => {
  assert.equal(CHAT_TURN_TIMEOUT_MS, 10 * 60 * 1000);
});

test("isChatTurnRunning uses sending or streaming", () => {
  assert.equal(isChatTurnRunning({ sending: true, turnStatus: "idle" }), true);
  assert.equal(isChatTurnRunning({ sending: false, turnStatus: "streaming" }), true);
  assert.equal(isChatTurnRunning({ sending: false, turnStatus: "idle" }), false);
});

test("formatTurnElapsed and turnRunningHint", () => {
  assert.equal(formatTurnElapsed(5_000), "0:05");
  assert.equal(formatTurnElapsed(65_000), "1:05");
  assert.equal(turnRunningHint(null), "生成中…");
  assert.match(turnRunningHint(12_000), /生成中/);
});
