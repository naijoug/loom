import { useEffect, useId, useRef, useState } from "react";
import type { AgentConfig, AgentDiagnostic } from "../../domain";
import { diagnosticStatusLabel } from "./chatPermission";

export interface ChatAgentStatusPopoverProps {
  agent: AgentConfig | undefined;
  diagnostic: AgentDiagnostic | undefined;
  loading: boolean;
  useBackend: boolean;
  onRefresh: () => void;
}

export function ChatAgentStatusPopover({
  agent,
  diagnostic,
  loading,
  useBackend,
  onRefresh,
}: ChatAgentStatusPopoverProps) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const titleId = useId();

  useEffect(() => {
    if (!open) return;
    function onPointerDown(event: MouseEvent) {
      if (!rootRef.current?.contains(event.target as Node)) {
        setOpen(false);
      }
    }
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") setOpen(false);
    }
    document.addEventListener("mousedown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("mousedown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [open]);

  const status = diagnostic?.status;
  const statusLabel = diagnosticStatusLabel(status);
  const tone =
    status === "ready" ? "ok" : status === "missing" || status === "disabled" ? "err" : "unknown";

  return (
    <div className="chat-agent-status" ref={rootRef}>
      <button
        type="button"
        className={`chat-agent-status-trigger tone-${tone}`}
        aria-expanded={open}
        aria-controls={titleId}
        title="Agent 状态"
        onClick={() => {
          const next = !open;
          setOpen(next);
          if (next) onRefresh();
        }}
      >
        状态 · {useBackend ? statusLabel : "模拟"}
      </button>
      {open ? (
        <div
          className="chat-agent-status-popover"
          role="dialog"
          aria-labelledby={titleId}
          id={titleId}
        >
          <div className="chat-agent-status-popover-header">
            <strong>{agent?.name ?? "未选择 Agent"}</strong>
            <button type="button" className="chat-link-btn" onClick={onRefresh} disabled={loading}>
              {loading ? "检查中…" : "刷新"}
            </button>
          </div>
          {!useBackend ? (
            <p className="chat-hint">浏览器预览模式：不探测本机 CLI。</p>
          ) : (
            <dl className="chat-agent-status-dl">
              <div>
                <dt>状态</dt>
                <dd data-tone={tone}>{statusLabel}</dd>
              </div>
              <div>
                <dt>命令</dt>
                <dd>
                  <code>{agent?.command ?? "—"}</code>
                </dd>
              </div>
              <div>
                <dt>路径</dt>
                <dd>
                  <code title={diagnostic?.resolvedPath}>
                    {diagnostic?.resolvedPath ?? "未解析"}
                  </code>
                </dd>
              </div>
              <div>
                <dt>版本</dt>
                <dd>{diagnostic?.version ?? "—"}</dd>
              </div>
              <div>
                <dt>说明</dt>
                <dd>{diagnostic?.detail ?? "尚未检查。点击刷新以复用 agent_diagnostics。"}</dd>
              </div>
            </dl>
          )}
        </div>
      ) : null}
    </div>
  );
}
