require.extensions[".css"] = () => undefined;
const test = require("node:test"),
  assert = require("node:assert/strict");
const { JSDOM } = require("jsdom");
// react-dom detects input-event support when it loads, so a DOM must exist first.
const initial = new JSDOM('<div id="root"></div>', { url: "http://localhost" });
global.window = initial.window;
global.document = initial.window.document;
global.navigator = initial.window.navigator;
const React = require("react"),
  { act } = React,
  { createRoot } = require("react-dom/client"),
  { renderToStaticMarkup } = require("react-dom/server");
const { parseSourceTarget, LEARN_ADAPTERS } = require("../../.tmp/test-build/src/api/learn.js");
const { learnAgents } = require("../../.tmp/test-build/src/features/learn/NewStudyForm.js");
const { splitError } = require("../../.tmp/test-build/src/components/common/ErrorText.js");
const { Markdown } = require("../../.tmp/test-build/src/components/Markdown.js");
const { AppStateProvider, useAppState } = require("../../.tmp/test-build/src/state/AppStateContext.js");
const { initialAppState } = require("../../.tmp/test-build/src/state/reducer.js");
const { ThemeProvider } = require("../../.tmp/test-build/src/contexts/ThemeContext.js");
const { AppContent } = require("../../.tmp/test-build/src/App.js");
const samples = require("../../contracts/tauri-contract.json").modelSamples;
global.IS_REACT_ACT_ENVIRONMENT = true;

test("source targets parse paths and line ranges like the backend", () => {
  assert.deepEqual(parseSourceTarget("src/main.rs#L3-L9"), { path: "src/main.rs", startLine: 3, endLine: 9 });
  assert.deepEqual(parseSourceTarget("./src/a%20b.rs#L4"), { path: "src/a b.rs", startLine: 4, endLine: 4 });
  assert.deepEqual(parseSourceTarget("src/x.rs#L9-L3"), { path: "src/x.rs", startLine: 9, endLine: 9 });
  assert.deepEqual(parseSourceTarget("README.md"), { path: "README.md" });
});

test("errors split into a readable summary and raw details", () => {
  assert.deepEqual(
    splitError(
      "Codex is installed but cannot run (`--version` exited with 1); repair or reinstall it, then try again.\n\n/bin/codex --version exited with 1\nError: Missing optional dependency",
    ),
    {
      summary: "Codex is installed but cannot run (`--version` exited with 1); repair or reinstall it, then try again.",
      details: "/bin/codex --version exited with 1\nError: Missing optional dependency",
    },
  );
  assert.deepEqual(splitError("agent exited with status exit status: 1; stderr: boom\nat x"), {
    summary: "agent exited with status 1",
    details: "boom\nat x",
  });
  assert.deepEqual(splitError("Error: Agent not found"), { summary: "Agent not found" });
});

test("only enabled built-in agents are offered for Learn", () => {
  const agent = (id, adapterType, enabled = true) => ({ id, adapterType, enabled, available: true });
  const offered = learnAgents([
    agent("c", "claude_code_cli"),
    agent("x", "custom"),
    agent("o", "opencode_cli", false),
    agent("g", "grok_cli"),
  ]).map((a) => a.id);
  assert.deepEqual(offered, ["c", "g"]);
  assert.equal(LEARN_ADAPTERS.length, 4);
});

test("Chat markdown drops loom links while Learn pages turn them into source and wiki actions", () => {
  const dom = new JSDOM('<div id="root"></div>', { url: "http://localhost" });
  global.window = dom.window;
  global.document = dom.window.document;
  global.navigator = dom.window.navigator;
  const text = "[main](loom-src:src/main.rs#L1-L3) [gone](loom-src:gone.rs#L1-L2) [Core](loom-wiki:module/core)";
  const chat = renderToStaticMarkup(React.createElement(Markdown, { text }));
  assert.doesNotMatch(chat, /loom-src|loom-wiki|<button/);
  const opened = [],
    wiki = [];
  const links = {
    onSource: (t) => opened.push(t),
    onWiki: (t) => wiki.push(t),
    sourceStatus: (t) => (t.startsWith("gone") ? "missing" : "valid"),
  };
  const root = createRoot(document.getElementById("root"));
  act(() => root.render(React.createElement(Markdown, { text, links })));
  const buttons = [...document.querySelectorAll(".loom-src-link")];
  assert.deepEqual(
    buttons.map((b) => b.dataset.status),
    ["valid", "missing"],
  );
  act(() => buttons[0].dispatchEvent(new window.MouseEvent("click", { bubbles: true })));
  act(() =>
    document
      .querySelector(".loom-wiki-link")
      .dispatchEvent(new window.MouseEvent("click", { bubbles: true, cancelable: true })),
  );
  assert.deepEqual(opened, ["src/main.rs#L1-L3"]);
  assert.deepEqual(wiki, ["module/core"]);
  act(() => root.unmount());
  dom.window.close();
});

