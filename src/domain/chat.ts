/** Chat-first domain types (M0 sketch). ChatSession ≠ Task. */

export type ChatPermissionMode = "read_only" | "read_write";

export type ChatMessageRole = "user" | "assistant" | "system";

export type ChatMessageStatus = "complete" | "streaming" | "aborted" | "error";

export type ChatTurnStatus = "idle" | "streaming" | "error";

export interface ChatMessage {
  id: string;
  role: ChatMessageRole;
  content: string;
  status: ChatMessageStatus;
  createdAtMs: number;
  /** Populated when status is error */
  errorSummary?: string;
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
  schemaVersion: 1;
}

export interface ChatSessionSummary {
  id: string;
  title: string;
  agentId: string;
  updatedAtMs: number;
  preview?: string;
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

export const DEFAULT_CHAT_PERMISSION_MODE: ChatPermissionMode = "read_only";
