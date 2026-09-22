import { useEffect, useRef, useState } from "react";
import type { ChatPermissionMode, ChatSession } from "../../domain";
import { chatResumeLabel } from "../../domain/chat";
import { permissionModeLabel } from "./chatPermission";

export interface ChatSessionHeaderProps {
  turnRunning?: boolean;
  elapsedHint?: string | null;
  session: ChatSession;
  useBackend: boolean;
  canPromote: boolean;
  contextOpen?: boolean;
  onToggleContext?: () => void;
  onClearResume: () => void;
  onPromote: () => void;
  onRename: (title: string) => void;
  onTitleFromFirstMessage: () => void;
  onToggleFlag: () => void;
  onArchive: () => void;
  onExport?: () => void;
  exporting?: boolean;
}

export function ChatSessionHeader({
  turnRunning = false,
  elapsedHint = null,
  session,
  useBackend,
  canPromote,
  contextOpen = false,
  onToggleContext,
  onClearResume,
  onPromote,
  onRename,
  onTitleFromFirstMessage,
  onToggleFlag,
  onArchive,
  onExport,
  exporting = false,
}: ChatSessionHeaderProps) {
  const mode = session.permissionMode as ChatPermissionMode;
  const [menuOpen, setMenuOpen] = useState(false);
  const [renaming, setRenaming] = useState(false);
  const [draftTitle, setDraftTitle] = useState(session.title);
  const menuRef = useRef<HTMLDivElement | null>(null);
  const renameCommitted = useRef(false);
  const originalTitle = useRef(session.title);

  useEffect(() => {
    if (!renaming) setDraftTitle(session.title);
  }, [session.title, session.id, renaming]);

  useEffect(() => {
    if (!menuOpen) return;
    const onPointerDown = (event: MouseEvent) => {
      if (!menuRef.current?.contains(event.target as Node)) {
        setMenuOpen(false);
      }
    };
    document.addEventListener("mousedown", onPointerDown);
    return () => document.removeEventListener("mousedown", onPointerDown);
  }, [menuOpen]);

  function beginRename() {
    renameCommitted.current = false;
    originalTitle.current = session.title;
    setDraftTitle(session.title);
    setRenaming(true);
  }

  function commitRename(value: string) {
    if (renameCommitted.current) return;
    renameCommitted.current = true;
    const trimmed = value.trim();
    if (trimmed && trimmed !== originalTitle.current) {
      onRename(trimmed);
    } else {
      setDraftTitle(session.title);
    }
    setRenaming(false);
  }

  const flagged = Boolean(session.flagged);
  const hasUserMessage = session.messages.some((message) => message.role === "user");

  return (
    <div className="chat-session-main-header">
      <div className="chat-session-main-heading">
        {renaming ? (
          <input
            className="chat-title-input"
            value={draftTitle}
            aria-label="会话标题"
            autoFocus
            onChange={(event) => setDraftTitle(event.target.value)}
            onBlur={(event) => commitRename(event.currentTarget.value)}
            onKeyDown={(event) => {
              if (event.nativeEvent.isComposing || event.nativeEvent.keyCode === 229) return;
              if (event.key === "Enter") {
                event.preventDefault();
                commitRename(event.currentTarget.value);
              } else if (event.key === "Escape") {
                renameCommitted.current = true;
                setDraftTitle(session.title);
                setRenaming(false);
              }
            }}
          />
        ) : (
          <h1 className="chat-main-title">{session.title}</h1>
        )}
        <span className="chat-permission-badge" data-mode={mode} title="当前权限档位">
          {permissionModeLabel(mode)}
        </span>
        {flagged ? (
          <span className="chat-flag-badge" title="已标记需关注">
            需关注
          </span>
        ) : null}
      </div>
      <div className="chat-main-meta">
        {turnRunning ? (
          <span className="chat-turn-running" role="status" aria-live="polite">
            <span className="chat-turn-spinner" aria-hidden="true" />
            {elapsedHint ?? "生成中…"}
          </span>
        ) : null}
        <span className="chat-hint" title={session.resumeHandle?.nativeSessionId ?? (session.resumeCommand ? "请在会话菜单开新 CLI 会话，历史消息会保留" : undefined)}>
          {chatResumeLabel(session)}
        </span>
        {onToggleContext ? (
          <button
            type="button"
            className="chat-link-btn"
            aria-pressed={contextOpen}
            onClick={onToggleContext}
            title="项目路径、权限与 Agent 诊断（无 MCP）"
          >
            {contextOpen ? "隐藏上下文" : "上下文"}
          </button>
        ) : null}
        <div className="chat-session-menu" ref={menuRef}>
          <button
            type="button"
            className="chat-session-menu-trigger"
            aria-haspopup="menu"
            aria-expanded={menuOpen}
            onClick={() => setMenuOpen((open) => !open)}
          >
            会话
          </button>
          {menuOpen ? (
            <div className="chat-session-menu-pop" role="menu">
              <button
                type="button"
                role="menuitem"
                onClick={() => {
                  setMenuOpen(false);
                  beginRename();
                }}
              >
                重命名
              </button>
              <button
                type="button"
                role="menuitem"
                disabled={!hasUserMessage}
                onClick={() => {
                  setMenuOpen(false);
                  onTitleFromFirstMessage();
                }}
              >
                用首条消息作标题
              </button>
              <button
                type="button"
                role="menuitem"
                onClick={() => {
                  setMenuOpen(false);
                  onToggleFlag();
                }}
              >
                {flagged ? "取消需关注" : "标记需关注"}
              </button>
              <hr />
              {useBackend && onExport && <button type="button" role="menuitem" disabled={exporting} onClick={() => {setMenuOpen(false); onExport();}}>
                {exporting ? "导出中…" : "导出会话（JSON + Markdown）"}
              </button>}
              {useBackend && (session.resumeHandle || session.resumeCommand) ? (
                <button
                  type="button"
                  role="menuitem"
                  onClick={() => {
                    setMenuOpen(false);
                    onClearResume();
                  }}
                >
                  开新 CLI 会话
                </button>
              ) : null}
              {useBackend && canPromote ? (
                <button
                  type="button"
                  role="menuitem"
                  onClick={() => {
                    setMenuOpen(false);
                    onPromote();
                  }}
                >
                  升格为任务（仅草稿，不自动开跑）
                </button>
              ) : null}
              {session.promotedTaskId ? (
                <div className="chat-session-menu-note">
                  已升格草稿 · {session.promotedTaskId}（不会自动推进任务状态机）
                </div>
              ) : null}
              <button
                type="button"
                role="menuitem"
                onClick={() => {
                  setMenuOpen(false);
                  onArchive();
                }}
              >
                归档
              </button>
            </div>
          ) : null}
        </div>
      </div>
    </div>
  );
}
