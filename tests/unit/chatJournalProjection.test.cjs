const test = require("node:test");
const assert = require("node:assert/strict");
const { ChatProjection } = require("../../.tmp/test-build/src/features/chat/state/ChatProjection.js");
const { createChatStoreSnapshot, chatStoreSelectProject, chatStoreSetDraft, chatStoreDraftFor } = require("../../.tmp/test-build/src/features/chat/state/chatStore.js");

function snapshot(projectPath = "/a", id = "s", seq = 2) {
  return { id, projectPath, agentId: "grok", title: "chat", permissionMode: "explore", messages: [{ id: "m", role: "assistant", content: "", status: "streaming", createdAtMs: 1 }], createdAtMs: 1, updatedAtMs: 1, activeTurnId: "t", turnStatus: "streaming", schemaVersion: 2, lastSeq: seq, revision: seq };
}
function stream(seq, delta, overrides = {}) {
  return { schemaVersion: 2, projectKey: "/a", sessionId: "s", seq, timestampMs: seq, kind: "stream", payload: { sessionId: "s", turnId: "t", messageId: "m", delta, done: false }, ...overrides };
}

test("out-of-order and duplicate durable events are applied exactly once", () => {
  const p = new ChatProjection("/a", "s");
  p.acceptSnapshot(snapshot());
  p.receive(stream(4, "B"));
  assert.equal(p.session.messages[0].content, "");
  assert.equal(p.needsReplay, true);
  p.receive(stream(3, "A"));
  p.receive(stream(3, "A"));
  assert.equal(p.session.messages[0].content, "AB");
  assert.equal(p.lastSeq, 4);
  assert.equal(p.needsReplay, false);
});

test("events arriving during snapshot load survive and stale responses cannot erase them", () => {
  const p = new ChatProjection("/a", "s");
  p.receive(stream(3, "早到"));
  assert.equal(p.session, null);
  p.acceptSnapshot(snapshot());
  assert.equal(p.session.messages[0].content, "早到");
  assert.equal(p.acceptSnapshot(snapshot()), false);
  assert.equal(p.session.messages[0].content, "早到");
  const recovered = snapshot("/a", "s", 4);
  recovered.messages[0].content = "authoritative";
  p.acceptSnapshot(recovered);
  p.receive(stream(3, "早到"));
  assert.equal(p.session.messages[0].content, "authoritative");
});

test("other projects/sessions and stale turn payloads cannot contaminate a selection", () => {
  const p = new ChatProjection("/a", "s");
  p.acceptSnapshot(snapshot());
  p.receive(stream(3, "wrong", { projectKey: "/b" }));
  p.receive(stream(3, "wrong", { sessionId: "other" }));
  assert.equal(p.lastSeq, 2);
  p.receive({ ...stream(3, "wrong"), payload: { ...stream(3, "wrong").payload, turnId: "old-turn" } });
  assert.equal(p.lastSeq, 2);
  assert.equal(p.session.messages[0].content, "");
  assert.equal(p.needsSnapshot, true);
});

test("patches persist terminal text, clear optional handles and reject identity mutation", () => {
  const p = new ChatProjection("/a", "s");
  p.acceptSnapshot(snapshot());
  p.receive({ ...stream(3, ""), kind: "session_patch", payload: {
    fields: { title: "renamed", turnStatus: "idle", activeTurnId: null },
    changedMessages: [{ ...snapshot().messages[0], content: "final", status: "complete" }], appendedMessages: [],
  } });
  assert.equal(p.session.activeTurnId, undefined);
  assert.equal(p.session.turnStatus, "idle");
  assert.equal(p.session.messages[0].content, "final");
  p.receive({ ...stream(4, ""), kind: "session_patch", payload: { fields: { projectPath: "/evil" }, changedMessages: [], appendedMessages: [] } });
  assert.equal(p.session.projectPath, "/a");
  assert.equal(p.needsSnapshot, true);
});

test("new sessions and restored snapshots establish the cursor without replaying old deltas", () => {
  const p = new ChatProjection("/a", "s");
  const initial = snapshot("/a", "s", 1);
  p.receive({ ...stream(1, ""), kind: "session_created", payload: initial });
  assert.equal(p.lastSeq, 1);
  p.receive(stream(2, "一次"));
  p.acceptSnapshot({ ...p.session, messages: p.session.messages.map((m) => ({ ...m })) });
  p.receive(stream(2, "一次"));
  assert.equal(p.session.messages[0].content, "一次");
});

test("drafts remain isolated and survive switching away from a project", () => {
  let state = createChatStoreSnapshot("/a");
  state = chatStoreSetDraft(state, "same-id", "A draft");
  state = chatStoreSelectProject(state, "/b");
  assert.equal(chatStoreDraftFor(state, "same-id"), "");
  state = chatStoreSetDraft(state, "same-id", "B draft");
  state = chatStoreSelectProject(state, "/a");
  assert.equal(chatStoreDraftFor(state, "same-id"), "A draft");
});

module.exports = { snapshot, stream };

test("a late send acknowledgment clears only the submitted draft in its own project", () => {
  const { chatStoreClearSubmittedDraft } = require("../../.tmp/test-build/src/features/chat/state/chatStore.js");
  let state = createChatStoreSnapshot("/a");
  state = chatStoreSetDraft(state, "s", "sent A");
  state = chatStoreSelectProject(state, "/b");
  state = chatStoreSetDraft(state, "s", "new B");
  state = chatStoreClearSubmittedDraft(state, "/a", "s", "sent A");
  assert.equal(chatStoreDraftFor(state, "s"), "new B");
  state = chatStoreSelectProject(state, "/a");
  assert.equal(chatStoreDraftFor(state, "s"), "");
  state = chatStoreSetDraft(state, "s", "next draft");
  state = chatStoreClearSubmittedDraft(state, "/a", "s", "old submitted");
  assert.equal(chatStoreDraftFor(state, "s"), "next draft");
});
