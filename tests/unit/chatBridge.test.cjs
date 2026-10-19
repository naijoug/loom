require.extensions[".css"] = () => undefined;
const test = require("node:test");
const assert = require("node:assert/strict");
const React = require("react");
const { act } = React;
const { createRoot } = require("react-dom/client");
const { JSDOM } = require("jsdom");
const { useChatBridge } = require("../../.tmp/test-build/src/features/chat/useChatBridge.js");
const { ChatComposer } = require("../../.tmp/test-build/src/features/chat/ChatComposer.js");
global.IS_REACT_ACT_ENVIRONMENT = true;
function deferred() { let resolve; const promise = new Promise((r) => { resolve = r; }); return { promise, resolve }; }
function snapshot(projectPath = "/a", id = "s", seq = 2) {
  return { id, projectPath, agentId: "grok", title: "chat", permissionMode: "explore", messages: [{ id: "m", role: "assistant", content: "", status: "streaming", createdAtMs: 1 }], createdAtMs: 1, updatedAtMs: 1, activeTurnId: "t", turnStatus: "streaming", schemaVersion: 2, lastSeq: seq, revision: seq };
}
function stream(seq, delta, projectKey = "/a", sessionId = "s") { return { schemaVersion: 2, projectKey, sessionId, seq, timestampMs: seq, kind: "stream", payload: { sessionId, turnId: "t", messageId: "m", delta, done: false } }; }
async function fixture(t) {
  const dom = new JSDOM("<div id='root'></div>");
  global.window = dom.window; global.document = dom.window.document; global.navigator = dom.window.navigator;
  const root = createRoot(document.getElementById("root"));
  t.after(async () => { await act(async () => root.unmount()); dom.window.close(); });
  return { root, dom, render: async (element) => act(async () => root.render(element)) };
}
function fakeApi() {
  let eventHandler, errorHandler;
  const api = {
    get: async (project, id) => snapshot(project, id),
    readEvents: async (_, __, after) => ({ events: [], hasMore: false, lastSeq: after }),
    listen: async (handler) => { eventHandler = handler; return () => {}; },
    listenErrors: async (handler) => { errorHandler = handler; return () => {}; },
  };
  return { api, emit: (event) => eventHandler(event), error: (event) => errorHandler(event) };
}

test("production bridge waits for listeners and buffers events before get resolves", async (t) => {
  const f = await fixture(t); const wire = fakeApi(); const listener = deferred(); const get = deferred();
  let latest, gets = 0;
  const baseListen = wire.api.listen;
  wire.api.listen = async (handler) => { await listener.promise; return baseListen(handler); };
  wire.api.get = async () => { gets += 1; return get.promise; };
  function Harness() { latest = useChatBridge("/a", "s", true, assert.fail, () => {}, wire.api); return null; }
  await f.render(React.createElement(Harness));
  assert.equal(latest.listenReady, false); assert.equal(gets, 0);
  await act(async () => listener.resolve());
  assert.equal(latest.listenReady, true);
  await act(async () => wire.emit(stream(3, "early")));
  await act(async () => get.resolve(snapshot()));
  assert.equal(latest.session.messages[0].content, "early");
  assert.equal(latest.session.lastSeq, 3);
  await act(async () => latest.setSession(snapshot()));
  assert.equal(latest.session.messages[0].content, "early");
});

test("production bridge ignores late A loads/errors after selecting B and replays sequence gaps", async (t) => {
  const f = await fixture(t); const wire = fakeApi(); const loadA = deferred(); const errors = [];
  let latest, replays = 0;
  wire.api.get = async (project, id) => id === "a" ? loadA.promise : snapshot(project, id);
  wire.api.readEvents = async (project, id, after) => {
    replays += 1;
    return { events: after === 2 ? [stream(3, "B", project, id), stream(4, "4", project, id)] : [], hasMore: false, lastSeq: 4 };
  };
  function Harness({ id }) { latest = useChatBridge("/a", id, true, (e) => errors.push(e), () => {}, wire.api); return null; }
  await f.render(React.createElement(Harness, { id: "a" }));
  await f.render(React.createElement(Harness, { id: "b" }));
  await act(async () => loadA.resolve(snapshot("/a", "a")));
  assert.equal(latest.session.id, "b");
  await act(async () => wire.error({ projectKey: "/a", sessionId: "a", turnId: "t", errorSummary: "old failure" }));
  await act(async () => wire.emit(stream(4, "4", "/a", "b")));
  assert.equal(latest.session.id, "b");
  assert.equal(latest.session.messages[0].content, "B4");
  assert.ok(replays > 0); assert.deepEqual(errors, []);
});

