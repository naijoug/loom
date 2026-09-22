require.extensions[".css"] = () => undefined;
const test = require("node:test");
const assert = require("node:assert/strict");
const React = require("react");
const { act } = React;
const { createRoot } = require("react-dom/client");
const { JSDOM } = require("jsdom");
const { AppStateProvider, useAppState } = require("../../.tmp/test-build/src/state/AppStateContext.js");
const { initialAppState } = require("../../.tmp/test-build/src/state/reducer.js");
const { ThemeProvider } = require("../../.tmp/test-build/src/contexts/ThemeContext.js");
const { AppContent } = require("../../.tmp/test-build/src/App.js");
const { TAURI_EVENTS } = require("../../.tmp/test-build/src/api/contract.js");
global.IS_REACT_ACT_ENVIRONMENT = true;
const project = { id: "project", path: "/synthetic", name: "Test project", detectedStacks: [], suggestedCommands: [], isGitRepository: false, loomDirReady: true, schemaVersion: 1 };

test("production Chat → workflow → Chat/Settings routes isolate task loading and Planning listeners", async (t) => {
  const dom = new JSDOM("<div id='root'></div>", { url: "http://localhost" });
  global.window = dom.window; global.document = dom.window.document; global.navigator = dom.window.navigator;
  global.localStorage = dom.window.localStorage;
  window.matchMedia = () => ({ matches: false, addEventListener() {}, removeEventListener() {} });
  const calls = [], listeners = new Map(), callbacks = new Map();
  let callbackId = 0, listenerId = 0, dispatch;
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: (id) => listeners.delete(id) };
  window.__TAURI_INTERNALS__ = {
    transformCallback(handler) { const id = ++callbackId; callbacks.set(id, handler); return id; },
    unregisterCallback(id) { callbacks.delete(id); },
    async invoke(command, args) {
      calls.push({ command, args });
      if (command === "plugin:event|listen") { const id = ++listenerId; listeners.set(id, args); return id; }
      if (command === "plugin:event|unlisten") { listeners.delete(args.eventId); return; }
      if (command === "list_recent_projects") return [project];
      if (["list_agents", "diagnose_agents", "chat_list_sessions", "list_tasks"].includes(command)) return [];
      if (command === "load_project_agent_preferences") return { planningAgentIds: [], updatedAtMs: 0 };
      if (command === "list_terminal_slots") return [];
      if (command === "health_check") return { status: "ok", app: "Loom", version: "test", backend: "tauri", timestampMs: 0 };
      if (command === "load_app_settings") return {};
      throw new Error(`Unexpected IPC: ${command}`);
    },
  };
  const root = createRoot(document.getElementById("root"));
  t.after(async () => { await act(async () => root.unmount()); dom.window.close(); });
  function Harness() { dispatch = useAppState().dispatch; return React.createElement(AppContent); }
  const state = { ...initialAppState, app: { ...initialAppState.app, currentView: "chat", activeProjectId: project.id },
    projects: { ...initialAppState.projects, current: project, recent: [project] } };
  await act(async () => root.render(React.createElement(ThemeProvider, null, React.createElement(AppStateProvider, { initialStateOverride: state }, React.createElement(Harness)))));
  const planning = () => [...listeners.values()].filter(({ event }) => [TAURI_EVENTS.planningAgentStatus, TAURI_EVENTS.planningAgentLog].includes(event));
  const taskLoads = () => calls.filter(({ command }) => command === "list_tasks").length;
  assert.equal(taskLoads(), 0);
  assert.equal(planning().length, 0);
  assert.equal(document.querySelector('.header-flow'), null);
  assert.equal(document.querySelector('.project-inline-add'), null);
  const advanced = [...document.querySelectorAll('button')].find((b) => b.textContent.includes('任务看板（高级）'));
  assert.ok(advanced, "an empty project still exposes the advanced workflow entry");
  await act(async () => advanced.click());
  assert.equal(taskLoads(), 1);
  assert.equal(planning().length, 2, "one status and one log listener per workflow route");
  // Re-renders within the workflow must not subscribe or reload again.
  await act(async () => dispatch({ type: "agents/loaded", agents: [] }));
  assert.equal(planning().length, 2);
  assert.equal(taskLoads(), 1);
  await act(async () => dispatch({ type: "app/viewSelected", view: "chat" }));
  assert.equal(planning().length, 0);
  assert.equal(taskLoads(), 1);
  assert.equal(document.querySelector('.header-flow'), null);
  await act(async () => dispatch({ type: "app/viewSelected", view: "settings" }));
  assert.equal(planning().length, 0);
  assert.equal(taskLoads(), 1);
  await act(async () => dispatch({ type: "app/viewSelected", view: "board" }));
  assert.equal(planning().length, 2);
  assert.equal(taskLoads(), 2, "re-entering reads persisted updates made while Chat was open");
});
