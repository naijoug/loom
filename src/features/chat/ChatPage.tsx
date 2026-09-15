import { useCallback, useEffect, useMemo, useState } from "react";
import type { AgentConfig, ChatPermissionMode, ChatSession, ChatSessionSummary, Task } from "../../domain";
import { useAppState } from "../../state/AppStateContext";
import { useAgentBridge } from "../../hooks/useAgentBridge";
import { hasTauriRuntime } from "../../hooks/runtime";
import { invokeCommand, listenToEvent, TAURI_COMMANDS, TAURI_EVENTS } from "../../api";
import { Button } from "../../components/common/Button";
import { mockChatStore } from "./mockStore";
import "./ChatPage.css";

interface ChatStreamEvent {
  sessionId: string;
  turnId: string;
  messageId: string;
  delta: string;
  done: boolean;
}

interface ChatTurnFinishedEvent {
  sessionId: string;
  turnId: string;
  messageId: string;
  status: string;
  errorSummary?: string;
}

interface ChatSendResult {
  turnId: string;
  session: ChatSession;
}

function enabledAgents(agents: AgentConfig[]) {
  return agents.filter((agent) => agent.enabled && agent.adapterType !== "dummy");
}

export function ChatPage() {
  const { state, dispatch } = useAppState();
  const { loadAgents } = useAgentBridge();
  const projectPath = state.projects.current?.path ?? null;
  const agents = useMemo(() => enabledAgents(state.agents), [state.agents]);
  const useBackend = hasTauriRuntime();

  const [sessionId, setSessionId] = useState<string | null>(null);
  const [session, setSession] = useState<ChatSession | null>(null);
  const [summaries, setSummaries] = useState<ChatSessionSummary[]>([]);
  const [draft, setDraft] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [sending, setSending] = useState(false);

  const refreshSummaries = useCallback(async () => {
    if (!projectPath) {
      setSummaries([]);
      return;
    }
    if (!useBackend) {
      setSummaries(mockChatStore.list(projectPath));
      return;
    }
    const listed = await invokeCommand<ChatSessionSummary[]>(TAURI_COMMANDS.chatListSessions, {
      projectPath,
    });
    setSummaries(listed);
  }, [projectPath, useBackend]);

  const loadSession = useCallback(
    async (id: string) => {
      if (!projectPath) return;
      if (!useBackend) {
        setSession(mockChatStore.get(id) ?? null);
        return;
      }
      const next = await invokeCommand<ChatSession>(TAURI_COMMANDS.chatGet, {
        projectPath,
        sessionId: id,
      });
      setSession(next);
    },
    [projectPath, useBackend],
  );

  useEffect(() => {
    void loadAgents();
  }, [loadAgents]);

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
            messages: current.messages.map((message) =>
              message.id === payload.messageId
                ? {
                    ...message,
                    content: payload.done ? message.content : `${message.content}${payload.delta}`,
                    status: payload.done ? message.status : "streaming",
                  }
                : message,
            ),
            turnStatus: payload.done ? current.turnStatus : "streaming",
          };
        });
      });
      const unlistenFinished = await listenToEvent<ChatTurnFinishedEvent>(
        TAURI_EVENTS.chatTurnFinished,
        (payload) => {
          if (disposed) return;
          setSending(false);
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
                      status: payload.status === "error" ? "error" : "complete",
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
    const agentId = agents[0]?.id;
    if (!agentId) {
      setError("还没有可用的 Agent。请先到设置里配置 Codex / Claude Code / CLI。");
      return;
    }
    setError(null);
    if (!useBackend) {
      const created = mockChatStore.create({ projectPath, agentId });
      setSessionId(created.id);
      setSession(created);
      setDraft("");
      await refreshSummaries();
      return;
    }
    const created = await invokeCommand<ChatSession>(TAURI_COMMANDS.chatCreate, {
      projectPath,
      agentId,
    });
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
      const result = await invokeCommand<ChatSendResult>(TAURI_COMMANDS.chatSend, {
        projectPath,
        sessionId: session.id,
        text: draft,
        permissionMode: session.permissionMode,
      });
      setDraft("");
      setSession(result.session);
      await refreshSummaries();
      // sending cleared on chat-turn-finished; safety timeout
      window.setTimeout(() => setSending(false), 120_000);
    } catch (err) {
      setSending(false);
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  async function handleAbort() {
    if (!session || !projectPath || !useBackend) return;
    await invokeCommand(TAURI_COMMANDS.chatAbort, {
      projectPath,
      sessionId: session.id,
      turnId: session.activeTurnId,
    });
    setSending(false);
  }

  if (!projectPath) {
    return (
      <div className="chat-page-empty" role="status">
        先在左侧选择一个项目，再开始对话。
        <br />
        Chat 与 Task 是分开的：这里不会推进任务状态机。
      </div>
    );
  }

  return (
    <div className="chat-page" data-testid="chat-page">
      <aside className="chat-session-pane" aria-label="会话列表">
        <div className="chat-session-header">
          <h2>对话</h2>
          <Button type="button" onClick={() => void handleCreate()}>
            新建
          </Button>
        </div>
        {summaries.length === 0 ? (
          <div className="chat-session-empty">
            还没有会话。点「新建」开始。
            {useBackend ? " 将通过本机 Agent CLI 流式回复。" : " （浏览器预览使用模拟回复）"}
          </div>
        ) : (
          <ul className="chat-session-list">
            {summaries.map((item) => (
              <li key={item.id}>
                <button
                  type="button"
                  className={`chat-session-item${item.id === sessionId ? " selected" : ""}`}
                  onClick={() => {
                    setSessionId(item.id);
                    setError(null);
                  }}
                >
                  <span className="chat-session-title">{item.title}</span>
                  {item.preview ? <span className="chat-session-preview">{item.preview}</span> : null}
                </button>
              </li>
            ))}
          </ul>
        )}
      </aside>

      <section className="chat-main" aria-label="当前会话">
        {!session ? (
          <div className="chat-messages-empty">选择或新建一个会话开始聊天。</div>
        ) : (
          <>
            <div className="chat-main-header">
              <h1 className="chat-main-title">{session.title}</h1>
              <div className="chat-main-meta">
                <label>
                  <span className="visually-hidden">Agent</span>
                  <select
                    className="chat-agent-select"
                    value={session.agentId}
                    onChange={(event) => {
                      const agentId = event.target.value;
                      void (async () => {
                        if (!useBackend) {
                          const next = mockChatStore.setAgent(session.id, agentId);
                          setSession(next ?? null);
                          return;
                        }
                        const next = await invokeCommand<ChatSession>(TAURI_COMMANDS.chatSetAgent, {
                          projectPath,
                          sessionId: session.id,
                          agentId,
                        });
                        setSession(next);
                      })();
                    }}
                    aria-label="选择 Agent"
                  >
                    {agents.length === 0 ? (
                      <option value={session.agentId}>无可用 Agent</option>
                    ) : (
                      agents.map((agent) => (
                        <option key={agent.id} value={agent.id}>
                          {agent.name}
                        </option>
                      ))
                    )}
                  </select>
                </label>
                <label className="chat-permission">
                  <input
                    type="checkbox"
                    checked={session.permissionMode === "read_write"}
                    onChange={(event) => {
                      const mode: ChatPermissionMode = event.target.checked
                        ? "read_write"
                        : "read_only";
                      if (!useBackend) {
                        setSession(mockChatStore.setPermissionMode(session.id, mode) ?? null);
                        return;
                      }
                      setSession({ ...session, permissionMode: mode });
                    }}
                  />
                  允许写入
                </label>
                {session.resumeCommand ? (
                  <span className="chat-hint" title={session.resumeCommand}>
                    可续聊
                    <button
                      type="button"
                      className="chat-agent-select"
                      style={{ marginLeft: 8 }}
                      onClick={() => {
                        void (async () => {
                          if (!useBackend || !projectPath) return;
                          try {
                            const next = await invokeCommand<ChatSession>(
                              TAURI_COMMANDS.chatClearResume,
                              { projectPath, sessionId: session.id },
                            );
                            setSession(next);
                          } catch (err) {
                            setError(
                              err instanceof Error
                                ? err.message
                                : `清除续聊失败：${String(err)}`,
                            );
                          }
                        })();
                      }}
                    >
                      开新 CLI 会话
                    </button>
                  </span>
                ) : (
                  <span className="chat-hint">新 CLI 会话</span>
                )}
              </div>
            </div>

            <div className="chat-messages" data-testid="chat-messages">
              {session.messages.length === 0 ? (
                <div className="chat-messages-empty">
                  发送第一条消息。默认只读；勾选「允许写入」后才会用可写阶段调用 Agent。
                </div>
              ) : (
                session.messages.map((message) => (
                  <div key={message.id} className={`chat-bubble ${message.role}`}>
                    {message.content || (message.status === "streaming" ? "…" : "")}
                  </div>
                ))
              )}
            </div>

            <div className="chat-composer">
              {error ? <div className="chat-hint" role="alert">{error}</div> : null}
              <textarea
                value={draft}
                onChange={(event) => setDraft(event.target.value)}
                placeholder="输入消息…（Enter 发送，Shift+Enter 换行）"
                aria-label="消息输入"
                disabled={sending}
                onKeyDown={(event) => {
                  if (event.key === "Enter" && !event.shiftKey) {
                    event.preventDefault();
                    void handleSend();
                  }
                }}
              />
              <div className="chat-composer-actions">
                <span className="chat-hint">
                  {useBackend ? "本机 CLI" : "模拟"} ·{" "}
                  {session.permissionMode === "read_write" ? "可写" : "只读"}
                  {session.resumeCommand ? " · 续聊中" : ""}
                  {sending ? " · 生成中…" : ""}
                </span>
                <div style={{ display: "flex", gap: 8 }}>
                  {useBackend ? (
                    <Button
                      type="button"
                      variant="ghost"
                      disabled={sending || session.messages.every((m) => m.role !== "user")}
                      onClick={() => {
                        void (async () => {
                          if (!projectPath) return;
                          try {
                            const result = await invokeCommand<{
                              taskId: string;
                              task: Task;
                              session: ChatSession;
                            }>(TAURI_COMMANDS.chatPromoteToTask, {
                              projectPath,
                              sessionId: session.id,
                            });
                            setSession(result.session);
                            dispatch({ type: "tasks/upserted", task: result.task });
                            dispatch({ type: "tasks/selected", taskId: result.taskId });
                          } catch (err) {
                            setError(err instanceof Error ? err.message : String(err));
                          }
                        })();
                      }}
                    >
                      升格为任务
                    </Button>
                  ) : null}
                  {sending && useBackend ? (
                    <Button type="button" variant="ghost" onClick={() => void handleAbort()}>
                      停止
                    </Button>
                  ) : null}
                  <Button type="button" onClick={() => void handleSend()} disabled={!draft.trim() || sending}>
                    发送
                  </Button>
                </div>
              </div>
            </div>
          </>
        )}
      </section>
    </div>
  );
}

export default ChatPage;
