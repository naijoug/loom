import { TAURI_COMMANDS, TAURI_EVENTS } from "./contract";
import { invokeCommand, listenToEvent } from "./tauriClient";

export type LearnStageStatus = "pending" | "running" | "done" | "failed" | "cancelled" | "interrupted";
export type LearnLanguage = "en" | "zh-CN";
export type LearnDepth = "quick" | "standard" | "deep";

export interface LearnStageState {
  stage: string; status: LearnStageStatus; jobId?: string;
  startedAtMs?: number; finishedAtMs?: number; error?: string; warnings: string[];
}
export interface LearnRefStats { valid: number; invalid: number }
export interface LearnPageInfo {
  pageId: string; title: string; kind: "overview" | "architecture" | "module" | "glossary" | string;
  moduleId?: string; level?: number; ready: boolean; refs?: LearnRefStats;
}
export interface LearnCoverage { sourceFiles: number; covered: number; uncovered: string[] }
export interface LearnStudy {
  schemaVersion: number; studyId: string; url: string; owner: string; repo: string;
  gitRef?: string; commit?: string; agentId: string; agentName: string; adapterType: string;
  model?: string; language: LearnLanguage | string; depth: LearnDepth | string; promptVersion: number;
  createdAtMs: number; updatedAtMs: number; stages: LearnStageState[]; pages: LearnPageInfo[];
  coverage?: LearnCoverage; checkoutFingerprint?: string;
}
export interface LearnStudySummary {
  studyId: string; owner: string; repo: string; commit?: string; agentName: string; model?: string;
  status: "running" | "failed" | "ready" | "idle" | string; pagesReady: number; pagesTotal: number; updatedAtMs: number;
}
export interface LearnStudyList { studies: LearnStudySummary[]; storageErrors: string[] }
export interface LearnKeyFile { path: string; startLine: number; endLine: number; why: string }
export interface LearnModule {
  id: string; title: string; summary: string; level: number; order: number;
  paths: string[]; dependsOn: string[]; keyFiles: LearnKeyFile[];
}
export interface LearnArchitecture {
  schemaVersion: number;
  project: { name: string; summary: string; overview: string; techStack: string[]; entryPoints: { path: string; line: number; why: string }[] };
  layers: { id: string; title: string; moduleIds: string[] }[];
  modules: LearnModule[];
  learningPath: { level: number; title: string; goal: string; moduleIds: string[] }[];
}
export interface LearnStudyDetail { study: LearnStudy; architecture?: LearnArchitecture }
export interface LearnSourceRef { target: string; path: string; startLine: number; endLine: number; status: "valid" | "missing" | "range" | "outside" | string }
export interface LearnPageContent { pageId: string; markdown: string; refs: LearnSourceRef[] }
export interface LearnSourceText { path: string; text: string; firstLine: number; totalLines: number; truncated: boolean; language: string }
export interface LearnTreeEntry { name: string; path: string; kind: "dir" | "file" | "symlink" | string; size: number }
export interface LearnJobLogChunk { text: string; nextOffset: number; hasMore: boolean }
export interface LearnEvent { studyId: string; jobId?: string; stage: string; status: LearnStageStatus | "deleted" | string; error?: string }
export interface CreateStudyInput { url: string; gitRef?: string; agentId: string; model?: string; language: LearnLanguage; depth: LearnDepth }
export type LearnLogStream = "stdout" | "stderr" | "prompt";

/** Everything the Learn UI needs; the DEV fixture supplies an in-memory implementation. */
export interface LearnApi {
  list(): Promise<LearnStudyList>;
  create(input: CreateStudyInput): Promise<LearnStudyDetail>;
  get(studyId: string): Promise<LearnStudyDetail>;
  readPage(studyId: string, pageId: string): Promise<LearnPageContent>;
  generate(studyId: string, stage: string): Promise<string>;
  cancel(studyId: string): Promise<boolean>;
  readLogs(studyId: string, jobId: string, stream: LearnLogStream, offset?: number): Promise<LearnJobLogChunk>;
  listTree(studyId: string, dir?: string): Promise<LearnTreeEntry[]>;
  readSource(studyId: string, path: string, startLine?: number, endLine?: number): Promise<LearnSourceText>;
  remove(studyId: string): Promise<void>;
  subscribe(onEvent: (event: LearnEvent) => void): Promise<() => void>;
}

export const tauriLearnApi: LearnApi = {
  list: () => invokeCommand(TAURI_COMMANDS.learnListStudies),
  create: input => invokeCommand(TAURI_COMMANDS.learnCreateStudy, { input }),
  get: studyId => invokeCommand(TAURI_COMMANDS.learnGetStudy, { studyId }),
  readPage: (studyId, pageId) => invokeCommand(TAURI_COMMANDS.learnReadPage, { studyId, pageId }),
  generate: (studyId, stage) => invokeCommand(TAURI_COMMANDS.learnGenerate, { studyId, stage }),
  cancel: studyId => invokeCommand(TAURI_COMMANDS.learnCancelJob, { studyId }),
  readLogs: (studyId, jobId, stream, offset) => invokeCommand(TAURI_COMMANDS.learnReadJobLogs, { studyId, jobId, stream, offset }),
  listTree: (studyId, dir) => invokeCommand(TAURI_COMMANDS.learnListTree, { studyId, dir }),
  readSource: (studyId, path, startLine, endLine) => invokeCommand(TAURI_COMMANDS.learnReadSource, { studyId, path, startLine, endLine }),
  remove: studyId => invokeCommand(TAURI_COMMANDS.learnDeleteStudy, { studyId }),
  subscribe: onEvent => listenToEvent<LearnEvent>(TAURI_EVENTS.learnEvent, onEvent),
};

/** Built-in adapters accept `--model`; custom agents are not offered for Learn. */
export const LEARN_ADAPTERS = ["claude_code_cli", "codex_cli", "opencode_cli", "grok_cli"];

/** `loom-src:path#L3-L9` → path and range; mirrors `generate::parse_target`. */
export function parseSourceTarget(target: string): { path: string; startLine?: number; endLine?: number } {
  const [rawPath, anchor = ""] = target.split("#", 2);
  const path = decodeURIComponent(rawPath).replace(/^\.\//, "");
  const match = /^L(\d+)(?:-L?(\d+))?$/.exec(anchor);
  if (!match) return { path };
  const startLine = Number(match[1]);
  return { path, startLine, endLine: match[2] ? Math.max(startLine, Number(match[2])) : startLine };
}
