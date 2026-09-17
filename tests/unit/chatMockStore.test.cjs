const test = require("node:test");
const assert = require("node:assert/strict");
const { MockChatStore } = require("../../.tmp/test-build/src/features/chat/mockStore.js");

test("MockChatStore create/list/send roundtrip", () => {
  const store = new MockChatStore();
  const created = store.create({ projectPath: "/p", agentId: "grok" });
  assert.equal(created.agentId, "grok");
  assert.equal(store.list("/p").length, 1);
  const next = store.send(created.id, "hello");
  assert.ok(next.messages.length >= 2);
  assert.equal(store.list("/other").length, 0);
});

test("MockChatStore archive and title helpers", () => {
  const store = new MockChatStore();
  const created = store.create({ projectPath: "/p", agentId: "grok" });
  store.setStatus(created.id, "archived");
  store.setTitle(created.id, "重命名");
  const got = store.get(created.id);
  assert.equal(got.status, "archived");
  assert.equal(got.title, "重命名");
});
