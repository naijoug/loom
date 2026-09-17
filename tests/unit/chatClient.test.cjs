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
