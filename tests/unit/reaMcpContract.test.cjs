const assert = require("node:assert/strict");
const test = require("node:test");
const { readFileSync } = require("node:fs");

const contract = JSON.parse(readFileSync(require.resolve("../fixtures/rea/mcp-contract-6.5.0.json"), "utf8"));

const HIGH_RISK_EFFECTS = new Set([
  "mutatesTarget",
  "writesFilesystem",
  "launchesProcess",
  "accessesNetwork",
  "changesUiState",
  "mayDiscardData",
  "executesForeignCode",
  "mutatesDevice",
]);

function availability() {
  return contract.binarySession.groups.flatMap(group => group.names.map(name => ({ ...group, name })));
}

function effect(group) {
  if (!group.available) return "unavailable";
  return group.effects.some(value => HIGH_RISK_EFFECTS.has(value)) ? "mutation" : "read-only";
}

test("REA 6.5.0 MCP identity and catalog are pinned", () => {
  assert.equal(contract.package.name, "rea-agents");
  assert.equal(contract.package.version, "6.5.0");
  assert.equal(contract.transport, "stdio");
  assert.equal(contract.initialize.protocolVersion, "2025-06-18");
  assert.deepEqual(contract.initialize.serverInfo, { name: "rea", version: "6.5.0" });
  assert.equal(contract.catalog.counts.mcp_tools, 207);
  assert.equal(contract.tools.length, 207);
  assert.equal(new Set(contract.tools).size, 207);
  assert.equal(contract.catalog.digests.tools_sha256, "805433f17bb4f709558ff20c04f96fe561fca2938fd0f459b1f8cc0f6b438ded");
  assert.equal(contract.catalog.digests.prompts_sha256, "7152916ab48bd2a9677a5ad9cdbde0a51dc53c113934952ad096e93f5f18aa7e");
  assert.equal(contract.catalog.digests.combined_sha256, "405725e736ec8dc8483435b970acbf20f9f26b9f1fd25203b992f5ab4cd2a4f4");
});

test("binary_session availability covers every advertised tool exactly once", () => {
  const entries = availability();
  const names = entries.map(entry => entry.name);
  assert.equal(names.length, 207);
  assert.equal(new Set(names).size, 207);
  assert.deepEqual(names.slice().sort(), contract.tools);
  for (const entry of entries) {
    assert.ok(Array.isArray(entry.effects), `${entry.name} has no effects`);
    assert.equal(typeof entry.available, "boolean", `${entry.name} has no availability`);
    if (!entry.available) assert.ok(entry.reason, `${entry.name} has no unavailable reason`);
  }
});

test("REA effect metadata maps to the conservative Loom policy", () => {
  const entries = new Map(availability().map(entry => [entry.name, entry]));
  const expected = new Map([
    ["analyze_javascript_application", "read-only"],
    ["get_evidence_bundle", "read-only"],
    ["inspect_analysis_view", "read-only"],
    ["evaluate_reconstruction_coverage", "read-only"],
    ["binary_session", "mutation"],
    ["open_binary", "mutation"],
    ["close_binary", "mutation"],
    ["export_evidence_bundle", "mutation"],
    ["capture_process_scenario", "mutation"],
    ["capture_browser_scenario", "mutation"],
    ["install_adb_package", "unavailable"],
  ]);

  for (const [name, wanted] of expected) {
    assert.equal(effect(entries.get(name)), wanted, `${name} policy changed`);
  }
});

test("provider candidates retain availability and operation identity", () => {
  assert.equal(contract.binarySession.providerCandidates.length, 3);
  const expected = new Map([
    ["ghidra", "not_configured"],
    ["hopper", "executable_missing"],
    ["ida", "not_configured"],
  ]);
  for (const [id, code] of expected) {
    const provider = contract.binarySession.providerCandidates.find(item => item.id === id);
    assert.ok(provider, `missing provider ${id}`);
    assert.equal(provider.availability.status, "unavailable");
    assert.equal(provider.availability.code, code);
    assert.ok(provider.operations.length > 0);
  }
  assert.equal(contract.binarySession.clientFeatures.elicitation_form, false);
  assert.equal(contract.binarySession.clientFeatures.elicitation_url, false);
  assert.equal(contract.binarySession.clientFeatures.roots, false);
  assert.equal(contract.binarySession.clientFeatures.sampling, false);
});
