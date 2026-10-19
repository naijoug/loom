const test = require("node:test");
const assert = require("node:assert/strict");

const {
  chatInputEnvelope,
  chatCreate,
  chatUpdateMeta,
  chatSend,
  chatAbort,
  chatPromoteToTask,
  chatListSessions,
  chatGet,
  chatSetAgent,
  chatClearResume,
} = require("../../.tmp/test-build/src/api/chatClient.js");
const { TAURI_COMMANDS } = require("../../.tmp/test-build/src/api/contract.js");

test("chatInputEnvelope wraps fields under input", () => {
  assert.deepEqual(chatInputEnvelope({ projectPath: "/p", agentId: "a" }), {
    input: { projectPath: "/p", agentId: "a" },
  });
});

test("five input-envelope chat commands use chat_* names and input wrapper shape", () => {
  const cases = [
    {
      command: TAURI_COMMANDS.chatCreate,
      payload: chatInputEnvelope({
        projectPath: "/proj",
        agentId: "grok",
        permissionMode: "explore",
      }),
    },
    {
      command: TAURI_COMMANDS.chatUpdateMeta,
      payload: chatInputEnvelope({
        projectPath: "/proj",
        sessionId: "s1",
        title: "hi",
        titleFromFirstMessage: true,
      }),
    },
    {
      command: TAURI_COMMANDS.chatSend,
      payload: chatInputEnvelope({
        projectPath: "/proj",
        sessionId: "s1",
        text: "hello",
        permissionMode: "ask",
      }),
    },
    {
      command: TAURI_COMMANDS.chatAbort,
      payload: chatInputEnvelope({
        projectPath: "/proj",
        sessionId: "s1",
        turnId: "t1",
      }),
    },
    {
      command: TAURI_COMMANDS.chatPromoteToTask,
      payload: chatInputEnvelope({
        projectPath: "/proj",
        sessionId: "s1",
      }),
    },
  ];

  for (const item of cases) {
    assert.equal(Object.keys(item.payload).join(","), "input");
    assert.match(item.command, /^chat_/);
  }
  assert.equal(cases.length, 5);
});

test("flat-arg chat commands stay flat (no input wrapper)", () => {
  assert.equal(TAURI_COMMANDS.chatListSessions, "chat_list_sessions");
  assert.equal(TAURI_COMMANDS.chatGet, "chat_get");
  assert.equal(TAURI_COMMANDS.chatSetAgent, "chat_set_agent");
  assert.equal(TAURI_COMMANDS.chatClearResume, "chat_clear_resume");
});

test("chatClient wrappers are exported", () => {
  for (const fn of [
    chatCreate,
    chatUpdateMeta,
    chatSend,
    chatAbort,
    chatPromoteToTask,
    chatListSessions,
    chatGet,
    chatSetAgent,
    chatClearResume,
  ]) {
    assert.equal(typeof fn, "function");
  }
});

test("production Chat wrappers invoke the real transport with the correct envelopes and replay cursor", async () => {
  const { chatReadEvents, chatReadRunLogs, chatExport } = require("../../.tmp/test-build/src/api/chatClient.js");
  const previous = global.window;
  const calls = [];
  global.window = { __TAURI_INTERNALS__: { invoke: async (command, args) => { calls.push([command, JSON.parse(JSON.stringify(args))]); return {}; } } };
  try {
    await chatCreate({ projectPath: "/a", agentId: "g" });
    await chatUpdateMeta({ projectPath: "/a", sessionId: "s", title: "new" });
    await chatSend({ projectPath: "/a", sessionId: "s", clientRequestId: "req-1", text: "hi" });
    await chatAbort({ projectPath: "/a", sessionId: "s", turnId: "t" });
    await chatPromoteToTask({ projectPath: "/a", sessionId: "s" });
    await chatGet("/a", "s");
    await chatReadEvents("/a", "s", 17, 50);
    await chatReadRunLogs("/a", "s", "t", "stderr", 21, 4096);
    await chatExport({ projectPath: "/a", sessionId: "s" });
    assert.deepEqual(calls, [
      ["chat_create", { input: { projectPath: "/a", agentId: "g" } }],
      ["chat_update_meta", { input: { projectPath: "/a", sessionId: "s", title: "new" } }],
      ["chat_send", { input: { projectPath: "/a", sessionId: "s", clientRequestId: "req-1", text: "hi" } }],
      ["chat_abort", { input: { projectPath: "/a", sessionId: "s", turnId: "t" } }],
      ["chat_promote_to_task", { input: { projectPath: "/a", sessionId: "s" } }],
      ["chat_get", { projectPath: "/a", sessionId: "s" }],
      ["chat_read_events", { projectPath: "/a", sessionId: "s", afterSeq: 17, limit: 50 }],
      ["chat_read_run_logs", { projectPath: "/a", sessionId: "s", turnId: "t", stream: "stderr", offset: 21, limit: 4096 }],
      ["chat_export", { input: { projectPath: "/a", sessionId: "s" } }],
    ]);
  } finally { global.window = previous; }
});
