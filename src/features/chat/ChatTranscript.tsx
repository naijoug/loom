import type { ChatMessage, ChatTurn } from "../../domain";
import { useT } from "../../i18n";
import { ChatRunDetails } from "./ChatRunDetails";
import { Markdown } from "./MarkdownView";
import { ToolCallCard } from "./ToolCallCard";
import { chatMessageErrorFallbackText } from "../../domain";

export interface ChatTranscriptProps {
  messages: ChatMessage[]; emptyHint?: string; turns?: ChatTurn[];
  projectPath?: string; sessionId?: string; toolEvents?: boolean;
}
export function ChatTranscript({ messages, emptyHint, turns, projectPath, sessionId, toolEvents = true }: ChatTranscriptProps) {
  const t = useT();
  return <div className="chat-transcript" data-testid="chat-messages"><div className="chat-transcript-inner">
    {!messages.length && <p className="chat-transcript-empty">{emptyHint ?? t("emptyHint")}</p>}
    {messages.map(message => {
      const turn = turns?.find(turn => turn.assistantMessageId === message.id);
      const logs = turn && projectPath && sessionId ? <ChatRunDetails projectPath={projectPath} sessionId={sessionId} turn={turn} /> : null;
      const parts = message.parts ?? [];
      const tools = toolEvents ? parts.filter(part => part.type === "tool") : [];
      const error = chatMessageErrorFallbackText(message);
      const stoppedEmpty = message.status === "aborted" && !message.content;
      return <article key={message.id} className={message.role === "user" ? "chat-user-row" : "chat-turn"} data-status={message.status} data-role={message.role}>
        <div className={message.role === "user" ? "chat-user-bubble" : "chat-turn-body"}>
          {message.role === "user" ? message.content : <>
            {message.content && <Markdown text={message.content} />}
            {!message.content && parts.filter(part => part.type === "text").map((part, index) => part.type === "text" && <Markdown key={index} text={part.text} />)}
            {tools.length > 0 && <div className="chat-tool-group">{tools.map((part, index) => part.type === "tool" ? <ToolCallCard key={part.callId ?? index} part={part} /> : null)}</div>}
            {parts.map((part, index) => part.type === "error" ? <p className="chat-error" role="alert" key={index}>{part.message}{part.code ? ` (${part.code})` : ""}</p> : null)}
            {error && <div role="alert" className="chat-error">{error}</div>}
            {stoppedEmpty && <p className="chat-stopped">{t("stopped")}</p>}
            {message.status === "streaming" && <span className="chat-streaming" role="status" aria-label={t("running")}><i /><i /><i /></span>}
            {logs}
          </>}
        </div>
      </article>;
    })}
  </div></div>;
}
