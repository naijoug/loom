import { useCallback, useEffect, useMemo, useState, type FormEvent } from "react";
import { FolderTree, LoaderCircle, RotateCcw, Send, Square, Trash2 } from "lucide-react";
import type { Research, ResearchApi } from "../../api/research";
import { parseSourceTarget, type SourceReader, type SourceTarget, type TreeEntry } from "../../api/sourceFiles";
import { researchRequestId, targetResearchApi, type EvidenceRecord, type Material, type TargetExport, type TargetRun, type TargetStatus } from "../../api/researchTargets";
import { useT } from "../../i18n";
import type { MessageKey } from "../../i18n/en";
import { Markdown, type MarkdownLinks } from "../../components/Markdown";
import { ErrorText } from "../../components/common/ErrorText";
import { SourceViewer } from "../../components/SourceViewer";

const ACTIVE = new Set<TargetStatus>(["queued", "preflight", "collecting", "analyzing", "reporting", "cancelling"]);
const PHASE: Record<string, MessageKey> = {
  queued: "researchPhase_queued", preflight: "researchPhase_preflight", collecting: "researchPhase_collecting",
  analyzing: "researchPhase_analyzing", reporting: "researchPhase_reporting", cancelling: "researchPhase_cancelling",
  completed: "researchPhase_completed", cancelled: "researchPhase_cancelled", failed: "researchPhase_failed",
  interrupted: "researchPhase_interrupted", awaiting_user: "researchPhase_awaiting_user",
};

function targetText(research: Research) {
  const target = research.target?.target;
  if (!target) return "";
  if (target.kind === "github") return target.gitRef ? `${target.url} @ ${target.gitRef}` : target.url;
  return target.kind === "website" ? target.url : target.path;
}

