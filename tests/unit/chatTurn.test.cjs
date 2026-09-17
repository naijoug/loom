const test = require("node:test");
const assert = require("node:assert/strict");

/** Mirror of src/features/chat/chatTurn.ts */
const CHAT_TURN_TIMEOUT_MS = 10 * 60 * 1000;

function isChatTurnRunning(input) {
  return Boolean(input.sending || input.turnStatus === "streaming");
}

function formatTurnElapsed(elapsedMs) {
  const totalSeconds = Math.max(0, Math.floor(elapsedMs / 1000));
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  const mm = hours > 0 ? String(minutes).padStart(2, "0") : String(minutes);
  const ss = String(seconds).padStart(2, "0");
  if (hours > 0) {
    return `${hours}:${mm}:${ss}`;
  }
  return `${mm}:${ss}`;
}

function turnRunningHint(elapsedMs) {
  if (elapsedMs == null || elapsedMs < 0) {
    return "生成中…";
  }
  return `生成中 · ${formatTurnElapsed(elapsedMs)}`;
}

test("chat turn timeout constant matches 10 minute product default", () => {
  assert.equal(CHAT_TURN_TIMEOUT_MS, 600_000);
});

test("isChatTurnRunning uses sending or streaming turnStatus", () => {
  assert.equal(isChatTurnRunning({ sending: false, turnStatus: "idle" }), false);
  assert.equal(isChatTurnRunning({ sending: true, turnStatus: "idle" }), true);
  assert.equal(isChatTurnRunning({ sending: false, turnStatus: "streaming" }), true);
});

test("formatTurnElapsed renders m:ss and h:mm:ss", () => {
  assert.equal(formatTurnElapsed(0), "0:00");
  assert.equal(formatTurnElapsed(5_000), "0:05");
  assert.equal(formatTurnElapsed(65_000), "1:05");
  assert.equal(formatTurnElapsed(3_661_000), "1:01:01");
});

test("turnRunningHint includes elapsed text", () => {
  assert.equal(turnRunningHint(null), "生成中…");
  assert.match(turnRunningHint(12_000), /生成中 · 0:12/);
});
