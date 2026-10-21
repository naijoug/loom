const test = require("node:test");
const assert = require("node:assert/strict");

const {
  chatMessageErrorFallbackText,
  chatSessionNeedsAttention,
  titleFromUserMessage,
} = require("../../.tmp/test-build/src/domain/chat.js");

test("titleFromUserMessage truncates on word boundary", () => {
  const title = titleFromUserMessage(
    "hello world this is a fairly long first line that should wrap",
    24,
  );
  assert.ok(title.endsWith("…"), title);
  assert.equal(title.includes("wrap"), false);
  assert.ok(title.startsWith("hello"), title);
});

test("titleFromUserMessage uses first non-empty line", () => {
  assert.equal(titleFromUserMessage("\n\n  alpha beta  \nsecond"), "alpha beta");
});

test("needsAttention from flag or error, not archived", () => {
  assert.equal(chatSessionNeedsAttention({ status: "active", flagged: true }), true);
  assert.equal(
    chatSessionNeedsAttention({
      status: "active",
      messages: [{ status: "error" }],
    }),
    true,
  );
  assert.equal(
    chatSessionNeedsAttention({
      status: "archived",
      flagged: true,
      messages: [{ status: "error" }],
    }),
    false,
  );
});

test("message error fallback stays visible beside structured tool parts", () => {
  assert.equal(
    chatMessageErrorFallbackText({
      status: "error",
      errorSummary: "Codex exited 1",
      parts: [{ type: "tool" }],
    }),
    "（调用失败）Codex exited 1",
  );
  assert.equal(
    chatMessageErrorFallbackText({
      status: "error",
      content: "partial stdout",
      errorSummary: "Codex exited 1",
      parts: [{ type: "tool" }],
    }),
    "（调用失败）Codex exited 1",
  );
  assert.equal(
    chatMessageErrorFallbackText({
      status: "error",
      errorSummary: "Codex exited 1",
      parts: [{ type: "error" }],
    }),
    undefined,
  );
});
