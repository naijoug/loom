/** Chat-first / Craft Chat domain types (Phase 1–2). ChatSession ≠ Task. */

/** Craft-inspired permission tiers (Phase 2 Ask = writable + confirm). */
export type ChatPermissionMode = "explore" | "ask" | "auto";

/** Legacy values persisted by chat-first M0–M5; normalize on read. */
export type LegacyChatPermissionMode = "read_only" | "read_write";

export type ChatPermissionModeInput = ChatPermissionMode | LegacyChatPermissionMode | string;

export type ChatSessionStatus = "active" | "archived";

/** Inbox filter tabs (status stays active|archived; needs_attention is derived/filter). */

export type ChatMessageRole = "user" | "assistant" | "system";

export type ChatMessageStatus = "complete" | "streaming" | "aborted" | "error";

export type ChatTurnStatus = "idle" | "streaming" | "error";

/**
 * Turn-visible fragments (M0 sketch; UI cards land M2+).
 * Keep `content` as the plain-text fallback for older sessions.
 */
export type ChatMessagePart =
  | { type: "text"; text: string }
  | {
      type: "tool";
      name: string;
      callId?: string;
      kind?: "read" | "edit" | "write" | "bash" | "search" | "web" | "other";
      target?: string;
      exitCode?: number;
      diffStats?: { added: number; removed: number };
      diffPreview?: string;
      inputSummary?: string;
      outputSummary?: string;
      status?: "running" | "done" | "error";
    }
  | { type: "error"; message: string; code?: string };

export interface ChatMessage {
  id: string;
  role: ChatMessageRole;
  content: string;
  status: ChatMessageStatus;
  createdAtMs: number;
  /** Populated when status is error (legacy + bubble summary). */
  errorSummary?: string;
  /** Optional structured parts; omit / empty on old sessions. */
  parts?: ChatMessagePart[];
}

export interface ChatSession {
  id: string;
  projectPath: string;
  agentId: string;
  title: string;
  permissionMode: ChatPermissionMode;
  messages: ChatMessage[];
  /** Durable acceptance receipts; absent in sessions predating request IDs. */
  sendReceipts?: ChatSendReceipt[];
  turns?: ChatTurn[];
  createdAtMs: number;
  updatedAtMs: number;
  /** Safe resume handle from adapter when available */
  resumeCommand?: string;
  resumeHandle?: ChatResumeHandle;
  activeTurnId?: string;
  turnStatus: ChatTurnStatus;
  /** Set only after promote-to-task stub */
  promotedTaskId?: string;
  /** Phase 1 inbox status; default active for old sessions. */
  status?: ChatSessionStatus;
  /** User flag — contributes to needs_attention. */
  flagged?: boolean;
  schemaVersion: 1 | 2;
  /** v2 durable event cursor; absent only in legacy/mock data. */
  lastSeq?: number;
  revision?: number;
}

export interface ChatResumeHandle {
  version: 1;
  adapterType: string;
  nativeSessionId: string;
  configFingerprint: string;
}

export type ChatRunStatus = "starting" | "running" | "cancelling" | "completed" | "failed" | "cancelled" | "timed_out" | "interrupted";
export interface ChatInvocationSnapshot {
  agentId: string; adapterType: string; program: string; args: string[]; cwd: string;
  permissionMode: ChatPermissionMode; stdinPrompt: boolean; outputMode: string; configFingerprint: string;
}
export interface ChatTurn {
  id: string; clientRequestId: string; userMessageId: string; assistantMessageId: string;
  invocation: ChatInvocationSnapshot; status: ChatRunStatus; acceptedAtMs: number;
  startedAtMs: number | null; finishedAtMs: number | null; processId: number | null;
  exitCode: number | null; terminationReason: string | null; errorSummary: string | null;
  stdoutLogRef: string; stderrLogRef: string;
}
export interface ChatLogPage { text: string; nextOffset: number; hasMore: boolean }

export interface ChatExportInput { projectPath: string; sessionId: string }
export interface ChatExportResult {
  directory: string; jsonPath: string; markdownPath: string; snapshotSeq: number;
  inProgress: boolean; warnings: string[];
}

export interface ChatStreamPayload {
  sessionId: string;
  turnId: string;
  messageId: string;
  delta: string;
  done: boolean;
  part?: ChatMessagePart;
}

export interface ChatSessionPatch {
  fields: Record<string, unknown>;
  changedMessages: ChatMessage[];
  appendedMessages: ChatMessage[];
}

