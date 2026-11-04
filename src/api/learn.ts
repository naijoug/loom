import { TAURI_COMMANDS, TAURI_EVENTS } from "./contract";
import { invokeCommand, listenToEvent } from "./tauriClient";
import type { RefStats, SourceRef, SourceText, TreeEntry } from "./sourceFiles";
export { parseSourceTarget } from "./sourceFiles";

export type LearnStageStatus = "pending" | "running" | "done" | "failed" | "cancelled" | "interrupted";
export type LearnLanguage = "en" | "zh-CN";
export type LearnDepth = "quick" | "standard" | "deep";

export interface LearnStageState {
  stage: string;
  status: LearnStageStatus;
  jobId?: string;
  startedAtMs?: number;
  finishedAtMs?: number;
  error?: string;
  warnings: string[];
}
export type LearnRefStats = RefStats;
export interface LearnPageInfo {
  pageId: string;
  title: string;
  kind: "overview" | "architecture" | "module" | "glossary" | string;
  moduleId?: string;
  level?: number;
  ready: boolean;
  refs?: LearnRefStats;
}
export interface LearnCoverage {
  sourceFiles: number;
  covered: number;
  uncovered: string[];
}
export interface LearnStudy {
  schemaVersion: number;
  studyId: string;
  url: string;
  owner: string;
  repo: string;
  gitRef?: string;
  commit?: string;
  agentId: string;
  agentName: string;
  adapterType: string;
  model?: string;
  language: LearnLanguage | string;
  depth: LearnDepth | string;
  promptVersion: number;
  createdAtMs: number;
  updatedAtMs: number;
  stages: LearnStageState[];
  pages: LearnPageInfo[];
  coverage?: LearnCoverage;
  checkoutFingerprint?: string;
}
export interface LearnStudySummary {
  studyId: string;
  owner: string;
  repo: string;
  commit?: string;
  agentName: string;
  model?: string;
  status: "running" | "failed" | "ready" | "idle" | string;
  pagesReady: number;
  pagesTotal: number;
  updatedAtMs: number;
}
export interface LearnStudyList {
  studies: LearnStudySummary[];
  storageErrors: string[];
}
export interface LearnKeyFile {
  path: string;
  startLine: number;
  endLine: number;
  why: string;
}
export interface LearnModule {
  id: string;
  title: string;
  summary: string;
  level: number;
  order: number;
  paths: string[];
  dependsOn: string[];
  keyFiles: LearnKeyFile[];
}
export interface LearnArchitecture {
  schemaVersion: number;
  project: {
    name: string;
    summary: string;
    overview: string;
    techStack: string[];
    entryPoints: { path: string; line: number; why: string }[];
  };
  layers: { id: string; title: string; moduleIds: string[] }[];
  modules: LearnModule[];
  learningPath: { level: number; title: string; goal: string; moduleIds: string[] }[];
}
export interface LearnStudyDetail {
  study: LearnStudy;
  architecture?: LearnArchitecture;
}
export type LearnSourceRef = SourceRef;
export interface LearnPageContent {
  pageId: string;
  markdown: string;
  refs: LearnSourceRef[];
}
export type LearnSourceText = SourceText;
export type LearnTreeEntry = TreeEntry;
export interface LearnJobLogChunk {
  text: string;
  nextOffset: number;
  hasMore: boolean;
}
export interface LearnEvent {
  studyId: string;
  jobId?: string;
  stage: string;
  status: LearnStageStatus | "deleted" | string;
  error?: string;
}
export interface CreateStudyInput {
  url: string;
  gitRef?: string;
  agentId: string;
  model?: string;
  language: LearnLanguage;
  depth: LearnDepth;
}
export type LearnLogStream = "stdout" | "stderr" | "prompt";

/** A GitHub repository found by search; `url` is always https://github.com/<owner>/<name>. */
export interface GithubRepoCard {
  fullName: string;
  owner: string;
  name: string;
  url: string;
  description?: string;
  language?: string;
  stars: number;
  forks: number;
  topics: string[];
  pushedAt?: string;
  createdAt?: string;
  fork: boolean;
  archived: boolean;
}
export type TrendingPeriod = "day" | "week" | "month";
export interface TrendingResult {
  repos: GithubRepoCard[];
  /** GitHub search query behind the list, shown so the ranking basis is visible. */
  query: string;
  fetchedAtMs: number;
  cached: boolean;
}
export interface DiscoverMessage {
  role: "user" | "assistant";
  text: string;
}
export interface DiscoverInput {
  runId: string;
  agentId: string;
  model?: string;
  language: LearnLanguage;
  messages: DiscoverMessage[];
}
export interface DiscoverReply {
  reply: string;
  repos: GithubRepoCard[];
  queries: string[];
  warnings: string[];
}

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
  trending(period: TrendingPeriod, language?: string, refresh?: boolean): Promise<TrendingResult>;
  discoverSend(input: DiscoverInput): Promise<DiscoverReply>;
  discoverCancel(runId: string): Promise<boolean>;
}

export const tauriLearnApi: LearnApi = {
  list: () => invokeCommand(TAURI_COMMANDS.learnListStudies),
  create: (input) => invokeCommand(TAURI_COMMANDS.learnCreateStudy, { input }),
  get: (studyId) => invokeCommand(TAURI_COMMANDS.learnGetStudy, { studyId }),
  readPage: (studyId, pageId) => invokeCommand(TAURI_COMMANDS.learnReadPage, { studyId, pageId }),
  generate: (studyId, stage) => invokeCommand(TAURI_COMMANDS.learnGenerate, { studyId, stage }),
  cancel: (studyId) => invokeCommand(TAURI_COMMANDS.learnCancelJob, { studyId }),
  readLogs: (studyId, jobId, stream, offset) =>
    invokeCommand(TAURI_COMMANDS.learnReadJobLogs, { studyId, jobId, stream, offset }),
  listTree: (studyId, dir) => invokeCommand(TAURI_COMMANDS.learnListTree, { studyId, dir }),
  readSource: (studyId, path, startLine, endLine) =>
    invokeCommand(TAURI_COMMANDS.learnReadSource, { studyId, path, startLine, endLine }),
  remove: (studyId) => invokeCommand(TAURI_COMMANDS.learnDeleteStudy, { studyId }),
  subscribe: (onEvent) => listenToEvent<LearnEvent>(TAURI_EVENTS.learnEvent, onEvent),
  trending: (period, language, refresh) =>
    invokeCommand(TAURI_COMMANDS.learnGithubTrending, { period, language, refresh }),
  discoverSend: (input) => invokeCommand(TAURI_COMMANDS.learnDiscoverSend, { input }),
  discoverCancel: (runId) => invokeCommand(TAURI_COMMANDS.learnDiscoverCancel, { runId }),
};

/** Built-in adapters accept `--model`; custom agents are not offered for Learn. */
export const LEARN_ADAPTERS = ["claude_code_cli", "codex_cli", "opencode_cli", "grok_cli"];

/** Discover chat accepts built-in CLIs and direct-API agents (it needs no tools). */
export const DISCOVER_ADAPTERS = [...LEARN_ADAPTERS, "api"];
