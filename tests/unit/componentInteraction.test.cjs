require.extensions[".css"] = () => undefined;

const test = require("node:test");
const assert = require("node:assert/strict");
const React = require("react");
const { act } = React;
const { createRoot } = require("react-dom/client");
const { JSDOM } = require("jsdom");
const { ToggleControl } = require("../../.tmp/test-build/src/features/settings/controls.js");
const { ValidationGate } = require("../../.tmp/test-build/src/features/testing/ValidationGate.js");
const { FeedbackComposer } = require("../../.tmp/test-build/src/features/testing/FeedbackComposer.js");
const {
  deriveValidationEvidence,
  gateStatus,
} = require("../../.tmp/test-build/src/features/testing/model.js");

global.IS_REACT_ACT_ENVIRONMENT = true;

function installDom() {
  const dom = new JSDOM("<!doctype html><html><body><div id='root'></div></body></html>");
  global.window = dom.window;
  global.document = dom.window.document;
  global.navigator = dom.window.navigator;
  global.HTMLElement = dom.window.HTMLElement;
  global.Event = dom.window.Event;
  return dom;
}

async function render(element) {
  const dom = installDom();
  const container = document.getElementById("root");
  const root = createRoot(container);
  await act(async () => root.render(element));
  return {
    dom,
    container,
    root,
    async close() {
      await act(async () => root.unmount());
      dom.window.close();
    },
  };
}

function validationRun(overrides = {}) {
  return {
    id: overrides.id ?? "run-1",
    taskId: overrides.taskId ?? "task-1",
    command: overrides.command ?? "pnpm test",
    cwd: overrides.cwd ?? "/repo",
    intent: overrides.intent ?? "validation",
    status: overrides.status ?? "succeeded",
    startedAtMs: overrides.startedAtMs ?? 1,
    endedAtMs: overrides.endedAtMs ?? overrides.startedAtMs ?? 1,
    exitCode: overrides.exitCode ?? 0,
  };
}

const validationSlots = [{ id: "validation", name: "Tests", command: "pnpm test", cwd: "", kind: "validation" }];

function gatePropsForRuns(runs, taskId = "task-1") {
  const evidence = deriveValidationEvidence(runs, taskId, validationSlots);
  const gate = gateStatus({
    hasPassingEvidence: evidence.hasPassingEvidence,
    hasRunningValidationRun: evidence.hasRunningEvidence,
    latestBlockingFailure: evidence.blockingFailure,
    hasValidationEvidence: evidence.hasEvidence,
  });
  return {
    gate,
    validationCommandLabel: "pnpm test",
    successfulRun: evidence.successfulRun,
    failedRun: evidence.failedRun,
    runningRun: evidence.runningRun,
    blockingFailure: evidence.blockingFailure,
    canAccept: evidence.hasPassingEvidence && !evidence.hasRunningEvidence,
    readOnly: false,
  };
}

test("Settings toggle emits the next value and respects disabled state", async () => {
  const values = [];
  const view = await render(React.createElement(ToggleControl, {
    checked: false,
    onChange: (value) => values.push(value),
  }));
  const button = view.container.querySelector("button");
  await act(async () => button.click());
  assert.deepEqual(values, [true]);
  await view.close();

  const disabledView = await render(React.createElement(ToggleControl, {
    checked: true,
    disabled: true,
    onChange: (value) => values.push(value),
  }));
  disabledView.container.querySelector("button").click();
  assert.deepEqual(values, [true]);
  await disabledView.close();
});

test("Testing acceptance gate exposes the blocking reason and only fires when enabled", async () => {
  let accepts = 0;
  const baseProps = {
    gate: { tone: "err", title: "需要修复", copy: "有新的失败验证阻塞验收。" },
    validationCommandLabel: "pnpm test",
    canAccept: false,
    readOnly: false,
    onAccept: () => { accepts += 1; },
  };
  const blocked = await render(React.createElement(ValidationGate, baseProps));
  assert.match(blocked.container.textContent, /需要修复/);
  const blockedButton = blocked.container.querySelector("button");
  assert.equal(blockedButton.disabled, true);
  blockedButton.click();
  assert.equal(accepts, 0);
  await blocked.close();

  const ready = await render(React.createElement(ValidationGate, {
    ...baseProps,
    gate: { tone: "ok", title: "验证已通过", copy: "已有通过证据。" },
    canAccept: true,
  }));
  await act(async () => ready.container.querySelector("button").click());
  assert.equal(accepts, 1);
  await ready.close();
});

