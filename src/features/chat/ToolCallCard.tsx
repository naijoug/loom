import { useEffect, useState, type ReactNode } from "react";
import type { ChatMessagePart } from "../../domain";
import { useT } from "../../i18n";
const icons: Record<string, string> = { read: "≡", edit: "±", write: "+", bash: "›_", search: "⌕", web: "◎", other: "◇" };
export function ToolCallCard({ part, logs }: { part: Extract<ChatMessagePart, { type: "tool" }>; logs?: ReactNode }) {
  const running = part.status === "running";
  const [open, setOpen] = useState(running);
  const t = useT();
  useEffect(() => { setOpen(running); }, [running]);
  return <div className="chat-tool-card" data-status={part.status ?? "done"} data-kind={part.kind ?? "other"}>
    <button type="button" aria-expanded={open} onClick={() => setOpen(!open)}>
      <span aria-hidden="true">{open ? "▾" : "▸"} {icons[part.kind ?? "other"] ?? icons.other}</span>
      <strong>{part.name}</strong><code>{part.target ?? part.inputSummary}</code>
      {part.diffStats && <span className="chat-diff-stats"><b>+{part.diffStats.added}</b><i>−{part.diffStats.removed}</i></span>}
      {part.exitCode != null && <small>{t("exitCode", { code: part.exitCode })}</small>}
      <small>{t(running ? "toolRunning" : part.status === "error" ? "toolError" : "toolDone")}</small>
    </button>
    {open && <div className="chat-tool-body">
      {part.diffPreview ? <pre className="chat-diff">{part.diffPreview.split("\n").map((line, index) =>
        <span key={index} data-diff={line.startsWith("+") ? "added" : line.startsWith("-") ? "removed" : ""}>{line}{"\n"}</span>)}</pre>
        : <pre>{[part.inputSummary, part.outputSummary].filter(Boolean).join("\n")}</pre>}
      {logs}
    </div>}
  </div>;
}
