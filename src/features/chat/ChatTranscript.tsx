import type { ChatMessage, ChatMessagePart } from "../../domain";

function renderPart(part: ChatMessagePart, index: number) {
  switch (part.type) {
    case "text":
      return (
        <div key={index} className="chat-part chat-part-text">
          {part.text}
        </div>
      );
    case "tool":
      return (
        <div key={index} className="chat-part chat-part-tool" data-status={part.status ?? "done"}>
          <div className="chat-part-tool-name">工具 · {part.name}</div>
          {part.inputSummary ? (
            <div className="chat-part-tool-body">{part.inputSummary}</div>
          ) : null}
          {part.outputSummary ? (
            <div className="chat-part-tool-body">{part.outputSummary}</div>
          ) : null}
        </div>
      );
    case "error":
      return (
        <div key={index} className="chat-part chat-part-error" role="alert">
          {part.message}
          {part.code ? <span className="chat-part-error-code"> ({part.code})</span> : null}
        </div>
      );
    default:
      return null;
  }
}

function MessageBody({ message }: { message: ChatMessage }) {
  const parts = message.parts ?? [];
  if (parts.length > 0) {
    return <>{parts.map((part, index) => renderPart(part, index))}</>;
  }
  if (message.errorSummary && message.status === "error") {
    return <>{message.content || `（调用失败）${message.errorSummary}`}</>;
  }
  return <>{message.content || (message.status === "streaming" ? "…" : "")}</>;
}

export interface ChatTranscriptProps {
  messages: ChatMessage[];
  emptyHint?: string;
}

export function ChatTranscript({ messages, emptyHint }: ChatTranscriptProps) {
  if (messages.length === 0) {
    return (
      <div className="chat-transcript chat-transcript-empty" data-testid="chat-messages">
        <div className="chat-messages-empty">
          {emptyHint ??
            "发送第一条消息。权限三档：探索（只读）/ 询问编辑（Phase 1 保守，无审批弹窗）/ 自动（可写）。Shift+Tab 循环切换。"}
        </div>
      </div>
    );
  }

  return (
    <div className="chat-transcript" data-testid="chat-messages">
      {messages.map((message) => (
        <div
          key={message.id}
          className={`chat-bubble ${message.role}${message.status === "error" ? " error" : ""}`}
          data-status={message.status}
        >
          <MessageBody message={message} />
        </div>
      ))}
    </div>
  );
}
