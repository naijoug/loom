/** Explicit DEV-only Research fixture (`?researchFixture=…`, including `targets`). No IPC, network, git or model calls. */
import { lastTurn, type Research, type ResearchApi, type ResearchEvent, type ResearchTurn } from "../../api/research";
import type { EvidenceRecord, TargetEvent, TargetInput, TargetPreflight, TargetReportContent, TargetRun, TargetStatus } from "../../api/researchTargets";
import type { TargetPreviewApi } from "./previewContext";
import { parseSourceTarget, type TreeEntry } from "../../api/sourceFiles";

const scenario = new URLSearchParams(location.search).get("researchFixture") ?? "ready";
const files: Record<string, string> = {
  "src-tauri/src/agent/run.rs": Array.from({ length: 140 }, (_, i) => i === 39 ? "pub(crate) async fn run(spec: RunSpec, cancel: impl Fn() -> bool) -> AgentOutcome {" : i === 59 ? "    let child = supervisor.spawn(&spec.prepared, spec.owner)?;" : i === 89 ? "    if cancel() { supervisor.terminate(child, TerminationReason::Cancelled); }" : `// synthetic agent run line ${i + 1}`).join("\n") + "\n",
  "src-tauri/src/process_supervisor.rs": Array.from({ length: 80 }, (_, i) => i === 19 ? "pub fn terminate(&self, id: u32, reason: TerminationReason) {" : `// synthetic supervisor line ${i + 1}`).join("\n") + "\n",
  "README.md": "# Loom\n\nA local agent workbench.\n",
};
const reports: Record<string, string> = {
  "research-preview:1": "# How Loom runs and stops an agent CLI\n\n## Conclusion\n\nEvery feature hands a prepared invocation to `agent::run`, which spawns the CLI through the process supervisor and terminates the whole process group when the user cancels.\n\n## Evidence\n\n- `run` owns the lifecycle ([run.rs:40-60](loom-src:src-tauri/src/agent/run.rs#L40-L60)).\n- Cancellation is polled and forwarded to the supervisor ([run.rs:90](loom-src:src-tauri/src/agent/run.rs#L90-L90)).\n- The supervisor signals the group ([process_supervisor.rs:20-24](loom-src:src-tauri/src/process_supervisor.rs#L20-L24)).\n\n| Step | Source |\n|---|---|\n| Spawn | [run.rs:60](loom-src:src-tauri/src/agent/run.rs#L60-L60) |\n| A stale reference | [gone.rs](loom-src:src-tauri/src/gone.rs#L1-L2) |\n\n## References\n\n- [Tauri process docs](https://tauri.app)\n\n## Open questions\n\n- Whether a hung child that ignores SIGTERM is escalated to SIGKILL.\n",
  "research-preview:2": "# Limits on a run\n\n## Conclusion\n\nEach turn has a 20 minute timeout and a capped output buffer.\n\n## Evidence\n\n- Limits are passed in `RunSpec` ([run.rs:40](loom-src:src-tauri/src/agent/run.rs#L40-L40)).\n\n## Open questions\n\n- None.\n",
  "research-web:1": "# Rust async runtimes\n\n## Conclusion\n\nTokio is the common default for desktop apps built on Tauri.\n\n## References\n\n- [Tokio](https://tokio.rs)\n",
};
let sequence = 0;
const turn = (index: number, question: string, extra: Partial<ResearchTurn> = {}): ResearchTurn => ({ index, question, status: "done", jobId: `job-preview-${index}`, startedAtMs: 1, finishedAtMs: 2, warnings: [], ...extra });
const research = (id: string, extra: Partial<Research> = {}): Research => ({
  schemaVersion: 1, researchId: id, title: "How does Loom start a local agent CLI and stop it when the user cancels?", agentId: "agent-codex", agentName: "Codex", adapterType: "codex_cli",
  model: "gpt-5", projectPath: "/preview/loom", projectName: "loom", language: "en", promptVersion: 1, createdAtMs: 1, updatedAtMs: 2,
  turns: [
    turn(1, "How does Loom start a local agent CLI and stop it when the user cancels?", { refs: { valid: 4, invalid: 1 } }),
    turn(2, "Which limits (timeout, output size) apply to that run, and where are they set?", { context: "native", refs: { valid: 1, invalid: 0 } }),
  ], ...extra,
});
const items: Research[] = scenario === "empty" ? [] : [research("research-preview"), research("research-web", { title: "Which async runtime do Tauri apps use?", projectPath: undefined, projectName: undefined, agentId: "agent-claude", agentName: "Claude Code", adapterType: "claude_code_cli", model: undefined, updatedAtMs: 1, turns: [turn(1, "Which async runtime do Tauri apps use?")] })];
if (scenario === "running") items[0].turns.push(turn(3, "Is the process group killed on Windows too?", { status: "running", finishedAtMs: undefined, context: "native" }));
if (scenario === "failed") items[0].turns.push(turn(3, "Is the process group killed on Windows too?", { status: "failed", context: "replay", error: "agent exited with status exit status: 1; stderr: Error: Missing optional dependency @openai/codex-darwin-arm64. Reinstall Codex: npm install -g @openai/codex@latest", warnings: ["Resuming the agent session failed (exit status 1); earlier questions were replayed instead"] }));
const targetRuns = new Map<string, TargetRun[]>();
const targetReports = new Map<string, TargetReportContent>();
const targetFiles = new Map<string, Record<string, string>>();
const targetEvidence = new Map<string, EvidenceRecord[]>();
const targetEvidenceText = new Map<string, string>();
const preflights = new Map<string, TargetPreflight>();
const targetListeners = new Set<(event: TargetEvent) => void>();
const usage = { calls: 1, collections: 1, rounds: 1, activeMs: 1200, diskBytes: 40, diskReservedBytes: 0, reservationMs: 0 };
const budget = { maxRounds: 1, maxCalls: 4, maxCollections: 1, maxActiveMs: 120000, maxDiskBytes: 64 * 1024 * 1024 };
function targetRun(researchId: string, runId: string, input: TargetInput, status: TargetStatus, reportIds: string[], extra: Partial<TargetRun["state"]> = {}): TargetRun {
  return {
    spec: { schemaVersion: 2, researchId, runId, kind: "initial", input, budget, configDigest: "preview", parentReportId: null, agentConfigDigest: "preview", targetIdentity: researchId, sourceSnapshots: reportIds.length ? [`snap-${reportIds[0]}`] : [], sourceEvidence: [], createdAtMs: 1 },
    state: { runId, status, attempt: 1, sequence: 1, usage: { ...usage, collections: input.target.kind === "github" ? 1 : 1 }, snapshotIds: reportIds.length ? [`snap-${reportIds[0]}`] : [], evidenceIds: [], reportIds, stopReason: null, error: null, updatedAtMs: 2, exchange: { calls: 1, reads: 0, corrections: 0 }, ...extra },
  };
}
function publishReport(reportId: string, runId: string, markdown: string, filesForReport: Record<string, string>, records: EvidenceRecord[]) {
  targetReports.set(reportId, { report: { reportId, runId, revision: 1, snapshotIds: [`snap-${reportId}`], evidenceIds: records.map(record => record.evidenceId), defaultSourceSnapshotId: `snap-${reportId}`, markdownSha256: "preview", createdAtMs: 1 }, markdown });
  targetFiles.set(reportId, filesForReport);
  targetEvidence.set(reportId, records);
  for (const record of records) targetEvidenceText.set(record.evidenceId, `${record.tool}: ${record.coverage}`);
}
function seedTargets() {
  items.length = 0;
  const github = research("research-gh", { workflow: "target", title: "What is pinned on GitHub?", projectPath: undefined, projectName: undefined, updatedAtMs: 50, turns: [], target: { target: { kind: "github", url: "https://github.com/acme/loom", gitRef: null }, runIds: ["run-gh"] } });
  const website = research("research-web-target", { workflow: "target", title: "What does the website load?", projectPath: undefined, projectName: undefined, updatedAtMs: 40, turns: [], target: { target: { kind: "website", url: "https://example.test/docs", browser: null }, runIds: ["run-web"] } });
  const app = research("research-app", { workflow: "target", title: "What is in the local app?", projectPath: undefined, projectName: undefined, updatedAtMs: 30, turns: [], target: { target: { kind: "app", path: "/preview/app" }, runIds: ["run-app"] } });
  const running = research("research-collecting", { workflow: "target", title: "Watching a page", projectPath: undefined, projectName: undefined, updatedAtMs: 20, turns: [], target: { target: { kind: "website", url: "https://example.test/live", browser: null }, runIds: ["run-live"] } });
  const failed = research("research-failed-target", { workflow: "target", title: "App analysis failed", projectPath: undefined, projectName: undefined, updatedAtMs: 10, turns: [], target: { target: { kind: "app", path: "/preview/broken" }, runIds: ["run-failed"] } });
  const ghInput: TargetInput = { target: { kind: "github", url: "https://github.com/acme/loom", gitRef: null }, question: github.title, agentId: "agent-codex", model: null, language: "en", mode: "single", budget: null, tools: null };
  const webInput: TargetInput = { target: { kind: "website", url: "https://example.test/docs" }, question: website.title, agentId: "agent-codex", model: null, language: "en", mode: "single", budget: null, tools: { node: "/preview/node", entry: "/preview/rea.mjs" } };
  const appInput: TargetInput = { target: { kind: "app", path: "/preview/app" }, question: app.title, agentId: "agent-codex", model: null, language: "en", mode: "single", budget: null, tools: { node: "/preview/node", entry: "/preview/rea.mjs" } };
  const ghEvidence: EvidenceRecord = { evidenceId: "ev-gh", runId: "run-gh", sourceId: "snap-report-new", tool: "snapshot", toolVersion: "preview", rawSha256: "a", modelSha256: "b", bytes: 20, coverage: "pinned commit", limitations: ["repository code was not run"], createdAtMs: 1 };
  const pageEvidence: EvidenceRecord = { evidenceId: "ev-page", runId: "run-web", sourceId: null, tool: "inspect_web_page", toolVersion: "preview", rawSha256: "c", modelSha256: "d", bytes: 20, coverage: "page", limitations: ["scripts ran"], createdAtMs: 1 };
  const shotEvidence: EvidenceRecord = { evidenceId: "ev-shot", runId: "run-web", sourceId: null, tool: "screenshot", toolVersion: "preview", rawSha256: "e", modelSha256: "f", bytes: 20, coverage: "viewport", limitations: ["not in the text prompt"], createdAtMs: 1 };
  const appEvidence: EvidenceRecord = { evidenceId: "ev-app", runId: "run-app", sourceId: null, tool: "analyze_javascript", toolVersion: "preview", rawSha256: "g", modelSha256: "h", bytes: 20, coverage: "static", limitations: ["the app was not executed"], createdAtMs: 1 };
  publishReport("report-old", "run-gh", "# Older checkout\n\nSee [main.rs](loom-src:src/main.rs#L1-L1) and [manifest](loom-evidence:ev-gh).\n", { "src/main.rs": "fn old() {}\n" }, [ghEvidence]);
  publishReport("report-new", "run-gh", "# Pinned checkout\n\nSee [main.rs](loom-src:src/main.rs#L1-L1) and [manifest](loom-evidence:ev-gh).\n", { "src/main.rs": "fn new() {}\n" }, [ghEvidence]);
  publishReport("report-web", "run-web", "# Website\n\nThe page ran scripts. See [app.js](loom-src:scripts/app.js#L1-L1) and [page](loom-evidence:ev-page).\n", { "scripts/app.js": "console.log('site');\n" }, [pageEvidence, shotEvidence]);
  publishReport("report-app", "run-app", "# Local app\n\nSee [index.js](loom-src:index.js#L1-L1) and [analysis](loom-evidence:ev-app).\n", { "index.js": "module.exports = {};\n" }, [appEvidence]);
  targetEvidenceText.set("ev-shot", "Screenshot saved outside the text prompt.");
  targetRuns.set("research-gh", [targetRun("research-gh", "run-gh", ghInput, "completed", ["report-old", "report-new"])]);
  targetRuns.set("research-web-target", [targetRun("research-web-target", "run-web", webInput, "completed", ["report-web"])]);
  targetRuns.set("research-app", [targetRun("research-app", "run-app", appInput, "completed", ["report-app"])]);
  targetRuns.set("research-collecting", [targetRun("research-collecting", "run-live", { ...webInput, question: running.title, target: { kind: "website", url: "https://example.test/live" } }, "collecting", [])]);
  targetRuns.set("research-failed-target", [targetRun("research-failed-target", "run-failed", { ...appInput, question: failed.title, target: { kind: "app", path: "/preview/broken" } }, "failed", [], { error: "agent exited with status exit status: 1; stderr: boom" })]);
  items.push(github, website, app, running, failed);
}
if (scenario === "targets") seedTargets();
const listeners = new Set<(event: ResearchEvent) => void>();
const emit = (event: ResearchEvent) => listeners.forEach(listener => listener(event));
const find = (id: string) => { const found = items.find(item => item.researchId === id); if (!found) throw Error("Research not found"); return found; };
function answer(target: Research, index: number) {
  emit({ researchId: target.researchId, jobId: `job-preview-${index}`, turn: index, status: "running" });
  setTimeout(() => {
    const current = target.turns.find(item => item.index === index);
    if (current?.status !== "running") return;
    reports[`${target.researchId}:${index}`] ??= `# ${current.question}\n\n## Conclusion\n\nSynthetic answer for visual acceptance.${target.projectPath ? " See [run.rs:40-42](loom-src:src-tauri/src/agent/run.rs#L40-L42)." : ""}\n\n## Open questions\n\n- None.\n`;
    Object.assign(current, { status: "done", finishedAtMs: Date.now(), refs: target.projectPath ? { valid: 1, invalid: 0 } : { valid: 0, invalid: 0 } });
    target.updatedAtMs = Date.now();
    emit({ researchId: target.researchId, jobId: current.jobId, turn: index, status: "done" });
  }, 900);
}
const summary = (item: Research) => {
  if (item.workflow === "target") {
    const runsForItem = targetRuns.get(item.researchId) ?? [];
    const status = runsForItem.length ? runsForItem[runsForItem.length - 1].state.status : "failed";
    return { researchId: item.researchId, title: item.title, agentName: item.agentName, model: item.model, projectName: item.projectName, workflow: "target" as const, status, turns: targetRuns.get(item.researchId)?.length ?? 0, updatedAtMs: item.updatedAtMs };
  }
  const last = lastTurn(item);
  return { researchId: item.researchId, title: item.title, agentName: item.agentName, model: item.model, projectName: item.projectName, status: last?.status === "running" ? "running" : last && last.status !== "done" ? "failed" : "ready", turns: item.turns.length, updatedAtMs: item.updatedAtMs };
};
function tree(dir: string): TreeEntry[] {
  const prefix = dir ? `${dir}/` : "";
  const names = new Map<string, TreeEntry>();
  for (const [path, text] of Object.entries(files)) {
    if (!path.startsWith(prefix)) continue;
    const [name, ...rest] = path.slice(prefix.length).split("/");
    names.set(name, { name, path: prefix + name, kind: rest.length ? "dir" : "file", size: rest.length ? 0 : text.length });
  }
  return [...names.values()].sort((a, b) => Number(a.kind !== "dir") - Number(b.kind !== "dir") || a.name.localeCompare(b.name));
}

