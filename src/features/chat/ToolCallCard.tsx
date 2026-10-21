import { useEffect, useState } from "react";
import { Check, ChevronRight, CircleX, LoaderCircle } from "lucide-react";
import type { ChatMessagePart } from "../../domain";
import { useT } from "../../i18n";
export function ToolCallCard({ part }: { part: Extract<ChatMessagePart, { type: "tool" }> }) {
  const running = part.status === "running";
  const [open, setOpen] = useState(running);
  const t = useT();
  useEffect(() => { setOpen(running); }, [running]);
  const status = t(running ? "toolRunning" : part.status === "error" ? "toolError" : "toolDone");
  return <div className="chat-tool-card" data-status={part.status ?? "done"} data-kind={part.kind ?? "other"}>
    <button type="button" aria-expanded={open} onClick={() => setOpen(!open)}>
      <span className="chat-tool-status" role="img" aria-label={status} title={status}>
        {running ? <LoaderCircle size={14} className="chat-spin" /> : part.status === "error" ? <CircleX size={14} /> : <Check size={14} strokeWidth={2.5} />}
      </span>
      <strong>{part.name}</strong><code>{part.target ?? part.inputSummary}</code>
      <span className="chat-tool-meta">
        {part.diffStats && <span className="chat-diff-stats"><b>+{part.diffStats.added}</b><i>−{part.diffStats.removed}</i></span>}
        {part.exitCode != null && <small data-failed={part.exitCode !== 0}>{t("exitCode", { code: part.exitCode })}</small>}
      </span>
      <ChevronRight size={14} className="chat-tool-chevron" aria-hidden="true" />
    </button>
    {open && <div className="chat-tool-body">
      {part.diffPreview ? <pre className="chat-diff">{part.diffPreview.split("\n").map((line, index) =>
        <span key={index} data-diff={line.startsWith("+") ? "added" : line.startsWith("-") ? "removed" : ""}>{line}</span>)}</pre>
        : <pre>{[part.inputSummary, part.outputSummary].filter(Boolean).join("\n")}</pre>}
    </div>}
  </div>;
}
