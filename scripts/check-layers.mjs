#!/usr/bin/env node
// Layer rules for the agent invocation layer (docs/plans/2026-10-26/22:42-agent-layer-and-activity-rail.md).
// 1. src-tauri/src/agent/ never imports feature modules.
// 2. Learn, Research and Chat never import each other.
// 3. Feature modules never spawn processes outside test code; agent CLIs start only through agent::run.
// 4. Frontend: src/features/<x> never imports src/features/<y> (code or CSS); shared UI lives in src/components,
//    and src/components, src/hooks and src/api never import features. Only src/shell and the entry wire features together.
import { productionCode, rustCodeMask } from "./lib/rust-production.mjs";
import { moduleReferences, spawnsProcess } from "./lib/rust-layer-rules.mjs";
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(fileURLToPath(new URL(".", import.meta.url)), "..");
const src = join(root, "src-tauri", "src");

const FEATURE_MODULES = ["chat", "chat_context", "learn", "research", "discuss", "experiments", "skills", "workbench"];
const CHAT_MODULES = ["chat", "chat_context"];
const SPAWNING_FEATURES = ["chat", "chat_context.rs", "learn", "research", "discuss", "experiments", "research_sources", "rea", "browser_host", "repository_sources"];
// Non-agent processes a feature may start itself.
const SPAWN_ALLOWLIST = new Map([["learn/fetch.rs", "git clone/fetch for study sources"]]);

function rustFiles(path) {
  if (!existsSync(path)) return [];
  if (statSync(path).isFile()) return path.endsWith(".rs") ? [path] : [];
  return readdirSync(path).flatMap((name) => rustFiles(join(path, name)));
}

function referencesModule(text, modules) {
  return moduleReferences(text, modules);
}

const violations = [];
const report = (file, message) => violations.push(`${relative(root, file)}: ${message}`);

for (const shared of ["agent", "tool_process", "research_sources", "rea", "browser_host", "repository_sources"]) {
  for (const file of rustFiles(join(src, shared))) {
    const hits = referencesModule(productionCode(file, readFileSync(file, "utf8")), FEATURE_MODULES);
    if (hits.length) report(file, `${shared}/ must not import feature modules (${hits.join(", ")})`);
  }
}

for (const file of rustFiles(join(src, "learn"))) {
  const hits = referencesModule(readFileSync(file, "utf8"), [...CHAT_MODULES, "research", "discuss", "experiments"]);
  if (hits.length) report(file, `learn/ must not import other features (${hits.join(", ")}); use crate::agent`);
}

for (const file of rustFiles(join(src, "research"))) {
  const hits = referencesModule(readFileSync(file, "utf8"), [...CHAT_MODULES, "learn", "discuss", "experiments"]);
  if (hits.length) report(file, `research/ must not import other features (${hits.join(", ")}); use crate::agent`);
}

for (const file of rustFiles(join(src, "experiments"))) {
  const hits = referencesModule(productionCode(file, readFileSync(file, "utf8")), [...CHAT_MODULES, "learn", "research", "discuss"]);
  if (hits.length) report(file, `experiments/ must not import other features (${hits.join(", ")})`);
}

for (const file of [...rustFiles(join(src, "chat")), join(src, "chat_context.rs")]) {
  const hits = referencesModule(readFileSync(file, "utf8"), ["learn", "research", "discuss"]);
  if (hits.length) report(file, `Chat must not import other features (${hits.join(", ")})`);
}

for (const file of rustFiles(join(src, "discuss"))) {
  const code = productionCode(file, readFileSync(file, "utf8"));
  const hits = referencesModule(code, [...CHAT_MODULES, "learn", "research", "skills", "process_supervisor"]);
  if (hits.length) report(file, `discuss/ must use shared agent and component APIs (${hits.join(", ")})`);
}

for (const entry of SPAWNING_FEATURES) {
  for (const file of rustFiles(join(src, entry))) {
    const path = relative(src, file);
    if (SPAWN_ALLOWLIST.has(path)) continue;
    const code = rustCodeMask(productionCode(path, readFileSync(file, "utf8")));
    if (/\bprocess_supervisor\b/.test(code)) report(file, "feature lifecycle must use the agent public API, not process_supervisor");
    if (/\bagent::(?:registry|diagnostics|runner|invocation|lifecycle|stream|tools|event|capabilities)::/.test(code)) report(file, "feature modules must use agent re-exports, not private submodules");
    if (spawnsProcess(code)) {
      report(file, "feature modules must start agent CLIs through agent::run, not spawn processes");
    }
  }
}

const web = join(root, "src");
// Cross-feature imports that are part of the product design.
const FEATURE_IMPORT_ALLOWLIST = new Map([["settings->skills", "Skills is a section of Settings"]]);

function webFiles(path) {
  if (statSync(path).isFile()) return /\.(tsx?|css)$/.test(path) ? [path] : [];
  return readdirSync(path).flatMap((name) => webFiles(join(path, name)));
}

// Relative specifiers from `import … from`, side-effect imports, dynamic import() and CSS @import.
function importTargets(file) {
  const text = readFileSync(file, "utf8");
  const specs = [...text.matchAll(/(?:\bfrom\s*|\bimport\s*\(?\s*|@import\s+(?:url\()?)["']([^"']+)["']/g)].map((match) => match[1]);
  return specs.filter((spec) => spec.startsWith(".")).map((spec) => resolve(dirname(file), spec));
}

const featureOf = (path) => {
  const rel = relative(join(web, "features"), path);
  return rel.startsWith("..") ? null : rel.split(sep)[0];
};

for (const file of webFiles(join(web, "features"))) {
  const own = featureOf(file);
  for (const target of importTargets(file)) {
    const other = featureOf(target);
    if (other && other !== own && !FEATURE_IMPORT_ALLOWLIST.has(`${own}->${other}`)) {
      report(file, `features/${own} must not import features/${other} (${relative(web, target)}); move shared code to src/components`);
    }
  }
}

for (const shared of ["components", "hooks", "api"]) {
  for (const file of webFiles(join(web, shared))) {
    const hits = importTargets(file).map(featureOf).filter(Boolean);
    if (hits.length) report(file, `src/${shared} must not import features (${[...new Set(hits)].join(", ")})`);
  }
}

if (violations.length) {
  console.error(`Layer check failed:\n  ${violations.join("\n  ")}`);
  process.exit(1);
}
console.log("Layer check passed.");
