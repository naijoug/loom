import type {
  ChatPermissionMode,
  ChatEventPage,
  ChatSession,
  ChatSessionSummary,
  Task,
} from "../domain";
import { TAURI_COMMANDS } from "./contract";
import { invokeCommand } from "./tauriClient";

/**
 * Typed Chat IPC wrappers.
 *
 * Tauri commands that take a single `input: ...` argument MUST be invoked as
 * `{ input: { ...fields } }`. Flat top-level fields fail deserialization before
 * any Chat business logic runs (create / update_meta / send / abort / promote).
 *
 * Flat-arg commands (list / get / set_agent / clear_resume) stay flat.
 */

export type ChatCreateArgs = {
  projectPath: string;
  agentId: string;
  title?: string;
  permissionMode?: ChatPermissionMode;
};

export type ChatUpdateMetaArgs = {
  projectPath: string;
  sessionId: string;
  title?: string;
  permissionMode?: ChatPermissionMode;
  status?: "active" | "archived";
  flagged?: boolean;
  titleFromFirstMessage?: boolean;
};

export type ChatSendArgs = {
  projectPath: string;
  sessionId: string;
  clientRequestId: string;
  text: string;
  permissionMode?: ChatPermissionMode;
};

export type ChatAbortArgs = {
  projectPath: string;
  sessionId: string;
  turnId: string;
};

export type ChatPromoteArgs = {
  projectPath: string;
  sessionId: string;
};

export type ChatSendResult = {
  turnId: string;
  session: ChatSession;
};

export type ChatPromoteResult = {
  taskId: string;
  task: Task;
  session: ChatSession;
};

/** Test/helper: build the exact invoke payload for input-envelope commands. */
export function chatInputEnvelope<T extends Record<string, unknown>>(fields: T): { input: T } {
  return { input: fields };
}

export function chatListSessions(projectPath: string): Promise<ChatSessionSummary[]> {
  return invokeCommand<ChatSessionSummary[]>(TAURI_COMMANDS.chatListSessions, {
    projectPath,
  });
}

export function chatCreate(args: ChatCreateArgs): Promise<ChatSession> {
  return invokeCommand<ChatSession>(
    TAURI_COMMANDS.chatCreate,
    chatInputEnvelope({
      projectPath: args.projectPath,
      agentId: args.agentId,
      title: args.title,
      permissionMode: args.permissionMode,
    }),
  );
}

export function chatGet(projectPath: string, sessionId: string): Promise<ChatSession> {
  return invokeCommand<ChatSession>(TAURI_COMMANDS.chatGet, {
    projectPath,
    sessionId,
  });
}

export function chatReadEvents(projectPath: string, sessionId: string, afterSeq: number, limit = 200): Promise<ChatEventPage> {
  return invokeCommand<ChatEventPage>(TAURI_COMMANDS.chatReadEvents, { projectPath, sessionId, afterSeq, limit });
}

export function chatReadRunLogs(projectPath: string, sessionId: string, turnId: string, stream: "stdout" | "stderr", offset = 0, limit = 32_768): Promise<import("../domain/chat").ChatLogPage> {
  return invokeCommand(TAURI_COMMANDS.chatReadRunLogs, { projectPath, sessionId, turnId, stream, offset, limit });
}

export function chatExport(args: import("../domain/chat").ChatExportInput): Promise<import("../domain/chat").ChatExportResult> {
  return invokeCommand(TAURI_COMMANDS.chatExport, chatInputEnvelope({ projectPath: args.projectPath, sessionId: args.sessionId }));
}

export function chatSetAgent(
  projectPath: string,
  sessionId: string,
  agentId: string,
): Promise<ChatSession> {
  return invokeCommand<ChatSession>(TAURI_COMMANDS.chatSetAgent, {
    projectPath,
    sessionId,
    agentId,
  });
}

export function chatUpdateMeta(args: ChatUpdateMetaArgs): Promise<ChatSession> {
  return invokeCommand<ChatSession>(
    TAURI_COMMANDS.chatUpdateMeta,
    chatInputEnvelope({
      projectPath: args.projectPath,
      sessionId: args.sessionId,
      title: args.title,
      permissionMode: args.permissionMode,
      status: args.status,
      flagged: args.flagged,
      titleFromFirstMessage: args.titleFromFirstMessage,
    }),
  );
}

export function chatClearResume(projectPath: string, sessionId: string): Promise<ChatSession> {
  return invokeCommand<ChatSession>(TAURI_COMMANDS.chatClearResume, {
    projectPath,
    sessionId,
  });
}

export function chatSend(args: ChatSendArgs): Promise<ChatSendResult> {
  return invokeCommand<ChatSendResult>(
    TAURI_COMMANDS.chatSend,
    chatInputEnvelope({
      projectPath: args.projectPath,
      sessionId: args.sessionId,
      clientRequestId: args.clientRequestId,
      text: args.text,
      permissionMode: args.permissionMode,
    }),
  );
}

export function chatAbort(args: ChatAbortArgs): Promise<void> {
  return invokeCommand<void>(
    TAURI_COMMANDS.chatAbort,
    chatInputEnvelope({
      projectPath: args.projectPath,
      sessionId: args.sessionId,
      turnId: args.turnId ?? undefined,
    }),
  );
}

export function chatPromoteToTask(args: ChatPromoteArgs): Promise<ChatPromoteResult> {
  return invokeCommand<ChatPromoteResult>(
    TAURI_COMMANDS.chatPromoteToTask,
    chatInputEnvelope({
      projectPath: args.projectPath,
      sessionId: args.sessionId,
    }),
  );
}
