#!/usr/bin/env node
// Explicit, isolated R0 probe. Does not install tools or change user agent settings.
import { mkdirSync, writeFileSync, readFileSync, existsSync } from "node:fs";
import { resolve, dirname, join } from "node:path";
import { createHash } from "node:crypto";
import { spawn } from "node:child_process";
import { createServer } from "node:http";
import { setTimeout as delay } from "node:timers/promises";
import { ProbeMcp, structured } from "./lib/research-probe-mcp.mjs";

const options = {};
for (let i = 2; i < process.argv.length; i += 2) {
  const key = process.argv[i]; const value = process.argv[i + 1];
  if (!key?.startsWith("--") || !value) throw new Error("Use --node PATH --rea PATH --output DIRECTORY");
  options[key.slice(2)] = value;
}
for (const key of ["node", "rea", "output"]) if (!options[key]) throw new Error(`Missing --${key}`);
const output = resolve(options.output); mkdirSync(output, { recursive: true, mode: 0o700 });
const save = (name, value) => writeFileSync(join(output, name), JSON.stringify(value, null, 2) + "\n", { mode: 0o600 });
const node = resolve(options.node); const rea = resolve(options.rea);
const mcp = new ProbeMcp(node, [rea, "mcp"], { cwd: output, env: { PATH: `${dirname(node)}:${process.env.PATH}`, CLANG_MODULE_CACHE_PATH: join(output, "clang-cache"), SWIFT_MODULECACHE_PATH: join(output, "swift-cache"), REA_MCP_INPUT_SCHEMA_PROFILE: "full" } });
const started = Date.now();
const summary = { version: 1, startedAt: new Date().toISOString(), status: "running", checks: {} };
let browser, server;
const unpack = result => { const value = structured(result); return value.result ?? value; };
const evidence = result => { const record = unpack(result); if (!/^ev_[a-f0-9]{64}$/.test(record.evidence_id)) throw new Error("Missing Evidence identity"); return record; };
try {
  const identity = await mcp.request("initialize", { protocolVersion: "2025-06-18", capabilities: {}, clientInfo: { name: "loom-research-r0", version: "1" } });
  mcp.notify("notifications/initialized");
  const catalog = await mcp.request("tools/list");
  const session = structured(await mcp.call("binary_session", {}));
  summary.identity = identity.serverInfo;
  summary.serverCatalog = (session.result ?? session).server_identity?.catalog;
  summary.catalogCount = catalog.tools.length;
  summary.catalogSha256 = createHash("sha256").update(JSON.stringify(catalog.tools)).digest("hex");
  save("catalog.json", catalog); save("session.json", session);
  save("selected-schemas.json", catalog.tools.filter(t => ["inspect_desktop_application", "analyze_javascript_application", "list_browser_targets", "inspect_web_page", "get_evidence_bundle", "export_evidence_bundle", "inspect_analysis_view"].includes(t.name)));
  summary.checks.handshake = "passed";
  const app = join(output, "LoomProbe.app", "Contents", "Resources", "app");
  mkdirSync(app, { recursive: true });
  writeFileSync(join(app, "package.json"), '{"name":"loom-probe-app","version":"1.0.0","main":"index.js"}');
  const source = 'import fs from "node:fs"; fs.writeFileSync("APP_EXECUTED", "unexpected"); export function search(items, query) { return items.filter(x => x.includes(query)); }';
  writeFileSync(join(app, "index.js"), source);
  const desktop = evidence(await mcp.call("inspect_desktop_application", { path: join(output, "LoomProbe.app"), detail: "complete" }));
  save("desktop.json", desktop);
  const js = evidence(await mcp.call("analyze_javascript_application", { input_path: app, detail: "summary", source_paths: ["index.js"], analysis_timeout_ms: 30_000 }));
  save("javascript.json", js);
  if (existsSync(join(output, "APP_EXECUTED")) || existsSync(join(app, "APP_EXECUTED")) || readFileSync(join(app, "index.js"), "utf8") !== source) throw new Error("Static target mutated/executed");
  summary.checks.staticApp = "passed";
  summary.appEvidence = [desktop.evidence_id, js.evidence_id];
  const invalid = await mcp.call("inspect_desktop_application", { path: join(output, "missing-target") });
  if (!invalid.isError && !invalid.structuredContent?.error) throw new Error("Missing target unexpectedly succeeded");
  summary.checks.invalidTarget = "passed";

  if (options.chrome) {
    server = createServer((req, res) => {
      res.setHeader("Content-Type", req.url === "/app.js" ? "text/javascript" : "text/html");
      res.end(req.url === "/app.js" ? 'document.querySelector("#result").textContent="Local filtering"; window.loomProbe = {version: 1};' : '<!doctype html><title>Loom research fixture</title><main><h1>Search fixture</h1><input aria-label="Search"><p id="result"></p></main><script src="/app.js"></script>');
    });
    await new Promise((resolve, reject) => { server.once("error", reject); server.listen(0, "127.0.0.1", resolve); });
    const origin = `http://127.0.0.1:${server.address().port}`;
    const profile = join(output, "browser-profile"); mkdirSync(profile, { recursive: true });
    browser = spawn(resolve(options.chrome), ["--headless=new", "--remote-debugging-port=0", `--user-data-dir=${profile}`, "--no-first-run", "--no-default-browser-check", origin], { detached: true, stdio: "ignore" });
    browser.on("error", error => { summary.browserError = error.code; });
    browser.closed = new Promise(resolve => browser.once("close", (code, signal) => { browser.ended = true; resolve({ code, signal }); }));
    let endpoint;
    for (let n = 0; n < 100; n++) {
      const portFile = join(profile, "DevToolsActivePort");
      if (existsSync(portFile)) { endpoint = `http://127.0.0.1:${readFileSync(portFile, "utf8").split("\n")[0]}`; break; }
      if (browser.ended) throw new Error("Browser exited before CDP ready");
      await delay(100);
    }
    if (!endpoint) throw new Error("Browser CDP startup deadline");
    const targets = evidence(await mcp.call("list_browser_targets", { cdp_endpoint: endpoint, allowed_origins: [origin] }));
    save("targets.json", targets);
    const rows = targets.normalized_result.targets;
    const target = rows.find(t => t.url?.startsWith(origin));
    if (!target) throw new Error("Expected browser target missing");
    const page = evidence(await mcp.call("inspect_web_page", { cdp_endpoint: endpoint, target_id: target.target_id ?? target.id, allowed_origins: [origin], include_script_sources: true, include_accessibility_text: true, observation_ms: 100 }));
    save("website.json", page); summary.websiteEvidence = page.evidence_id;
    summary.checks.website = "passed";
  }
  const bundle = unpack(await mcp.call("export_evidence_bundle", { path: join(output, "evidence.bundle.json") }));
  save("export-receipt.json", bundle);
  if (!existsSync(join(output, "evidence.bundle.json"))) throw new Error("Evidence export did not publish a file");
  summary.checks.export = "passed";
  // Force an oversized frame on a second real server: no successful parse is allowed.
  const small = new ProbeMcp(node, [rea, "mcp"], { cwd: output, maxFrameBytes: 128 });
  try {
    await small.request("initialize", { protocolVersion: "2025-06-18", capabilities: {}, clientInfo: { name: "loom-small-frame", version: "1" } });
    throw new Error("Oversized MCP frame unexpectedly accepted");
  } catch (error) {
    if (!String(error.message).includes("frame limit")) throw error;
    summary.checks.frameLimit = "passed";
  } finally { summary.limitExit = await small.close(); }
  const large = join(output, "cancel-app"); mkdirSync(large, { recursive: true });
  writeFileSync(join(large, "package.json"), '{"name":"cancel-probe","version":"1.0.0"}');
  for (let i=0; i<200; i++) writeFileSync(join(large, `module-${i}.js`), `export function search${i}(items, query) { return items.filter(x => x.includes(query)); }`);
  const pending = mcp.call("analyze_javascript_application", { input_path: large, detail: "summary" }, 5000)
    .then(value => ({ value }), error => ({ error: error.message }));
  const requestId = mcp.sequence;
  await delay(150);
  const cancelStarted = Date.now();
  mcp.notify("notifications/cancelled", { requestId, reason: "R0 cancellation probe" });
  // MCP SDK may suppress the cancelled request's reply. Host-owned shutdown,
  // rather than receiving that reply, is the completion condition.
  await delay(200); await mcp.close();
  const cancelled = await pending;
  if (cancelled.value && !cancelled.value.isError) throw new Error("Analysis completed before cancellation; cancellation was not exercised");
  let alive = true;
  for (let attempt=0; attempt<20 && alive; attempt++) {
    try { process.kill(-mcp.child.pid, 0); await delay(100); }
    catch (error) { if (error.code !== "ESRCH") throw error; alive = false; }
  }
  if (alive) throw new Error("Owned REA process group survived cancellation");
  summary.cancelElapsedMs = Date.now()-cancelStarted;
  summary.cancelReply = cancelled.value?.structuredContent?.error?.code ?? "no_reply_host_reaped";
  summary.checks.cancellation = "passed";
  summary.status = "passed";
} catch (error) {
  summary.status = "failed"; summary.error = String(error.message).slice(0, 400); process.exitCode = 1;
} finally {
  if (browser && !browser.ended) {
    const kill = signal => { try { process.kill(-browser.pid, signal); } catch (e) { if (e.code !== "ESRCH") throw e; } };
    kill("SIGTERM"); const timer = setTimeout(() => kill("SIGKILL"), 3000);
    try { summary.browserExit = await browser.closed; } finally { clearTimeout(timer); }
  }
  if (server) { server.closeAllConnections(); await new Promise(resolve => server.close(resolve)); }
  summary.exit = await mcp.close(); summary.elapsedMs = Date.now() - started;
  summary.stderrBytes = mcp.stderrBytes; summary.notifications = mcp.notifications;
  save("summary.json", summary); console.log(JSON.stringify(summary, null, 2));
}
