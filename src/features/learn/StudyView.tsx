import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import {
  CircleAlert,
  CircleCheck,
  CircleDashed,
  Ellipsis,
  ExternalLink,
  FileText,
  FolderTree,
  LoaderCircle,
  Play,
  ScrollText,
  Square,
  Trash2,
} from "lucide-react";
import {
  parseSourceTarget,
  type LearnApi,
  type LearnLogStream,
  type LearnPageContent,
  type LearnStageState,
  type LearnStudy,
  type LearnStudyDetail,
} from "../../api/learn";
import { useT } from "../../i18n";
import type { MessageKey } from "../../i18n/en";
import { Markdown, type MarkdownLinks } from "../../components/Markdown";
import { openExternalLink } from "../../components/externalLink";
import { ArchitectureDiagram, architectureNotes } from "./ArchitectureDiagram";
import { LearnLayout } from "./LearnLayout";
import { forgetReadPages, OverviewEntry, PagePending, Pager, useReadPages, WikiTree } from "./LearningPath";
import { useMenuKeys, usePopover } from "./usePopover";
import { ErrorText } from "../../components/common/ErrorText";
import { SourceViewer } from "../../components/SourceViewer";
import type { SourceReader, SourceTarget } from "../../api/sourceFiles";

const PIPELINE = ["fetch", "inventory", "architecture"] as const;
const STAGE_LABEL: Record<string, MessageKey> = {
  fetch: "learnStageFetch",
  inventory: "learnStageInventory",
  architecture: "learnStageArchitecture",
  glossary: "learnStageGlossary",
};

