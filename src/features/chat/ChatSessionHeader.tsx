import type { ChatPermissionMode, ChatSession } from "../../domain";
import { Button } from "../../components/common/Button";
import { permissionModeLabel } from "./chatPermission";

export interface ChatSessionHeaderProps {
  session: ChatSession;
  useBackend: boolean;
  canPromote: boolean;
  onClearResume: () => void;
  onPromote: () => void;
}

export function ChatSessionHeader({
  session,
  useBackend,
  canPromote,
  onClearResume,
  onPromote,
}: ChatSessionHeaderProps) {
  const mode = session.permissionMode as ChatPermissionMode;

  return (
    <div className="chat-session-main-header">
      <div className="chat-session-main-heading">
        <h1 className="chat-main-title">{session.title}</h1>
        <span className="chat-permission-badge" data-mode={mode} title="当前权限档位">
          {permissionModeLabel(mode)}
        </span>
      </div>
      <div className="chat-main-meta">
        {session.resumeCommand ? (
          <span className="chat-hint" title={session.resumeCommand}>
            可续聊
            {useBackend ? (
              <button type="button" className="chat-link-btn" onClick={onClearResume}>
                开新 CLI 会话
              </button>
            ) : null}
          </span>
        ) : (
          <span className="chat-hint">新 CLI 会话</span>
        )}
        {useBackend && canPromote ? (
          <Button type="button" variant="ghost" onClick={onPromote}>
            升格为任务
          </Button>
        ) : null}
        {session.promotedTaskId ? (
          <span className="chat-hint">已升格 · {session.promotedTaskId}</span>
        ) : null}
      </div>
    </div>
  );
}
