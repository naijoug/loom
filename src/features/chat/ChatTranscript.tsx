import { useState } from "react";
import type { ChatMessage, ChatMessagePart, ChatTurn } from "../../domain";
import { ChatRunDetails } from "./ChatRunDetails";
import { chatMessageErrorFallbackText } from "../../domain";

function ToolActivityRow({ part }: { part: Extract<ChatMessagePart, { type: "tool" }> }) {
  const [open, setOpen] = useState(false);
  const status = part.status ?? "done";
  const detail = [part.inputSummary, part.outputSummary].filter(Boolean).join("\n");
  return (
    <div className="chat-activity" data-status={status}>
      <button
        type="button"
        className="chat-activity-row"
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
      >
        <span className="chat-activity-chevron" aria-hidden="true">
          {open ? "▾" : "▸"}
        </span>
        <span className={`chat-activity-icon status-${status}`} aria-hidden="true" />
        <span className="chat-activity-name">{part.name}</span>
        <span className="chat-activity-status">{status}</span>
      </button>
      {open && detail ? <pre className="chat-activity-detail">{detail}</pre> : null}
    </div>
  );
}

function renderPart(part: ChatMessagePart, index: number) {
  switch (part.type) {
    case "text":
      return (
        <div key={index} className="chat-part chat-part-text">
          {part.text}
        </div>
      );
    case "tool":
      return <ToolActivityRow key={index} part={part} />;
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
  const errorFallback = chatMessageErrorFallbackText(message);
  if (parts.length > 0) {
    const hasTextPart = parts.some((part) => part.type === "text");
    return (
      <>
        {!hasTextPart && message.content ? (
          <div className="chat-part chat-part-text">{message.content}</div>
        ) : null}
        {parts.map((part, index) => renderPart(part, index))}
        {errorFallback ? (
          <div className="chat-part chat-part-error" role="alert">
            {errorFallback}
          </div>
        ) : null}
      </>
    );
  }
  if (message.errorSummary && message.status === "error") {
    return <>{message.content || errorFallback}</>;
  }
  if (message.status === "aborted" && !message.content) {
    return <>（已停止）</>;
  }
  return (
    <>
      {message.content || (message.status === "streaming" ? "…" : "")}
      {message.status === "streaming" ? (
        <span className="chat-streaming-dots" aria-hidden="true">
          <i />
          <i />
          <i />
        </span>
      ) : null}
    </>
  );
}

export interface ChatTranscriptProps {
  messages: ChatMessage[];
  emptyHint?: string;
  turns?: ChatTurn[];
  projectPath?: string;
  sessionId?: string;
}

/**
 * Layout mirrors craft-agents-oss ChatDisplay + TurnCard / UserMessageBubble:
 * user bubbles right-aligned; assistant turns as open cards with collapsible tool rows.
 */
export function ChatTranscript({ messages, emptyHint, turns, projectPath, sessionId }: ChatTranscriptProps) {
  if (messages.length === 0) {
    return (
      <div className="chat-transcript chat-transcript--empty" data-testid="chat-messages">
        <div className="chat-transcript-empty-card">
          {emptyHint ??
            "发送第一条消息。权限模式由所选 Agent 执行；询问编辑会在发送前确认本回合授权。"}
        </div>
      </div>
    );
  }

  return (
    <div className="chat-transcript" data-testid="chat-messages">
      <div className="chat-transcript-inner">
        {messages.map((message) => {
          const turn = turns?.find((turn) => turn.assistantMessageId === message.id);
          if (message.role === "user") {
            return (
              <div key={message.id} className="chat-user-row">
                <div className="chat-user-bubble" data-status={message.status}>
                  <MessageBody message={message} />
                </div>
              </div>
            );
          }
          return (
            <article
              key={message.id}
              className={`chat-turn${message.status === "error" ? " is-error" : ""}${message.status === "streaming" ? " is-streaming" : ""}`}
              data-status={message.status}
              data-role={message.role}
            >
              <header className="chat-turn-header">
                <span className="chat-turn-label">
                  {message.role === "assistant" ? "Agent" : message.role}
                </span>
                {message.status === "streaming" ? (
                  <span className="chat-turn-live">生成中</span>
                ) : null}
              </header>
              <div className="chat-turn-body">
                <MessageBody message={message} />
              </div>
              {turn && projectPath && sessionId && <ChatRunDetails key={JSON.stringify([projectPath, sessionId, turn.id])} projectPath={projectPath} sessionId={sessionId} turn={turn} />}
            </article>
          );
        })}
      </div>
    </div>
  );
}
