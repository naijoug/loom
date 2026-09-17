import type { ChatInboxFilter, ChatSessionSummary } from "../../domain";
import { filterChatSummaries } from "../../domain";
import { Button } from "../../components/common/Button";

export type InboxFilter = ChatInboxFilter;

function formatUpdatedAt(updatedAtMs: number): string {
  try {
    return new Date(updatedAtMs).toLocaleString("zh-CN", {
      month: "numeric",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return "";
  }
}

export interface ChatInboxProps {
  summaries: ChatSessionSummary[];
  selectedId: string | null;
  filter: InboxFilter;
  agentNameById: Record<string, string>;
  useBackend: boolean;
  onFilterChange: (filter: InboxFilter) => void;
  onSelect: (sessionId: string) => void;
  onCreate: () => void;
  onArchive: (sessionId: string) => void;
}

export function ChatInbox({
  summaries,
  selectedId,
  filter,
  agentNameById,
  useBackend,
  onFilterChange,
  onSelect,
  onCreate,
  onArchive,
}: ChatInboxProps) {
  const filtered = filterChatSummaries(summaries, filter);

  return (
    <aside className="chat-inbox" aria-label="会话收件箱">
      <div className="chat-inbox-header">
        <h2>对话</h2>
        <Button type="button" onClick={onCreate}>
          新建
        </Button>
      </div>

      <div className="chat-inbox-filters" role="tablist" aria-label="会话过滤">
        <button
          type="button"
          role="tab"
          aria-selected={filter === "active"}
          className={`chat-inbox-filter${filter === "active" ? " active" : ""}`}
          onClick={() => onFilterChange("active")}
        >
          进行中
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={filter === "needs_attention"}
          className={`chat-inbox-filter${filter === "needs_attention" ? " active" : ""}`}
          onClick={() => onFilterChange("needs_attention")}
        >
          需关注
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={filter === "archived"}
          className={`chat-inbox-filter${filter === "archived" ? " active" : ""}`}
          onClick={() => onFilterChange("archived")}
        >
          已归档
        </button>
      </div>

      {filtered.length === 0 ? (
        <div className="chat-inbox-empty">
          {filter === "archived"
            ? "没有已归档会话。"
            : filter === "needs_attention"
              ? "没有需要关注的会话。"
              : `还没有会话。点「新建」开始。${
                  useBackend ? " 将通过本机 Agent CLI 流式回复。" : " （浏览器预览使用模拟回复）"
                }`}
        </div>
      ) : (
        <ul className="chat-inbox-list">
          {filtered.map((item) => (
            <li key={item.id} className="chat-inbox-row">
              <button
                type="button"
                className={`chat-inbox-item${item.id === selectedId ? " selected" : ""}`}
                onClick={() => onSelect(item.id)}
              >
                <span className="chat-inbox-title-row">
                  <span className="chat-inbox-title">{item.title}</span>
                  {item.needsAttention || item.flagged ? (
                    <span className="chat-inbox-attention" title="需要关注">
                      !
                    </span>
                  ) : null}
                </span>
                {item.preview ? <span className="chat-inbox-preview">{item.preview}</span> : null}
                <span className="chat-inbox-meta">
                  <span>{agentNameById[item.agentId] ?? item.agentId}</span>
                  <span>{formatUpdatedAt(item.updatedAtMs)}</span>
                </span>
              </button>
              {filter !== "archived" ? (
                <button
                  type="button"
                  className="chat-inbox-archive"
                  aria-label={`归档 ${item.title}`}
                  title="归档"
                  onClick={(event) => {
                    event.stopPropagation();
                    onArchive(item.id);
                  }}
                >
                  归档
                </button>
              ) : null}
            </li>
          ))}
        </ul>
      )}
    </aside>
  );
}
