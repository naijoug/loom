/** Explicit DEV-only Learn fixture (`?learnFixture=…`). No IPC, network, git or model calls. */
import { parseSourceTarget, type LearnApi, type LearnArchitecture, type LearnEvent, type LearnPageInfo, type LearnStageState, type LearnStudy, type LearnTreeEntry } from "../../api/learn";

const scenario = new URLSearchParams(location.search).get("learnFixture") ?? "ready";
const files: Record<string, string> = {
  "Cargo.toml": "[package]\nname = \"hexyl\"\nversion = \"0.16.0\"\nedition = \"2021\"\n\n[dependencies]\nclap = \"4\"\nowo-colors = \"4\"\n",
  "README.md": "# hexyl\n\nA command-line hex viewer.\n",
  "src/main.rs": Array.from({ length: 60 }, (_, i) => i === 0 ? "use clap::Parser;" : i === 9 ? "fn main() {" : i === 10 ? "    let opt = Opt::parse();" : i === 11 ? "    let mut printer = hexyl::PrinterBuilder::new(std::io::stdout()).build();" : i === 12 ? "    printer.print_all(input(&opt)).unwrap();" : i === 13 ? "}" : `// synthetic line ${i + 1}`).join("\n") + "\n",
  "src/lib.rs": Array.from({ length: 120 }, (_, i) => i === 19 ? "pub struct Printer<'a, Writer: Write> {" : i === 39 ? "    pub fn print_all<Reader: Read>(&mut self, reader: Reader) -> io::Result<()> {" : `// synthetic printer line ${i + 1}`).join("\n") + "\n",
  "src/colors.rs": "pub const COLOR_NULL: &str = \"\\x1b[38;5;242m\";\npub const COLOR_ASCII: &str = \"\\x1b[36m\";\n",
};
const architecture: LearnArchitecture = {
  schemaVersion: 1,
  project: { name: "hexyl", summary: "A colourful command-line hex viewer.", overview: "hexyl reads bytes from a file or stdin and prints offset, hex and character panels with a colour per byte category.", techStack: ["Rust", "clap", "owo-colors"], entryPoints: [{ path: "src/main.rs", line: 10, why: "Parses options and drives the printer" }] },
  layers: [{ id: "entry", title: "Entry", moduleIds: ["cli"] }, { id: "core", title: "Core", moduleIds: ["printer", "input"] }, { id: "support", title: "Support", moduleIds: ["colors"] }],
  modules: [
    { id: "cli", title: "Command line", summary: "Parses flags with clap and chooses the input.", level: 1, order: 1, paths: ["src/main.rs"], dependsOn: ["printer", "input"], keyFiles: [{ path: "src/main.rs", startLine: 10, endLine: 14, why: "main" }] },
    { id: "printer", title: "Printer", summary: "Formats each 16-byte line into panels.", level: 1, order: 2, paths: ["src/lib.rs"], dependsOn: ["colors"], keyFiles: [{ path: "src/lib.rs", startLine: 20, endLine: 45, why: "Printer" }] },
    { id: "input", title: "Input", summary: "Seeks and limits the byte stream.", level: 2, order: 3, paths: ["src/main.rs"], dependsOn: [], keyFiles: [] },
    { id: "colors", title: "Colours", summary: "Byte category colours.", level: 3, order: 4, paths: ["src/colors.rs"], dependsOn: [], keyFiles: [{ path: "src/colors.rs", startLine: 1, endLine: 2, why: "palette" }] },
  ],
  learningPath: [{ level: 1, title: "Run it", goal: "Follow one byte from argv to the terminal", moduleIds: ["cli", "printer"] }, { level: 2, title: "Inputs", goal: "Offsets and lengths", moduleIds: ["input"] }, { level: 3, title: "Details", goal: "Colour rules", moduleIds: ["colors"] }],
};
const pages: Record<string, string> = {
  overview: "# hexyl\n\nA colourful command-line hex viewer.\n\n## Tech stack\n\n- Rust\n- clap\n\n## Entry points\n\n- [src/main.rs:10](loom-src:src/main.rs#L10-L14) — Parses options and drives the printer\n\n## Learning path\n\n### 1. Run it\n\n1. [Command line](loom-wiki:module/cli)\n2. [Printer](loom-wiki:module/printer)\n",
  architecture: "# Architecture\n\n## Layers\n\n### Entry\n\n#### [Command line](loom-wiki:module/cli)\n\nParses flags. Key source: [main.rs:10-14](loom-src:src/main.rs#L10-L14)\n\n### Core\n\n#### [Printer](loom-wiki:module/printer)\n\nFormats panels. Key source: [lib.rs:20-45](loom-src:src/lib.rs#L20-L45)\n",
  "module:cli": "# Command line\n\n## What it does\n\n`main` parses options with clap ([main.rs:10-11](loom-src:src/main.rs#L10-L11)) and hands the reader to the [Printer](loom-wiki:module/printer).\n\n## Walkthrough\n\n1. `Opt::parse()` reads argv.\n2. The printer is built for stdout ([main.rs:12](loom-src:src/main.rs#L12-L12)).\n3. `print_all` streams the input ([lib.rs:40-45](loom-src:src/lib.rs#L40-L45)).\n\n| Step | Source |\n|---|---|\n| Parse | [main.rs](loom-src:src/main.rs#L11-L11) |\n| A stale reference | [gone.rs](loom-src:src/gone.rs#L1-L2) |\n",
};
let sequence = 0;
const now = () => Date.now();
const done = (stage: string): LearnStageState => ({ stage, status: "done", jobId: "job-preview", startedAtMs: 1, finishedAtMs: 2, warnings: [] });
function pagePlan(): LearnPageInfo[] {
  return [
    { pageId: "overview", title: "Overview", kind: "overview" },
    { pageId: "architecture", title: "Architecture", kind: "architecture" },
    ...architecture.modules.map(module => ({ pageId: `module:${module.id}`, title: module.title, kind: "module", moduleId: module.id, level: module.level })),
    { pageId: "glossary", title: "Glossary", kind: "glossary" },
  ].map(page => ({ ...page, ready: page.pageId in pages, refs: page.pageId in pages ? { valid: page.pageId === "module:cli" ? 4 : 2, invalid: page.pageId === "module:cli" ? 1 : 0 } : undefined }));
}
const study = (id: string, extra: Partial<LearnStudy> = {}): LearnStudy => ({
  schemaVersion: 1, studyId: id, url: "https://github.com/sharkdp/hexyl", owner: "sharkdp", repo: "hexyl", commit: "2e2643782d6ced9b5ac75596169a79127d8e535a",
  agentId: "agent-claude", agentName: "Claude Code", adapterType: "claude_code_cli", model: "sonnet", language: "en", depth: "standard", promptVersion: 1,
  createdAtMs: 1, updatedAtMs: 2, stages: ["fetch", "inventory", "architecture", "module:cli"].map(done), pages: pagePlan(),
  coverage: { sourceFiles: 3, covered: 3, uncovered: [] }, ...extra,
});
const studies: LearnStudy[] = scenario === "empty" ? [] : [study("study-preview")];
if (scenario === "running") studies[0] = study("study-preview", { stages: [done("fetch"), done("inventory"), { stage: "architecture", status: "running", jobId: "job-preview", startedAtMs: now(), warnings: [] }], pages: [], commit: undefined });
if (scenario === "failed") studies[0] = study("study-preview", { stages: [done("fetch"), done("inventory"), { stage: "architecture", status: "failed", jobId: "job-preview", startedAtMs: 1, finishedAtMs: 2, error: "agent exited with status exit status: 1; stderr: file:///usr/local/lib/node_modules/@openai/codex/bin/codex.js:107\n  throw new Error(\nError: Missing optional dependency @openai/codex-darwin-arm64. Reinstall Codex: npm install -g @openai/codex@latest", warnings: [] }], pages: [] });
const listeners = new Set<(event: LearnEvent) => void>();
const emit = (event: LearnEvent) => listeners.forEach(listener => listener(event));
const find = (id: string) => { const found = studies.find(item => item.studyId === id); if (!found) throw Error("Study not found"); return found; };
const setStage = (target: LearnStudy, stage: string, status: LearnStageState["status"]) => {
  const state = target.stages.find(item => item.stage === stage);
  const next: LearnStageState = { stage, status, jobId: "job-preview", startedAtMs: 1, finishedAtMs: status === "running" ? undefined : 2, warnings: [] };
  if (state) Object.assign(state, next); else target.stages.push(next);
  emit({ studyId: target.studyId, jobId: "job-preview", stage, status });
};
function run(target: LearnStudy, stages: string[]) {
  const [stage, ...rest] = stages;
  if (!stage) { emit({ studyId: target.studyId, jobId: "job-preview", stage: "job", status: "done" }); return; }
  setStage(target, stage, "running");
  setTimeout(() => {
    if (target.stages.find(item => item.stage === stage)?.status !== "running") return;
    if (stage === "architecture") target.pages = pagePlan();
    if (stage === "fetch") target.commit = "2e2643782d6ced9b5ac75596169a79127d8e535a";
    const page = target.pages.find(item => item.pageId === stage);
    if (page) { page.ready = true; page.refs = { valid: 3, invalid: 0 }; pages[stage] ??= `# ${page.title}\n\nSynthetic page for visual acceptance. See [lib.rs:20-22](loom-src:src/lib.rs#L20-L22).\n`; }
    setStage(target, stage, "done");
    run(target, rest);
  }, 700);
}
const summary = (item: LearnStudy) => ({
  studyId: item.studyId, owner: item.owner, repo: item.repo, commit: item.commit, agentName: item.agentName, model: item.model,
  status: item.stages.some(s => s.status === "running") ? "running" : item.stages.some(s => s.status === "failed") ? "failed" : item.pages.some(p => p.ready) ? "ready" : "idle",
  pagesReady: item.pages.filter(p => p.ready).length, pagesTotal: item.pages.length, updatedAtMs: item.updatedAtMs,
});
function tree(dir: string): LearnTreeEntry[] {
  const prefix = dir ? `${dir}/` : "";
  const names = new Map<string, LearnTreeEntry>();
  for (const [path, text] of Object.entries(files)) {
    if (!path.startsWith(prefix)) continue;
    const [name, ...rest] = path.slice(prefix.length).split("/");
    names.set(name, { name, path: prefix + name, kind: rest.length ? "dir" : "file", size: rest.length ? 0 : text.length });
  }
  return [...names.values()].sort((a, b) => Number(a.kind !== "dir") - Number(b.kind !== "dir") || a.name.localeCompare(b.name));
}

