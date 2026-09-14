import { useEffect, useMemo, useState } from "react";
import type { AgentConfig, ChatPermissionMode, ChatSession } from "../../domain";
import { useAppState } from "../../state/AppStateContext";
import { useAgentBridge } from "../../hooks/useAgentBridge";
import { Button } from "../../components/common/Button";
import { mockChatStore } from "./mockStore";
import "./ChatPage.css";

function enabledAgents(agents: AgentConfig[]) {
  return agents.filter((agent) => agent.enabled && agent.adapterType !== "dummy");
}

export function ChatPage() {
  const { state } = useAppState();
  const { loadAgents } = useAgentBridge();
  const projectPath = state.projects.current?.path ?? null;
  const agents = useMemo(() => enabledAgents(state.agents), [state.agents]);

  const [sessionId, setSessionId] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [tick, setTick] = useState(0);

  useEffect(() => {
    void loadAgents();
  }, [loadAgents]);

  const summaries = useMemo(() => {
    if (!projectPath) return [];
    void tick;
    return mockChatStore.list(projectPath);
  }, [projectPath, tick]);

  const session: ChatSession | null = useMemo(() => {
    if (!sessionId) return null;
    void tick;
    return mockChatStore.get(sessionId) ?? null;
  }, [sessionId, tick]);

  function refresh() {
    setTick((value) => value + 1);
  }

  function handleCreate() {
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
    const created = mockChatStore.create({ projectPath, agentId });
    setSessionId(created.id);
    setDraft("");
    refresh();
  }

  function handleSend() {
    if (!session) return;
    if (!draft.trim()) return;
    setError(null);
    mockChatStore.send(session.id, draft);
    setDraft("");
    refresh();
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
          <Button type="button" onClick={handleCreate}>
            新建
          </Button>
        </div>
        {summaries.length === 0 ? (
          <div className="chat-session-empty">
            还没有会话。点「新建」开始；M1 使用本地模拟回复，M2 会接通真实 CLI。
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
                      mockChatStore.setAgent(session.id, event.target.value);
                      refresh();
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
                      mockChatStore.setPermissionMode(session.id, mode);
                      refresh();
                    }}
                  />
                  允许写入
                </label>
              </div>
            </div>

            <div className="chat-messages" data-testid="chat-messages">
              {session.messages.length === 0 ? (
                <div className="chat-messages-empty">
                  发送第一条消息试试。当前是 M1 壳层，不会调用本机 CLI。
                </div>
              ) : (
                session.messages.map((message) => (
                  <div key={message.id} className={`chat-bubble ${message.role}`}>
                    {message.content}
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
                onKeyDown={(event) => {
                  if (event.key === "Enter" && !event.shiftKey) {
                    event.preventDefault();
                    handleSend();
                  }
                }}
              />
              <div className="chat-composer-actions">
                <span className="chat-hint">
                  权限：{session.permissionMode === "read_write" ? "可写" : "只读"} · 模拟回复
                </span>
                <Button type="button" onClick={handleSend} disabled={!draft.trim()}>
                  发送
                </Button>
              </div>
            </div>
          </>
        )}
      </section>
    </div>
  );
}

export default ChatPage;