test("production bridge catches a missed final event on focus and reports durable changes", async (t) => {
  const f = await fixture(t); const wire = fakeApi(); let latest; let changed = 0;
  wire.api.readEvents = async () => ({ events: [{ ...stream(3, ""), kind: "session_patch", payload: { fields: { turnStatus: "idle", activeTurnId: null }, changedMessages: [{ ...snapshot().messages[0], content: "done", status: "complete" }], appendedMessages: [] } }], hasMore: false, lastSeq: 3 });
  function Harness() { latest = useChatBridge("/a", "s", true, assert.fail, () => { changed += 1; }, wire.api); return null; }
  await f.render(React.createElement(Harness));
  await act(async () => window.dispatchEvent(new window.Event("focus")));
  assert.equal(latest.session.turnStatus, "idle");
  assert.equal(latest.session.messages[0].content, "done");
  assert.equal(changed, 1);
});

test("production bridge reports listener initialization failures without marking the listener ready", async (t) => {
  const f = await fixture(t); const wire = fakeApi(); const errors = [];
  let latest;
  wire.api.listen = async () => { throw new Error("chat event listener failed"); };
  function Harness() { latest = useChatBridge("/a", "s", true, (message) => errors.push(message), () => {}, wire.api); return null; }
  await f.render(React.createElement(Harness));
  await act(async () => {});
  assert.equal(latest.listenReady, false);
  assert.equal(latest.session, null);
  assert.deepEqual(errors, ["chat event listener failed"]);
});

test("production bridge reports only errors for the active turn", async (t) => {
  const f = await fixture(t); const wire = fakeApi(); const errors = [];
  function Harness() { useChatBridge("/a", "s", true, (message) => errors.push(message), () => {}, wire.api); return null; }
  await f.render(React.createElement(Harness));
  await act(async () => wire.error({ projectKey: "/a", sessionId: "s", turnId: "old-turn", errorSummary: "stale failure" }));
  assert.deepEqual(errors, []);
  await act(async () => wire.error({ projectKey: "/a", sessionId: "s", turnId: "t", errorSummary: "active failure" }));
  assert.deepEqual(errors, ["active failure"]);
});

test("production bridge drains paged recovery using the updated cursor", async (t) => {
  const f = await fixture(t); const wire = fakeApi(); let latest; const afters = [];
  wire.api.readEvents = async (_project, _id, after) => {
    afters.push(after);
    if (after === 2) return { events: [stream(3, "A")], hasMore: true, lastSeq: 3 };
    if (after === 3) return { events: [stream(4, "B")], hasMore: false, lastSeq: 4 };
    return { events: [], hasMore: false, lastSeq: after };
  };
  function Harness() { latest = useChatBridge("/a", "s", true, assert.fail, () => {}, wire.api); return null; }
  await f.render(React.createElement(Harness));
  await act(async () => window.dispatchEvent(new window.Event("focus")));
  assert.deepEqual(afters, [2, 3]);
  assert.equal(latest.session.messages[0].content, "AB");
  assert.equal(latest.session.lastSeq, 4);
});

test("production bridge falls back to a fresh snapshot after the recovery page cap", async (t) => {
  const f = await fixture(t); const wire = fakeApi(); let latest; let gets = 0; let reads = 0;
  wire.api.get = async (project, id) => {
    gets += 1;
    return { ...snapshot(project, id), updatedAtMs: gets, revision: gets };
  };
  wire.api.readEvents = async (_project, _id, after) => {
    reads += 1;
    return { events: [], hasMore: true, lastSeq: after };
  };
  function Harness() { latest = useChatBridge("/a", "s", true, assert.fail, () => {}, wire.api); return null; }
  await f.render(React.createElement(Harness));
  assert.equal(gets, 1);
  await act(async () => window.dispatchEvent(new window.Event("focus")));
  assert.equal(reads, 50);
  assert.equal(gets, 2);
  assert.equal(latest.session.revision, 2);
});

