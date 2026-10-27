import { useCallback, useEffect, useMemo, useRef, useState, type FormEvent } from "react";
import { FolderTree, LoaderCircle, RotateCcw, ScrollText, Send, Square, Trash2 } from "lucide-react";
import { lastTurn, type Research, type ResearchApi, type ResearchLogStream, type ResearchTurn, type ResearchTurnContent } from "../../api/research";
import { parseSourceTarget, type SourceReader, type SourceTarget } from "../../api/sourceFiles";
import { useT } from "../../i18n";
import type { MessageKey } from "../../i18n/en";
import { Markdown, type MarkdownLinks } from "../../components/Markdown";
import { ErrorText } from "../../components/common/ErrorText";
import { SourceViewer } from "../../components/SourceViewer";

const RETRYABLE = ["failed", "cancelled", "interrupted"];

export function ResearchView({ api, researchId, revision, onDeleted }: { api: ResearchApi; researchId: string; revision: number; onDeleted: () => void }) {
  const t = useT();
  const [research, setResearch] = useState<Research | null>(null);
  const [error, setError] = useState("");
  const [source, setSource] = useState<SourceTarget | null>(null);
  const [sourceOpen, setSourceOpen] = useState(false);
  const [logsOpen, setLogsOpen] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [actionError, setActionError] = useState("");
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const turnsRef = useRef<HTMLOListElement>(null);
  useEffect(() => { setSource(null); setSourceOpen(false); setLogsOpen(false); setConfirmDelete(false); setDraft(""); setActionError(""); }, [researchId]);
  useEffect(() => {
    let disposed = false;
    api.get(researchId).then(result => { if (!disposed) { setResearch(result); setError(""); } }).catch(reason => { if (!disposed) setError(String(reason)); });
    return () => { disposed = true; };
  }, [api, researchId, revision]);
  // Opening a research or adding a question brings the newest question into view. Reports load after the
  // list renders, so the scroll repeats as each one lays out until the user scrolls themselves.
  const scrollPending = useRef(false);
  const turnCount = research?.researchId === researchId ? research.turns.length : 0;
  // Only the report column scrolls; scrollIntoView would also shift the overflow-hidden page and hide the header.
  const scrollToNewest = useCallback(() => {
    const newest = turnsRef.current?.lastElementChild, scroller = turnsRef.current?.parentElement;
    if (scrollPending.current && newest && scroller) scroller.scrollTop += newest.getBoundingClientRect().top - scroller.getBoundingClientRect().top - 16;
  }, []);
  useEffect(() => { scrollPending.current = turnCount > 0; scrollToNewest(); }, [researchId, turnCount, scrollToNewest]);
  const userScrolled = () => { scrollPending.current = false; };
  const openSource = useCallback((target: SourceTarget) => { setSource(target); setSourceOpen(true); }, []);
  const reader = useMemo<SourceReader>(() => ({
    readSource: (path, startLine, endLine) => api.readSource(researchId, path, startLine, endLine),
    listTree: dir => api.listTree(researchId, dir),
  }), [api, researchId]);
  if (error) return <div className="research-main-empty"><p className="research-error" role="alert">{error}</p></div>;
  if (!research || research.researchId !== researchId) return <div className="research-main-empty" role="status">{t("loading")}</div>;
  const last = lastTurn(research);
  const running = last?.status === "running";
  const retryable = !!last && RETRYABLE.includes(last.status);
  // Stale state between an action and its event is harmless: the backend rejects a second running question.
  async function act(work: () => Promise<unknown>) {
    setActionError(""); setBusy(true);
    try { await work(); return true; } catch (reason) { setActionError(String(reason)); return false; } finally { setBusy(false); }
  }
  async function ask(event: FormEvent) {
    event.preventDefault();
    const question = draft.trim();
    if (!question || running || busy) return;
    if (await act(async () => setResearch(await api.ask(researchId, question)))) setDraft("");
  }
  async function remove() {
    try { await api.remove(researchId); onDeleted(); } catch (reason) { setActionError(String(reason)); setConfirmDelete(false); }
  }
  return <div className="research-view" data-testid="research-view">
    <header className="research-head">
      <div className="research-title">
        <h1 title={research.title}>{research.title}</h1>
        <small>{research.agentName}{research.model ? ` · ${research.model}` : ""} · {research.projectName ?? t("researchNoProjectLabel")}</small>
      </div>
      <div className="research-actions">
        {running ? <button type="button" onClick={() => void act(() => api.cancel(researchId))}><Square size={13} aria-hidden="true" />{t("researchStop")}</button>
          : retryable && <button type="button" className="chat-primary-button" disabled={busy} onClick={() => void act(async () => setResearch(await api.retry(researchId)))}><RotateCcw size={13} aria-hidden="true" />{t("researchRetry")}</button>}
        {research.projectPath && <button type="button" className="chat-icon-button" aria-pressed={sourceOpen} aria-label={t("sourceBrowse")} title={t("sourceBrowse")} onClick={() => setSourceOpen(!sourceOpen)}><FolderTree size={15} /></button>}
        <button type="button" className="chat-icon-button" aria-pressed={logsOpen} aria-label={t("researchLogs")} title={t("researchLogs")} disabled={!last} onClick={() => setLogsOpen(!logsOpen)}><ScrollText size={15} /></button>
        <button type="button" className="chat-icon-button" aria-label={t("researchDelete")} title={t("researchDelete")} disabled={running} onClick={() => setConfirmDelete(true)}><Trash2 size={15} /></button>
      </div>
    </header>
    {confirmDelete && <div className="research-banner" role="alertdialog" aria-label={t("researchDelete")}>
      <span>{t("researchDeleteConfirm", { title: research.title })}</span>
      <button type="button" onClick={() => setConfirmDelete(false)}>{t("cancel")}</button>
      <button type="button" className="research-danger" onClick={() => void remove()}>{t("researchDelete")}</button>
    </div>}
    {actionError && <ErrorText text={actionError} alert className="research-error research-action-error" />}
    {logsOpen && last && <JobLogs api={api} researchId={researchId} jobId={last.jobId} revision={revision} />}
    <div className="research-body">
      <div className="research-scroll" onWheel={userScrolled} onTouchMove={userScrolled} onKeyDown={userScrolled}>
        <ol ref={turnsRef} className="research-turns" aria-live="polite">
          {research.turns.map(turn => <TurnView key={`${turn.index}:${turn.jobId}`} api={api} researchId={researchId} turn={turn} revision={revision} onSource={openSource} onLayout={scrollToNewest} />)}
        </ol>
        <form className="research-followup" onSubmit={ask}>
          <label className="research-sr" htmlFor="research-followup-input">{t("researchFollowUp")}</label>
          <textarea id="research-followup-input" rows={2} maxLength={8000} value={draft} disabled={running} placeholder={t("researchFollowUpPlaceholder")}
            onChange={event => setDraft(event.target.value)}
            onKeyDown={event => { if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) void ask(event); }} />
          <button type="submit" className="chat-primary-button" disabled={running || busy || !draft.trim()} aria-label={t("researchAsk")} title={t("researchAsk")}><Send size={14} aria-hidden="true" />{t("researchAsk")}</button>
        </form>
      </div>
      {sourceOpen && <SourceViewer reader={reader} target={source} onOpen={openSource} onClose={() => setSourceOpen(false)} />}
    </div>
  </div>;
}

