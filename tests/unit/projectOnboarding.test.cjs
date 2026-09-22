require.extensions[".css"] = () => undefined;
const test = require("node:test");
const assert = require("node:assert/strict");
const React = require("react");
const { act } = React;
const { createRoot } = require("react-dom/client");
const { JSDOM } = require("jsdom");
const { AppStateProvider } = require("../../.tmp/test-build/src/state/AppStateContext.js");
const { initialAppState } = require("../../.tmp/test-build/src/state/reducer.js");
const { AddProjectModal } = require("../../.tmp/test-build/src/components/Sidebar/AddProjectModal.js");
const { Navigation } = require("../../.tmp/test-build/src/components/Sidebar/Navigation.js");
global.IS_REACT_ACT_ENVIRONMENT = true;
const project = { id: "project", path: "/old", name: "old", detectedStacks: ["Python"], suggestedCommands: ["python run.py"], isGitRepository: true, gitBranch: "feature/real", hasUncommittedChanges: false, loomDirReady: true, schemaVersion: 1 };
async function mount(t, component, state, invoke) {
  const dom = new JSDOM("<div id='root'></div>");
  global.window = dom.window; global.document = dom.window.document; global.navigator = dom.window.navigator;
  window.__TAURI_INTERNALS__ = { invoke };
  const root = createRoot(document.getElementById("root"));
  t.after(async () => { await act(async () => root.unmount()); dom.window.close(); });
  await act(async () => root.render(React.createElement(AppStateProvider, { initialStateOverride: state }, component)));
  return document.getElementById("root");
}

test("project dialog never invents analysis or reuses a different directory's cached metadata", async (t) => {
  const state = { ...initialAppState, projects: { ...initialAppState.projects, current: project, recent: [project] } };
  const view = await mount(t, React.createElement(AddProjectModal, { onClose() {} }), state, async (command) => {
    assert.equal(command, "plugin:dialog|open"); return "/new-empty-project";
  });
  assert.match(view.textContent, /Python/); assert.match(view.textContent, /feature\/real/);
  const browse = [...view.querySelectorAll("button")].find((b) => b.textContent === "浏览…");
  await act(async () => browse.click());
  assert.match(view.textContent, /待检测/);
  assert.doesNotMatch(view.textContent, /Python|feature\/real|Vite \+ React|pnpm dev|已分析/);
});

test("recent-project hydration does not show an empty-state modal before persisted projects arrive", async (t) => {
  let resolve;
  const pending = new Promise((r) => { resolve = r; });
  const view = await mount(t, React.createElement(Navigation), { ...initialAppState, projects: { ...initialAppState.projects, current: null, recent: [] } }, async (command) => {
    assert.equal(command, "list_recent_projects"); return pending;
  });
  assert.equal(view.querySelector('[role="dialog"]'), null);
  await act(async () => resolve([project]));
  assert.match(view.textContent, /old/);
  assert.equal(view.querySelector('[role="dialog"]'), null);
});

test("a genuinely empty hydrated project list still opens onboarding without fake stacks", async (t) => {
  const view = await mount(t, React.createElement(Navigation), { ...initialAppState, projects: { ...initialAppState.projects, current: null, recent: [] } }, async () => []);
  assert.ok(view.querySelector('[role="dialog"]'));
  assert.match(view.textContent, /待检测/);
  assert.doesNotMatch(view.textContent, /Vite \+ React|pnpm dev|已分析/);
});
