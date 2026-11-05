require.extensions[".css"] = () => undefined;
const test = require("node:test"),
  assert = require("node:assert/strict");
const { JSDOM } = require("jsdom");
// react-dom detects input-event support when it loads, so a DOM must exist first.
const dom = new JSDOM('<div id="root"></div>', { url: "http://localhost" });
global.window = dom.window;
global.document = dom.window.document;
global.navigator = dom.window.navigator;
const React = require("react"),
  { act } = React,
  { createRoot } = require("react-dom/client");
const { AgentPicker } = require("../../.tmp/test-build/src/components/AgentPicker.js");
const { AppStateProvider } = require("../../.tmp/test-build/src/state/AppStateContext.js");
const { initialAppState } = require("../../.tmp/test-build/src/state/reducer.js");
const { ProvidersPanel } = require("../../.tmp/test-build/src/features/settings/ProvidersPanel.js");
const {
  keySourceLabel,
  newProviderId,
  providerInput,
  draftFromPreset,
  draftFromProvider,
  PROVIDER_PRESETS,
  providerTestLabel,
} = require("../../.tmp/test-build/src/features/settings/providerDraft.js");
const { translate } = require("../../.tmp/test-build/src/i18n/index.js");
global.IS_REACT_ACT_ENVIRONMENT = true;

const t = (key, values) => translate("en", key, values);

test("model-list checks never claim to verify conversations in either locale", () => {
  for (const locale of ["en", "zh-CN"]) {
    const localize = (key, values) => translate(locale, key, values);
    for (const result of [
      { outcome: "ok", httpStatus: 200 },
      { outcome: "model_list_unavailable", httpStatus: 404 },
      { outcome: "model_list_unavailable", httpStatus: 405 },
      { outcome: "model_list_unavailable", httpStatus: 501 },
    ]) {
      const label = providerTestLabel(result, localize);
      assert.match(label, locale === "en" ? /chat not tested/ : /未测试对话/);
      if (result.outcome !== "ok") assert.ok(label.includes(String(result.httpStatus)));
      assert.doesNotMatch(label, /URL or model not found|地址或模型不存在/);
    }
  }
});
const agent = (id, adapterType, extra = {}) => ({
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
  ...extra,
});
const provider = (id, extra = {}) => ({
  id,
  name: id,
  kind: "anthropic",
  baseUrl: `https://${id}.example.com`,
  model: null,
  storedKey: true,
  keyEnv: null,
  keySource: "stored",
  configured: true,
  ...extra,
});
const providers = [
  provider("deepseek", { name: "DeepSeek", model: "deepseek-chat" }),
  provider("gateway", {
    kind: "openai-chat",
    baseUrl: "http://127.0.0.1:8080/v1",
    storedKey: false,
    keySource: "none",
  }),
  provider("openai", {
    kind: "openai-responses",
    storedKey: false,
    keyEnv: "OPENAI_API_KEY",
    keySource: "none",
    configured: false,
  }),
];
const agents = [
  agent("codex", "codex_cli"),
  agent("claude", "claude_code_cli", { providerId: "openai" }),
  agent("API · DeepSeek", "api", { providerId: "deepseek" }),
];

async function mount(context, element) {
  const root = createRoot(document.getElementById("root"));
  context.after(() => act(async () => root.unmount()));
  await act(async () => root.render(element));
  return root;
}
const menuItems = () =>
  Array.from(document.querySelectorAll('[role="menuitemradio"]')).map((item) => ({
    name: item.querySelector("strong").textContent,
    disabled: item.disabled,
    detail: item.querySelector("small")?.textContent ?? "",
  }));

