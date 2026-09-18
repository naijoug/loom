const test = require("node:test");
const assert = require("node:assert/strict");

const {
  chatInboxEmptyMessage,
  chatInboxSearchPlaceholder,
  chatSessionNeedsAttention,
  filterChatSummaries,
  filterChatSummariesByQuery,
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

test("inbox query filters title and preview without changing empty query order", () => {
  const summaries = [
    { id: "1", title: "继续修复 CLI", preview: "Codex stdout is green" },
    { id: "2", title: "UI polish", preview: "permission menu still needs copy" },
    { id: "3", title: "Release note", preview: "desktop smoke passed" },
  ];
  assert.deepEqual(
    filterChatSummariesByQuery(summaries, "cli").map((item) => item.id),
    ["1"],
  );
  assert.deepEqual(
    filterChatSummariesByQuery(summaries, "PERMISSION").map((item) => item.id),
    ["2"],
  );
  assert.deepEqual(filterChatSummariesByQuery(summaries, "  "), summaries);
});

test("inbox query can include visible fallback metadata", () => {
  const summaries = [
    { id: "1", title: "Continue CLI fix", agentId: "codex", preview: "green logs" },
    { id: "2", title: "Release review", agentId: "openclaw" },
    { id: "3", title: "Visual polish", agentId: "hermes" },
  ];

  assert.deepEqual(
    filterChatSummariesByQuery(
      summaries,
      "OpenClaw",
      (item) => ({ openclaw: "OpenClaw 本地助手" })[item.agentId] ?? item.agentId,
    ).map((item) => item.id),
    ["2"],
  );
});

test("inbox search placeholder lists all searchable visible fields", () => {
  assert.equal(chatInboxSearchPlaceholder(), "搜索标题、内容或 Agent…");
});

test("inbox empty copy distinguishes empty tabs from empty search results", () => {
  assert.equal(
    chatInboxEmptyMessage({ filter: "active", searchQuery: "", useBackend: true }),
    "还没有会话。点「新建」开始。 将通过本机 Agent CLI 流式回复。",
  );
  assert.equal(
    chatInboxEmptyMessage({ filter: "archived", searchQuery: "", useBackend: true }),
    "没有已归档会话。",
  );
  assert.equal(
    chatInboxEmptyMessage({ filter: "needs_attention", searchQuery: "", useBackend: false }),
    "没有需要关注的会话。",
  );
  assert.equal(
    chatInboxEmptyMessage({ filter: "active", searchQuery: "  openclaw  ", useBackend: false }),
    "没有匹配的会话。试试换个关键词或清除搜索。",
  );
});
