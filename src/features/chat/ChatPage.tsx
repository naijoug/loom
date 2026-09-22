import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type {
  AgentConfig,
  AgentDiagnostic,
  ChatPermissionMode,
  ChatSession,
  ChatSessionSummary,
} from "../../domain";
import { useAppState } from "../../state/AppStateContext";
import { useAgentCatalog } from "../../hooks/useAgentCatalog";
import { hasTauriRuntime } from "../../hooks/runtime";
import { useChatBridge } from "./useChatBridge";
import {
  chatAbort,
  chatClearResume,
  chatCreate,
  chatListSessions,
  chatPromoteToTask,
  chatSetAgent,
  chatUpdateMeta,
} from "../../api/chatClient";
import { mockChatStore } from "./mockStore";
import { chatSendController } from "./state/ChatSendController";
import { ChatInbox, type InboxFilter } from "./ChatInbox";
import { ChatTranscript } from "./ChatTranscript";
import { ChatComposer } from "./ChatComposer";
import { ChatSessionHeader } from "./ChatSessionHeader";
import { ChatContextPanel } from "./ChatContextPanel";
import { ChatExportNotice } from "./ChatExportNotice";
import { useChatExport } from "./useChatExport";
import {
  isChatTurnRunning,
  turnRunningHint,
} from "./chatTurn";
import {
  chatStoreDraftFor,
  chatStoreClearSubmittedDraft,
  chatStoreSelectProject,
  chatStoreSetDraft,
  createChatStoreSnapshot,
  type ChatStoreSnapshot,
} from "./state/chatStore";
import { AddProjectModal } from "../../components/Sidebar/AddProjectModal";
import { Button } from "../../components/common/Button";
import "./ChatPage.css";

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
  const { loadAgents, diagnoseAgents } = useAgentCatalog();
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
  const [summaries, setSummaries] = useState<ChatSessionSummary[]>([]);
  const [error, setError] = useState<string | null>(null);
  const pending = useRef(new Set<string>());
  const [pendingKeys, setPendingKeys] = useState(new Set<string>());
  const pendingStops = useRef(new Set<string>());
  const projectRef = useRef(projectPath);
  const selectionRef = useRef(sessionId);
  projectRef.current = projectPath;
  selectionRef.current = sessionId;
  const [elapsedMs, setElapsedMs] = useState(0);
  const [inboxFilter, setInboxFilter] = useState<InboxFilter>("active");
  const [inboxSearch, setInboxSearch] = useState("");
  const [addProjectOpen, setAddProjectOpen] = useState(false);
  const [chatStore, setChatStore] = useState<ChatStoreSnapshot>(() =>
    createChatStoreSnapshot(projectPath),
  );

  useEffect(() => {
    setChatStore((prev) => chatStoreSelectProject(prev, projectPath));
    setSessionId(null);
    setSummaries([]);
    setError(null);
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
    if (projectRef.current === projectPath) setSummaries(listed);
  }, [projectPath, useBackend]);

  const { session, setSession, listenReady } = useChatBridge(
    projectPath, sessionId, useBackend, setError, () => { void refreshSummaries().catch((err) => {
      if (projectRef.current === projectPath) setError(String(err));
    }); },
  );
  const currentKey = JSON.stringify([projectPath, sessionId]);
  const exportState = useChatExport(useBackend ? projectPath : null, useBackend ? session?.id ?? null : null);
  const sending = pendingKeys.has(currentKey) || session?.turnStatus === "streaming";
  const turnStartedAtMs = session?.turnStatus === "streaming"
    ? [...session.messages].reverse().find((message) => message.role === "assistant")?.createdAtMs ?? null
    : null;
  const isCurrentView = () => projectRef.current === projectPath && selectionRef.current === sessionId;

  useEffect(() => {
    if (!session?.activeTurnId || !projectPath) return;
    const key = JSON.stringify([projectPath, session.id]);
    if (pendingStops.current.delete(key)) {
      void chatAbort({ projectPath, sessionId: session.id, turnId: session.activeTurnId }).catch((err) => {
        if (projectRef.current === projectPath && selectionRef.current === session.id) setError(String(err));
      });
    }
  }, [session?.activeTurnId, session?.id, projectPath]);

  useEffect(() => {
    void loadAgents();
    void refreshDiagnostics();
  }, [loadAgents, refreshDiagnostics]);



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
    void refreshSummaries().catch((err) => {
      if (projectRef.current === projectPath) setError(String(err));
    });
  }, [refreshSummaries, projectPath]);

  async function handleCreate() {
    if (!projectPath) { setError("请先在侧栏选择或添加一个项目。"); return; }
    const agentId = preferredAgentId(agents);
    if (!agentId) { setError("还没有可用的 Agent，请先到设置里配置。"); return; }
    setError(null);
    try {
      const created = useBackend ? await chatCreate({ projectPath, agentId }) : mockChatStore.create({ projectPath, agentId });
      if (!isCurrentView()) return;
      setInboxFilter("active");
      setSessionId(created.id);
      await refreshSummaries();
    } catch (err) { if (isCurrentView()) setError(String(err)); }
  }

  async function handleSend() {
    if (!session || !projectPath || !draft.trim() || sending || !listenReady) return;
    const key = JSON.stringify([projectPath, session.id]);
    if (pending.current.has(key)) return;
    pending.current.add(key);
    setPendingKeys(new Set(pending.current));
    setError(null);
    try {
      let turnToAbort: string | undefined;
      if (!useBackend) {
        setSession(mockChatStore.send(session.id, draft) ?? null);
      } else {
        const result = await chatSendController.send({ projectPath, sessionId: session.id, text: draft, permissionMode: session.permissionMode });
        if (isCurrentView()) setSession(result.session);
        if (pendingStops.current.delete(key)) turnToAbort = result.turnId;
      }
      // A later abort/list failure must not leave accepted text ready to send again.
      setChatStore((previous) => chatStoreClearSubmittedDraft(previous, projectPath, session.id, draft));
      if (turnToAbort) await chatAbort({ projectPath, sessionId: session.id, turnId: turnToAbort });
      if (projectRef.current === projectPath) await refreshSummaries();
    } catch (err) { if (isCurrentView()) setError(String(err)); }
    finally {
      pending.current.delete(key);
      pendingStops.current.delete(key);
      setPendingKeys(new Set(pending.current));
    }
  }

  async function handleAbort() {
    if (!session || !projectPath || !useBackend) return;
    if (!session.activeTurnId) {
      pendingStops.current.add(JSON.stringify([projectPath, session.id]));
      return;
    }
    try { await chatAbort({ projectPath, sessionId: session.id, turnId: session.activeTurnId }); }
    catch (err) { if (isCurrentView()) setError(String(err)); }
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
      if (isCurrentView()) setError(err instanceof Error ? err.message : String(err));
    }
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
      if (isCurrentView()) setError(err instanceof Error ? err.message : String(err));
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
      if (isCurrentView() && sessionId === id) {
        setSessionId(null);
        setSession(null);
      }
      await refreshSummaries();
    } catch (err) {
      if (isCurrentView()) setError(err instanceof Error ? err.message : String(err));
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
      if (!isCurrentView()) return;
      setInboxFilter("active");
      setSessionId(id);
      await refreshSummaries();
    } catch (err) {
      if (isCurrentView()) setError(err instanceof Error ? err.message : String(err));
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
      if (isCurrentView()) setError(err instanceof Error ? err.message : String(err));
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
      if (isCurrentView()) setError(err instanceof Error ? err.message : String(err));
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
      if (isCurrentView()) setError(err instanceof Error ? err.message : String(err));
    }
  }

