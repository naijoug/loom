import { Bot, Paperclip, Quote, Send, User, X } from "lucide-react";
import type { AgentConfig, CommandLogEvent } from "../../domain";
import { Button } from "../../components/common/Button";
import type { ConversationTurn, QuoteDraft } from "./model";

interface FeedbackComposerProps {
  readOnly: boolean;
  agents: AgentConfig[];
  selectedAgent: AgentConfig | null;
  autoMode: boolean;
  conversation: ConversationTurn[];
  logs: Record<string, CommandLogEvent[]>;
  quote: QuoteDraft | null;
  note: string;
  reproductionSteps: string;
  expectedBehavior: string;
  attachmentPaths: string[];
  onAgentChange: (agentId: string) => void;
  onModeChange: (auto: boolean) => void;
  onQuoteClear: () => void;
  onNoteChange: (value: string) => void;
  onReproductionStepsChange: (value: string) => void;
  onExpectedBehaviorChange: (value: string) => void;
  onSelectAttachments: () => void;
  onRemoveAttachment: (path: string) => void;
  onSubmit: () => void;
}

export function FeedbackComposer({
  readOnly,
  agents,
  selectedAgent,
  autoMode,
  conversation,
  logs,
  quote,
  note,
  reproductionSteps,
  expectedBehavior,
  attachmentPaths,
  onAgentChange,
  onModeChange,
  onQuoteClear,
  onNoteChange,
  onReproductionStepsChange,
  onExpectedBehaviorChange,
  onSelectAttachments,
  onRemoveAttachment,
  onSubmit,
}: FeedbackComposerProps) {
  const isEmpty =
    !note.trim() &&
    !reproductionSteps.trim() &&
    !expectedBehavior.trim() &&
    !quote?.text.trim() &&
    attachmentPaths.length === 0;

  return (
    <section className="debug-card testing-chat-card">
      <div className="testing-chat-head">
        <div className="debug-card-label"><Bot size={14} />调试 Agent</div>
        <div className="testing-chat-controls">
          <select
            className="testing-agent-select"
            value={selectedAgent?.id ?? ""}
            disabled={agents.length === 0 || readOnly}
            onChange={(event) => onAgentChange(event.target.value)}
          >
            {agents.length === 0 && <option value="">No agent</option>}
            {agents.map((agent) => <option value={agent.id} key={agent.id}>{agent.name}</option>)}
          </select>
          <div className="testing-mode-toggle" role="group" aria-label="Repair mode">
            <button type="button" className={autoMode ? "" : "active"} disabled={readOnly} onClick={() => onModeChange(false)}>
              手动
            </button>
            <button type="button" className={autoMode ? "active" : ""} disabled={readOnly} onClick={() => onModeChange(true)}>
              自动
            </button>
          </div>
        </div>
      </div>

      <div className="testing-chat-stream">
        {conversation.length === 0 ? (
          <p className="testing-chat-empty">选中终端日志并引用到这里，或直接写反馈给 Agent 修复。</p>
        ) : conversation.map((turn) => (
          <div className={`testing-chat-turn turn-${turn.role}`} key={`${turn.role}-${turn.id}`}>
            <div className="testing-chat-avatar">
              {turn.role === "human" ? <User size={13} /> : <Bot size={13} />}
            </div>
            <div className="testing-chat-bubble">
              {turn.role === "agent" && turn.run ? (
                <>
                  <div className="testing-chat-runline">
                    <span className={`testing-status-pill testing-status-${
                      turn.run.status === "running"
                        ? "running"
                        : turn.run.status === "succeeded"
                          ? "ok"
                          : turn.run.status === "failed" ? "err" : "idle"
                    }`}>
                      <span />{turn.run.status}
                    </span>
                    <span className="testing-chat-runcmd">fix run</span>
                  </div>
                  <pre className="testing-chat-runlog">
                    {(logs[turn.run.id] ?? []).slice(-12).map((entry) => entry.line).join("\n")
                      || "Agent started. Waiting for output…"}
                  </pre>
                </>
              ) : <pre className="testing-chat-text">{turn.content}</pre>}
            </div>
          </div>
        ))}
      </div>

      {quote && (
        <div className="testing-chat-quote">
          <Quote size={12} />
          <div className="testing-chat-quote-body">
            <span className="testing-chat-quote-cmd">{quote.command}</span>
            <pre>{quote.text.trim() || "(empty selection)"}</pre>
          </div>
          <button type="button" title="Remove quote" onClick={onQuoteClear}><X size={12} /></button>
        </div>
      )}

      {!readOnly && (
        <div className="testing-chat-composer">
          <textarea value={note} onChange={(event) => onNoteChange(event.target.value)} placeholder="告诉 Agent 哪里不对，也可以从终端引用日志定位问题…" />
          <textarea className="testing-chat-structured-field" value={reproductionSteps} onChange={(event) => onReproductionStepsChange(event.target.value)} placeholder="复现步骤（可选）" />
          <textarea className="testing-chat-structured-field" value={expectedBehavior} onChange={(event) => onExpectedBehaviorChange(event.target.value)} placeholder="期望行为（可选）" />
          <div className="testing-chat-attachments">
            <button type="button" onClick={onSelectAttachments}><Paperclip size={13} /> 添加截图或文件</button>
            {attachmentPaths.map((path) => (
              <span key={path} title={path}>
                {path.split(/[\\/]/).pop()}
                <button type="button" aria-label="Remove attachment" onClick={() => onRemoveAttachment(path)}><X size={10} /></button>
              </span>
            ))}
          </div>
          <Button
            type="button"
            variant="primary"
            iconRight={<Send size={14} />}
            disabled={isEmpty || !selectedAgent}
            onClick={onSubmit}
          >
            打回修复
          </Button>
        </div>
      )}
    </section>
  );
}
