import { TAURI_COMMANDS, TAURI_EVENTS } from "./contract";
import { invokeCommand, listenToEvent } from "./tauriClient";
import type { RefStats, SourceRef, SourceText, TreeEntry } from "./sourceFiles";

export type ResearchTurnStatus = "running" | "done" | "failed" | "cancelled" | "interrupted";
export type ResearchLanguage = "en" | "zh-CN";

export interface ResearchTurn {
  index: number;
  question: string;
  status: ResearchTurnStatus | string;
  jobId: string;
  startedAtMs: number;
  finishedAtMs?: number;
  error?: string;
  warnings: string[];
  refs?: RefStats;
  /** `native` (the agent resumed its session) or `replay` (Loom replayed earlier turns); absent on the first question. */
  context?: "native" | "replay" | string;
}
export interface Research {
  activeOwner?: { runId: string; workflow: string; leaseToken: string };
  workflow?: "legacy" | "target";
  target?: { target: import("./researchTargets").ResearchTarget; runIds: string[] };
  schemaVersion: number;
  researchId: string;
  title: string;
  agentId: string;
  agentName: string;
  adapterType: string;
  model?: string;
  projectPath?: string;
  projectName?: string;
  language: ResearchLanguage | string;
  promptVersion: number;
  createdAtMs: number;
  updatedAtMs: number;
  turns: ResearchTurn[];
  resumeCommand?: string;
  resumeAgentCommand?: string;
}
export interface ResearchSummary {
  activeOwner?: { runId: string; workflow: string; leaseToken: string };
  workflow?: "legacy" | "target";
  researchId: string;
  title: string;
  agentName: string;
  model?: string;
  projectName?: string;
  status: "running" | "failed" | "ready" | string;
  turns: number;
  updatedAtMs: number;
}
export interface ResearchList {
  items: ResearchSummary[];
  storageErrors: string[];
}
export interface CreateResearchInput {
  question: string;
  agentId: string;
  model?: string;
  projectPath?: string;
  language: ResearchLanguage;
}
export interface ResearchTurnContent {
  index: number;
  markdown: string;
  refs: SourceRef[];
}
export interface ResearchJobLogChunk {
  text: string;
  nextOffset: number;
  hasMore: boolean;
}
export interface ResearchEvent {
  researchId: string;
  jobId?: string;
  turn?: number;
  status: ResearchTurnStatus | "deleted" | string;
  error?: string;
}
export type ResearchLogStream = "stdout" | "stderr" | "prompt";

/** Everything the Research UI needs; the DEV fixture supplies an in-memory implementation. */
export interface ResearchApi {
  list(): Promise<ResearchList>;
  create(input: CreateResearchInput): Promise<Research>;
  get(researchId: string): Promise<Research>;
  readTurn(researchId: string, turn: number): Promise<ResearchTurnContent>;
  ask(researchId: string, question: string): Promise<Research>;
  retry(researchId: string): Promise<Research>;
  cancel(researchId: string): Promise<boolean>;
  readLogs(researchId: string, jobId: string, stream: ResearchLogStream, offset?: number): Promise<ResearchJobLogChunk>;
  listTree(researchId: string, dir?: string): Promise<TreeEntry[]>;
  readSource(researchId: string, path: string, startLine?: number, endLine?: number): Promise<SourceText>;
  remove(researchId: string): Promise<void>;
  subscribe(onEvent: (event: ResearchEvent) => void): Promise<() => void>;
}

export const tauriResearchApi: ResearchApi = {
  list: () => invokeCommand(TAURI_COMMANDS.researchList),
  create: (input) => invokeCommand(TAURI_COMMANDS.researchCreate, { input }),
  get: (researchId) => invokeCommand(TAURI_COMMANDS.researchGet, { researchId }),
  readTurn: (researchId, turn) => invokeCommand(TAURI_COMMANDS.researchReadTurn, { researchId, turn }),
  ask: (researchId, question) => invokeCommand(TAURI_COMMANDS.researchAsk, { researchId, question }),
  retry: (researchId) => invokeCommand(TAURI_COMMANDS.researchRetry, { researchId }),
  cancel: (researchId) => invokeCommand(TAURI_COMMANDS.researchCancel, { researchId }),
  readLogs: (researchId, jobId, stream, offset) =>
    invokeCommand(TAURI_COMMANDS.researchReadJobLogs, { researchId, jobId, stream, offset }),
  listTree: (researchId, dir) => invokeCommand(TAURI_COMMANDS.researchListTree, { researchId, dir }),
  readSource: (researchId, path, startLine, endLine) =>
    invokeCommand(TAURI_COMMANDS.researchReadSource, { researchId, path, startLine, endLine }),
  remove: (researchId) => invokeCommand(TAURI_COMMANDS.researchDelete, { researchId }),
  subscribe: (onEvent) => listenToEvent<ResearchEvent>(TAURI_EVENTS.researchEvent, onEvent),
};

/** Built-in adapters accept `--model` (backend `model_selection`); custom agents are not offered for Research. */
export const RESEARCH_ADAPTERS = ["claude_code_cli", "codex_cli", "opencode_cli", "grok_cli"];

export function lastTurn(research: Research): ResearchTurn | undefined {
  return research.turns[research.turns.length - 1];
}