async function handleClearResume() {
    if (!session || !projectPath || !useBackend) return;
    try {
      const next = await chatClearResume(projectPath, session.id);
      setSession(next);
    } catch (err) {
      if (isCurrentView()) setError(err instanceof Error ? err.message : `清除续聊失败：${String(err)}`);
    }
  }

  async function handlePromote() {
    if (!session || !projectPath || !useBackend) return;
    try {
      const result = await chatPromoteToTask({
        projectPath,
        sessionId: session.id,
      });
      if (!isCurrentView()) return;
      setSession(result.session);
      dispatch({ type: "tasks/upserted", task: result.task });
      dispatch({ type: "tasks/selected", taskId: result.taskId });
    } catch (err) {
      if (isCurrentView()) setError(err instanceof Error ? err.message : String(err));
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
        {!session && error ? <div className="chat-hint" role="alert">{error}</div> : null}
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
              onExport={() => void exportState.exportSession()}
              exporting={exportState.busy}
            />
            <ChatExportNotice key={`${currentKey}:${exportState.result?.directory ?? ""}`} busy={exportState.busy} error={exportState.error} result={exportState.result} />
            <ChatTranscript messages={session.messages} turns={session.turns} projectPath={projectPath ?? undefined} sessionId={session.id} />
            <ChatComposer
              draft={draft}
              sending={turnRunning}
              canSend={listenReady}
              elapsedHint={turnRunning ? turnRunningHint(elapsedMs) : null}
              error={error}
              useBackend={useBackend}
              agentId={session.agentId}
              agents={agents}
              permissionMode={session.permissionMode}
              resumeHint={Boolean(session.resumeHandle)}
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
