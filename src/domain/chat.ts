/** Chat-first / Craft Chat domain types (Phase 1–2). ChatSession ≠ Task. */

/** Craft-inspired permission tiers (Phase 2 Ask = writable + confirm). */
export type ChatPermissionMode = "explore" | "ask" | "auto";

/** Legacy values persisted by chat-first M0–M5; normalize on read. */
export type LegacyChatPermissionMode = "read_only" | "read_write";

export type ChatPermissionModeInput = ChatPermissionMode | LegacyChatPermissionMode | string;

export type ChatSessionStatus = "active" | "archived";

/** Inbox filter tabs (status stays active|archived; needs_attention is derived/filter). */
export type ChatInboxFilter = ChatSessionStatus | "needs_attention";

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
  createdAtMs: number;
  updatedAtMs: number;
  /** Safe resume handle from adapter when available */
  resumeCommand?: string;
  activeTurnId?: string;
  turnStatus: ChatTurnStatus;
  /** Set only after promote-to-task stub */
  promotedTaskId?: string;
  /** Phase 1 inbox status; default active for old sessions. */
  status?: ChatSessionStatus;
  /** User flag — contributes to needs_attention. */
  flagged?: boolean;
  schemaVersion: 1;
}

export interface ChatSessionSummary {
  id: string;
  title: string;
  agentId: string;
  updatedAtMs: number;
  preview?: string;
  status?: ChatSessionStatus;
  /** True when flagged or last turn/message is error (active sessions). */
  needsAttention?: boolean;
  flagged?: boolean;
}

export interface ChatCreateInput {
  projectPath: string;
  agentId: string;
  title?: string;
  permissionMode?: ChatPermissionMode;
}

export interface ChatSendInput {
  projectPath: string;
  sessionId: string;
  text: string;
  permissionMode?: ChatPermissionMode;
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

/**
 * True when mode maps to write-capable adapter stage.
 * Phase 2: ask and auto both use Debugging / acceptEdits; ask still requires
 * a per-turn UI confirm before send (see chatPermission.askTurnRequiresConfirm).
 */
export function chatPermissionAllowsWrite(mode: ChatPermissionModeInput): boolean {
  const normalized = normalizeChatPermissionMode(mode);
  return normalized === "ask" || normalized === "auto";
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

/** Filter inbox summaries for a tab (active includes needs_attention items). */
export function filterChatSummaries<T extends {
  status?: ChatSessionStatus | string | null;
  needsAttention?: boolean | null;
  flagged?: boolean | null;
}>(summaries: T[], filter: ChatInboxFilter): T[] {
  if (filter === "archived") {
    return summaries.filter((item) => (item.status ?? "active") === "archived");
  }
  if (filter === "needs_attention") {
    return summaries.filter(
      (item) =>
        (item.status ?? "active") === "active" &&
        Boolean(item.needsAttention || item.flagged),
    );
  }
  return summaries.filter((item) => (item.status ?? "active") === "active");
}