export const previewResearchApi: ResearchApi = {
  list: async () => { if (scenario === "error") throw Error("Synthetic storage error"); return { items: [...items].sort((a, b) => b.updatedAtMs - a.updatedAtMs).map(summary), storageErrors: [] }; },
  create: async input => {
    const project = input.projectPath ? input.projectPath.split("/").pop() : undefined;
    const created = research(`research-preview-${++sequence}`, { title: input.question.split("\n")[0].slice(0, 80), model: input.model, projectPath: input.projectPath, projectName: project, language: input.language, createdAtMs: Date.now(), updatedAtMs: Date.now(), turns: [turn(1, input.question, { status: "running", finishedAtMs: undefined })] });
    items.unshift(created);
    answer(created, 1);
    return structuredClone(created);
  },
  get: async id => structuredClone(find(id)),
  readTurn: async (id, index) => {
    const target = find(id);
    const markdown = reports[`${id}:${index}`];
    if (!markdown) throw Error("This question has no report yet");
    const refs = target.projectPath ? [...markdown.matchAll(/\]\(loom-src:([^)]+)\)/g)].map(([, ref]) => {
      const { path, startLine = 0, endLine = 0 } = parseSourceTarget(ref);
      const total = files[path]?.split("\n").length;
      return { target: ref, path, startLine, endLine, status: total === undefined ? "missing" : endLine < total ? "valid" : "range" };
    }) : [];
    return { index, markdown, refs };
  },
  ask: async (id, question) => {
    const target = find(id);
    if (lastTurn(target)?.status === "running") throw Error("This research is already answering a question");
    const index = target.turns.length + 1;
    target.turns.push(turn(index, question, { status: "running", finishedAtMs: undefined, context: "native" }));
    answer(target, index);
    return structuredClone(target);
  },
  retry: async id => {
    const target = find(id);
    const last = lastTurn(target);
    if (!last || last.status === "done" || last.status === "running") throw Error("Only a failed or stopped question can be retried");
    Object.assign(last, turn(last.index, last.question, { status: "running", finishedAtMs: undefined, context: last.context }));
    answer(target, last.index);
    return structuredClone(target);
  },
  cancel: async id => {
    const last = lastTurn(find(id));
    if (last?.status !== "running") return false;
    Object.assign(last, { status: "cancelled", finishedAtMs: Date.now() });
    emit({ researchId: id, jobId: last.jobId, turn: last.index, status: "cancelled" });
    return true;
  },
  readLogs: async (_id, _job, stream) => ({ text: stream === "prompt" ? "You are researching a question for the user…\n(synthetic prompt)" : stream === "stderr" ? "" : "{\"type\":\"thread.started\",\"thread_id\":\"preview\"}\n{\"type\":\"item.completed\"}\n", nextOffset: 0, hasMore: false }),
  listTree: async (_id, dir = "") => tree(dir),
  readSource: async (_id, path) => {
    const text = files[path];
    if (text === undefined) throw Error("File not found");
    return { path, text, firstLine: 1, totalLines: text.split("\n").length - 1, truncated: false, language: path.endsWith(".rs") ? "Rust" : "Text" };
  },
  remove: async id => { items.splice(items.findIndex(item => item.researchId === id), 1); targetRuns.delete(id); emit({ researchId: id, status: "deleted" }); },
  subscribe: async onEvent => { listeners.add(onEvent); return () => { listeners.delete(onEvent); }; },
};

