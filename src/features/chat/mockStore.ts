import type {
  ChatMessage,
  ChatPermissionMode,
  ChatSession,
  ChatSessionStatus,
  ChatSessionSummary,
} from "../../domain/chat";
import {
  DEFAULT_CHAT_PERMISSION_MODE,
  DEFAULT_CHAT_SESSION_STATUS,
  normalizeChatPermissionMode,
  titleFromUserMessage,
  chatSessionNeedsAttention,
} from "../../domain/chat";

function now() {
  return Date.now();
}

function id(prefix: string) {
  return `${prefix}-${Math.random().toString(36).slice(2, 10)}`;
}

/** In-memory mock for non-Tauri / browser preview. */
export class MockChatStore {
  private sessions = new Map<string, ChatSession>();

  list(projectPath: string): ChatSessionSummary[] {
    return [...this.sessions.values()]
      .filter((session) => session.projectPath === projectPath)
      .sort((a, b) => b.updatedAtMs - a.updatedAtMs)
      .map((session) => ({
        id: session.id,
        title: session.title,
        agentId: session.agentId,
        updatedAtMs: session.updatedAtMs,
        preview: session.messages.length
          ? session.messages[session.messages.length - 1].content.slice(0, 80)
          : undefined,
        status: session.status ?? DEFAULT_CHAT_SESSION_STATUS,
        flagged: Boolean(session.flagged),
        needsAttention: chatSessionNeedsAttention(session),
      }));
  }

  get(sessionId: string): ChatSession | undefined {
    return this.sessions.get(sessionId);
  }

  create(input: {
    projectPath: string;
    agentId: string;
    title?: string;
    permissionMode?: ChatPermissionMode;
  }): ChatSession {
    const createdAtMs = now();
    const session: ChatSession = {
      id: id("chat"),
      projectPath: input.projectPath,
      agentId: input.agentId,
      title: input.title?.trim() || "新对话",
      permissionMode: normalizeChatPermissionMode(
        input.permissionMode ?? DEFAULT_CHAT_PERMISSION_MODE,
      ),
      status: DEFAULT_CHAT_SESSION_STATUS,
      flagged: false,
      messages: [],
      createdAtMs,
      updatedAtMs: createdAtMs,
      turnStatus: "idle",
      schemaVersion: 1,
    };
    this.sessions.set(session.id, session);
    return session;
  }

  setAgent(sessionId: string, agentId: string): ChatSession | undefined {
    const session = this.sessions.get(sessionId);
    if (!session) return undefined;
    const next = { ...session, agentId, updatedAtMs: now() };
    this.sessions.set(sessionId, next);
    return next;
  }

  setPermissionMode(sessionId: string, permissionMode: ChatPermissionMode): ChatSession | undefined {
    const session = this.sessions.get(sessionId);
    if (!session) return undefined;
    const next = {
      ...session,
      permissionMode: normalizeChatPermissionMode(permissionMode),
      updatedAtMs: now(),
    };
    this.sessions.set(sessionId, next);
    return next;
  }

  setStatus(sessionId: string, status: ChatSessionStatus): ChatSession | undefined {
    const session = this.sessions.get(sessionId);
    if (!session) return undefined;
    const nextStatus: ChatSessionStatus = status === "archived" ? "archived" : "active";
    const next: ChatSession = {
      ...session,
      status: nextStatus,
      updatedAtMs: now(),
    };
    this.sessions.set(sessionId, next);
    return next;
  }

  setTitle(sessionId: string, title: string): ChatSession | undefined {
    const session = this.sessions.get(sessionId);
    if (!session) return undefined;
    const trimmed = title.trim();
    if (!trimmed) return session;
    const next = { ...session, title: trimmed, updatedAtMs: now() };
    this.sessions.set(sessionId, next);
    return next;
  }


  setFlagged(sessionId: string, flagged: boolean): ChatSession | undefined {
    const session = this.sessions.get(sessionId);
    if (!session) return undefined;
    const next: ChatSession = {
      ...session,
      flagged,
      updatedAtMs: now(),
    };
    this.sessions.set(sessionId, next);
    return next;
  }

  setTitleFromFirstMessage(sessionId: string): ChatSession | undefined {
    const session = this.sessions.get(sessionId);
    if (!session) return undefined;
    const firstUser = session.messages.find((message) => message.role === "user");
    if (!firstUser) return session;
    const next = {
      ...session,
      title: titleFromUserMessage(firstUser.content),
      updatedAtMs: now(),
    };
    this.sessions.set(sessionId, next);
    return next;
  }

  /** Mock send: append user + fake assistant reply (no CLI). */
  send(sessionId: string, text: string): ChatSession | undefined {
    const session = this.sessions.get(sessionId);
    if (!session) return undefined;
    const trimmed = text.trim();
    if (!trimmed) return session;
    const t = now();
    const user: ChatMessage = {
      id: id("msg"),
      role: "user",
      content: trimmed,
      status: "complete",
      createdAtMs: t,
    };
    const assistant: ChatMessage = {
      id: id("msg"),
      role: "assistant",
      content:
        "（M1 壳层模拟回复）真实 Codex / Claude / Grok 流式调用会在 M2+ 接通。你刚才说：\n\n" +
        trimmed,
      status: "complete",
      createdAtMs: t + 1,
    };
    const title =
      session.messages.length === 0 && session.title === "新对话"
        ? titleFromUserMessage(trimmed)
        : session.title;
    const next: ChatSession = {
      ...session,
      title,
      messages: [...session.messages, user, assistant],
      updatedAtMs: t + 1,
      turnStatus: "idle",
    };
    this.sessions.set(sessionId, next);
    return next;
  }
}

export const mockChatStore = new MockChatStore();