test("Learn view keeps Chat mounted, opens wiki pages and jumps to cited source lines", async (t) => {
  const dom = new JSDOM('<div id="root"></div>', { url: "http://localhost" });
  global.window = dom.window;
  global.document = dom.window.document;
  global.navigator = dom.window.navigator;
  global.localStorage = dom.window.localStorage;
  window.matchMedia = () => ({ matches: false, addEventListener() {}, removeEventListener() {} });
  window.HTMLElement.prototype.scrollIntoView = () => {};
  const project = { id: "p", path: "/p", name: "Project", detectedStacks: [], suggestedCommands: [] };
  const study = {
    ...structuredClone(samples.learnStudy),
    stages: [
      { stage: "fetch", status: "done", warnings: [] },
      { stage: "inventory", status: "done", warnings: [] },
      { stage: "architecture", status: "done", jobId: "job-1", warnings: [] },
    ],
    pages: [
      { pageId: "overview", title: "Overview", kind: "overview", ready: true },
      { pageId: "architecture", title: "Architecture", kind: "architecture", ready: true },
      { pageId: "module:cli", title: "CLI", kind: "module", moduleId: "cli", level: 1, ready: true },
      { pageId: "glossary", title: "Glossary", kind: "glossary", ready: false },
    ],
  };
  const agent = {
    id: "agent-claude",
    name: "Claude Code",
    command: "claude",
    args: [],
    workingDirectoryPolicy: "project_root",
    capabilities: [],
    adapterType: "claude_code_cli",
    canWriteFiles: false,
    canRunCommands: false,
    enabled: true,
    available: true,
  };
  const calls = [];
  let dispatch;
  window.__TAURI_INTERNALS__ = {
    transformCallback() {
      return 1;
    },
    unregisterCallback() {},
    async invoke(command, args) {
      calls.push({ command, args });
      if (command === "list_recent_projects") return [project];
      if (command === "chat_list_project_groups") return [{ project, sessionCount: 0, sessions: [] }];
      if (command === "list_agents") return [agent];
      if (command === "diagnose_agents")
        return [
          {
            agentId: "agent-claude",
            status: "ready",
            authStatus: "unknown",
            detail: "",
            checkedAtMs: 1,
            capabilities: {
              structuredStream: true,
              toolEvents: true,
              nativeResume: true,
              inlineCards: false,
              modelSelection: true,
              modes: [],
            },
          },
        ];
      if (command === "load_app_settings") return {};
      if (command === "health_check") return { version: "test" };
      if (command === "learn_list_studies")
        return {
          studies: [
            {
              studyId: study.studyId,
              owner: "sharkdp",
              repo: "hexyl",
              agentName: "Claude Code",
              status: "ready",
              pagesReady: 3,
              pagesTotal: 4,
              updatedAtMs: 2,
            },
          ],
          storageErrors: [],
        };
      if (command === "learn_get_study") return { study, architecture: samples.learnArchitecture };
      if (command === "learn_read_page")
        return args.pageId === "overview"
          ? {
              pageId: "overview",
              markdown: "# hexyl\n\nStart at [main](loom-src:src/main.rs#L2-L3).",
              refs: [{ target: "src/main.rs#L2-L3", path: "src/main.rs", startLine: 2, endLine: 3, status: "valid" }],
            }
          : { pageId: args.pageId, markdown: "# Architecture", refs: [] };
      if (command === "learn_read_source")
        return {
          path: "src/main.rs",
          text: "fn a() {}\nfn main() {\n    run();\n}\n",
          firstLine: 1,
          totalLines: 4,
          truncated: false,
          language: "Rust",
        };
      if (command === "learn_create_study") return { study: { ...study, studyId: "study-2-0", pages: [], stages: [] } };
      if (command.startsWith("plugin:event|")) return 1;
      throw Error(command);
    },
  };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
  const root = createRoot(document.getElementById("root"));
  t.after(async () => {
    await act(async () => root.unmount());
    dom.window.close();
  });
  function Harness() {
    dispatch = useAppState().dispatch;
    return React.createElement(AppContent);
  }
  await act(async () =>
    root.render(
      React.createElement(
        ThemeProvider,
        null,
        React.createElement(
          AppStateProvider,
          {
            initialStateOverride: {
              ...initialAppState,
              projects: { current: project, recent: [project] },
              app: { ...initialAppState.app, activeProjectId: "p" },
            },
          },
          React.createElement(Harness),
        ),
      ),
    ),
  );
  const chat = document.querySelector('[data-testid="chat-page"]');
  // Learn opens from the activity rail; the Chat sidebar no longer links to it.
  assert.equal(document.querySelector(".chat-tree-nav"), null);
  const learnButton = document.querySelector('.activity-rail-item[aria-label="Learn (Ctrl+2)"]');
  assert.ok(learnButton);
  await act(async () => learnButton.dispatchEvent(new window.MouseEvent("click", { bubbles: true })));
  await act(async () => {
    await new Promise((r) => setTimeout(r, 20));
  });
  assert.ok(document.querySelector('[data-testid="learn-page"]'));
  assert.equal(document.querySelector('[data-testid="chat-page"]'), chat);
  assert.equal(chat.closest(".app-shell-pane").hidden, true);
  assert.match(document.querySelector(".learn-study-title h1").textContent, /sharkdp\/hexyl/);
  assert.deepEqual(
    [...document.querySelectorAll(".learn-wiki-tree button")].map((b) => b.textContent.replace(/Not generated/, "")),
    ["Overview", "Architecture", "CLI", "Glossary"],
  );
  const src = document.querySelector(".loom-src-link");
  assert.equal(src.dataset.status, "valid");
  await act(async () => src.dispatchEvent(new window.MouseEvent("click", { bubbles: true })));
  await act(async () => {
    await new Promise((r) => setTimeout(r, 10));
  });
  assert.deepEqual(calls.find((c) => c.command === "learn_read_source").args, {
    studyId: study.studyId,
    path: "src/main.rs",
    startLine: 2,
    endLine: 3,
  });
  assert.deepEqual(
    [...document.querySelectorAll(".source-code-line[data-marked]")].map(
      (l) => l.querySelector(".source-code-number").textContent,
    ),
    ["2", "3"],
  );
  const archButton = [...document.querySelectorAll(".learn-wiki-tree button")].find(
    (b) => b.textContent === "Architecture",
  );
  await act(async () => archButton.dispatchEvent(new window.MouseEvent("click", { bubbles: true })));
  await act(async () => {
    await new Promise((r) => setTimeout(r, 10));
  });
  assert.equal(document.querySelectorAll(".learn-diagram-node").length, samples.learnArchitecture.modules.length);
  const glossary = [...document.querySelectorAll(".learn-wiki-tree button")].find((b) =>
    b.textContent.startsWith("Glossary"),
  );
  await act(async () => glossary.dispatchEvent(new window.MouseEvent("click", { bubbles: true })));
  assert.ok([...document.querySelectorAll("button")].some((b) => b.textContent === "Generate page"));
  // New study: the create command uses the `input` envelope with the selected agent and trimmed model.
  await act(async () =>
    document
      .querySelector('button[aria-label="New study"]')
      .dispatchEvent(new window.MouseEvent("click", { bubbles: true })),
  );
  const form = document.querySelector(".learn-new");
  const setValue = (el, value) => {
    const proto = Object.getPrototypeOf(el);
    Object.getOwnPropertyDescriptor(proto, "value").set.call(el, value);
    el.dispatchEvent(new window.Event("input", { bubbles: true }));
    el.dispatchEvent(new window.Event("change", { bubbles: true }));
  };
  const inputs = form.querySelectorAll("input");
  await act(async () => {
    setValue(inputs[0], "https://github.com/sharkdp/hexyl");
    setValue(inputs[1], " sonnet ");
  });
  await act(async () => form.dispatchEvent(new window.Event("submit", { bubbles: true, cancelable: true })));
  assert.deepEqual(calls.find((c) => c.command === "learn_create_study").args, {
    input: {
      url: "https://github.com/sharkdp/hexyl",
      gitRef: undefined,
      agentId: "agent-claude",
      model: "sonnet",
      language: "en",
      depth: "standard",
    },
  });
  assert.equal(
    calls.some((call) => /^chat_send|task|planning/.test(call.command)),
    false,
  );
});