const ACTIVE_TARGET = new Set<TargetStatus>(["queued", "preflight", "collecting", "analyzing", "reporting", "cancelling"]);
function notify(researchId: string, status: TargetStatus) {
  targetListeners.forEach(listener => listener({ researchId, runId: "preview", stepId: null, status, phase: status, progress: usage, stopReason: null, error: null }));
  emit({ researchId, status });
}
function requireReport(researchId: string, reportId: string) {
  if (!(targetRuns.get(researchId) ?? []).some(run => run.state.reportIds.includes(reportId))) throw Error("report_scope_mismatch");
  const content = targetReports.get(reportId);
  if (!content) throw Error("report_unavailable");
  return content;
}
function finishLater(researchId: string, run: TargetRun, reportId: string) {
  setTimeout(() => {
    if (run.state.status !== "collecting") return;
    run.state.status = "completed";
    if (!run.state.reportIds.includes(reportId)) run.state.reportIds.push(reportId);
    run.state.updatedAtMs = Date.now();
    notify(researchId, "completed");
  }, 900);
}
export const previewTargetApi: TargetPreviewApi = {
  capabilities: async () => ({ singleShot: true, iteration: false, experiments: false }),
  subscribe: async onEvent => { targetListeners.add(onEvent); return () => { targetListeners.delete(onEvent); }; },
  preflight: async spec => {
    if (spec.mode !== "single") throw Error("research_iteration_not_enabled");
    if (spec.target.kind !== "github" && !spec.tools) throw Error("rea_runtime_required");
    const limitations = spec.target.kind === "website"
      ? ["Loading the page runs its scripts and contacts the network; this is not a read-only action.", "Screenshots stay in local evidence and are not pasted into the text model prompt."]
      : spec.target.kind === "github"
        ? ["Citations resolve only through the report's pinned snapshot.", "A later commit is a new snapshot and cannot rewrite this report."]
        : ["Native engines are diagnostic only; static observations do not prove runtime behavior."];
    const operations = spec.target.kind === "website"
      ? ["Launch an isolated Chromium profile and bind its loopback debugging port to this run.", "Load the page, observe it and its scripts, and save one screenshot outside the text prompt."]
      : spec.target.kind === "github"
        ? ["Fetch the public GitHub repository over HTTPS and pin one commit.", "Publish an immutable source snapshot, path manifest and paged excerpts."]
        : ["Inspect the selected app statically.", "Analyze JavaScript without executing the app."];
    const checked: TargetPreflight = { preflightId: `preflight-${++sequence}`, researchId: null, input: { ...spec, mode: "single", tools: spec.target.kind === "github" ? null : spec.tools }, budget, digest: "preview", expiresAtMs: Date.now() + 60_000, operations, limitations };
    preflights.set(checked.preflightId, checked);
    return structuredClone(checked);
  },
  start: async (preflightId, requestId) => {
    const checked = preflights.get(preflightId);
    if (!checked) throw Error("preflight_expired");
    const replay = targetRuns.get(`request-${requestId}`)?.[0];
    if (replay) return structuredClone(replay);
    const researchId = `research-preview-${++sequence}`;
    const runId = `run-${sequence}`;
    const reportId = `report-${sequence}`;
    const created = research(researchId, { workflow: "target", title: checked.input.question.split("\n")[0].slice(0, 80), projectPath: undefined, projectName: undefined, createdAtMs: Date.now(), updatedAtMs: Date.now(), turns: [], target: { target: checked.input.target, runIds: [runId] } });
    const filesForReport: Record<string, string> = checked.input.target.kind === "github" ? { "src/main.rs": "fn started() {}\n" } : checked.input.target.kind === "website" ? { "scripts/app.js": "console.log('started');\n" } : { "index.js": "module.exports = { started: true };\n" };
    const evidenceId = `ev-${sequence}`;
    const tool = checked.input.target.kind === "website" ? "inspect_web_page" : checked.input.target.kind === "github" ? "snapshot" : "analyze_javascript";
    publishReport(reportId, runId, `# ${created.title}\n\nSee [source](loom-src:${Object.keys(filesForReport)[0]}#L1-L1) and [evidence](loom-evidence:${evidenceId}).\n`, filesForReport, [{ evidenceId, runId, sourceId: null, tool, toolVersion: "preview", rawSha256: "p", modelSha256: "q", bytes: 12, coverage: "started", limitations: ["single pass"], createdAtMs: Date.now() }]);
    const run = targetRun(researchId, runId, checked.input, "completed", [reportId]);
    targetRuns.set(researchId, [run]);
    targetRuns.set(`request-${requestId}`, [run]);
    items.unshift(created);
    notify(researchId, "completed");
    return structuredClone(run);
  },
  getRun: async (researchId, runId) => {
    const run = targetRuns.get(researchId)?.find(item => item.spec.runId === runId);
    if (!run) throw Error("run_not_found");
    return structuredClone(run);
  },
  listRuns: async researchId => structuredClone(targetRuns.get(researchId) ?? []),
  cancel: async (researchId, runId) => {
    const run = targetRuns.get(researchId)?.find(item => item.spec.runId === runId);
    if (!run || !ACTIVE_TARGET.has(run.state.status)) return false;
    run.state.status = "cancelled";
    notify(researchId, "cancelled");
    return true;
  },
  ask: async (researchId, parentReportId, question, requestId) => {
    const replay = targetRuns.get(`request-${requestId}`)?.[0];
    if (replay) return structuredClone(replay);
    requireReport(researchId, parentReportId);
    if ((targetRuns.get(researchId) ?? []).some(run => ACTIVE_TARGET.has(run.state.status))) throw Error("research_busy");
    const runId = `run-${++sequence}`;
    const reportId = `report-${sequence}`;
    const input: TargetInput = { target: find(researchId).target?.target ?? { kind: "app", path: "/preview" }, question, agentId: "agent-codex", model: null, language: "en", mode: "single", budget: null, tools: null };
    const run = targetRun(researchId, runId, input, "collecting", []);
    run.spec.kind = "evidence_followup";
    run.spec.parentReportId = parentReportId;
    run.spec.budget = { ...budget, maxCollections: 0 };
    run.state.usage = { ...usage, collections: 0, calls: 0 };
    const sourcePath = Object.keys(targetFiles.get(parentReportId) ?? { "src/main.rs": "" })[0];
    publishReport(reportId, runId, `# ${question}\n\nRead only the saved report. See [source](loom-src:${sourcePath}#L1-L1).\n`, { ...(targetFiles.get(parentReportId) ?? {}) }, targetEvidence.get(parentReportId) ?? []);
    targetRuns.get(researchId)?.push(run);
    targetRuns.set(`request-${requestId}`, [run]);
    notify(researchId, "collecting");
    finishLater(researchId, run, reportId);
    return structuredClone(run);
  },
  retry: async (researchId, runId, requestId) => {
    const replay = targetRuns.get(`request-${requestId}`)?.[0];
    if (replay) return structuredClone(replay);
    const run = targetRuns.get(researchId)?.find(item => item.spec.runId === runId);
    if (!run || (run.state.status !== "failed" && run.state.status !== "cancelled")) throw Error("run_not_retryable");
    run.state.status = "collecting";
    run.state.error = null;
    run.state.attempt += 1;
    const reportId = run.state.reportIds[0] ?? `report-${++sequence}`;
    if (!targetReports.has(reportId)) publishReport(reportId, runId, `# Retried\n\nSee [source](loom-src:index.js#L1-L1).\n`, { "index.js": "module.exports = { retried: true };\n" }, []);
    notify(researchId, "collecting");
    finishLater(researchId, run, reportId);
    targetRuns.set(`request-${requestId}`, [run]);
    return structuredClone(run);
  },
  resume: async (researchId, runId, requestId) => {
    const run = targetRuns.get(researchId)?.find(item => item.spec.runId === runId);
    if (!run || (run.state.status !== "interrupted" && run.state.status !== "awaiting_user")) throw Error("run_not_resumable");
    run.state.status = "collecting";
    notify(researchId, "collecting");
    finishLater(researchId, run, run.state.reportIds[0] ?? `report-${++sequence}`);
    targetRuns.set(`request-${requestId}`, [run]);
    return structuredClone(run);
  },
  report: async (researchId, reportId) => structuredClone(requireReport(researchId, reportId)),
  tree: async (researchId, reportId, offset = 0, limit = 200) => {
    requireReport(researchId, reportId);
    const paths = Object.keys(targetFiles.get(reportId) ?? {});
    const end = Math.min(paths.length, offset + limit);
    return { text: paths.slice(offset, end).join("\n"), nextCursor: end < paths.length ? end : null, truncated: end < paths.length };
  },
  source: async (researchId, reportId, path, startLine, endLine) => {
    requireReport(researchId, reportId);
    const text = targetFiles.get(reportId)?.[path];
    if (text === undefined) throw Error("snapshot_unavailable");
    const lines = text.replace(/\n$/, "").split("\n");
    return { text: `${lines.slice(startLine - 1, endLine).join("\n")}\n`, nextCursor: null, truncated: false };
  },
  evidence: async (researchId, reportId) => structuredClone(targetEvidence.get(requireReport(researchId, reportId).report.reportId) ?? []),
  readEvidence: async (researchId, reportId, evidenceId, offset = 0, limit = 12000) => {
    const content = requireReport(researchId, reportId);
    if (!content.report.evidenceIds.includes(evidenceId)) throw Error("report_evidence_scope_mismatch");
    const chars = [...(targetEvidenceText.get(evidenceId) ?? "")];
    const end = Math.min(chars.length, offset + limit);
    return { text: chars.slice(offset, end).join(""), nextCursor: end < chars.length ? end : null, truncated: end < chars.length };
  },
  export: async (researchId, reportId, _evidenceIds, includeRaw = false) => {
    requireReport(researchId, reportId);
    return { directory: `/preview/export/${reportId}`, files: includeRaw ? ["report.md", "evidence.json", "raw.bin"] : ["report.md", "evidence.json"], includesRaw: includeRaw,     warning: includeRaw ? "Raw evidence can still contain target data." : null };
  },
};
