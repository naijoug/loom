/** Explicit DEV-only Research fixture (`?researchFixture=…`). No IPC, network, git or model calls. */
import { lastTurn, type Research, type ResearchApi, type ResearchEvent, type ResearchTurn } from "../../api/research";
import { parseSourceTarget, type TreeEntry } from "../../api/sourceFiles";

const scenario = new URLSearchParams(location.search).get("researchFixture") ?? "ready";
const files: Record<string, string> = {
  "src-tauri/src/agent/run.rs":
    Array.from({ length: 140 }, (_, i) =>
      i === 39
        ? "pub(crate) async fn run(spec: RunSpec, cancel: impl Fn() -> bool) -> AgentOutcome {"
        : i === 59
          ? "    let child = supervisor.spawn(&spec.prepared, spec.owner)?;"
          : i === 89
            ? "    if cancel() { supervisor.terminate(child, TerminationReason::Cancelled); }"
            : `// synthetic agent run line ${i + 1}`,
    ).join("\n") + "\n",
  "src-tauri/src/process_supervisor.rs":
    Array.from({ length: 80 }, (_, i) =>
      i === 19
        ? "pub fn terminate(&self, id: u32, reason: TerminationReason) {"
        : `// synthetic supervisor line ${i + 1}`,
    ).join("\n") + "\n",
  "README.md": "# Loom\n\nA local agent workbench.\n",
};
const reports: Record<string, string> = {
  "research-preview:1":
    "# How Loom runs and stops an agent CLI\n\n## Conclusion\n\nEvery feature hands a prepared invocation to `agent::run`, which spawns the CLI through the process supervisor and terminates the whole process group when the user cancels.\n\n## Evidence\n\n- `run` owns the lifecycle ([run.rs:40-60](loom-src:src-tauri/src/agent/run.rs#L40-L60)).\n- Cancellation is polled and forwarded to the supervisor ([run.rs:90](loom-src:src-tauri/src/agent/run.rs#L90-L90)).\n- The supervisor signals the group ([process_supervisor.rs:20-24](loom-src:src-tauri/src/process_supervisor.rs#L20-L24)).\n\n| Step | Source |\n|---|---|\n| Spawn | [run.rs:60](loom-src:src-tauri/src/agent/run.rs#L60-L60) |\n| A stale reference | [gone.rs](loom-src:src-tauri/src/gone.rs#L1-L2) |\n\n## References\n\n- [Tauri process docs](https://tauri.app)\n\n## Open questions\n\n- Whether a hung child that ignores SIGTERM is escalated to SIGKILL.\n",
  "research-preview:2":
    "# Limits on a run\n\n## Conclusion\n\nEach turn has a 20 minute timeout and a capped output buffer.\n\n## Evidence\n\n- Limits are passed in `RunSpec` ([run.rs:40](loom-src:src-tauri/src/agent/run.rs#L40-L40)).\n\n## Open questions\n\n- None.\n",
  "research-web:1":
    "# Rust async runtimes\n\n## Conclusion\n\nTokio is the common default for desktop apps built on Tauri.\n\n## References\n\n- [Tokio](https://tokio.rs)\n",
};
let sequence = 0;
const turn = (index: number, question: string, extra: Partial<ResearchTurn> = {}): ResearchTurn => ({
  index,
  question,
  status: "done",
  jobId: `job-preview-${index}`,
  startedAtMs: 1,
  finishedAtMs: 2,
  warnings: [],
  ...extra,
});
const research = (id: string, extra: Partial<Research> = {}): Research => ({
  schemaVersion: 1,
  researchId: id,
  title: "How does Loom start a local agent CLI and stop it when the user cancels?",
  agentId: "agent-codex",
  agentName: "Codex",
  adapterType: "codex_cli",
  model: "gpt-5",
  projectPath: "/preview/loom",
  projectName: "loom",
  language: "en",
  promptVersion: 1,
  createdAtMs: 1,
  updatedAtMs: 2,
  turns: [
    turn(1, "How does Loom start a local agent CLI and stop it when the user cancels?", {
      refs: { valid: 4, invalid: 1 },
    }),
    turn(2, "Which limits (timeout, output size) apply to that run, and where are they set?", {
      context: "native",
      refs: { valid: 1, invalid: 0 },
    }),
  ],
  ...extra,
});
const items: Research[] =
  scenario === "empty"
    ? []
    : [
        research("research-preview"),
        research("research-web", {
          title: "Which async runtime do Tauri apps use?",
          projectPath: undefined,
          projectName: undefined,
          agentId: "agent-claude",
          agentName: "Claude Code",
          adapterType: "claude_code_cli",
          model: undefined,
          updatedAtMs: 1,
          turns: [turn(1, "Which async runtime do Tauri apps use?")],
        }),
      ];
if (scenario === "running")
  items[0].turns.push(
    turn(3, "Is the process group killed on Windows too?", {
      status: "running",
      finishedAtMs: undefined,
      context: "native",
    }),
  );
if (scenario === "failed")
  items[0].turns.push(
    turn(3, "Is the process group killed on Windows too?", {
      status: "failed",
      context: "replay",
      error:
        "agent exited with status exit status: 1; stderr: Error: Missing optional dependency @openai/codex-darwin-arm64. Reinstall Codex: npm install -g @openai/codex@latest",
      warnings: ["Resuming the agent session failed (exit status 1); earlier questions were replayed instead"],
    }),
  );