export function TargetResearchView({ research, targets, canStart, legacy, revision, onDeleted }: {
  research: Research; targets: typeof targetResearchApi; canStart: boolean; legacy: ResearchApi; revision: number; onDeleted: () => void;
}) {
  const t = useT();
  const [runs, setRuns] = useState<TargetRun[]>([]);
  const [error, setError] = useState("");
  const [reportId, setReportId] = useState<string | null>(null);
  const [markdown, setMarkdown] = useState("");
  const [hasSnapshot, setHasSnapshot] = useState(false);
  const [evidence, setEvidence] = useState<EvidenceRecord[]>([]);
  const [evidenceId, setEvidenceId] = useState<string | null>(null);
  const [evidenceText, setEvidenceText] = useState("");
  const [evidenceCursor, setEvidenceCursor] = useState<number | null>(null);
  const [source, setSource] = useState<SourceTarget | null>(null);
  const [sourceOpen, setSourceOpen] = useState(false);
  const [paths, setPaths] = useState<string[]>([]);
  const [treeNote, setTreeNote] = useState("");
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const [actionError, setActionError] = useState("");
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [includeRaw, setIncludeRaw] = useState(false);
  const [exported, setExported] = useState<TargetExport | null>(null);
  const researchId = research.researchId;
  const reload = useCallback(() => {
    setError("");
    return targets.listRuns(researchId).then(setRuns);
  }, [targets, researchId]);
  useEffect(() => { setReportId(null); setSource(null); setSourceOpen(false); setEvidenceId(null); setDraft(""); setConfirmDelete(false); setExported(null); setActionError(""); }, [researchId]);
  useEffect(() => { let disposed = false; reload().catch(reason => { if (!disposed) setError(String(reason)); }); return () => { disposed = true; }; }, [reload, revision]);
  useEffect(() => {
    let disposed = false;
    const stop = targets.subscribe(() => { if (!disposed) void reload().catch(reason => { if (!disposed) setActionError(String(reason)); }); });
    return () => { disposed = true; void stop.then(unlisten => unlisten()); };
  }, [targets, reload]);
  const choices = runs.flatMap(run => run.state.reportIds.map(id => ({ reportId: id, runId: run.spec.runId })));
  const choiceKey = choices.map(item => item.reportId).join("\n");
  useEffect(() => {
    const ids = choiceKey ? choiceKey.split("\n") : [];
    setReportId(ids.length ? ids[ids.length - 1] : null);
  }, [choiceKey]);
  useEffect(() => {
    if (!reportId) { setMarkdown(""); setEvidence([]); setHasSnapshot(false); return; }
    let disposed = false;
    setExported(null);
    targets.report(researchId, reportId).then(result => { if (!disposed) { setMarkdown(result.markdown); setHasSnapshot(!!result.report.defaultSourceSnapshotId); } }).catch(reason => { if (!disposed) setActionError(String(reason)); });
    targets.evidence(researchId, reportId).then(records => { if (!disposed) setEvidence(records); }).catch(reason => { if (!disposed) setActionError(String(reason)); });
    setPaths([]); setTreeNote("");
    const loadTree = async () => {
      const found: string[] = [];
      let offset = 0;
      for (let page = 0; page < 5; page += 1) {
        const material = await targets.tree(researchId, reportId, offset, 200);
        found.push(...material.text.split("\n").map(line => line.trim()).filter(Boolean));
        if (material.nextCursor == null) { if (!disposed) { setPaths(found); setTreeNote(""); } return; }
        offset = material.nextCursor;
      }
      if (!disposed) { setPaths(found); setTreeNote("truncated"); }
    };
    void loadTree().catch(reason => { if (!disposed && !String(reason).includes("report_has_no_source_snapshot")) setTreeNote(String(reason)); });
    return () => { disposed = true; };
  }, [targets, researchId, reportId]);
  const active = runs.find(run => ACTIVE.has(run.state.status));
  const latest = runs.length ? runs[runs.length - 1] : undefined;
  const control = active ?? latest;
  const reader = useMemo<SourceReader>(() => ({
    readSource: async (path, startLine, endLine) => {
      if (!reportId) throw Error("report_required");
      const start = startLine ?? 1;
      const end = endLine ?? start;
      const material = await targets.source(researchId, reportId, path, start, end);
      const lines = material.text.replace(/\n$/, "").split("\n");
      return { path, text: material.text, firstLine: start, totalLines: start + Math.max(lines.length, 1) - 1, truncated: material.truncated, language: "Text" };
    },
    listTree: (dir = "") => Promise.resolve(treeLevel(paths, dir)),
  }), [targets, researchId, reportId, paths]);
  useEffect(() => {
    if (!evidenceId || !reportId) return;
    let disposed = false;
    targets.readEvidence(researchId, reportId, evidenceId, 0, 12000).then(material => {
      if (!disposed) { setEvidenceText(material.text); setEvidenceCursor(material.nextCursor); }
    }).catch(reason => { if (!disposed) setActionError(String(reason)); });
    return () => { disposed = true; };
  }, [targets, researchId, reportId, evidenceId]);
  const links = useMemo<MarkdownLinks>(() => ({
    onSource: target => { setSource(parseSourceTarget(target)); setSourceOpen(true); },
    onWiki: () => undefined,
    onEvidence: id => setEvidenceId(id),
  }), []);
  async function act(work: () => Promise<unknown>) {
    setActionError(""); setBusy(true);
    try { await work(); await reload(); return true; }
    catch (reason) { setActionError(String(reason)); return false; }
    finally { setBusy(false); }
  }
  async function ask(event: FormEvent) {
    event.preventDefault();
    const question = draft.trim();
    if (!question || !reportId || active || busy || !canStart) return;
    if (await act(() => targets.ask(researchId, reportId, question, researchRequestId()))) setDraft("");
  }
  async function remove() {
    try { await legacy.remove(researchId); onDeleted(); }
    catch (reason) { setActionError(String(reason)); setConfirmDelete(false); }
  }
  async function openEvidence(id: string, offset = 0) {
    if (!reportId) return;
    setEvidenceId(id);
    const material: Material = await targets.readEvidence(researchId, reportId, id, offset, 12000);
    setEvidenceText(current => offset === 0 ? material.text : `${current}${material.text}`);
    setEvidenceCursor(material.nextCursor);
  }
  async function exportReport() {
    if (!reportId) return;
    setActionError(""); setBusy(true);
    try { setExported(await targets.export(researchId, reportId, [], includeRaw)); }
    catch (reason) { setActionError(String(reason)); }
    finally { setBusy(false); }
  }
  const usage = control?.state.usage;
  const kind = research.target?.target.kind;
  return <div className="research-view" data-testid="research-view" data-workflow="target">
    <header className="research-head">
      <div className="research-title">
        <h1 title={research.title}>{research.title}</h1>
        <small>{research.agentName}{research.model ? ` · ${research.model}` : ""}{kind ? ` · ${t(kind === "github" ? "researchTargetGithub" : kind === "website" ? "researchTargetWebsite" : "researchTargetApp")}` : ""} · {targetText(research)}</small>
      </div>
      <div className="research-actions">
        {active ? <button type="button" onClick={() => void act(() => targets.cancel(researchId, active.spec.runId))}><Square size={13} aria-hidden="true" />{t("researchStop")}</button>
          : canStart && control && (control.state.status === "failed" || control.state.status === "cancelled") && <button type="button" className="chat-primary-button" disabled={busy} onClick={() => void act(() => targets.retry(researchId, control.spec.runId, researchRequestId()))}><RotateCcw size={13} aria-hidden="true" />{t("researchRetry")}</button>}
        {canStart && !active && control && (control.state.status === "interrupted" || control.state.status === "awaiting_user") && <button type="button" className="chat-primary-button" disabled={busy} onClick={() => void act(() => targets.resume(researchId, control.spec.runId, researchRequestId()))}>{t("researchResume")}</button>}
        <button type="button" className="chat-icon-button" aria-pressed={sourceOpen} aria-label={t("sourceBrowse")} title={hasSnapshot ? t("sourceBrowse") : t("researchNoSnapshot")} disabled={!hasSnapshot} onClick={() => setSourceOpen(!sourceOpen)}><FolderTree size={15} /></button>
        <button type="button" className="chat-icon-button" aria-label={t("researchDelete")} title={t("researchDelete")} disabled={!!active} onClick={() => setConfirmDelete(true)}><Trash2 size={15} /></button>
      </div>
    </header>
    {confirmDelete && <div className="research-banner" role="alertdialog" aria-label={t("researchDelete")}>
      <span>{t("researchDeleteConfirm", { title: research.title })}</span>
      <button type="button" onClick={() => setConfirmDelete(false)}>{t("cancel")}</button>
      <button type="button" className="research-danger" onClick={() => void remove()}>{t("researchDelete")}</button>
    </div>}
    {!canStart && <p className="research-note research-action-error">{t("researchTargetOff")}</p>}
    {(error || actionError) && <ErrorText text={error || actionError} alert className="research-error research-action-error" />}
    <div className="research-body">
      <div className="research-scroll">
        {control && <p className="research-running" role="status" data-status={control.state.status}>
          {active && <LoaderCircle size={14} className="chat-spin" aria-hidden="true" />}
          {PHASE[control.state.status] ? t(PHASE[control.state.status]) : control.state.status}
          {usage && <> · {t("researchUsage", { rounds: usage.rounds, calls: usage.calls, collections: usage.collections, seconds: Math.round(usage.activeMs / 1000) })}</>}
        </p>}
        {control?.state.stopReason && <p className="research-note">{control.state.stopReason}</p>}
        {control?.state.error && <ErrorText text={control.state.error} className="research-error" />}
        {choices.length > 1 && <label className="research-report-select">{t("researchReport")}
          <select aria-label={t("researchReport")} value={reportId ?? ""} onChange={event => { setReportId(event.target.value); setEvidenceId(null); setSource(null); }}>
            {choices.map(item => <option key={item.reportId} value={item.reportId}>{item.reportId}</option>)}
          </select>
        </label>}
        {choices.length > 1 && <p className="research-note">{t("researchHistorical")}</p>}
        {treeNote === "truncated" ? <p className="research-note">{t("researchEvidenceMore")}</p> : treeNote && <p className="research-note">{treeNote}</p>}
        {markdown && <article className="research-report" data-report-id={reportId ?? ""}><Markdown text={markdown} links={links} /></article>}
        <section className="research-evidence" aria-label={t("researchEvidence")}>
          <h2>{t("researchEvidence")}</h2>
          {!evidence.length && <p className="research-note">{t("researchEvidenceEmpty")}</p>}
          <ul>{evidence.map(record => <li key={record.evidenceId}>
            <button type="button" className="research-evidence-item" aria-current={evidenceId === record.evidenceId} onClick={() => setEvidenceId(record.evidenceId)}>
              <strong>{record.tool}</strong><span>{record.coverage}</span>
              {record.limitations.length > 0 && <small>{record.limitations.join(" ")}</small>}
            </button>
            {evidenceId === record.evidenceId && <div>
              {/screenshot/i.test(record.tool) && <p className="research-note">{t("researchScreenshotNote")}</p>}
              <pre>{evidenceText}</pre>
              {evidenceCursor != null && <button type="button" onClick={() => void openEvidence(record.evidenceId, evidenceCursor).catch(reason => setActionError(String(reason)))}>{t("researchEvidenceMore")}</button>}
            </div>}
          </li>)}</ul>
        </section>
        <section className="research-export" aria-label={t("researchExport")}>
          <label><input type="checkbox" checked={includeRaw} onChange={event => setIncludeRaw(event.target.checked)} />{t("researchExportRaw")}</label>
          {includeRaw && <p className="research-note">{t("researchExportRawWarning")}</p>}
          <button type="button" disabled={!reportId || busy} onClick={() => void exportReport().catch(reason => setActionError(String(reason)))}>{t("researchExport")}</button>
          {exported && <p className="research-note">{t("researchExportDone", { directory: exported.directory })}{exported.warning ? ` ${exported.warning}` : ""}</p>}
        </section>
        {canStart && <div className="research-followup-block">
          <p className="research-note">{t("researchFollowUpReadonly")}</p>
          <form className="research-followup" onSubmit={ask}>
          <label className="research-sr" htmlFor="research-followup-input">{t("researchFollowUp")}</label>
          <textarea id="research-followup-input" rows={2} maxLength={8000} value={draft} disabled={!!active || !reportId} placeholder={t("researchFollowUpPlaceholder")}
            onChange={event => setDraft(event.target.value)}
            onKeyDown={event => { if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) void ask(event); }} />
          <button type="submit" className="chat-primary-button" disabled={!!active || busy || !draft.trim() || !reportId} aria-label={t("researchAsk")} title={t("researchAsk")}><Send size={14} aria-hidden="true" />{t("researchAsk")}</button>
        </form></div>}
      </div>
      {sourceOpen && <SourceViewer reader={reader} target={source} onOpen={next => setSource(next)} onClose={() => setSourceOpen(false)} />}
    </div>
  </div>;
}

function treeLevel(paths: string[], dir: string): TreeEntry[] {
  const prefix = dir ? `${dir}/` : "";
  const names = new Map<string, TreeEntry>();
  for (const path of paths) {
    if (prefix && !path.startsWith(prefix)) continue;
    const rest = path.slice(prefix.length);
    const [name, ...more] = rest.split("/");
    if (!name) continue;
    const child = `${prefix}${name}`;
    if (more.length) names.set(name, { name, path: child, kind: "dir", size: 0 });
    else if (!names.has(name)) names.set(name, { name, path: child, kind: "file", size: 0 });
  }
  return [...names.values()].sort((a, b) => Number(a.kind !== "dir") - Number(b.kind !== "dir") || a.name.localeCompare(b.name));
}
