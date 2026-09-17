const test = require("node:test");
const assert = require("node:assert/strict");

function titleFromUserMessage(text, maxLen = 48) {
  const firstLine =
    text
      .split(/\r?\n/)
      .map((line) => line.trim())
      .find((line) => line.length > 0) ?? text.trim();
  const collapsed = firstLine.replace(/\s+/g, " ").trim();
  if (!collapsed) return "新对话";
  if ([...collapsed].length <= maxLen) return collapsed;
  const chars = [...collapsed];
  let cut = maxLen;
  for (let i = maxLen; i >= Math.floor(maxLen * 0.5); i -= 1) {
    if (/\s/.test(chars[i] ?? "")) {
      cut = i;
      break;
    }
  }
  const sliced = chars.slice(0, cut).join("").trimEnd();
  return `${sliced}…`;
}

function chatSessionNeedsAttention(session) {
  const status = session.status ?? "active";
  if (status === "archived") return false;
  if (session.flagged) return true;
  if (session.turnStatus === "error") return true;
  return (session.messages ?? []).some((message) => message.status === "error");
}

function filterChatSummaries(summaries, filter) {
  if (filter === "archived") {
    return summaries.filter((item) => (item.status ?? "active") === "archived");
  }
  if (filter === "needs_attention") {
    return summaries.filter(
      (item) =>
        (item.status ?? "active") === "active" &&
        Boolean(item.needsAttention || item.flagged),
    );
  }
  return summaries.filter((item) => (item.status ?? "active") === "active");
}

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

test("inbox filters active / needs_attention / archived", () => {
  const summaries = [
    { id: "1", status: "active", needsAttention: false },
    { id: "2", status: "active", needsAttention: true },
    { id: "3", status: "archived", needsAttention: true, flagged: true },
    { id: "4", status: "active", flagged: true },
  ];
  assert.deepEqual(
    filterChatSummaries(summaries, "active").map((item) => item.id),
    ["1", "2", "4"],
  );
  assert.deepEqual(
    filterChatSummaries(summaries, "needs_attention").map((item) => item.id),
    ["2", "4"],
  );
  assert.deepEqual(
    filterChatSummaries(summaries, "archived").map((item) => item.id),
    ["3"],
  );
});