const listeners = new Set<(event: ResearchEvent) => void>();
const emit = (event: ResearchEvent) => listeners.forEach((listener) => listener(event));
const find = (id: string) => {
  const found = items.find((item) => item.researchId === id);
  if (!found) throw Error("Research not found");
  return found;
};
function answer(target: Research, index: number) {
  emit({ researchId: target.researchId, jobId: `job-preview-${index}`, turn: index, status: "running" });
  setTimeout(() => {
    const current = target.turns.find((item) => item.index === index);
    if (current?.status !== "running") return;
    reports[`${target.researchId}:${index}`] ??=
      `# ${current.question}\n\n## Conclusion\n\nSynthetic answer for visual acceptance.${target.projectPath ? " See [run.rs:40-42](loom-src:src-tauri/src/agent/run.rs#L40-L42)." : ""}\n\n## Open questions\n\n- None.\n`;
    Object.assign(current, {
      status: "done",
      finishedAtMs: Date.now(),
      refs: target.projectPath ? { valid: 1, invalid: 0 } : { valid: 0, invalid: 0 },
    });
    target.updatedAtMs = Date.now();
    emit({ researchId: target.researchId, jobId: current.jobId, turn: index, status: "done" });
  }, 900);
}
const summary = (item: Research) => {
  const last = lastTurn(item);
  return {
    researchId: item.researchId,
    title: item.title,
    agentName: item.agentName,
    model: item.model,
    projectName: item.projectName,
    status: last?.status === "running" ? "running" : last && last.status !== "done" ? "failed" : "ready",
    turns: item.turns.length,
    updatedAtMs: item.updatedAtMs,
  };
};
function tree(dir: string): TreeEntry[] {
  const prefix = dir ? `${dir}/` : "";
  const names = new Map<string, TreeEntry>();
  for (const [path, text] of Object.entries(files)) {
    if (!path.startsWith(prefix)) continue;
    const [name, ...rest] = path.slice(prefix.length).split("/");
    names.set(name, {
      name,
      path: prefix + name,
      kind: rest.length ? "dir" : "file",
      size: rest.length ? 0 : text.length,
    });
  }
  return [...names.values()].sort(
    (a, b) => Number(a.kind !== "dir") - Number(b.kind !== "dir") || a.name.localeCompare(b.name),
  );
}

export const previewResearchApi: ResearchApi = {
  list: async () => {
    if (scenario === "error") throw Error("Synthetic storage error");
    return { items: [...items].sort((a, b) => b.updatedAtMs - a.updatedAtMs).map(summary), storageErrors: [] };
  },
  create: async (input) => {
    const project = input.projectPath ? input.projectPath.split("/").pop() : undefined;
    const created = research(`research-preview-${++sequence}`, {
      title: input.question.split("\n")[0].slice(0, 80),
      model: input.model,
      projectPath: input.projectPath,
      projectName: project,
      language: input.language,
      createdAtMs: Date.now(),
      updatedAtMs: Date.now(),
      turns: [turn(1, input.question, { status: "running", finishedAtMs: undefined })],
    });
    items.unshift(created);
    answer(created, 1);
    return structuredClone(created);
  },
  get: async (id) => structuredClone(find(id)),
  readTurn: async (id, index) => {
    const target = find(id);
    const markdown = reports[`${id}:${index}`];
    if (!markdown) throw Error("This question has no report yet");
    const refs = target.projectPath
      ? [...markdown.matchAll(/\]\(loom-src:([^)]+)\)/g)].map(([, ref]) => {
          const { path, startLine = 0, endLine = 0 } = parseSourceTarget(ref);
          const total = files[path]?.split("\n").length;
          return {
            target: ref,
            path,
            startLine,
            endLine,
            status: total === undefined ? "missing" : endLine < total ? "valid" : "range",
          };
        })
      : [];
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
  retry: async (id) => {
    const target = find(id);
    const last = lastTurn(target);
    if (!last || last.status === "done" || last.status === "running")
      throw Error("Only a failed or stopped question can be retried");
    Object.assign(
      last,
      turn(last.index, last.question, { status: "running", finishedAtMs: undefined, context: last.context }),
    );
    answer(target, last.index);
    return structuredClone(target);
  },
  cancel: async (id) => {
    const last = lastTurn(find(id));
    if (last?.status !== "running") return false;
    Object.assign(last, { status: "cancelled", finishedAtMs: Date.now() });
    emit({ researchId: id, jobId: last.jobId, turn: last.index, status: "cancelled" });
    return true;
  },
  readLogs: async (_id, _job, stream) => ({
    text:
      stream === "prompt"
        ? "You are researching a question for the user…\n(synthetic prompt)"
        : stream === "stderr"
          ? ""
          : '{"type":"thread.started","thread_id":"preview"}\n{"type":"item.completed"}\n',
    nextOffset: 0,
    hasMore: false,
  }),
  listTree: async (_id, dir = "") => tree(dir),
  readSource: async (_id, path) => {
    const text = files[path];
    if (text === undefined) throw Error("File not found");
    return {
      path,
      text,
      firstLine: 1,
      totalLines: text.split("\n").length - 1,
      truncated: false,
      language: path.endsWith(".rs") ? "Rust" : "Text",
    };
  },
  remove: async (id) => {
    items.splice(
      items.findIndex((item) => item.researchId === id),
      1,
    );
    emit({ researchId: id, status: "deleted" });
  },
  subscribe: async (onEvent) => {
    listeners.add(onEvent);
    return () => {
      listeners.delete(onEvent);
    };
  },
};
