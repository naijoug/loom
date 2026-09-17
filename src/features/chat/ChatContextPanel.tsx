import type { AgentConfig, AgentDiagnostic, ChatPermissionMode } from "../../domain";
import { diagnosticStatusLabel, permissionModeLabel } from "./chatPermission";

export interface ChatContextPanelProps {
  projectPath: string;
  permissionMode: ChatPermissionMode | null;
  agent: AgentConfig | undefined;
  diagnostic: AgentDiagnostic | undefined;
  diagnosticsLoading: boolean;
  useBackend: boolean;
  onRefreshDiagnostics: () => void;
  onClose: () => void;
}

/** Phase 1 empty-state context rail — no MCP / Sources connect. */
export function ChatContextPanel({
  projectPath,
  permissionMode,
  agent,
  diagnostic,
  diagnosticsLoading,
  useBackend,
  onRefreshDiagnostics,
  onClose,
}: ChatContextPanelProps) {
  const statusLabel = diagnosticStatusLabel(diagnostic?.status);
  const tone =
    diagnostic?.status === "ready"
      ? "ok"
      : diagnostic?.status === "missing" || diagnostic?.status === "disabled"
        ? "err"
        : "unknown";

  return (
    <aside className="chat-context-panel" aria-label="上下文" data-testid="chat-context-panel">
      <div className="chat-context-panel-header">
        <h2>上下文</h2>
        <button type="button" className="chat-link-btn" onClick={onClose} aria-label="关闭上下文面板">
          关闭
        </button>
      </div>

      <section className="chat-context-section">
        <h3>项目</h3>
        <p className="chat-context-path" title={projectPath}>
          <code>{projectPath}</code>
        </p>
        <p className="chat-hint">
          会话落在项目 <code>.loom/chat/</code>；与 Board Task 分存，升格只复制草稿。
        </p>
      </section>

      <section className="chat-context-section">
        <h3>权限</h3>
        {permissionMode ? (
          <p>
            当前档位：<strong>{permissionModeLabel(permissionMode)}</strong>
            <span className="chat-hint">（Shift+Tab 可循环）</span>
            {permissionMode === "ask" ? (
              <span className="chat-hint"> · 询问编辑：可写 CLI，发送前需确认本回合授权</span>
            ) : null}
          </p>
        ) : (
          <p className="chat-hint">选择会话后显示权限档位。</p>
        )}
      </section>

      <section className="chat-context-section">
        <h3>Agent 诊断</h3>
        <div className="chat-context-diag-header">
          <strong>{agent?.name ?? "未选择 Agent"}</strong>
          <button
            type="button"
            className="chat-link-btn"
            onClick={onRefreshDiagnostics}
            disabled={diagnosticsLoading || !useBackend}
          >
            {diagnosticsLoading ? "检查中…" : "刷新"}
          </button>
        </div>
        {!useBackend ? (
          <p className="chat-hint">浏览器预览：不探测本机 CLI。与 Composer「状态」同源。</p>
        ) : (
          <dl className="chat-agent-status-dl">
            <div>
              <dt>状态</dt>
              <dd data-tone={tone}>{statusLabel}</dd>
            </div>
            <div>
              <dt>路径</dt>
              <dd>
                <code title={diagnostic?.resolvedPath}>{diagnostic?.resolvedPath ?? "未解析"}</code>
              </dd>
            </div>
            <div>
              <dt>版本</dt>
              <dd>{diagnostic?.version ?? "—"}</dd>
            </div>
            <div>
              <dt>说明</dt>
              <dd>{diagnostic?.detail ?? "点击刷新以复用 agent_diagnostics。"}</dd>
            </div>
          </dl>
        )}
      </section>

      <section className="chat-context-section chat-context-later">
        <h3>Sources / MCP</h3>
        <p className="chat-hint">
          Phase 1 仅展示本机项目路径与 Agent 诊断。MCP / Sources 连接列为{" "}
          <strong>后续阶段</strong>，本面板不提供连接入口。
        </p>
      </section>
    </aside>
  );
}