export type ChatEvent = {
  schemaVersion: 2;
  projectKey: string;
  sessionId: string;
  seq: number;
  timestampMs: number;
} & (
  | { kind: "session_created"; payload: ChatSession }
  | { kind: "session_patch"; payload: ChatSessionPatch }
  | { kind: "stream"; payload: ChatStreamPayload }
);

export interface ChatEventPage {
  events: ChatEvent[];
  hasMore: boolean;
  lastSeq: number;
}

export interface ChatRuntimeError {
  projectKey: string;
  sessionId: string;
  turnId: string;
  messageId: string;
  status: "error";
  errorSummary: string;
}

export interface ChatSessionSummary {
  permissionMode?: ChatPermissionMode;
  turnStatus?: ChatTurnStatus;
  id: string;
  title: string;
  agentId: string;
  updatedAtMs: number;
  preview?: string;
  status?: ChatSessionStatus;
  /** True when flagged or last turn/message is error (active sessions). */
  needsAttention?: boolean;
  flagged?: boolean;
  storageError?: string;
}

export interface ChatSendInput {
  projectPath: string;
  sessionId: string;
  clientRequestId: string;
  text: string;
  permissionMode?: ChatPermissionMode;
}

export interface ChatSendReceipt {
  clientRequestId: string;
  turnId: string;
  userMessageId: string;
  assistantMessageId: string;
  requestedPermissionMode: string | null;
}

export const DEFAULT_CHAT_PERMISSION_MODE: ChatPermissionMode = "explore";

export const DEFAULT_CHAT_SESSION_STATUS: ChatSessionStatus = "active";

/**
 * Normalize disk / UI permission strings.
 * - read_only → explore
 * - read_write → ask (safer: old "writable" sessions load as Ask = writable + per-turn confirm)
 */
export function normalizeChatPermissionMode(
  value: ChatPermissionModeInput | null | undefined,
): ChatPermissionMode {
  switch (value) {
    case "explore":
    case "ask":
    case "auto":
      return value;
    case "read_only":
      return "explore";
    case "read_write":
      return "ask";
    default:
      return DEFAULT_CHAT_PERMISSION_MODE;
  }
}

/** Whether an active session should appear under the needs_attention filter. */
export function chatSessionNeedsAttention(session: {
  status?: ChatSessionStatus | string | null;
  flagged?: boolean | null;
  turnStatus?: ChatTurnStatus | string | null;
  messages?: Array<{ status?: ChatMessageStatus | string | null }>;
}): boolean {
  const status = session.status ?? DEFAULT_CHAT_SESSION_STATUS;
  if (status === "archived") return false;
  if (session.flagged) return true;
  if (session.turnStatus === "error") return true;
  return (session.messages ?? []).some((message) => message.status === "error");
}

/**
 * Title from first user text: first non-empty line, collapse whitespace,
 * truncate at a word boundary near maxLen (default 48) — not a blind slice.
 */
export function titleFromUserMessage(text: string, maxLen = 48): string {
  const firstLine =
    text
      .split(/\r?\n/)
      .map((line) => line.trim())
      .find((line) => line.length > 0) ?? text.trim();
  const collapsed = firstLine.replace(/\s+/g, " ").trim();
  if (!collapsed) return "新对话";
  if ([...collapsed].length <= maxLen) return collapsed;
  const chars = [...collapsed];
  let cut = maxLen;
  for (let i = maxLen; i >= Math.floor(maxLen * 0.5); i -= 1) {
    if (/\s/.test(chars[i] ?? "")) {
      cut = i;
      break;
    }
  }
  const sliced = chars.slice(0, cut).join("").trimEnd();
  return `${sliced}…`;
}

/**
 * Fallback text for failed messages whose structured parts do not already carry
 * an explicit error card. Keeps agent failure summaries visible beside tool
 * rows and partial stdout.
 */
export function chatMessageErrorFallbackText(message: {
  status?: ChatMessageStatus | string | null;
  content?: string | null;
  errorSummary?: string | null;
  parts?: Array<{ type?: string | null }> | null;
}): string | undefined {
  if (message.status !== "error" || !message.errorSummary) return undefined;
  if ((message.parts ?? []).some((part) => part.type === "error")) return undefined;
  return `（调用失败）${message.errorSummary}`;
}

export interface ChatProjectGroup {
  project: import("./project").ProjectSummary;
  sessionCount: number;
  sessions: ChatSessionSummary[];
  storageError?: string;
}