test("Testing acceptance gate keeps preview and unrelated task runs blocked", async () => {
  let accepts = 0;
  const props = gatePropsForRuns([
    validationRun({ id: "preview", command: "pnpm dev", intent: "preview", startedAtMs: 10 }),
    validationRun({ id: "other-task-validation", taskId: "task-2", startedAtMs: 11 }),
  ]);

  const view = await render(React.createElement(ValidationGate, {
    ...props,
    onAccept: () => { accepts += 1; },
  }));
  assert.match(view.container.textContent, /暂无验证证据/);
  assert.match(view.container.textContent, /尚未运行/);
  const button = view.container.querySelector("button");
  assert.equal(button.disabled, true);
  button.click();
  assert.equal(accepts, 0);
  await view.close();
});

test("Testing acceptance gate accepts explicit validation intent without configured command match", async () => {
  let accepts = 0;
  const props = gatePropsForRuns([
    validationRun({
      id: "custom-validation",
      command: "npm run verify:release",
      intent: "validation",
      startedAtMs: 12,
      endedAtMs: 13,
    }),
  ]);

  const view = await render(React.createElement(ValidationGate, {
    ...props,
    onAccept: () => { accepts += 1; },
  }));
  assert.match(view.container.textContent, /验证已通过/);
  assert.match(view.container.textContent, /成功/);
  const button = view.container.querySelector("button");
  assert.equal(button.disabled, false);
  await act(async () => button.click());
  assert.equal(accepts, 1);
  await view.close();
});

test("Testing acceptance gate waits for explicit custom validation runs after a prior pass", async () => {
  let accepts = 0;
  const props = gatePropsForRuns([
    validationRun({ id: "previous-pass", startedAtMs: 10, endedAtMs: 11 }),
    validationRun({
      id: "custom-running-validation",
      command: "npm run verify:release",
      intent: "validation",
      status: "running",
      startedAtMs: 12,
      endedAtMs: undefined,
      exitCode: undefined,
    }),
  ]);

  const view = await render(React.createElement(ValidationGate, {
    ...props,
    onAccept: () => { accepts += 1; },
  }));
  assert.match(view.container.textContent, /检查运行中/);
  const button = view.container.querySelector("button");
  assert.equal(button.disabled, true);
  button.click();
  assert.equal(accepts, 0);
  await view.close();
});

test("Testing feedback composer disables empty repair requests and submits actionable feedback", async () => {
  const agent = {
    id: "agent-codex",
    name: "Codex",
    command: "codex",
    args: [],
    workingDirectoryPolicy: "project_root",
    capabilities: ["implementation"],
    adapterType: "codex_cli",
    canWriteFiles: true,
    canRunCommands: true,
    enabled: true,
    available: true,
  };
  let submissions = 0;
  const props = {
    readOnly: false,
    agents: [agent],
    selectedAgent: agent,
    autoMode: false,
    conversation: [],
    logs: {},
    quote: null,
    note: "",
    reproductionSteps: "",
    expectedBehavior: "",
    attachmentPaths: [],
    onAgentChange: () => undefined,
    onModeChange: () => undefined,
    onQuoteClear: () => undefined,
    onNoteChange: () => undefined,
    onReproductionStepsChange: () => undefined,
    onExpectedBehaviorChange: () => undefined,
    onSelectAttachments: () => undefined,
    onRemoveAttachment: () => undefined,
    onSubmit: () => { submissions += 1; },
  };
  const empty = await render(React.createElement(FeedbackComposer, props));
  const emptySubmit = [...empty.container.querySelectorAll("button")].find((button) => button.textContent.includes("打回修复"));
  assert.equal(emptySubmit.disabled, true);
  await empty.close();

  const actionable = await render(React.createElement(FeedbackComposer, { ...props, note: "保存后窗口没有关闭" }));
  const submit = [...actionable.container.querySelectorAll("button")].find((button) => button.textContent.includes("打回修复"));
  assert.equal(submit.disabled, false);
  await act(async () => submit.click());
  assert.equal(submissions, 1);
  await actionable.close();
});
