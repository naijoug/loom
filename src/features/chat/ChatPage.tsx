import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type {
  AgentConfig,
  AgentDiagnostic,
  ChatMessagePart,
  ChatPermissionMode,
  ChatSession,
  ChatSessionSummary,
} from "../../domain";
import { useAppState } from "../../state/AppStateContext";
import { useAgentBridge } from "../../hooks/useAgentBridge";
import { hasTauriRuntime } from "../../hooks/runtime";
import { listenToEvent, TAURI_EVENTS } from "../../api";
import {
  chatAbort,
  chatClearResume,
  chatCreate,
  chatGet,
  chatListSessions,
  chatPromoteToTask,
  chatSend,
  chatSetAgent,
  chatUpdateMeta,
} from "../../api/chatClient";
import { mockChatStore } from "./mockStore";
import { ChatInbox, type InboxFilter } from "./ChatInbox";
import { ChatTranscript } from "./ChatTranscript";
import { ChatComposer } from "./ChatComposer";
import { ChatSessionHeader } from "./ChatSessionHeader";
import { ChatContextPanel } from "./ChatContextPanel";
import { cyclePermissionMode } from "./chatPermission";
import {
  CHAT_TURN_TIMEOUT_MS,
  isChatTurnRunning,
  turnRunningHint,
} from "./chatTurn";
import {
  chatStoreDraftFor,
  chatStoreSelectProject,
  chatStoreSetDraft,
  createChatStoreSnapshot,
  type ChatStoreSnapshot,
} from "./state/chatStore";
import { AddProjectModal } from "../../components/Sidebar/AddProjectModal";
import { Button } from "../../components/common/Button";
import "./ChatPage.css";

interface ChatStreamEvent {
  sessionId: string;
  turnId: string;
  messageId: string;
  delta: string;
  done: boolean;
  part?: ChatMessagePart;
}

interface ChatTurnFinishedEvent {
  sessionId: string;
  turnId: string;
  messageId: string;
  status: string;
  errorSummary?: string;
}


function enabledAgents(agents: AgentConfig[]) {
  return agents.filter((agent) => agent.enabled && agent.adapterType !== "dummy");
}

/** Dogfood preference: grok → codex → claude → first enabled. */
function preferredAgentId(agents: AgentConfig[]): string | undefined {
  const enabled = enabledAgents(agents);
  const byId = (id: string) => enabled.find((agent) => agent.id === id);
  return (
    byId("agent-grok")?.id ??
    enabled.find((agent) => agent.adapterType === "grok_cli")?.id ??
    byId("agent-codex")?.id ??
    byId("agent-claude")?.id ??
    enabled[0]?.id
  );
}

async function updateSessionMeta(input: {
  useBackend: boolean;
  projectPath: string;
  sessionId: string;
  title?: string;
  permissionMode?: ChatPermissionMode;
  status?: "active" | "archived";
  flagged?: boolean;
  titleFromFirstMessage?: boolean;
}): Promise<ChatSession | null> {
  if (!input.useBackend) {
    if (input.permissionMode) {
      mockChatStore.setPermissionMode(input.sessionId, input.permissionMode);
    }
    if (input.status) {
      mockChatStore.setStatus(input.sessionId, input.status);
    }
    if (input.title !== undefined) {
      mockChatStore.setTitle(input.sessionId, input.title);
    }
    if (input.flagged !== undefined) {
      mockChatStore.setFlagged(input.sessionId, input.flagged);
    }
    if (input.titleFromFirstMessage) {
      mockChatStore.setTitleFromFirstMessage(input.sessionId);
    }
    return mockChatStore.get(input.sessionId) ?? null;
  }
  return chatUpdateMeta({
    projectPath: input.projectPath,
    sessionId: input.sessionId,
    title: input.title,
    permissionMode: input.permissionMode,
    status: input.status,
    flagged: input.flagged,
    titleFromFirstMessage: input.titleFromFirstMessage,
  });
}