export const previewLearnApi: LearnApi = {
  list: async () => { if (scenario === "error") throw Error("Synthetic storage error"); return { studies: studies.map(summary), storageErrors: [] }; },
  create: async input => {
    const created = study(`study-preview-${++sequence}`, { url: input.url, owner: input.url.split("/")[3] ?? "owner", repo: input.url.split("/")[4] ?? "repo", model: input.model, language: input.language, depth: input.depth, commit: undefined, stages: ["fetch", "inventory", "architecture"].map(stage => ({ stage, status: "pending", warnings: [] })), pages: [] });
    studies.unshift(created);
    run(created, ["fetch", "inventory", "architecture", "module:cli", "module:printer"]);
    return { study: created };
  },
  get: async id => { const found = find(id); return { study: structuredClone(found), architecture: found.pages.length ? architecture : undefined }; },
  readPage: async (id, pageId) => {
    find(id);
    const markdown = pages[pageId];
    if (!markdown) throw Error("This page has not been generated yet");
    const refs = [...markdown.matchAll(/\]\(loom-src:([^)]+)\)/g)].map(([, target]) => {
      const { path, startLine = 0, endLine = 0 } = parseSourceTarget(target);
      const total = files[path]?.split("\n").length;
      return { target, path, startLine, endLine, status: total === undefined ? "missing" : endLine < total ? "valid" : "range" };
    });
    return { pageId, markdown, refs };
  },
  generate: async (id, stage) => {
    const target = find(id);
    const stages = stage === "all" ? [...target.pages.filter(p => !p.ready && p.kind === "module").map(p => p.pageId), "glossary"] : stage === "fetch" ? ["fetch", "inventory", "architecture"] : [stage];
    run(target, stages);
    return "job-preview";
  },
  cancel: async id => { const target = find(id); const running = target.stages.find(s => s.status === "running"); if (running) setStage(target, running.stage, "cancelled"); return !!running; },
  readLogs: async (_id, _job, stream) => ({ text: stream === "prompt" ? "You are studying a repository…\n(synthetic prompt)" : stream === "stderr" ? "" : "{\"type\":\"system\",\"subtype\":\"init\"}\n{\"type\":\"assistant\",\"message\":\"synthetic\"}\n", nextOffset: 0, hasMore: false }),
  listTree: async (_id, dir = "") => tree(dir),
  readSource: async (_id, path) => {
    const text = files[path];
    if (text === undefined) throw Error("File not found");
    return { path, text, firstLine: 1, totalLines: text.split("\n").length - 1, truncated: false, language: path.endsWith(".rs") ? "Rust" : "Text" };
  },
  remove: async id => { studies.splice(studies.findIndex(item => item.studyId === id), 1); emit({ studyId: id, stage: "study", status: "deleted" }); },
  subscribe: async onEvent => { listeners.add(onEvent); return () => { listeners.delete(onEvent); }; },
};
