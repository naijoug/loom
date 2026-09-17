import type { AgentConfig, AgentDiagnostic, ChatPermissionMode } from "../../domain";
import { Button } from "../../components/common/Button";
import {
  CHAT_PERMISSION_CYCLE,
  permissionCliHint,
  permissionModeLabel,
} from "./chatPermission";
import { ChatAgentStatusPopover } from "./ChatAgentStatusPopover";

export interface ChatComposerProps {
  draft: string;
  sending: boolean;
  error: string | null;
  useBackend: boolean;
  agentId: string;
  agents: AgentConfig[];
  permissionMode: ChatPermissionMode;
  resumeHint: boolean;
  diagnostics: AgentDiagnostic[];
  diagnosticsLoading: boolean;
  onRefreshDiagnostics: () => void;
  onDraftChange: (value: string) => void;
  onSend: () => void;
  onAbort: () => void;
  onAgentChange: (agentId: string) => void;
  onPermissionChange: (mode: ChatPermissionMode) => void;
}

export function ChatComposer({
  draft,
  sending,
  error,
  useBackend,
  agentId,
  agents,
  permissionMode,
  resumeHint,
  diagnostics,
  diagnosticsLoading,
  onRefreshDiagnostics,
  onDraftChange,
  onSend,
  onAbort,
  onAgentChange,
  onPermissionChange,
}: ChatComposerProps) {
  const selectedAgent = agents.find((agent) => agent.id === agentId);
  const selectedDiagnostic = diagnostics.find((item) => item.agentId === agentId);
  const cliHint = permissionCliHint(selectedAgent?.adapterType, permissionMode);

  return (
    <div className="chat-composer">
      {error ? (
        <div className="chat-hint" role="alert">
          {error}
        </div>
      ) : null}

      <div className="chat-composer-toolbar">
        <label className="chat-composer-field">
          <span className="visually-hidden">Agent</span>
          <select
            className="chat-agent-select"
            value={agentId}
            onChange={(event) => onAgentChange(event.target.value)}
            aria-label="选择 Agent"
            disabled={sending}
          >
            {agents.length === 0 ? (
              <option value={agentId}>无可用 Agent</option>
            ) : (
              agents.map((agent) => {
                const diagnostic = diagnostics.find((item) => item.agentId === agent.id);
                const suffix =
                  diagnostic?.status === "missing"
                    ? "（缺失）"
                    : diagnostic?.status === "disabled"
                      ? "（已停用）"
                      : "";
                return (
                  <option key={agent.id} value={agent.id}>
                    {agent.name}
                    {suffix}
                  </option>
                );
              })
            )}
          </select>
        </label>

        <ChatAgentStatusPopover
          agent={selectedAgent}
          diagnostic={selectedDiagnostic}
          loading={diagnosticsLoading}
          useBackend={useBackend}
          onRefresh={onRefreshDiagnostics}
        />

        <div
          className="chat-permission-tiers"
          role="radiogroup"
          aria-label="权限档位（Shift+Tab 循环）"
        >
          {CHAT_PERMISSION_CYCLE.map((mode) => (
            <button
              key={mode}
              type="button"
              role="radio"
              aria-checked={permissionMode === mode}
              className={`chat-permission-tier${permissionMode === mode ? " selected" : ""}`}
              data-mode={mode}
              disabled={sending}
              onClick={() => onPermissionChange(mode)}
            >
              {permissionModeLabel(mode)}
            </button>
          ))}
        </div>
      </div>

      <textarea
        value={draft}
        onChange={(event) => onDraftChange(event.target.value)}
        placeholder="输入消息…（Enter 发送，Shift+Enter 换行；Shift+Tab 切换权限）"
        aria-label="消息输入"
        disabled={sending}
        onKeyDown={(event) => {
          if (event.key === "Enter" && !event.shiftKey) {
            event.preventDefault();
            onSend();
          }
        }}
      />

      <div className="chat-composer-actions">
        <span className="chat-hint">
          {useBackend ? "本机 CLI" : "模拟"} · {permissionModeLabel(permissionMode)}
          {selectedAgent ? ` · ${cliHint}` : ""}
          {resumeHint ? " · 续聊中" : ""}
          {sending ? " · 生成中…" : ""}
        </span>
        <div className="chat-composer-buttons">
          {sending && useBackend ? (
            <Button type="button" variant="ghost" onClick={onAbort}>
              停止
            </Button>
          ) : null}
          <Button type="button" onClick={onSend} disabled={!draft.trim() || sending}>
            发送
          </Button>
        </div>
      </div>
    </div>
  );
}