test("production bridge refreshes from snapshot when recovery marks projection stale", async (t) => {
  const f = await fixture(t); const wire = fakeApi(); let latest; let gets = 0; let changed = 0;
  wire.api.get = async (project, id) => {
    gets += 1;
    return { ...snapshot(project, id), messages: [{ ...snapshot(project, id).messages[0], content: gets === 1 ? "" : "fresh" }], revision: gets };
  };
  wire.api.readEvents = async () => ({
    events: [{ ...stream(3, "ignored"), schemaVersion: 1 }],
    hasMore: false,
    lastSeq: 3,
  });
  function Harness() { latest = useChatBridge("/a", "s", true, assert.fail, () => { changed += 1; }, wire.api); return null; }
  await f.render(React.createElement(Harness));
  assert.equal(gets, 1);
  await act(async () => window.dispatchEvent(new window.Event("focus")));
  assert.equal(gets, 2);
  assert.equal(latest.session.messages[0].content, "fresh");
  assert.equal(latest.session.revision, 2);
  assert.equal(changed, 0);
});

test("production bridge suppresses recovery failures after switching sessions", async (t) => {
  const f = await fixture(t); const wire = fakeApi(); const recovery = deferred(); const errors = [];
  let latest;
  wire.api.get = async (project, id) => snapshot(project, id);
  wire.api.readEvents = async (_project, id) => {
    if (id === "a") await recovery.promise;
    throw new Error(`${id} recovery failed`);
  };
  function Harness({ id }) { latest = useChatBridge("/a", id, true, (message) => errors.push(message), () => {}, wire.api); return null; }
  await f.render(React.createElement(Harness, { id: "a" }));
  await act(async () => window.dispatchEvent(new window.Event("focus")));
  await f.render(React.createElement(Harness, { id: "b" }));
  await act(async () => recovery.resolve());
  assert.equal(latest.session.id, "b");
  assert.deepEqual(errors, []);
});

test("production bridge reports recovery failures for the current session", async (t) => {
  const f = await fixture(t); const wire = fakeApi(); const errors = [];
  let latest;
  wire.api.get = async (project, id) => snapshot(project, id);
  wire.api.readEvents = async (_project, id) => { throw new Error(`${id} recovery failed`); };
  function Harness() { latest = useChatBridge("/a", "s", true, (message) => errors.push(message), () => {}, wire.api); return null; }
  await f.render(React.createElement(Harness));
  await act(async () => window.dispatchEvent(new window.Event("focus")));
  assert.equal(latest.session.id, "s");
  assert.deepEqual(errors, ["s recovery failed"]);
});

test("composer cannot send before listener readiness or while IME is composing", async (t) => {
  const f = await fixture(t); let sent = 0;
  const props = { draft: "hello", sending: false, canSend: false, error: null, useBackend: true, agentId: "g", agents: [{ id: "g", name: "Grok", adapterType: "grok_cli" }], permissionMode: "auto", resumeHint: false, diagnostics: [], diagnosticsLoading: false, onRefreshDiagnostics() {}, onDraftChange() {}, onSend() { sent += 1; }, onAbort() {}, onAgentChange() {}, onPermissionChange() {} };
  await f.render(React.createElement(ChatComposer, props));
  const sendButton = () => [...document.querySelectorAll("button")].find((b) => b.textContent.trim() === "发送");
  assert.equal(sendButton().disabled, true);
  await act(async () => document.querySelector("textarea").dispatchEvent(new window.KeyboardEvent("keydown", { key: "Enter", bubbles: true })));
  assert.equal(sent, 0);
  await f.render(React.createElement(ChatComposer, { ...props, canSend: true }));
  await act(async () => document.querySelector("textarea").dispatchEvent(new window.KeyboardEvent("keydown", { key: "Enter", bubbles: true, isComposing: true })));
  assert.equal(sent, 0);
  await act(async () => document.querySelector("textarea").dispatchEvent(new window.KeyboardEvent("keydown", { key: "Enter", bubbles: true, keyCode: 229 })));
  assert.equal(sent, 0);
  await act(async () => sendButton().click());
  assert.equal(sent, 1);
});

test("Ask still requires the explicit per-turn confirmation after bridge integration", async (t) => {
  const f = await fixture(t); let sent = 0;
  const props = { draft: "hello", sending: false, canSend: true, error: null, useBackend: true, agentId: "g", agents: [{ id: "g", name: "Grok", adapterType: "grok_cli" }], permissionMode: "ask", resumeHint: false, diagnostics: [], diagnosticsLoading: false, onRefreshDiagnostics() {}, onDraftChange() {}, onSend() { sent += 1; }, onAbort() {}, onAgentChange() {}, onPermissionChange() {} };
  const button = (label) => [...document.querySelectorAll("button")].find((b) => b.textContent.trim() === label);
  await f.render(React.createElement(ChatComposer, props));
  await act(async () => button("发送…").click());
  assert.equal(sent, 0); assert.ok(document.querySelector('[role="dialog"]'));
  await act(async () => button("取消").click());
  assert.equal(sent, 0);
  await act(async () => button("发送…").click());
  await act(async () => button("确认并发送").click());
  assert.equal(sent, 1);
});

