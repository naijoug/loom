import { useCallback, useEffect, useMemo, useState } from "react";
import { CircleAlert, CircleCheck, CircleDashed, ExternalLink, FileText, FolderTree, LoaderCircle, Play, ScrollText, Square, Trash2 } from "lucide-react";
import { parseSourceTarget, type LearnApi, type LearnLogStream, type LearnPageContent, type LearnPageInfo, type LearnStageState, type LearnStudyDetail } from "../../api/learn";
import { useT } from "../../i18n";
import type { MessageKey } from "../../i18n/en";
import { Markdown, type MarkdownLinks } from "../chat/MarkdownView";
import { openExternalLink } from "../chat/externalLink";
import { ArchitectureDiagram } from "./ArchitectureDiagram";
import { ErrorText } from "./ErrorText";
import { SourceViewer, type SourceTarget } from "./SourceViewer";

const PIPELINE = ["fetch", "inventory", "architecture"] as const;
const STAGE_LABEL: Record<string, MessageKey> = { fetch: "learnStageFetch", inventory: "learnStageInventory", architecture: "learnStageArchitecture", glossary: "learnStageGlossary" };
const LEVEL_LABEL: Record<number, MessageKey> = { 1: "learnLevel1", 2: "learnLevel2", 3: "learnLevel3" };

