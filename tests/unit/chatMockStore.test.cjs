const test = require("node:test");
const assert = require("node:assert/strict");

function now() {
  return Date.now();
}

function id(prefix) {
  return `${prefix}-${Math.random().toString(36).slice(2, 10)}`;
}

class MockChatStore {
  constructor() {
    this.sessions = new Map();
  }

  list(projectPath) {
    return [...this.sessions.values()]
      .filter((session) => session.projectPath === projectPath)
      .sort((a, b) => b.updatedAtMs - a.updatedAtMs)
      .map((session) => ({
        id: session.id,
        title: session.title,
        agentId: session.agentId,
        updatedAtMs: session.updatedAtMs,
        preview: session.messages.length
          ? session.messages[session.messages.length - 1].content.slice(0, 80)
          : undefined,
        status: session.status ?? "active",
      }));
  }

  create(input) {
    const createdAtMs = now();
    const session = {
      id: id("chat"),
      projectPath: input.projectPath,
      agentId: input.agentId,
      title: input.title?.trim() || "新对话",
      permissionMode: input.permissionMode ?? "explore",
      status: "active",
      messages: [],
      createdAtMs,
      updatedAtMs: createdAtMs,
      turnStatus: "idle",
      schemaVersion: 1,
    };
    this.sessions.set(session.id, session);
    return session;
  }

  setStatus(sessionId, status) {
    const session = this.sessions.get(sessionId);
    if (!session) return undefined;
    const next = {
      ...session,
      status: status === "archived" ? "archived" : "active",
      updatedAtMs: now(),
    };
    this.sessions.set(sessionId, next);
    return next;
  }

  setPermissionMode(sessionId, permissionMode) {
    const session = this.sessions.get(sessionId);
    if (!session) return undefined;
    const next = { ...session, permissionMode, updatedAtMs: now() };
    this.sessions.set(sessionId, next);
    return next;
  }

  send(sessionId, text) {
    const session = this.sessions.get(sessionId);
    if (!session) return undefined;
    const trimmed = text.trim();
    if (!trimmed) return session;
    const t = now();
    const user = {
      id: id("msg"),
      role: "user",
      content: trimmed,
      status: "complete",
      createdAtMs: t,
    };
    const assistant = {
      id: id("msg"),
      role: "assistant",
      content: `echo:${trimmed}`,
      status: "complete",
      createdAtMs: t + 1,
    };
    const title =
      session.messages.length === 0 && session.title === "新对话"
        ? trimmed.slice(0, 32)
        : session.title;
    const next = {
      ...session,
      title,
      messages: [...session.messages, user, assistant],
      updatedAtMs: t + 1,
      turnStatus: "idle",
    };
    this.sessions.set(sessionId, next);
    return next;
  }
}

test("mock chat store creates sessions and mock replies", () => {
  const store = new MockChatStore();
  const session = store.create({
    projectPath: "/tmp/demo",
    agentId: "agent-codex",
  });
  assert.equal(session.permissionMode, "explore");
  assert.equal(session.status, "active");
  assert.equal(store.list("/tmp/demo").length, 1);
  const after = store.send(session.id, "hello loom");
  assert.ok(after);
  assert.equal(after.messages.length, 2);
  assert.equal(after.messages[0].role, "user");
  assert.equal(after.messages[1].role, "assistant");
  assert.match(after.title, /hello/);
});

test("mock chat store archives sessions and filters via status", () => {
  const store = new MockChatStore();
  const session = store.create({
    projectPath: "/tmp/demo",
    agentId: "agent-codex",
  });
  store.setStatus(session.id, "archived");
  store.setPermissionMode(session.id, "auto");
  const listed = store.list("/tmp/demo");
  assert.equal(listed.length, 1);
  assert.equal(listed[0].status, "archived");
  assert.equal(store.sessions.get(session.id).permissionMode, "auto");
  assert.equal(store.sessions.get(session.id).status, "archived");
});