function TurnView({ api, researchId, turn, revision, onSource, onLayout }: { api: ResearchApi; researchId: string; turn: ResearchTurn; revision: number; onSource: (target: SourceTarget) => void; onLayout: () => void }) {
  const t = useT();
  const [content, setContent] = useState<ResearchTurnContent | null>(null);
  const [error, setError] = useState("");
  const done = turn.status === "done";
  useEffect(() => {
    if (!done) { setContent(null); setError(""); return; }
    let disposed = false;
    api.readTurn(researchId, turn.index).then(result => { if (!disposed) { setContent(result); setError(""); } }).catch(reason => { if (!disposed) { setContent(null); setError(String(reason)); } });
    return () => { disposed = true; };
  }, [api, researchId, turn.index, turn.jobId, done, revision]);
  useEffect(() => { if (content) onLayout(); }, [content, onLayout]);
  const refStatus = useMemo(() => new Map(content?.refs.map(ref => [ref.target, ref.status]) ?? []), [content]);
  const links = useMemo<MarkdownLinks>(() => ({
    onSource: target => onSource(parseSourceTarget(target)),
    onWiki: () => undefined,
    sourceStatus: target => refStatus.get(target),
  }), [onSource, refStatus]);
  const invalid = content ? content.refs.filter(ref => ref.status !== "valid").length : 0;
  return <li className="research-turn" data-status={turn.status} data-testid="research-turn">
    <div className="research-question">
      <span className="research-turn-index">{t("researchTurn", { index: turn.index })}</span>
      <p>{turn.question}</p>
      {turn.context && <span className="research-chip">{t(`researchContext_${turn.context}` as MessageKey)}</span>}
      {turn.status !== "done" && turn.status !== "running" && <span className="research-chip" data-status={turn.status}>{t(`researchStatus_${turn.status}` as MessageKey)}</span>}
    </div>
    {turn.status === "running" && <p className="research-running" role="status"><LoaderCircle size={14} className="chat-spin" aria-hidden="true" />{t("researchRunning")}</p>}
    {turn.error && <ErrorText text={turn.error} className="research-error" />}
    {turn.status === "cancelled" && <p className="research-note">{t("researchCancelled")}</p>}
    {turn.warnings.length > 0 && <ul className="research-warnings">{turn.warnings.map(warning => <li key={warning}>{warning}</li>)}</ul>}
    {invalid > 0 && <p className="research-note">{t("researchInvalidRefs", { count: invalid })}</p>}
    {error && <p className="research-error" role="alert">{error}</p>}
    {content && <article className="research-report"><Markdown text={content.markdown} links={links} /></article>}
  </li>;
}

function JobLogs({ api, researchId, jobId, revision }: { api: ResearchApi; researchId: string; jobId: string; revision: number }) {
  const t = useT();
  const [stream, setStream] = useState<ResearchLogStream>("stdout");
  const [text, setText] = useState("");
  const [more, setMore] = useState(false);
  const [error, setError] = useState("");
  useEffect(() => {
    let disposed = false;
    api.readLogs(researchId, jobId, stream, 0).then(chunk => { if (!disposed) { setText(chunk.text); setMore(chunk.hasMore); setError(""); } })
      .catch(reason => { if (!disposed) setError(String(reason)); });
    return () => { disposed = true; };
  }, [api, researchId, jobId, stream, revision]);
  return <section className="research-logs" aria-label={t("researchLogs")}>
    <div role="tablist">{(["stdout", "stderr", "prompt"] as const).map(name => <button key={name} type="button" role="tab" aria-selected={stream === name} onClick={() => setStream(name)}>{name}</button>)}
      <code>{jobId}</code></div>
    {error ? <p className="research-error">{error}</p> : <pre>{text || t("researchLogsEmpty")}</pre>}
    {more && <p className="research-note">{t("researchLogsTruncated")}</p>}
  </section>;
}