export function StudyView({ api, studyId, revision, onDeleted }: { api: LearnApi; studyId: string; revision: number; onDeleted: () => void }) {
  const t = useT();
  const [detail, setDetail] = useState<LearnStudyDetail | null>(null);
  const [error, setError] = useState("");
  const [pageId, setPageId] = useState("overview");
  const [page, setPage] = useState<LearnPageContent | null>(null);
  const [pageError, setPageError] = useState("");
  const [source, setSource] = useState<SourceTarget | null>(null);
  const [sourceOpen, setSourceOpen] = useState(false);
  const [logsOpen, setLogsOpen] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [actionError, setActionError] = useState("");
  useEffect(() => { setPageId("overview"); setSource(null); setSourceOpen(false); setLogsOpen(false); setConfirmDelete(false); }, [studyId]);
  useEffect(() => {
    let disposed = false;
    api.get(studyId).then(result => { if (!disposed) { setDetail(result); setError(""); } }).catch(reason => { if (!disposed) setError(String(reason)); });
    return () => { disposed = true; };
  }, [api, studyId, revision]);
  const study = detail?.study;
  const info = study?.pages.find(item => item.pageId === pageId);
  const ready = !!info?.ready;
  useEffect(() => {
    if (!ready) { setPage(null); setPageError(""); return; }
    let disposed = false;
    api.readPage(studyId, pageId).then(result => { if (!disposed) { setPage(result); setPageError(""); } }).catch(reason => { if (!disposed) { setPage(null); setPageError(String(reason)); } });
    return () => { disposed = true; };
  }, [api, studyId, pageId, ready, revision]);
  const openSource = useCallback((target: SourceTarget) => { setSource(target); setSourceOpen(true); }, []);
  const refStatus = useMemo(() => new Map(page?.refs.map(ref => [ref.target, ref.status]) ?? []), [page]);
  const links = useMemo<MarkdownLinks>(() => ({
    onSource: target => openSource(parseSourceTarget(target)),
    onWiki: target => setPageId(target.replace(/^module\//, "module:")),
    sourceStatus: target => refStatus.get(target),
  }), [openSource, refStatus]);
  if (error) return <div className="learn-main-empty"><p className="learn-error" role="alert">{error}</p></div>;
  if (!study) return <div className="learn-main-empty" role="status">{t("loading")}</div>;
  const running = study.stages.find(stage => stage.status === "running");
  const architectureDone = study.stages.some(stage => stage.stage === "architecture" && stage.status === "done");
  const missing = study.pages.filter(item => !item.ready && (item.kind === "module" || item.kind === "glossary")).length;
  const latestJob = running?.jobId ?? [...study.stages].filter(stage => stage.jobId).sort((a, b) => (b.startedAtMs ?? 0) - (a.startedAtMs ?? 0))[0]?.jobId;
  const failedPipeline = PIPELINE.find(stage => ["failed", "cancelled", "interrupted"].includes(study.stages.find(item => item.stage === stage)?.status ?? ""));
  async function generate(stage: string) {
    setActionError("");
    try { await api.generate(studyId, stage); } catch (reason) { setActionError(String(reason)); }
  }
  async function remove() {
    try { await api.remove(studyId); onDeleted(); } catch (reason) { setActionError(String(reason)); setConfirmDelete(false); }
  }
  const stageLabel = (stage: string) => STAGE_LABEL[stage] ? t(STAGE_LABEL[stage]) : study.pages.find(item => item.pageId === stage)?.title ?? stage;
  return <div className="learn-study" data-testid="learn-study">
    <header className="learn-study-head">
      <div className="learn-study-title">
        <h1>{study.owner}/{study.repo}</h1>
        <small>{study.commit ? study.commit.slice(0, 10) : study.gitRef ?? "HEAD"} · {study.agentName}{study.model ? ` · ${study.model}` : ""}</small>
      </div>
      <div className="learn-study-actions">
        {running ? <button type="button" onClick={() => void api.cancel(studyId)}><Square size={13} aria-hidden="true" />{t("cancel")}</button>
          : failedPipeline ? <button type="button" className="chat-primary-button" onClick={() => void generate(failedPipeline === "architecture" ? "architecture" : "fetch")}><Play size={13} aria-hidden="true" />{t("learnRetry")}</button>
          : architectureDone && missing > 0 && <button type="button" className="chat-primary-button" onClick={() => void generate("all")}><Play size={13} aria-hidden="true" />{t("learnGenerateAll", { count: missing })}</button>}
        <button type="button" className="chat-icon-button" aria-pressed={sourceOpen} aria-label={t("learnBrowseFiles")} title={t("learnBrowseFiles")} disabled={!study.commit} onClick={() => setSourceOpen(!sourceOpen)}><FolderTree size={15} /></button>
        <button type="button" className="chat-icon-button" aria-pressed={logsOpen} aria-label={t("learnLogs")} title={t("learnLogs")} disabled={!latestJob} onClick={() => setLogsOpen(!logsOpen)}><ScrollText size={15} /></button>
        <button type="button" className="chat-icon-button" aria-label={t("learnOpenGithub")} title={t("learnOpenGithub")} onClick={() => openExternalLink(study.url)}><ExternalLink size={15} /></button>
        <button type="button" className="chat-icon-button" aria-label={t("learnDelete")} title={t("learnDelete")} disabled={!!running} onClick={() => setConfirmDelete(true)}><Trash2 size={15} /></button>
      </div>
    </header>
    {confirmDelete && <div className="learn-banner" role="alertdialog" aria-label={t("learnDelete")}>
      <span>{t("learnDeleteConfirm", { repo: `${study.owner}/${study.repo}` })}</span>
      <button type="button" onClick={() => setConfirmDelete(false)}>{t("cancel")}</button>
      <button type="button" className="learn-danger" onClick={() => void remove()}>{t("learnDelete")}</button>
    </div>}
    <ol className="learn-stages" aria-label={t("learnProgress")}>
      {PIPELINE.map(stage => <StageChip key={stage} label={stageLabel(stage)} state={study.stages.find(item => item.stage === stage)} />)}
      {study.pages.length > 0 && <li className="learn-stage" data-status={running && running.stage.includes(":") ? "running" : "done"}>
        <FileText size={13} aria-hidden="true" />{t("learnPagesReady", { ready: study.pages.filter(item => item.ready).length, total: study.pages.length })}
      </li>}
      {running && running.stage.includes(":") && <li className="learn-stage" data-status="running"><LoaderCircle size={13} className="chat-spin" aria-hidden="true" />{stageLabel(running.stage)}</li>}
      {study.coverage && study.coverage.sourceFiles > 0 && <li className="learn-stage" title={study.coverage.uncovered.join("\n")}>{t("learnCoverage", { covered: study.coverage.covered, total: study.coverage.sourceFiles })}</li>}
    </ol>
    {actionError && <ErrorText text={actionError} alert />}
    <StageProblems study={study.stages} label={stageLabel} />
    {logsOpen && latestJob && <JobLogs api={api} studyId={studyId} jobId={latestJob} revision={revision} />}
    <div className="learn-body">
      {study.pages.length > 0 && <WikiTree pages={study.pages} current={pageId} running={running?.stage} onSelect={setPageId} />}
      <article className="learn-article" aria-live="polite">
        {!study.pages.length ? <Pipeline study={detail} label={stageLabel} />
          : <>
            {pageId === "architecture" && detail.architecture && <ArchitectureDiagram architecture={detail.architecture} readyPages={new Set(study.pages.filter(item => item.ready).map(item => item.pageId))} onOpen={setPageId} />}
            {pageError && <p className="learn-error" role="alert">{pageError}</p>}
            {info && !info.ready ? <PagePending info={info} state={study.stages.find(item => item.stage === info.pageId)} busy={!!running} onGenerate={() => void generate(info.pageId)} />
              : page && <>
                {info?.refs && info.refs.invalid > 0 && <p className="learn-note">{t("learnInvalidRefs", { count: info.refs.invalid })}</p>}
                <Markdown text={page.markdown} links={links} />
              </>}
          </>}
      </article>
      {sourceOpen && <SourceViewer api={api} studyId={studyId} target={source} onOpen={openSource} onClose={() => setSourceOpen(false)} />}
    </div>
  </div>;
}

function StageIcon({ status }: { status?: string }) {
  if (status === "running") return <LoaderCircle size={13} className="chat-spin" aria-hidden="true" />;
  if (status === "done") return <CircleCheck size={13} aria-hidden="true" />;
  if (status === "failed" || status === "interrupted" || status === "cancelled") return <CircleAlert size={13} aria-hidden="true" />;
  return <CircleDashed size={13} aria-hidden="true" />;
}

function StageChip({ label, state }: { label: string; state?: LearnStageState }) {
  const t = useT();
  return <li className="learn-stage" data-status={state?.status ?? "pending"}>
    <StageIcon status={state?.status} />{label}<span className="learn-sr">{t(`learnStatus_${state?.status ?? "pending"}` as MessageKey)}</span>
  </li>;
}

function StageProblems({ study, label }: { study: LearnStageState[]; label: (stage: string) => string }) {
  const t = useT();
  const problems = study.filter(stage => stage.error || stage.warnings.length || stage.status === "cancelled");
  if (!problems.length) return null;
  return <ul className="learn-problems">{problems.map(stage => <li key={stage.stage} data-status={stage.status}>
    <strong>{label(stage.stage)}</strong> {stage.error ? <ErrorText text={stage.error} className="learn-problem-text" /> : stage.status === "cancelled" ? t("learnStatus_cancelled") : stage.warnings.join("; ")}
  </li>)}</ul>;
}

function Pipeline({ study, label }: { study: LearnStudyDetail; label: (stage: string) => string }) {
  const t = useT();
  return <div className="learn-pipeline">
    <h2>{t("learnPreparing")}</h2>
    <p className="learn-note">{t("learnPreparingDesc")}</p>
    <ol>{PIPELINE.map(stage => {
      const state = study.study.stages.find(item => item.stage === stage);
      return <li key={stage} data-status={state?.status ?? "pending"}><StageIcon status={state?.status} /><span>{label(stage)}</span><small>{t(`learnStatus_${state?.status ?? "pending"}` as MessageKey)}</small></li>;
    })}</ol>
  </div>;
}

function WikiTree({ pages, current, running, onSelect }: { pages: LearnPageInfo[]; current: string; running?: string; onSelect: (pageId: string) => void }) {
  const t = useT();
  const item = (page: LearnPageInfo) => <li key={page.pageId}>
    <button type="button" aria-current={page.pageId === current ? "page" : undefined} data-ready={page.ready || undefined} onClick={() => onSelect(page.pageId)}>
      <span className="learn-tree-title">{page.title}</span>
      {running === page.pageId ? <LoaderCircle size={12} className="chat-spin" role="img" aria-label={t("running")} />
        : !page.ready && <span className="learn-tree-todo" role="img" aria-label={t("learnNotGenerated")} />}
    </button>
  </li>;
  const top = pages.filter(page => page.kind === "overview" || page.kind === "architecture");
  const glossary = pages.filter(page => page.kind === "glossary");
  return <nav className="learn-wiki-tree" aria-label={t("learnWiki")}>
    <ul>{top.map(item)}</ul>
    {[1, 2, 3].map(level => {
      const modules = pages.filter(page => page.kind === "module" && page.level === level);
      return modules.length > 0 && <section key={level}><h3>{t(LEVEL_LABEL[level])}</h3><ul>{modules.map(item)}</ul></section>;
    })}
    <ul>{glossary.map(item)}</ul>
  </nav>;
}

function PagePending({ info, state, busy, onGenerate }: { info: LearnPageInfo; state?: LearnStageState; busy: boolean; onGenerate: () => void }) {
  const t = useT();
  return <div className="learn-main-empty">
    <h2>{info.title}</h2>
    {state?.status === "running" ? <p role="status"><LoaderCircle size={14} className="chat-spin" aria-hidden="true" /> {t("learnGenerating")}</p>
      : <>
        {state?.error && <ErrorText text={state.error} />}
        <p className="learn-note">{t("learnNotGeneratedDesc")}</p>
        <button type="button" className="chat-primary-button" disabled={busy} onClick={onGenerate}><Play size={13} aria-hidden="true" />{t("learnGeneratePage")}</button>
      </>}
  </div>;
}

function JobLogs({ api, studyId, jobId, revision }: { api: LearnApi; studyId: string; jobId: string; revision: number }) {
  const t = useT();
  const [stream, setStream] = useState<LearnLogStream>("stdout");
  const [text, setText] = useState("");
  const [more, setMore] = useState(false);
  const [error, setError] = useState("");
  useEffect(() => {
    let disposed = false;
    api.readLogs(studyId, jobId, stream, 0).then(chunk => { if (!disposed) { setText(chunk.text); setMore(chunk.hasMore); setError(""); } })
      .catch(reason => { if (!disposed) setError(String(reason)); });
    return () => { disposed = true; };
  }, [api, studyId, jobId, stream, revision]);
  return <section className="learn-logs" aria-label={t("learnLogs")}>
    <div role="tablist">{(["stdout", "stderr", "prompt"] as const).map(name => <button key={name} type="button" role="tab" aria-selected={stream === name} onClick={() => setStream(name)}>{name}</button>)}
      <code>{jobId}</code></div>
    {error ? <p className="learn-error">{error}</p> : <pre>{text || t("learnLogsEmpty")}</pre>}
    {more && <p className="learn-note">{t("learnLogsTruncated")}</p>}
  </section>;
}
