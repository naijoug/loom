import { TAURI_COMMANDS, TAURI_EVENTS } from "./contract";
import { invokeCommand, listenToEvent } from "./tauriClient";
import type { RefStats, SourceRef, SourceText, TreeEntry } from "./sourceFiles";
export type DiscussStatus = "pending" | "running" | "done" | "failed" | "cancelled" | "interrupted";
export interface DiscussParticipant { id: string; agentId: string; name: string; adapterType: string; model: string | null }
export interface DiscussTask { id: string; participantId: string; round: number; status: DiscussStatus; stale: boolean; attempts: string[]; error: string | null; warnings: string[]; refs: RefStats | null }
export interface Discussion { schemaVersion: number; discussionId: string; title: string; topic: string; language: string; projectPath: string | null; projectName: string | null; participants: DiscussParticipant[]; tasks: DiscussTask[]; status: string; activeRunId: string | null; createdAtMs: number; updatedAtMs: number; promptVersion: number }
export interface DiscussionSummary { discussionId: string; title: string; status: string; participants: number; projectName: string | null; updatedAtMs: number }
export interface CreateDiscussionInput { topic: string; participants: { agentId: string; model?: string }[]; projectPath?: string; language: "en" | "zh-CN"; confirmedCalls: number }
export interface DiscussReport { taskId: string; markdown: string; refs: SourceRef[] }
export interface DiscussEvent { discussionId: string; taskId: string | null; status: string }
export type DiscussLogStream = "stdout" | "stderr" | "prompt" | "record";
export interface DiscussLogChunk { text: string; nextOffset: number; hasMore: boolean }
export interface DiscussApi {
  list(): Promise<{ items: DiscussionSummary[]; storageErrors: string[] }>;
  create(input: CreateDiscussionInput): Promise<Discussion>;
  get(discussionId: string): Promise<Discussion>;
  readReport(discussionId: string, taskId: string): Promise<DiscussReport>;
  retry(discussionId: string, taskId: string, confirmedCalls: number): Promise<Discussion>;
  cancel(discussionId: string): Promise<boolean>;
  readLogs(discussionId: string, jobId: string, stream: DiscussLogStream, offset?: number): Promise<DiscussLogChunk>;
  listTree(discussionId: string, dir?: string): Promise<TreeEntry[]>;
  readSource(discussionId: string, path: string, startLine?: number, endLine?: number): Promise<SourceText>;
  remove(discussionId: string): Promise<void>;
  subscribe(onEvent: (event: DiscussEvent) => void): Promise<() => void>;
}
export const tauriDiscussApi: DiscussApi = {
  list: () => invokeCommand(TAURI_COMMANDS.discussList),
  create: input => invokeCommand(TAURI_COMMANDS.discussCreate, { input }),
  get: discussionId => invokeCommand(TAURI_COMMANDS.discussGet, { discussionId }),
  readReport: (discussionId, taskId) => invokeCommand(TAURI_COMMANDS.discussReadReport, { discussionId, taskId }),
  retry: (discussionId, taskId, confirmedCalls) => invokeCommand(TAURI_COMMANDS.discussRetry, { discussionId, taskId, confirmedCalls }),
  cancel: discussionId => invokeCommand(TAURI_COMMANDS.discussCancel, { discussionId }),
  readLogs: (discussionId, jobId, stream, offset) => invokeCommand(TAURI_COMMANDS.discussReadJobLogs, { discussionId, jobId, stream, offset }),
  listTree: (discussionId, dir) => invokeCommand(TAURI_COMMANDS.discussListTree, { discussionId, dir }),
  readSource: (discussionId, path, startLine, endLine) => invokeCommand(TAURI_COMMANDS.discussReadSource, { discussionId, path, startLine, endLine }),
  remove: discussionId => invokeCommand(TAURI_COMMANDS.discussDelete, { discussionId }),
  subscribe: fn => listenToEvent<DiscussEvent>(TAURI_EVENTS.discussEvent, fn),
};
export const DISCUSS_ADAPTERS = ["claude_code_cli", "codex_cli", "opencode_cli", "grok_cli"];
export const retryable = (task: DiscussTask) => task.status !== "running" && (task.status !== "done" || task.stale);
