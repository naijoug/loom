import { useT } from "../../i18n";
import { useCallback, useEffect, useRef, useState } from "react";
import { chatReadRunLogs } from "../../api/chatClient";
import type { ChatTurn } from "../../domain/chat";



export function ChatRunDetails({ projectPath, sessionId, turn, readLogs = chatReadRunLogs }: {
  projectPath: string; sessionId: string; turn: ChatTurn; readLogs?: typeof chatReadRunLogs;
}) {
  const t = useT();
  const labels = { starting: t("runStarting"), running: t("running"), cancelling: t("runCancelling"), completed: t("runCompleted"), failed: t("runFailed"), cancelled: t("stopped"), timed_out: t("runTimedOut"), interrupted: t("runInterrupted") };
  const [open, setOpen] = useState(false);
  const [stream, setStream] = useState<"stdout" | "stderr">("stdout");
  const [text, setText] = useState("");
  const [offset, setOffset] = useState(0);
  const [more, setMore] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const generation = useRef(0);
  const load = useCallback(async (cursor: number) => {
    const request = ++generation.current;
    setLoading(true); setError(null);
    if (cursor === 0) { setText(""); setOffset(0); setMore(false); }
    try {
      const page = await readLogs(projectPath, sessionId, turn.id, stream, cursor);
      if (generation.current !== request) return;
      setText((previous) => cursor === 0 ? page.text : previous + page.text);
      setOffset(page.nextOffset); setMore(page.hasMore);
    } catch (err) { if (generation.current === request) setError(String(err)); }
    finally { if (generation.current === request) setLoading(false); }
  }, [projectPath, sessionId, turn.id, stream, readLogs]);
  useEffect(() => {
    if (open) void load(0);
    return () => { generation.current++; };
  }, [open, load]);
  const elapsed = turn.finishedAtMs == null ? null : Math.max(0, turn.finishedAtMs - (turn.startedAtMs ?? turn.acceptedAtMs));
  return <div className="chat-run-details">
    <button type="button" className="chat-link-btn" aria-expanded={open} onClick={() => setOpen(!open)}>
      {labels[turn.status]} · {t("logs")}{elapsed == null ? "" : ` · ${t("seconds", { value: (elapsed / 1000).toFixed(1) })}`}
    </button>
    {open && <section aria-label={t("turnLogs")}  className="chat-run-panel">
      <dl>
        <dt>{t("program")}</dt><dd><code>{turn.invocation.program}</code></dd>
        <dt>{t("redactedArguments")}</dt><dd><pre>{JSON.stringify(turn.invocation.args, null, 2)}</pre></dd>
        <dt>{t("cwd")}</dt><dd><code>{turn.invocation.cwd}</code></dd>
        <dt>{t("permissionAgent")}</dt><dd>{turn.invocation.permissionMode} / {turn.invocation.agentId}</dd>
        <dt>{t("acceptedAt")}</dt><dd>{new Date(turn.acceptedAtMs).toLocaleString()}</dd>
        <dt>{t("startedAt")}</dt><dd>{turn.startedAtMs == null ? t("notStarted") : new Date(turn.startedAtMs).toLocaleString()}</dd>
        <dt>{t("finishedAt")}</dt><dd>{turn.finishedAtMs == null ? t("notFinished") : new Date(turn.finishedAtMs).toLocaleString()}</dd>
        <dt>{t("exitCodeLabel")}</dt><dd>{turn.exitCode ?? t("noExitCode")}</dd>
        {turn.terminationReason && <><dt>{t("termination")}</dt><dd>{turn.terminationReason}</dd></>}
      </dl>
      {turn.errorSummary && <p role="alert">{turn.errorSummary}</p>}
      <div className="chat-run-controls" role="group" aria-label={t("logs")} >
        <button type="button" aria-pressed={stream === "stdout"} onClick={() => setStream("stdout")}>stdout</button>
        <button type="button" aria-pressed={stream === "stderr"} onClick={() => setStream("stderr")}>stderr</button>
        <button type="button" disabled={loading} onClick={() => void load(0)}>{t("reload")}</button>
      </div>
      {error && <p role="alert">{error}</p>}
      <pre className="chat-run-log" aria-label={`${stream} ${t("logs")}`}>{text || (loading ? t("reading") : error ? t("logReadError") : t("noOutput"))}</pre>
      {more && <button type="button" disabled={loading} onClick={() => void load(offset)}>{loading ? t("reading") : t("loadMore")}</button>}
    </section>}
  </div>;
}