export function ChatPage() {
  const { state, dispatch } = useAppState();
  const { loadAgents, diagnoseAgents } = useAgentBridge();
  const projectPath = state.projects.current?.path ?? null;
  const agents = useMemo(() => enabledAgents(state.agents), [state.agents]);
  const agentNameById = useMemo(() => {
    const map: Record<string, string> = {};
    for (const agent of state.agents) {
      map[agent.id] = agent.name;
    }
    return map;
  }, [state.agents]);
  const useBackend = hasTauriRuntime();

  const [sessionId, setSessionId] = useState<string | null>(null);
  const [session, setSession] = useState<ChatSession | null>(null);
  const [summaries, setSummaries] = useState<ChatSessionSummary[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [sending, setSending] = useState(false);
  const [turnStartedAtMs, setTurnStartedAtMs] = useState<number | null>(null);
  const [elapsedMs, setElapsedMs] = useState(0);
  const turnTimeoutIdRef = useRef<number | null>(null);
  const clearTurnTimeout = useCallback(() => {
    if (turnTimeoutIdRef.current != null) {
      window.clearTimeout(turnTimeoutIdRef.current);
      turnTimeoutIdRef.current = null;
    }
  }, []);
  const [inboxFilter, setInboxFilter] = useState<InboxFilter>("active");
  const [inboxSearch, setInboxSearch] = useState("");
  const [addProjectOpen, setAddProjectOpen] = useState(false);
  const [chatStore, setChatStore] = useState<ChatStoreSnapshot>(() =>
    createChatStoreSnapshot(projectPath),
  );

  useEffect(() => {
    setChatStore((prev) => chatStoreSelectProject(prev, projectPath));
  }, [projectPath]);
  const draft = chatStoreDraftFor(chatStore, sessionId);
  const setDraft = (value: string) => {
    if (!sessionId) return;
    setChatStore((prev) => chatStoreSetDraft(prev, sessionId, value));
  };


  const [contextOpen, setContextOpen] = useState(false);

  const [diagnostics, setDiagnostics] = useState<AgentDiagnostic[]>([]);
  const [diagnosticsLoading, setDiagnosticsLoading] = useState(false);

  const refreshDiagnostics = useCallback(async () => {
    if (!useBackend) {
      setDiagnostics([]);
      return;
    }
    setDiagnosticsLoading(true);
    try {
      const next = await diagnoseAgents();
      setDiagnostics(next);
    } catch (err) {
      setError(err instanceof Error ? err.message : `诊断失败：${String(err)}`);
    } finally {
      setDiagnosticsLoading(false);
    }
  }, [useBackend, diagnoseAgents]);


  const refreshSummaries = useCallback(async () => {
    if (!projectPath) {
      setSummaries([]);
      return;
    }
    if (!useBackend) {
      setSummaries(mockChatStore.list(projectPath));
      return;
    }
    const listed = await chatListSessions(projectPath);
    setSummaries(listed);
  }, [projectPath, useBackend]);

  const loadSession = useCallback(
    async (id: string) => {
      if (!projectPath) return;
      if (!useBackend) {
        setSession(mockChatStore.get(id) ?? null);
        return;
      }
      const next = await chatGet(projectPath, id);
      setSession(next);
    },
    [projectPath, useBackend],
  );

  useEffect(() => {
    void loadAgents();
    void refreshDiagnostics();
  }, [loadAgents, refreshDiagnostics]);


  useEffect(() => () => clearTurnTimeout(), [clearTurnTimeout]);

  const turnRunning = isChatTurnRunning({
    sending,
    turnStatus: session?.turnStatus,
  });

  useEffect(() => {
    if (!turnRunning || turnStartedAtMs == null) {
      setElapsedMs(0);
      return;
    }
    const tick = () => setElapsedMs(Date.now() - turnStartedAtMs);
    tick();
    const id = window.setInterval(tick, 1000);
    return () => window.clearInterval(id);
  }, [turnRunning, turnStartedAtMs]);

  useEffect(() => {
    void refreshSummaries();
  }, [refreshSummaries]);

  useEffect(() => {
    if (!sessionId) {
      setSession(null);
      return;
    }
    void loadSession(sessionId).catch((err: unknown) => {
      setError(err instanceof Error ? err.message : String(err));
    });
  }, [sessionId, loadSession]);

  useEffect(() => {
    if (!useBackend) return;
    let disposed = false;
    const unsubscribers: Array<() => void> = [];

    void (async () => {
      const unlistenStream = await listenToEvent<ChatStreamEvent>(TAURI_EVENTS.chatStream, (payload) => {
        if (disposed) return;
        setSession((current) => {
          if (!current || current.id !== payload.sessionId) return current;
          return {
            ...current,
            messages: current.messages.map((message) => {
              if (message.id !== payload.messageId) return message;
              const nextParts = [...(message.parts ?? [])];
              if (payload.part) {
                nextParts.push(payload.part);
              }
              return {
                ...message,
                content: payload.done
                  ? message.content
                  : payload.delta
                    ? `${message.content}${payload.delta}`
                    : message.content,
                parts: nextParts.length > 0 ? nextParts : message.parts,
                status: payload.done ? message.status : "streaming",
              };
            }),
            turnStatus: payload.done ? current.turnStatus : "streaming",
          };
        });
      });
      const unlistenFinished = await listenToEvent<ChatTurnFinishedEvent>(
        TAURI_EVENTS.chatTurnFinished,
        (payload) => {
          if (disposed) return;
          setSending(false);
          clearTurnTimeout();
          setTurnStartedAtMs(null);
          setSession((current) => {
            if (!current || current.id !== payload.sessionId) return current;
            return {
              ...current,
              turnStatus: "idle",
              activeTurnId: undefined,
              messages: current.messages.map((message) =>
                message.id === payload.messageId
                  ? {
                      ...message,
                      status:
                        payload.status === "error"
                          ? "error"
                          : payload.status === "aborted"
                            ? "aborted"
                            : "complete",
                      errorSummary: payload.errorSummary,
                      content:
                        payload.status === "error" && !message.content
                          ? `（调用失败）${payload.errorSummary ?? "unknown error"}`
                          : message.content,
                    }
                  : message,
              ),
            };
          });
          void loadSession(payload.sessionId).catch(() => undefined);
          void refreshSummaries();
        },
      );
      if (disposed) {
        unlistenStream();
        unlistenFinished();
        return;
      }
      unsubscribers.push(unlistenStream, unlistenFinished);
    })();

    return () => {
      disposed = true;
      for (const unsubscribe of unsubscribers) unsubscribe();
    };
  }, [useBackend, refreshSummaries, loadSession]);

  async function handleCreate() {
    if (!projectPath) {
      setError("请先在侧栏选择或添加一个项目。");
      return;
    }
    const agentId = preferredAgentId(agents);
    if (!agentId) {
      setError("还没有可用的 Agent。请先到设置里配置 Codex / Claude Code / CLI。");
      return;
    }
    setError(null);
    setInboxFilter("active");
    if (!useBackend) {
      const created = mockChatStore.create({ projectPath, agentId });
      setSessionId(created.id);
      setSession(created);
      setDraft("");
      await refreshSummaries();
      return;
    }
    const created = await chatCreate({ projectPath, agentId });
    setSessionId(created.id);
    setSession(created);
    setDraft("");
    await refreshSummaries();
  }

  async function handleSend() {
    if (!session || !projectPath || !draft.trim() || sending) return;
    setError(null);
    setSending(true);
    try {
      if (!useBackend) {
        const next = mockChatStore.send(session.id, draft);
        setSession(next ?? null);
        setDraft("");
        setSending(false);
        await refreshSummaries();
        return;
      }
      const result = await chatSend({
        projectPath,
        sessionId: session.id,
        text: draft,
        permissionMode: session.permissionMode,
      });
      setDraft("");
      setSession(result.session);
      await refreshSummaries();
      setTurnStartedAtMs(Date.now());
      clearTurnTimeout();
      // Rust enforces CHAT_TURN_TIMEOUT_MS via ProcessSupervisor; UI mirrors it and
      // aborts through the same chat_abort / request_stop path if the finished event is late.
      turnTimeoutIdRef.current = window.setTimeout(() => {
        void chatAbort({
          projectPath,
          sessionId: session.id,
          turnId: session.activeTurnId,
        }).catch(() => undefined);
      }, CHAT_TURN_TIMEOUT_MS);
    } catch (err) {
      setSending(false);
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  async function handleAbort() {
    if (!session || !projectPath || !useBackend) return;
    await chatAbort({
          projectPath,
          sessionId: session.id,
          turnId: session.activeTurnId,
        });
    setSending(false);
    clearTurnTimeout();
          setTurnStartedAtMs(null);
  }

  async function handlePermissionChange(mode: ChatPermissionMode) {
    if (!session || !projectPath) return;
    try {
      const next = await updateSessionMeta({
        useBackend,
        projectPath,
        sessionId: session.id,
        permissionMode: mode,
      });
      setSession(next);
      await refreshSummaries();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  async function handleCyclePermission() {
    if (!session || sending) return;
    await handlePermissionChange(cyclePermissionMode(session.permissionMode));
  }

  async function handleAgentChange(agentId: string) {
    if (!session || !projectPath) return;
    try {
      if (!useBackend) {
        const next = mockChatStore.setAgent(session.id, agentId);
        setSession(next ?? null);
        await refreshSummaries();
        return;
      }
      const next = await chatSetAgent(projectPath, session.id, agentId);
      setSession(next);
      await refreshSummaries();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  async function handleArchive(id: string) {
    if (!projectPath) return;
    try {
      await updateSessionMeta({
        useBackend,
        projectPath,
        sessionId: id,
        status: "archived",
      });
      if (sessionId === id) {
        setSessionId(null);
        setSession(null);
      }
      await refreshSummaries();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  
  async function handleRestore(id: string) {
    if (!projectPath) return;
    try {
      await updateSessionMeta({
        useBackend,
        projectPath,
        sessionId: id,
        status: "active",
      });
      setInboxFilter("active");
      setSessionId(id);
      await refreshSummaries();
      await loadSession(id);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  async function handleRename(title: string) {
    if (!session || !projectPath) return;
    try {
      const next = await updateSessionMeta({
        useBackend,
        projectPath,
        sessionId: session.id,
        title,
      });
      setSession(next);
      await refreshSummaries();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  async function handleTitleFromFirstMessage() {
    if (!session || !projectPath) return;
    try {
      const next = await updateSessionMeta({
        useBackend,
        projectPath,
        sessionId: session.id,
        titleFromFirstMessage: true,
      });
      setSession(next);
      await refreshSummaries();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  async function handleToggleFlag() {
    if (!session || !projectPath) return;
    try {
      const next = await updateSessionMeta({
        useBackend,
        projectPath,
        sessionId: session.id,
        flagged: !session.flagged,
      });
      setSession(next);
      await refreshSummaries();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

async function handleClearResume() {
    if (!session || !projectPath || !useBackend) return;
    try {
      const next = await chatClearResume(projectPath, session.id);
      setSession(next);
    } catch (err) {
      setError(err instanceof Error ? err.message : `清除续聊失败：${String(err)}`);
    }
  }

  async function handlePromote() {
    if (!session || !projectPath || !useBackend) return;
    try {
      const result = await chatPromoteToTask({
        projectPath,
        sessionId: session.id,
      });
      setSession(result.session);
      dispatch({ type: "tasks/upserted", task: result.task });
      dispatch({ type: "tasks/selected", taskId: result.taskId });
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  if (!projectPath) {
    return (
      <div className="chat-page-empty" role="status">
        <div className="chat-page-empty-card">
          <h2 className="chat-page-empty-title">开始本机 Agent 对话</h2>
          <p className="chat-page-empty-copy">
            先添加一个本地项目目录。Chat 与 Task 相互独立，这里不会推进任务状态机。
          </p>
          <div className="chat-page-empty-actions">
            <Button type="button" variant="primary" onClick={() => setAddProjectOpen(true)}>
              添加项目
            </Button>
          </div>
          {state.projects.recent.length > 0 ? (
            <p className="chat-page-empty-hint">或在左侧「最近项目」里点选一个已有项目。</p>
          ) : (
            <p className="chat-page-empty-hint">还没有最近项目时，请用「添加项目」选择本地文件夹。</p>
          )}
        </div>
        {addProjectOpen ? <AddProjectModal onClose={() => setAddProjectOpen(false)} /> : null}
      </div>
    );
  }

  const canPromote = Boolean(session && session.messages.some((message) => message.role === "user"));

  const selectedAgent = session
    ? agents.find((agent) => agent.id === session.agentId) ??
      state.agents.find((agent) => agent.id === session.agentId)
    : undefined;
  const selectedDiagnostic = session
    ? diagnostics.find((item) => item.agentId === session.agentId)
    : undefined;

  return (
    <div
      className={`chat-page${contextOpen ? " chat-page--context-open" : ""}`}
      data-testid="chat-page"
      onKeyDown={(event) => {
        if (event.key !== "Tab" || !event.shiftKey || !session) return;
        const target = event.target as HTMLElement | null;
        if (!target?.closest(".chat-main")) return;
        event.preventDefault();
        void handleCyclePermission();
      }}
    >
      <ChatInbox
        summaries={summaries}
        selectedId={sessionId}
        filter={inboxFilter}
        agentNameById={agentNameById}
        useBackend={useBackend}
        onFilterChange={setInboxFilter}
        onSelect={(id) => {
          setSessionId(id);
          setError(null);
        }}
        onCreate={() => void handleCreate()}
        onArchive={(id) => void handleArchive(id)}
        onRestore={(id) => void handleRestore(id)}
        searchQuery={inboxSearch}
        onSearchQueryChange={setInboxSearch}
      />

      <section className="chat-main" aria-label="当前会话">
        {!session ? (
          <div className="chat-messages-empty">
            <h3 className="chat-messages-empty-title">还没有选中会话</h3>
            <p>在左侧新建会话，或点选已有会话。支持流式回复、停止生成、续聊与权限三档。</p>
            <div className="chat-messages-empty-actions">
              <Button type="button" variant="primary" onClick={() => void handleCreate()}>
                新建会话
              </Button>
              <button
                type="button"
                className="chat-link-btn"
                aria-pressed={contextOpen}
                onClick={() => setContextOpen((open) => !open)}
              >
                {contextOpen ? "隐藏上下文" : "查看上下文"}
              </button>
            </div>
            {agents.length === 0 ? (
              <p className="chat-messages-empty-warn" role="status">
                当前没有可用 Agent。请到「设置」检查本机 CLI（推荐 grok，或 Codex / Claude Code）是否已安装并登录。
              </p>
            ) : null}
            {error ? <p className="chat-messages-empty-warn" role="alert">{error}</p> : null}
          </div>
        ) : (
          <>
            <ChatSessionHeader
              turnRunning={turnRunning}
              elapsedHint={turnRunning ? turnRunningHint(elapsedMs) : null}
              session={session}
              useBackend={useBackend}
              canPromote={canPromote}
              contextOpen={contextOpen}
              onToggleContext={() => setContextOpen((open) => !open)}
              onClearResume={() => void handleClearResume()}
              onPromote={() => void handlePromote()}
              onRename={(title) => void handleRename(title)}
              onTitleFromFirstMessage={() => void handleTitleFromFirstMessage()}
              onToggleFlag={() => void handleToggleFlag()}
              onArchive={() => void handleArchive(session.id)}
            />
            <ChatTranscript messages={session.messages} />
            <ChatComposer
              draft={draft}
              sending={turnRunning}
              elapsedHint={turnRunning ? turnRunningHint(elapsedMs) : null}
              error={error}
              useBackend={useBackend}
              agentId={session.agentId}
              agents={agents}
              permissionMode={session.permissionMode}
              resumeHint={Boolean(session.resumeCommand)}
              diagnostics={diagnostics}
              diagnosticsLoading={diagnosticsLoading}
              onRefreshDiagnostics={() => void refreshDiagnostics()}
              onDraftChange={setDraft}
              onSend={() => void handleSend()}
              onAbort={() => void handleAbort()}
              onAgentChange={(id) => void handleAgentChange(id)}
              onPermissionChange={(mode) => void handlePermissionChange(mode)}
            />
          </>
        )}
      </section>

      {contextOpen ? (
        <ChatContextPanel
          projectPath={projectPath}
          permissionMode={session?.permissionMode ?? null}
          agent={selectedAgent}
          diagnostic={selectedDiagnostic}
          diagnosticsLoading={diagnosticsLoading}
          useBackend={useBackend}
          onRefreshDiagnostics={() => void refreshDiagnostics()}
          onClose={() => setContextOpen(false)}
        />
      ) : null}
    </div>
  );
}

export default ChatPage;
