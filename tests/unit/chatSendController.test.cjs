const test = require("node:test");
const assert = require("node:assert/strict");
const { ChatSendController } = require("../../.tmp/test-build/src/features/chat/state/ChatSendController.js");
const input = { projectPath: "/a", sessionId: "s", text: "  exact text\n中文  ", permissionMode: "explore" };

test("ambiguous IPC failure retains request identity, success permits an intentional new submission", async () => {
  const calls = []; let ids = 0;
  const controller = new ChatSendController(async (args) => {
    calls.push(args);
    if (calls.length === 1) throw new Error("reply lost after server accepted");
    return { turnId: "original-turn", session: { id: "s" } };
  }, () => `request-${++ids}`);
  await assert.rejects(controller.send(input), /reply lost/);
  const receipt = await controller.send(input);
  assert.equal(receipt.turnId, "original-turn");
  assert.equal(calls[0].clientRequestId, calls[1].clientRequestId);
  assert.equal(calls[1].text, input.text);
  await controller.send(input);
  assert.notEqual(calls[2].clientRequestId, calls[1].clientRequestId);
});

test("double submit shares the pending operation and project/session/content/permission are isolated", async () => {
  let release; const gate = new Promise((r) => { release = r; });
  const calls = []; let ids = 0;
  const controller = new ChatSendController(async (args) => {
    calls.push(args); await gate; throw new Error("uncertain");
  }, () => `request-${++ids}`);
  const a = controller.send(input); const b = controller.send(input);
  assert.equal(a, b);
  const variants = [{ projectPath: "/b" }, { sessionId: "other" }, { text: "changed" }, { permissionMode: "auto" }];
  const pending = variants.map((change) => controller.send({ ...input, ...change }));
  const result = Promise.allSettled([a, b, ...pending]); release(); await result;
  assert.equal(calls.length, 5);
  assert.equal(new Set(calls.map((c) => c.clientRequestId)).size, 5);
  await assert.rejects(controller.send(input), /uncertain/);
  assert.equal(calls[5].clientRequestId, calls[0].clientRequestId);
});

test("production controller and wrapper forward the same request ID through real IPC on retry", async () => {
  const prior = global.window; const calls = [];
  global.window = { __TAURI_INTERNALS__: { invoke: async (command, args) => {
    calls.push({ command, args });
    if (calls.length === 1) throw new Error("transport failure");
    return { turnId: "t", session: { id: "s" } };
  } } };
  try {
    const controller = new ChatSendController();
    await assert.rejects(controller.send(input), /transport failure/);
    await controller.send(input);
    assert.equal(calls[0].command, "chat_send");
    assert.match(calls[0].args.input.clientRequestId, /^[a-f0-9-]{36}$/);
    assert.deepEqual(calls[1], calls[0]);
  } finally { global.window = prior; }
});
