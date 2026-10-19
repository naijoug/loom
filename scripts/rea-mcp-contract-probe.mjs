#!/usr/bin/env node
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";

const args = process.argv.slice(2);
const options = Object.fromEntries(
  args.flatMap((arg, index) => {
    if (!arg.startsWith("--")) return [];
    const key = arg.slice(2);
    const value = args[index + 1];
    return value && !value.startsWith("--") ? [[key, value]] : [[key, "true"]];
  }),
);
// A direct REA executable avoids package-runner stdin forwarding differences.
const command = options.command ?? "/private/tmp/loom-rea-r0-project/node_modules/.bin/rea";
const commandArgs = ["mcp"];
const output = options.output ?? "tests/fixtures/rea/mcp-contract-6.5.0.json";

const child = spawn(command, commandArgs, { stdio: ["pipe", "pipe", "pipe"] });
const stderr = [];
const responses = new Map();
let buffer = "";
let stdoutText = "";

child.stdout.on("data", chunk => {
  const text = chunk.toString("utf8");
  stdoutText += text;
  buffer += text;
  let newline;
  while ((newline = buffer.indexOf("\n")) >= 0) {
    const line = buffer.slice(0, newline).trim();
    buffer = buffer.slice(newline + 1);
    if (!line) continue;
    try {
      const message = JSON.parse(line);
      if (message.id !== undefined) responses.set(String(message.id), message);
    } catch {
      // MCP stdio is line-delimited JSON; report the malformed line when a request times out.
    }
  }
});
child.stderr.on("data", chunk => stderr.push(chunk));

function request(id, method, params) {
  child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", id, method, params })}\n`);
}

async function wait(id) {
  const deadline = Date.now() + 30_000;
  while (!responses.has(String(id)) && Date.now() < deadline) {
    await new Promise(resolve => setTimeout(resolve, 20));
  }
  if (!responses.has(String(id))) {
    throw new Error(`Timed out waiting for MCP response ${id}; stdout so far: ${stdoutText.slice(0, 2000)}`);
  }
  return responses.get(String(id));
}

function digest(value) {
  return createHash("sha256").update(value).digest("hex");
}

function compactGroup(group) {
  return {
    names: group.names,
    surface: group.surface,
    effects: group.effects,
    annotations: group.annotations,
    available: group.available,
    reason: group.reason ?? null,
    remediation: group.remediation ?? null,
    defaultModeAvailable: group.default_mode_available ?? null,
    modes: group.modes?.map(mode => ({
      name: mode.name,
      available: mode.available,
      missingOperations: mode.missing_operations ?? [],
      remediation: mode.remediation ?? null,
    })) ?? null,
  };
}

try {
  request(1, "initialize", {
    protocolVersion: "2025-06-18",
    capabilities: {},
    clientInfo: { name: "loom-contract-probe", version: "0.1.0" },
  });
  const initialize = await wait(1);
  if (initialize.error) throw new Error(`initialize failed: ${JSON.stringify(initialize.error)}`);
  child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized" })}\n`);

  request(2, "tools/list", {});
  const toolsResponse = await wait(2);
  if (toolsResponse.error) throw new Error(`tools/list failed: ${JSON.stringify(toolsResponse.error)}`);
  const tools = toolsResponse.result.tools;

  request(3, "tools/call", { name: "binary_session", arguments: {} });
  const sessionResponse = await wait(3);
  if (sessionResponse.error) throw new Error(`binary_session failed: ${JSON.stringify(sessionResponse.error)}`);
  const session = sessionResponse.result.structuredContent.result;

  const schemaDigests = Object.fromEntries(tools.map(tool => [tool.name, {
    input: digest(JSON.stringify(tool.inputSchema)),
    output: tool.outputSchema ? digest(JSON.stringify(tool.outputSchema)) : null,
    annotations: tool.annotations ?? null,
  }]));
  const fixture = {
    schemaVersion: 1,
    capturedAt: new Date().toISOString(),
    package: {
      name: session.server_identity.package.name,
      version: session.server_identity.package.version,
    },
    transport: "stdio",
    initialize: {
      protocolVersion: initialize.result.protocolVersion,
      serverInfo: initialize.result.serverInfo,
      capabilities: initialize.result.capabilities,
    },
    catalog: session.server_identity.catalog,
    toolsDigest: digest(JSON.stringify(schemaDigests)),
    tools: tools.map(tool => tool.name).sort(),
    toolsListVersion: 1,
    binarySession: {
      clientFeatures: session.client_features,
      open: session.open,
      groups: session.tool_availability.groups.map(compactGroup),
      providerCandidates: session.analysis_provider_candidates.map(candidate => ({
        id: candidate.provider.id,
        name: candidate.provider.name,
        version: candidate.provider.version ?? null,
        selected: candidate.selected,
        availability: {
          status: candidate.availability.status,
          code: candidate.availability.code ?? null,
          reason: candidate.availability.reason ?? null,
        },
        targetSupport: {
          status: candidate.target_support.status,
          code: candidate.target_support.code ?? null,
          reason: candidate.target_support.reason ?? null,
        },
        operations: candidate.capabilities.flatMap(provider => provider.groups.map(group => group.operations)).flat(),
      })),
    },
    schemaDigests,
  };

  mkdirSync(dirname(output), { recursive: true });
  writeFileSync(output, `${JSON.stringify(fixture, null, 2)}\n`);
  process.stdout.write(`${JSON.stringify({
    output,
    packageVersion: fixture.package.version,
    protocolVersion: fixture.initialize.protocolVersion,
    toolCount: fixture.tools.length,
    availabilityGroupCount: fixture.binarySession.groups.length,
    catalogDigest: fixture.catalog.digests.combined_sha256,
    stderrBytes: Buffer.concat(stderr).length,
  })}\n`);
} finally {
  child.stdin.end();
  child.kill("SIGTERM");
}
