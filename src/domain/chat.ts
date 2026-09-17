/** Chat-first / Craft Phase 1 domain types. ChatSession ≠ Task. */

/** Craft-inspired permission tiers (Phase 1). */
export type ChatPermissionMode = "explore" | "ask" | "auto";

/** Legacy values persisted by chat-first M0–M5; normalize on read. */
export type LegacyChatPermissionMode = "read_only" | "read_write";

export type ChatPermissionModeInput = ChatPermissionMode | LegacyChatPermissionMode | string;

export type ChatSessionStatus = "active" | "archived";

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
  schemaVersion: 1;
}

export interface ChatSessionSummary {
  id: string;
  title: string;
  agentId: string;
  updatedAtMs: number;
  preview?: string;
  status?: ChatSessionStatus;
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
 * - read_write → ask (safer: old "writable" sessions load as Ask / conservative CLI)
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

/** True when mode maps to write-capable adapter stage (auto only in Phase 1). */
export function chatPermissionAllowsWrite(mode: ChatPermissionModeInput): boolean {
  return normalizeChatPermissionMode(mode) === "auto";
}
