import type { ChatInboxFilter, ChatSessionSummary } from "../../domain";
import { filterChatSummaries, filterChatSummariesByQuery } from "../../domain";
import { Button } from "../../components/common/Button";

export type InboxFilter = ChatInboxFilter;

/** Craft SessionItem trailing time: compact relative / locale short. */
function formatUpdatedAt(updatedAtMs: number): string {
  const delta = Date.now() - updatedAtMs;
  if (!Number.isFinite(delta) || delta < 0) return "";
  const minute = 60_000;
  const hour = 60 * minute;
  const day = 24 * hour;
  if (delta < minute) return "刚刚";
  if (delta < hour) return `${Math.floor(delta / minute)} 分钟前`;
  if (delta < day) return `${Math.floor(delta / hour)} 小时前`;
  if (delta < 7 * day) return `${Math.floor(delta / day)} 天前`;
  try {
    return new Date(updatedAtMs).toLocaleString("zh-CN", {
      month: "numeric",
      day: "numeric",
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
  onRestore: (sessionId: string) => void;
  searchQuery: string;
  onSearchQueryChange: (query: string) => void;
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
  onRestore,
  searchQuery,
  onSearchQueryChange,
}: ChatInboxProps) {
  const filtered = filterChatSummariesByQuery(
    filterChatSummaries(summaries, filter),
    searchQuery,
    (item) => agentNameById[item.agentId] ?? item.agentId,
  );

  return (
    <aside className="chat-inbox" aria-label="会话列表">
      <div className="chat-inbox-header">
        <h2>对话</h2>
        <Button type="button" variant="ghost" className="chat-inbox-new" onClick={onCreate}>
          新建
        </Button>
      </div>

      {/* Mirrors craft SessionSearchHeader: muted rounded search field */}
      <div className="chat-inbox-search">
        <span className="chat-inbox-search-icon" aria-hidden="true">
          ⌕
        </span>
        <input
          type="search"
          value={searchQuery}
          placeholder="搜索标题或内容…"
          aria-label="搜索会话"
          onChange={(event) => onSearchQueryChange(event.target.value)}
        />
        {searchQuery ? (
          <button
            type="button"
            className="chat-inbox-search-clear"
            aria-label="清除搜索"
            onClick={() => onSearchQueryChange("")}
          >
            ×
          </button>
        ) : null}
      </div>

      <div className="chat-inbox-filters" role="tablist" aria-label="会话过滤">
        {(
          [
            ["active", "进行中"],
            ["needs_attention", "需关注"],
            ["archived", "已归档"],
          ] as const
        ).map(([value, label]) => (
          <button
            key={value}
            type="button"
            role="tab"
            aria-selected={filter === value}
            className={`chat-inbox-filter${filter === value ? " active" : ""}`}
            onClick={() => onFilterChange(value)}
          >
            {label}
          </button>
        ))}
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
          {filtered.map((item, index) => {
            const selected = item.id === selectedId;
            const attention = item.needsAttention || item.flagged;
            return (
              <li key={item.id} className="chat-inbox-row">
                <button
                  type="button"
                  className={`chat-inbox-item${selected ? " selected" : ""}`}
                  data-session-id={item.id}
                  onClick={() => onSelect(item.id)}
                >
                  {index > 0 ? <span className="chat-inbox-sep" aria-hidden="true" /> : null}
                  <span className="chat-inbox-leading" aria-hidden="true">
                    <span className={`chat-inbox-status${attention ? " is-attention" : ""}`} />
                  </span>
                  <span className="chat-inbox-body">
                    <span className="chat-inbox-title-row">
                      <span className="chat-inbox-title">{item.title}</span>
                      <span className="chat-inbox-time">{formatUpdatedAt(item.updatedAtMs)}</span>
                    </span>
                    {item.preview ? (
                      <span className="chat-inbox-preview">{item.preview}</span>
                    ) : (
                      <span className="chat-inbox-preview chat-inbox-preview--muted">
                        {agentNameById[item.agentId] ?? item.agentId}
                      </span>
                    )}
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
                ) : (
                  <button
                    type="button"
                    className="chat-inbox-archive"
                    aria-label={`恢复 ${item.title}`}
                    title="恢复"
                    onClick={(event) => {
                      event.stopPropagation();
                      onRestore(item.id);
                    }}
                  >
                    恢复
                  </button>
                )}
              </li>
            );
          })}
        </ul>
      )}
    </aside>
  );
}
