require.extensions[".css"] = () => undefined;
const test = require("node:test"),
  assert = require("node:assert/strict");
const React = require("react"),
  { renderToStaticMarkup } = require("react-dom/server");
const { AgentPicker, agentSupportsModel } = require("../../.tmp/test-build/src/components/AgentPicker.js");

const agent = (id, adapterType) => ({
  id,
  name: id,
  command: id,
  args: [],
  workingDirectoryPolicy: "project_root",
  capabilities: [],
  adapterType,
  canWriteFiles: false,
  canRunCommands: false,
  enabled: true,
  available: true,
});
const diagnostic = (agentId, modelSelection) => ({
  agentId,
  status: "ready",
  authStatus: "unknown",
  detail: "",
  checkedAtMs: 1,
  capabilities: {
    structuredStream: true,
    toolEvents: true,
    nativeResume: true,
    inlineCards: false,
    modelSelection,
    modes: [],
  },
});
const agents = [agent("codex", "codex_cli"), agent("custom", "cli")];
const diagnostics = [diagnostic("codex", true), diagnostic("custom", false)];
const render = (props) =>
  renderToStaticMarkup(
    React.createElement(AgentPicker, { agents, diagnostics, onChange() {}, disabled: false, ...props }),
  );
const model = { value: "", onChange() {} };

test("the model field follows the diagnosed modelSelection capability", () => {
  assert.equal(agentSupportsModel(diagnostics, "codex"), true);
  assert.equal(agentSupportsModel(diagnostics, "custom"), false);
  assert.equal(agentSupportsModel([], "codex"), false);
  assert.match(render({ value: "codex", model }), /class="agent-model-input"/);
  assert.doesNotMatch(render({ value: "custom", model }), /agent-model-input/);
  // Chat passes no model, so it never shows the field.
  assert.doesNotMatch(render({ value: "codex" }), /agent-model-input/);
});
