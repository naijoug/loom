#!/usr/bin/env node
// Layer rules for the agent invocation layer (docs/plans/2026-10-26/22:42-agent-layer-and-activity-rail.md).
// 1. src-tauri/src/agent/ never imports feature modules.
// 2. Learn and Chat never import each other.
// 3. Feature modules never spawn processes outside test code; agent CLIs start only through agent::run.
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(fileURLToPath(new URL(".", import.meta.url)), "..");
const src = join(root, "src-tauri", "src");

const FEATURE_MODULES = ["chat", "chat_context", "learn", "skills", "workbench"];
const CHAT_MODULES = ["chat", "chat_context"];
const SPAWNING_FEATURES = ["chat", "chat_context.rs", "learn"];
// Non-agent processes a feature may start itself.
const SPAWN_ALLOWLIST = new Map([["learn/fetch.rs", "git clone/fetch for study sources"]]);

function rustFiles(path) {
  if (statSync(path).isFile()) return path.endsWith(".rs") ? [path] : [];
  return readdirSync(path).flatMap((name) => rustFiles(join(path, name)));
}

function referencesModule(text, modules) {
  return modules.filter((name) => new RegExp(`\\bcrate::${name}\\b`).test(text));
}

// Drop `#[cfg(test)] mod … {` blocks and whole *_tests.rs files.
function productionCode(file, text) {
  if (/(^|_)tests?\.rs$/.test(file)) return "";
  const testModule = text.search(/#\[cfg\(test\)\]\s*(pub(\([a-z]+\))?\s+)?mod\s+\w+\s*\{/);
  return testModule === -1 ? text : text.slice(0, testModule);
}

const violations = [];
const report = (file, message) => violations.push(`${relative(root, file)}: ${message}`);

for (const file of rustFiles(join(src, "agent"))) {
  const hits = referencesModule(readFileSync(file, "utf8"), FEATURE_MODULES);
  if (hits.length) report(file, `agent/ must not import feature modules (${hits.join(", ")})`);
}

for (const file of rustFiles(join(src, "learn"))) {
  const hits = referencesModule(readFileSync(file, "utf8"), CHAT_MODULES);
  if (hits.length) report(file, `learn/ must not import Chat (${hits.join(", ")}); use crate::agent`);
}

for (const file of [...rustFiles(join(src, "chat")), join(src, "chat_context.rs")]) {
  const hits = referencesModule(readFileSync(file, "utf8"), ["learn"]);
  if (hits.length) report(file, "Chat must not import learn/");
}

for (const entry of SPAWNING_FEATURES) {
  for (const file of rustFiles(join(src, entry))) {
    const path = relative(src, file);
    if (SPAWN_ALLOWLIST.has(path)) continue;
    const code = productionCode(path, readFileSync(file, "utf8"));
    if (/\btokio::process\b|\bstd::process::Command\b|\bCommand::new\(/.test(code)) {
      report(file, "feature modules must start agent CLIs through agent::run, not spawn processes");
    }
  }
}

if (violations.length) {
  console.error(`Layer check failed:\n  ${violations.join("\n  ")}`);
  process.exit(1);
}
console.log("Layer check passed.");