test("the picker names each source and greys out direct API only when tools are needed", async (context) => {
  for (const needsTools of [true, false]) {
    const root = await mount(
      context,
      React.createElement(AgentPicker, {
        agents,
        diagnostics: [],
        providers,
        value: "codex",
        onChange() {},
        disabled: false,
        needsTools,
      }),
    );
    await act(async () => document.querySelector(".chat-agent-button").click());
    const [codex, claude, api] = menuItems();
    assert.deepEqual(codex, { name: "codex", disabled: false, detail: "Own login" });
    // claude's provider has no key, so it cannot be picked in any feature.
    assert.deepEqual(claude, { name: "claude", disabled: true, detail: "Model source openai has no key" });
    assert.equal(api.disabled, needsTools);
    assert.equal(api.detail, needsTools ? t("agentNeedsTools") : "DeepSeek · Text answers only");
    await act(async () => root.unmount());
  }
});

test("Model sources list key sources only and block removing a provider in use", async (context) => {
  await mount(
    context,
    React.createElement(
      AppStateProvider,
      { initialStateOverride: { ...initialAppState, agents, providers: { revision: 4, providers } } },
      React.createElement(ProvidersPanel, { adding: false, onAddingChange() {} }),
    ),
  );
  const rows = Array.from(document.querySelectorAll(".settings-providers tbody tr"));
  assert.deepEqual(
    rows.map((row) => row.querySelector(".settings-provider-key .settings-status").textContent),
    ["Saved", "Not needed locally", "$OPENAI_API_KEY not set"],
  );
  const remove = (row) => Array.from(row.querySelectorAll("button")).find((button) => button.textContent === "Remove");
  assert.equal(remove(rows[0]).disabled, true);
  assert.equal(remove(rows[0]).title, "Used by API · DeepSeek; change it in Agents first");
  assert.equal(remove(rows[1]).disabled, false);
  assert.match(rows[0].textContent, /Used by API · DeepSeek/);
  // Editing keeps the saved key: no key field until Replace.
  await act(async () => rows[0].querySelector("button").click());
  assert.equal(document.querySelector('.settings-provider-form input[type="password"]'), null);
  assert.match(document.querySelector(".settings-key-state").textContent, /A key is saved/);
  const replace = Array.from(document.querySelectorAll(".settings-key-state button")).find(
    (button) => button.textContent === "Replace",
  );
  await act(async () => replace.click());
  assert.equal(document.querySelector('.settings-provider-form input[type="password"]').value, "");
});

test("provider drafts build safe inputs", () => {
  assert.equal(newProviderId("DeepSeek (China)", []), "deepseek-china");
  assert.equal(newProviderId("DeepSeek", ["deepseek", "deepseek-2"]), "deepseek-3");
  assert.equal(newProviderId("本机", []), "provider");
  const draft = { ...draftFromPreset(PROVIDER_PRESETS[0]), name: " Anthropic ", model: " ", keyEnv: "" };
  assert.deepEqual(providerInput(draft, 2, []), {
    id: "anthropic",
    expectedRevision: 2,
    name: "Anthropic",
    kind: "anthropic",
    baseUrl: "https://api.anthropic.com",
    model: null,
    // An empty key field sends no key rather than an empty one.
    key: { action: "keep" },
    keyEnv: null,
  });
  assert.deepEqual(providerInput({ ...draft, key: { action: "set", value: " sk-test " } }, 2, []).key, {
    action: "set",
    value: "sk-test",
  });
  const edited = draftFromProvider(providers[0]);
  assert.deepEqual(edited.key, { action: "keep" });
  assert.equal(providerInput(edited, 4, ["deepseek"]).id, "deepseek");
  assert.equal(keySourceLabel(providers[2], t).tone, "warn");
  assert.deepEqual(
    PROVIDER_PRESETS.map((preset) => [preset.kind, preset.baseUrl]),
    [
      ["anthropic", "https://api.anthropic.com"],
      ["openai-responses", "https://api.openai.com/v1"],
      ["openai-chat", "https://openrouter.ai/api/v1"],
      ["openai-chat", ""],
      ["anthropic", ""],
      ["openai-chat", "http://127.0.0.1:"],
    ],
  );
});
