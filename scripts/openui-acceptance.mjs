#!/usr/bin/env node
// Scores real-model inline card answers written by the ignored Rust test
// `claude_inline_cards_acceptance` (plan O5) with the frontend parser.
//   node scripts/openui-acceptance.mjs <dir> [<dir> ...]
// Each <dir> holds `<case>-<mode>.json` records; prints a Markdown table and a summary.

import { mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { transformWithEsbuild } from "vite";

process.env.OPENUI_TELEMETRY_DISABLED = "1";
const root = new URL("..", import.meta.url).pathname;
const source = join(root, "src/features/chat/openui/library.tsx");
const outDir = join(root, ".tmp", "openui-acceptance");
mkdirSync(outDir, { recursive: true });
const { code } = await transformWithEsbuild(readFileSync(source, "utf8"), source, { loader: "tsx", jsx: "automatic", format: "esm" });
writeFileSync(join(outDir, "library.mjs"), code);
const { checkInlineCard } = await import(pathToFileURL(join(outDir, "library.mjs")).href);

const dirs = process.argv.slice(2);
if (!dirs.length) {
  console.error("usage: node scripts/openui-acceptance.mjs <dir> [<dir> ...]");
  process.exit(2);
}

const rows = [];
for (const dir of dirs) {
  for (const name of readdirSync(dir).filter(file => /^q\d+-\w+\.json$/.test(file)).sort()) {
    const record = JSON.parse(readFileSync(join(dir, name), "utf8"));
    const blocks = [...record.content.matchAll(/```openui-lang\n([\s\S]*?)(?:```|$)/g)].map(match => match[1].trim());
    const checks = blocks.map(block => checkInlineCard(block));
    rows.push({ ...record, blocks: blocks.length, parsed: checks.filter(check => check.ok).length, problems: checks.flatMap(check => check.problems) });
  }
}

const valid = row => row.status === "complete";
const verdict = row => {
  if (!valid(row)) return "invalid";
  if (row.expectsCard) return row.parsed > 0 ? "card" : row.blocks ? "fallback" : "no card";
  return row.blocks ? "unwanted card" : "ok (no card)";
};
console.log("| case | mode | status | s | tools | blocks | parsed | problems | writes | verdict |");
console.log("|---|---|---|---|---|---|---|---|---|---|");
for (const row of rows) {
  console.log(`| ${row.id}${row.resumed ? " (resume)" : ""} | ${row.mode} | ${row.status} | ${row.seconds} | ${row.tools.length} | ${row.blocks} | ${row.parsed} | ${row.problems.join(",") || "—"} | ${row.projectWrites ? "yes" : "no"} | ${verdict(row)} |`);
}

for (const mode of [...new Set(rows.map(row => row.mode))]) {
  const scoped = rows.filter(row => row.mode === mode);
  const cards = scoped.filter(row => row.expectsCard && valid(row));
  const plain = scoped.filter(row => !row.expectsCard && valid(row));
  const blocks = scoped.reduce((sum, row) => sum + row.blocks, 0);
  const parsed = scoped.reduce((sum, row) => sum + row.parsed, 0);
  console.log(`\n${mode}: ${scoped.length} turns, ${scoped.filter(valid).length} complete; card cases with a parseable card ${cards.filter(row => row.parsed).length}/${cards.length}; ` +
    `no-card cases without a card ${plain.filter(row => !row.blocks).length}/${plain.length}; blocks parsed ${parsed}/${blocks} (fallbacks ${blocks - parsed}); ` +
    `project writes ${scoped.filter(row => row.projectWrites).length}; prompt redacted in logs ${scoped.filter(row => row.promptRedacted).length}/${scoped.length}`);
}
