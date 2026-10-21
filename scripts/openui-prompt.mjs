#!/usr/bin/env node
// Generates the OpenUI inline-card system prompt from the frontend component library, so the
// prompt the Rust adapter appends always matches what the renderer accepts.
//   node scripts/openui-prompt.mjs          write src-tauri/resources/openui-inline-prompt.md
//   node scripts/openui-prompt.mjs --check  fail when the committed prompt is stale

import { mkdirSync, readFileSync, writeFileSync, existsSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { transformWithEsbuild } from "vite";

// Runtime telemetry is opt-in upstream; disable it explicitly as well.
process.env.OPENUI_TELEMETRY_DISABLED = "1";

const root = new URL("..", import.meta.url).pathname;
const source = join(root, "src/features/chat/openui/library.tsx");
const target = join(root, "src-tauri/resources/openui-inline-prompt.md");
const check = process.argv.includes("--check");

// The library only imports packages, so a single-file transform is enough. Emit it inside the
// repository so bare imports resolve against the project's node_modules.
const outDir = join(root, ".tmp", "openui-prompt");
mkdirSync(outDir, { recursive: true });
const { code } = await transformWithEsbuild(readFileSync(source, "utf8"), source, { loader: "tsx", jsx: "automatic", format: "esm" });
const compiled = join(outDir, "library.mjs");
writeFileSync(compiled, code);
const { inlineCardPrompt } = await import(pathToFileURL(compiled).href);
const prompt = inlineCardPrompt();

if (check) {
  const current = existsSync(target) ? readFileSync(target, "utf8") : null;
  if (current !== prompt) {
    console.error(`${target} is out of date; run: node scripts/openui-prompt.mjs`);
    process.exit(1);
  }
  console.log(`openui prompt up to date (${prompt.length} chars)`);
} else {
  mkdirSync(join(root, "src-tauri/resources"), { recursive: true });
  writeFileSync(target, prompt);
  console.log(`wrote ${target} (${prompt.length} chars)`);
}