export function StudyView({
  api,
  studyId,
  revision,
  side,
  onDeleted,
}: {
  api: LearnApi;
  studyId: string;
  revision: number;
  /** The study switcher; the page outline is added below it in the left column. */
  side: ReactNode;
  onDeleted: () => void;
}) {
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
  const { read, markRead } = useReadPages(studyId);
  const article = useRef<HTMLElement>(null);
  useEffect(() => {
    setPageId("overview");
    setSource(null);
    setSourceOpen(false);
    setLogsOpen(false);
    setConfirmDelete(false);
  }, [studyId]);
  useEffect(() => {
    let disposed = false;
    api
      .get(studyId)
      .then((result) => {
        if (!disposed) {
          setDetail(result);
          setError("");
        }
      })
      .catch((reason) => {
        if (!disposed) setError(String(reason));
      });
    return () => {
      disposed = true;
    };
  }, [api, studyId, revision]);
  const study = detail?.study;
  const info = study?.pages.find((item) => item.pageId === pageId);
  const ready = !!info?.ready;
  useEffect(() => {
    if (!ready) {
      setPage(null);
      setPageError("");
      return;
    }
    let disposed = false;
    api
      .readPage(studyId, pageId)
      .then((result) => {
        if (!disposed) {
          setPage(result);
          setPageError("");
        }
      })
      .catch((reason) => {
        if (!disposed) {
          setPage(null);
          setPageError(String(reason));
        }
      });
    return () => {
      disposed = true;
    };
  }, [api, studyId, pageId, ready, revision]);
  // A page counts as read once its content has been shown.
  useEffect(() => {
    if (page?.pageId === pageId) markRead(pageId);
  }, [page, pageId, markRead]);
  useEffect(() => {
    article.current?.scrollTo?.({ top: 0 });
  }, [pageId]);
  const openSource = useCallback((target: SourceTarget) => {
    setSource(target);
    setSourceOpen(true);
  }, []);
  const reader = useMemo<SourceReader>(
    () => ({
      readSource: (path, startLine, endLine) => api.readSource(studyId, path, startLine, endLine),
      listTree: (dir) => api.listTree(studyId, dir),
    }),
    [api, studyId],
  );
  const refStatus = useMemo(() => new Map(page?.refs.map((ref) => [ref.target, ref.status]) ?? []), [page]);
  const links = useMemo<MarkdownLinks>(
    () => ({
      onSource: (target) => openSource(parseSourceTarget(target)),
      onWiki: (target) => setPageId(target.replace(/^module\//, "module:")),
      sourceStatus: (target) => refStatus.get(target),
    }),
    [openSource, refStatus],
  );
  if (error)
    return (
      <LearnLayout side={side}>
        <div className="learn-main-empty">
          <p className="learn-error" role="alert">
            {error}
          </p>
        </div>
      </LearnLayout>
    );
  if (!study)
    return (
      <LearnLayout side={side}>
        <div className="learn-main-empty" role="status">
          {t("loading")}
        </div>
      </LearnLayout>
    );
  const running = study.stages.find((stage) => stage.status === "running");
  const architectureDone = study.stages.some((stage) => stage.stage === "architecture" && stage.status === "done");
  const missing = study.pages.filter(
    (item) => !item.ready && (item.kind === "module" || item.kind === "glossary"),
  ).length;
  const latestJob =
    running?.jobId ??
    [...study.stages].filter((stage) => stage.jobId).sort((a, b) => (b.startedAtMs ?? 0) - (a.startedAtMs ?? 0))[0]
      ?.jobId;
  const failedPipeline = PIPELINE.find((stage) =>
    ["failed", "cancelled", "interrupted"].includes(study.stages.find((item) => item.stage === stage)?.status ?? ""),
  );
  async function generate(stage: string) {
    setActionError("");
    try {
      await api.generate(studyId, stage);
    } catch (reason) {
      setActionError(String(reason));
    }
  }
  async function remove() {
    try {
      await api.remove(studyId);
      forgetReadPages(studyId);
      onDeleted();
    } catch (reason) {
      setActionError(String(reason));
      setConfirmDelete(false);
    }
  }
  const moduleOf = (id?: string) => detail.architecture?.modules.find((module) => module.id === id);
  const pageOf = (moduleId: string) => study.pages.find((item) => item.moduleId === moduleId);
  const self = moduleOf(info?.moduleId);
  const related = {
    dependsOn: (self?.dependsOn ?? []).map(pageOf).filter((item) => !!item),
    usedBy: (detail.architecture?.modules ?? [])
      .filter((module) => self && module.dependsOn.includes(self.id))
      .map((module) => pageOf(module.id))
      .filter((item) => !!item),
  };
  const stageLabel = (stage: string) =>
    STAGE_LABEL[stage] ? t(STAGE_LABEL[stage]) : (study.pages.find((item) => item.pageId === stage)?.title ?? stage);
  const outline = study.pages.length > 0 && (
    <WikiTree pages={study.pages} read={read} running={running?.stage} current={pageId} onSelect={setPageId} />
  );
  return (
    <LearnLayout
      side={
        <>
          {side}
          {outline}
        </>
      }
    >
      <div className="learn-study" data-testid="learn-study">
        <header className="learn-study-head">
          <div className="learn-study-title">
            <h1>
              {study.owner}/{study.repo}
            </h1>
            <small>
              {study.commit ? study.commit.slice(0, 10) : (study.gitRef ?? "HEAD")} · {study.agentName}
              {study.model ? ` · ${study.model}` : ""}
            </small>
          </div>
          <div className="learn-study-actions">
            <ProgressPill study={study} running={running} failed={failedPipeline} label={stageLabel} />
            {running ? (
              <button type="button" className="learn-secondary-button" onClick={() => void api.cancel(studyId)}>
                <Square size={13} aria-hidden="true" />
                {t("cancel")}
              </button>
            ) : failedPipeline ? (
              <button
                type="button"
                className="chat-primary-button"
                onClick={() => void generate(failedPipeline === "architecture" ? "architecture" : "fetch")}
              >
                <Play size={13} aria-hidden="true" />
                {t("learnRetry")}
              </button>
            ) : (
              architectureDone &&
              missing > 0 && (
                <button type="button" className="learn-secondary-button" onClick={() => void generate("all")}>
                  <Play size={13} aria-hidden="true" />
                  {t("learnGenerateAll", { count: missing })}
                </button>
              )
            )}
            <button
              type="button"
              className="chat-icon-button"
              aria-pressed={sourceOpen}
              aria-label={t("sourceBrowse")}
              title={t("sourceBrowse")}
              disabled={!study.commit}
              onClick={() => setSourceOpen(!sourceOpen)}
            >
              <FolderTree size={15} />
            </button>
            <MoreMenu
              logsOpen={logsOpen}
              hasLogs={!!latestJob}
              busy={!!running}
              onGithub={() => openExternalLink(study.url)}
              onLogs={() => setLogsOpen(!logsOpen)}
              onDelete={() => setConfirmDelete(true)}
            />
          </div>
        </header>
        {confirmDelete && (
          <div className="learn-banner" role="alertdialog" aria-label={t("learnDelete")}>
            <span>{t("learnDeleteConfirm", { repo: `${study.owner}/${study.repo}` })}</span>
            <button type="button" onClick={() => setConfirmDelete(false)}>
              {t("cancel")}
            </button>
            <button type="button" className="learn-danger" onClick={() => void remove()}>
              {t("learnDelete")}
            </button>
          </div>
        )}
        {actionError && <ErrorText text={actionError} alert className="learn-error" />}
        <StageProblems study={study.stages} label={stageLabel} />
        {logsOpen && latestJob && <JobLogs api={api} studyId={studyId} jobId={latestJob} revision={revision} />}
        <div className="learn-body">
          <article className="learn-article" aria-live="polite" ref={article}>
            {!study.pages.length ? (
              <Pipeline study={detail} label={stageLabel} />
            ) : (
              <>
                {pageId === "overview" && (
                  <OverviewEntry
                    pages={study.pages}
                    architecture={detail.architecture}
                    read={read}
                    running={running?.stage}
                    current={pageId}
                    onSelect={setPageId}
                  />
                )}
                {pageError && (
                  <p className="learn-error" role="alert">
                    {pageError}
                  </p>
                )}
                {info && !info.ready ? (
                  <PagePending
                    info={info}
                    state={study.stages.find((item) => item.stage === info.pageId)}
                    busy={!!running}
                    agentName={study.agentName}
                    remaining={architectureDone ? missing : 0}
                    dependsOn={related.dependsOn}
                    usedBy={related.usedBy}
                    read={read}
                    running={running?.stage}
                    onGenerate={() => void generate(info.pageId)}
                    onGenerateAll={() => void generate("all")}
                    onSelect={setPageId}
                  />
                ) : (
                  page && (
                    <>
                      {info?.refs && info.refs.invalid > 0 && (
                        <p className="learn-note">{t("learnInvalidRefs", { count: info.refs.invalid })}</p>
                      )}
                      {pageId === "architecture" && detail.architecture ? (
                        <>
                          <div className="chat-markdown">
                            <h1>{architectureNotes(page.markdown).title}</h1>
                          </div>
                          <ArchitectureDiagram
                            architecture={detail.architecture}
                            pages={study.pages}
                            read={read}
                            running={running?.stage}
                            onOpen={setPageId}
                          />
                          <Markdown text={architectureNotes(page.markdown).body} links={links} />
                        </>
                      ) : (
                        <Markdown text={page.markdown} links={links} />
                      )}
                    </>
                  )
                )}
                <Pager pages={study.pages} current={pageId} onSelect={setPageId} />
              </>
            )}
          </article>
          {sourceOpen && (
            <SourceViewer reader={reader} target={source} onOpen={openSource} onClose={() => setSourceOpen(false)} />
          )}
        </div>
      </div>
    </LearnLayout>
  );
}

function StageIcon({ status }: { status?: string }) {
  if (status === "running") return <LoaderCircle size={13} className="chat-spin" aria-hidden="true" />;
  if (status === "done") return <CircleCheck size={13} aria-hidden="true" />;
  if (status === "failed" || status === "interrupted" || status === "cancelled")
    return <CircleAlert size={13} aria-hidden="true" />;
  return <CircleDashed size={13} aria-hidden="true" />;
}

function ProgressRing({ ready, total }: { ready: number; total: number }) {
  const length = 2 * Math.PI * 6;
  return (
    <svg className="learn-ring" width="16" height="16" viewBox="0 0 16 16" aria-hidden="true">
      <circle cx="8" cy="8" r="6" />
      <circle
        cx="8"
        cy="8"
        r="6"
        strokeDasharray={`${total ? (length * ready) / total : 0} ${length}`}
        transform="rotate(-90 8 8)"
      />
    </svg>
  );
}

/** Header pill: pages ready (or the pipeline stage) at a glance; stage details, coverage and warnings in its popover. */
function ProgressPill({
  study,
  running,
  failed,
  label,
}: {
  study: LearnStudy;
  running?: LearnStageState;
  failed?: string;
  label: (stage: string) => string;
}) {
  const t = useT();
  const pop = usePopover();
  const ready = study.pages.filter((item) => item.ready).length;
  const total = study.pages.length;
  const pageRunning = running?.stage.includes(":") ? running : undefined;
  const pageWarnings = study.stages.filter(
    (stage) => !(PIPELINE as readonly string[]).includes(stage.stage) && stage.warnings.length,
  );
  const coverage = study.coverage && study.coverage.sourceFiles > 0 ? study.coverage : undefined;
  return (
    <span className="learn-progress" ref={pop.ref}>
      <button
        type="button"
        className="learn-progress-pill"
        aria-haspopup="dialog"
        aria-expanded={pop.open}
        title={t("learnProgress")}
        onClick={() => pop.setOpen(!pop.open)}
      >
        {running ? (
          <LoaderCircle size={14} className="chat-spin" aria-hidden="true" />
        ) : failed ? (
          <CircleAlert size={14} className="learn-progress-alert" aria-hidden="true" />
        ) : (
          <ProgressRing ready={ready} total={total} />
        )}
        <span>{total ? t("learnPagesReady", { ready, total }) : label(running?.stage ?? failed ?? "fetch")}</span>
        {study.stages.some((stage) => stage.warnings.length) && (
          <span className="learn-warn-dot" role="img" aria-label={t("learnHasWarnings")} />
        )}
      </button>
      {pop.open && (
        <div className="learn-progress-pop" role="dialog" aria-label={t("learnProgress")}>
          <h4>{t("learnProgress")}</h4>
          <ol className="learn-stages">
            {PIPELINE.map((stage) => {
              const state = study.stages.find((item) => item.stage === stage);
              return (
                <li key={stage} className="learn-stage" data-status={state?.status ?? "pending"}>
                  <StageIcon status={state?.status} />
                  <span>
                    {label(stage)}
                    <small title={stage === "inventory" ? coverage?.uncovered.join("\n") : undefined}>
                      {t(`learnStatus_${state?.status ?? "pending"}` as MessageKey)}
                      {stage === "inventory" && coverage
                        ? ` · ${t("learnCoverage", { covered: coverage.covered, total: coverage.sourceFiles })}`
                        : ""}
                    </small>
                    {state?.warnings.map((warning) => (
                      <small key={warning} className="learn-stage-warning">
                        {warning}
                      </small>
                    ))}
                  </span>
                </li>
              );
            })}
            {total > 0 && (
              <li className="learn-stage" data-status={pageRunning ? "running" : ready === total ? "done" : "pending"}>
                {pageRunning ? (
                  <LoaderCircle size={13} className="chat-spin" aria-hidden="true" />
                ) : (
                  <FileText size={13} aria-hidden="true" />
                )}
                <span>
                  {t("learnPagesReady", { ready, total })}
                  {pageRunning && <small>{label(pageRunning.stage)}</small>}
                  {pageWarnings.map((stage) => (
                    <small key={stage.stage} className="learn-stage-warning">
                      {label(stage.stage)}: {stage.warnings.join("; ")}
                    </small>
                  ))}
                </span>
              </li>
            )}
          </ol>
        </div>
      )}
    </span>
  );
}

/** Less frequent study actions; Delete only opens the confirmation banner. */
function MoreMenu({
  logsOpen,
  hasLogs,
  busy,
  onGithub,
  onLogs,
  onDelete,
}: {
  logsOpen: boolean;
  hasLogs: boolean;
  busy: boolean;
  onGithub: () => void;
  onLogs: () => void;
  onDelete: () => void;
}) {
  const t = useT();
  const menu = usePopover();
  const keys = useMenuKeys(menu.open);
  const act = (run: () => void) => () => {
    menu.setOpen(false);
    run();
  };
  return (
    <span className="learn-more" ref={menu.ref}>
      <button
        type="button"
        className="chat-icon-button"
        aria-label={t("learnMore")}
        title={t("learnMore")}
        aria-haspopup="menu"
        aria-expanded={menu.open}
        onClick={() => menu.setOpen(!menu.open)}
      >
        <Ellipsis size={16} />
      </button>
      {menu.open && (
        <div className="learn-menu" role="menu" aria-label={t("learnMore")} ref={keys.list} onKeyDown={keys.onKeyDown}>
          <button type="button" role="menuitem" onClick={act(onGithub)}>
            <ExternalLink size={14} aria-hidden="true" />
            {t("learnOpenGithub")}
          </button>
          <button
            type="button"
            role="menuitemcheckbox"
            aria-checked={logsOpen}
            disabled={!hasLogs}
            onClick={act(onLogs)}
          >
            <ScrollText size={14} aria-hidden="true" />
            {t("learnLogs")}
          </button>
          <div role="separator" />
          <button type="button" role="menuitem" className="learn-menu-danger" disabled={busy} onClick={act(onDelete)}>
            <Trash2 size={14} aria-hidden="true" />
            {t("learnDeleteMenu")}
          </button>
        </div>
      )}
    </span>
  );
}

/** Errors and cancellations stay in view; warnings live in the progress popover. */
function StageProblems({ study, label }: { study: LearnStageState[]; label: (stage: string) => string }) {
  const t = useT();
  const problems = study.filter((stage) => stage.error || stage.status === "cancelled");
  if (!problems.length) return null;
  return (
    <ul className="learn-problems">
      {problems.map((stage) => (
        <li key={stage.stage} data-status={stage.status}>
          <strong>{label(stage.stage)}</strong>{" "}
          {stage.error ? <ErrorText text={stage.error} className="learn-problem-text" /> : t("learnStatus_cancelled")}
        </li>
      ))}
    </ul>
  );
}

function Pipeline({ study, label }: { study: LearnStudyDetail; label: (stage: string) => string }) {
  const t = useT();
  return (
    <div className="learn-pipeline">
      <h2>{t("learnPreparing")}</h2>
      <p className="learn-note">{t("learnPreparingDesc")}</p>
      <ol>
        {PIPELINE.map((stage) => {
          const state = study.study.stages.find((item) => item.stage === stage);
          return (
            <li key={stage} data-status={state?.status ?? "pending"}>
              <StageIcon status={state?.status} />
              <span>{label(stage)}</span>
              <small>{t(`learnStatus_${state?.status ?? "pending"}` as MessageKey)}</small>
            </li>
          );
        })}
      </ol>
    </div>
  );
}

function JobLogs({
  api,
  studyId,
  jobId,
  revision,
}: {
  api: LearnApi;
  studyId: string;
  jobId: string;
  revision: number;
}) {
  const t = useT();
  const [stream, setStream] = useState<LearnLogStream>("stdout");
  const [text, setText] = useState("");
  const [more, setMore] = useState(false);
  const [error, setError] = useState("");
  useEffect(() => {
    let disposed = false;
    api
      .readLogs(studyId, jobId, stream, 0)
      .then((chunk) => {
        if (!disposed) {
          setText(chunk.text);
          setMore(chunk.hasMore);
          setError("");
        }
      })
      .catch((reason) => {
        if (!disposed) setError(String(reason));
      });
    return () => {
      disposed = true;
    };
  }, [api, studyId, jobId, stream, revision]);
  return (
    <section className="learn-logs" aria-label={t("learnLogs")}>
      <div role="tablist">
        {(["stdout", "stderr", "prompt"] as const).map((name) => (
          <button key={name} type="button" role="tab" aria-selected={stream === name} onClick={() => setStream(name)}>
            {name}
          </button>
        ))}
        <code>{jobId}</code>
      </div>
      {error ? <p className="learn-error">{error}</p> : <pre>{text || t("learnLogsEmpty")}</pre>}
      {more && <p className="learn-note">{t("learnLogsTruncated")}</p>}
    </section>
  );
}
