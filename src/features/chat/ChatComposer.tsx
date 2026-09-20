import { useEffect, useState } from "react";
import type { AgentConfig, AgentDiagnostic, ChatPermissionMode } from "../../domain";
import { Button } from "../../components/common/Button";
import {
  ASK_TURN_CONFIRM_LABEL,
  CHAT_PERMISSION_CYCLE,
  askTurnRequiresConfirm,
  permissionCliHint,
  permissionModeLabel,
} from "./chatPermission";
import { ChatAgentStatusPopover } from "./ChatAgentStatusPopover";

export interface ChatComposerProps {
  draft: string;
  sending: boolean;
  /** Optional elapsed label while a turn is running. */
  elapsedHint?: string | null;
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
  elapsedHint = null,
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
  const [askConfirmOpen, setAskConfirmOpen] = useState(false);
  const selectedAgent = agents.find((agent) => agent.id === agentId);
  const selectedDiagnostic = diagnostics.find((item) => item.agentId === agentId);
  const cliHint = permissionCliHint(selectedAgent?.adapterType, permissionMode);
  const needsAskConfirm = askTurnRequiresConfirm(permissionMode);

  useEffect(() => {
    // Dismiss pending gate when mode changes away from ask or send starts.
    if (!needsAskConfirm || sending) {
      setAskConfirmOpen(false);
    }
  }, [needsAskConfirm, sending, permissionMode]);

  function requestSend() {
    if (!draft.trim() || sending) return;
    if (needsAskConfirm && !askConfirmOpen) {
      setAskConfirmOpen(true);
      return;
    }
    setAskConfirmOpen(false);
    onSend();
  }

  function cancelAskConfirm() {
    setAskConfirmOpen(false);
  }

  return (
    <div className="chat-composer-shell">
    <div className="chat-composer">
      {error ? (
        <div className="chat-hint" role="alert">
          {error}
        </div>
      ) : null}

      {askConfirmOpen && needsAskConfirm ? (
        <div
          className="chat-ask-confirm"
          role="dialog"
          aria-modal="true"
          aria-labelledby="chat-ask-confirm-title"
        >
          <div className="chat-ask-confirm-body">
            <strong id="chat-ask-confirm-title">询问编辑 · 本回合授权</strong>
            <p>
              确认后允许本回合写入，不会逐项询问工具操作。具体执行限制由所选 Agent 管理：
              <span className="chat-ask-confirm-label"> {ASK_TURN_CONFIRM_LABEL}</span>
            </p>
          </div>
          <div className="chat-ask-confirm-actions">
            <Button type="button" variant="ghost" onClick={cancelAskConfirm}>
              取消
            </Button>
            <Button type="button" onClick={requestSend}>
              确认并发送
            </Button>
          </div>
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
            requestSend();
          }
          if (event.key === "Escape" && askConfirmOpen) {
            event.preventDefault();
            cancelAskConfirm();
          }
        }}
      />

      <div className={`chat-composer-actions${sending ? " is-running" : ""}`}>
        <span className="chat-hint">
          {useBackend ? "本机 CLI" : "模拟"} · {permissionModeLabel(permissionMode)}
          {selectedAgent ? ` · ${cliHint}` : ""}
          {needsAskConfirm ? " · 每回合需确认可写" : ""}
          {resumeHint ? " · 续聊中" : ""}
          {sending ? ` · ${elapsedHint ?? "生成中…"}` : ""}
        </span>
        <div className="chat-composer-buttons">
          {sending && useBackend ? (
            <Button type="button" variant="ghost" onClick={onAbort}>
              停止
            </Button>
          ) : null}
          <Button type="button" onClick={requestSend} disabled={!draft.trim() || sending || agents.length === 0}>
            {needsAskConfirm ? "发送…" : "发送"}
          </Button>
        </div>
      </div>
    </div>
    </div>
  );
}
