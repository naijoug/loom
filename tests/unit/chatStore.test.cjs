const test = require("node:test");
const assert = require("node:assert/strict");
const {
  createChatStoreSnapshot,
  chatStoreSelectProject,
  chatStoreSetSession,
  chatStoreSetSummaries,
  chatStoreSetDraft,
  chatStoreDraftFor,
  chatStoreBumpGeneration,
} = require("../../.tmp/test-build/src/features/chat/state/chatStore.js");

test("switching project bumps generation and clears selection", () => {
  let state = createChatStoreSnapshot("/a");
  state = { ...state, selectedSessionId: "s1", session: { id: "s1" } };
  state = chatStoreSelectProject(state, "/b");
  assert.equal(state.projectPath, "/b");
  assert.equal(state.selectedSessionId, null);
  assert.equal(state.session, null);
  assert.equal(state.generation, 1);
});

test("stale generation cannot overwrite session or summaries", () => {
  let state = createChatStoreSnapshot("/a");
  state = chatStoreBumpGeneration(state);
  const gen = state.generation;
  state = chatStoreSetSummaries(state, [{ id: "s1", title: "ok" }], gen);
  assert.equal(state.summaries.length, 1);
  state = chatStoreSetSummaries(state, [{ id: "stale", title: "no" }], gen - 1);
  assert.equal(state.summaries[0].id, "s1");
  state = chatStoreSetSession(state, { id: "s1" }, gen, "s1");
  state = chatStoreSetSession(state, { id: "other" }, gen - 1, "s1");
  assert.equal(state.session.id, "s1");
});

test("drafts are per session", () => {
  let state = createChatStoreSnapshot("/a");
  state = chatStoreSetDraft(state, "s1", "hello");
  state = chatStoreSetDraft(state, "s2", "world");
  assert.equal(chatStoreDraftFor(state, "s1"), "hello");
  assert.equal(chatStoreDraftFor(state, "s2"), "world");
});
