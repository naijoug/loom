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

const {
  agentButtonLabel,
  agentHealth,
  defaultAgentId,
  pickerAgents,
} = require("../../.tmp/test-build/src/components/AgentPicker.js");
const provider = (id, configured = true, model = null) => ({
  id,
  name: id === "ds" ? "DeepSeek" : id,
  kind: "anthropic",
  baseUrl: "https://example.com",
  model,
  storedKey: configured,
  keyEnv: null,
  keySource: configured ? "stored" : "none",
  configured,
});

test("the button names a CLI's model source; own login and direct API show the name alone", () => {
  const providers = [provider("ds")];
  assert.equal(
    agentButtonLabel({ ...agent("claude", "claude_code_cli"), providerId: "ds" }, providers),
    "claude · DeepSeek",
  );
  assert.equal(agentButtonLabel(agent("codex", "codex_cli"), providers), "codex");
  assert.equal(agentButtonLabel({ ...agent("API · DeepSeek", "api"), providerId: "ds" }, providers), "API · DeepSeek");
});

test("direct-API agents are never picked by default and show the provider default model", () => {
  const api = { ...agent("api", "api"), providerId: "ds" };
  const providers = [provider("ds", true, "deepseek-chat")];
  assert.equal(defaultAgentId([api, agent("codex", "codex_cli")]), "codex");
  assert.equal(defaultAgentId([api]), "");
  assert.match(
    render({ agents: [api], diagnostics: [diagnostic("api", true)], providers, value: "api", model }),
    /placeholder="Default: deepseek-chat"/,
  );
  const eligible = [agent("codex", "codex_cli")];
  assert.deepEqual(
    pickerAgents(eligible, [...eligible, api, agent("custom", "cli")]).map((item) => item.id),
    ["codex", "api"],
  );
});

test("a provider without a key marks its agent down", () => {
  const claude = { ...agent("claude", "claude_code_cli"), providerId: "ds" };
  assert.equal(agentHealth(claude, diagnostics[0], provider("ds", false)), "down");
  assert.equal(agentHealth(claude, diagnostics[0], provider("ds", true)), "ok");
  assert.equal(agentHealth(claude, diagnostics[0]), "ok");
});