test("composer exposes stop as pending evidence while abort is in flight", async (t) => {
  const f = await fixture(t); let aborted = 0;
  const props = { draft: "next", sending: true, aborting: true, canSend: true, error: null, useBackend: true, agentId: "g", agents: [{ id: "g", name: "Grok", adapterType: "grok_cli" }], permissionMode: "auto", resumeHint: false, diagnostics: [], diagnosticsLoading: false, onRefreshDiagnostics() {}, onDraftChange() {}, onSend() {}, onAbort() { aborted += 1; }, onAgentChange() {}, onPermissionChange() {} };
  await f.render(React.createElement(ChatComposer, props));
  const stopButton = [...document.querySelectorAll("button")].find((b) => b.textContent.trim() === "停止中…");
  assert.ok(stopButton);
  assert.equal(stopButton.disabled, true);
  assert.match(document.body.textContent, /停止中…/);
  await act(async () => stopButton.click());
  assert.equal(aborted, 0);
});

test("renaming reads the visible input, ignores IME confirmation and commits once", async (t) => {
  const { ChatSessionHeader } = require("../../.tmp/test-build/src/features/chat/ChatSessionHeader.js");
  const f = await fixture(t); const renamed = [];
  const props = { session: { ...snapshot(), title: "Initial title" }, useBackend: true, canPromote: false, onClearResume() {}, onPromote() {}, onRename: (value) => renamed.push(value), onTitleFromFirstMessage() {}, onToggleFlag() {}, onArchive() {} };
  await f.render(React.createElement(ChatSessionHeader, props));
  await act(async () => document.querySelector('[aria-haspopup="menu"]').click());
  await act(async () => [...document.querySelectorAll('[role="menuitem"]')].find((b) => b.textContent.trim() === "重命名").click());
  const input = document.querySelector('input[aria-label="会话标题"]');
  const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value").set;
  await act(async () => { setter.call(input, "QA v2 restart"); input.dispatchEvent(new window.Event("input", { bubbles: true })); });
  await act(async () => input.dispatchEvent(new window.KeyboardEvent("keydown", { key: "Enter", isComposing: true, bubbles: true })));
  assert.equal(renamed.length, 0);
  assert.ok(document.querySelector('input[aria-label="会话标题"]'));
  await act(async () => input.dispatchEvent(new window.KeyboardEvent("keydown", { key: "Enter", bubbles: true })));
  assert.deepEqual(renamed, ["QA v2 restart"]);
  assert.equal(document.querySelector('input[aria-label="会话标题"]'), null);
});

test("legacy resume is labelled for reset; structured handles allow resume and the reset action", async (t) => {
  const { ChatSessionHeader } = require("../../.tmp/test-build/src/features/chat/ChatSessionHeader.js");
  const f = await fixture(t); let cleared = 0;
  const props = { session: { ...snapshot(), resumeCommand: "grok --resume old-id" }, useBackend: true, canPromote: false,
    onClearResume() { cleared++; }, onPromote() {}, onRename() {}, onTitleFromFirstMessage() {}, onToggleFlag() {}, onArchive() {} };
  await f.render(React.createElement(ChatSessionHeader, props));
  assert.match(document.body.textContent, /续聊需重建/);
  assert.doesNotMatch(document.body.textContent, /可续聊/);
  await act(async () => document.querySelector('[aria-haspopup="menu"]').click());
  await act(async () => [...document.querySelectorAll('[role="menuitem"]')].find((b) => b.textContent.trim() === "开新 CLI 会话").click());
  assert.equal(cleared, 1);
  await f.render(React.createElement(ChatSessionHeader, { ...props, session: { ...snapshot(),
    resumeHandle: { version: 1, adapterType: "grok_cli", nativeSessionId: "native", configFingerprint: "abc" } } }));
  assert.match(document.body.textContent, /可续聊/);
  await act(async () => document.querySelector('[aria-haspopup="menu"]').click());
  assert.match(document.body.textContent, /开新 CLI 会话/);
});
