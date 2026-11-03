import { useCallback, useEffect, useMemo, useState } from "react";
import { LoaderCircle, RotateCcw, ScrollText, Square, Trash2 } from "lucide-react";
import {
  retryable,
  type Discussion,
  type DiscussApi,
  type DiscussTask,
  type DiscussReport,
  type DiscussLogStream,
} from "../../api/discuss";
import { parseSourceTarget, type SourceTarget } from "../../api/sourceFiles";
import { Markdown, type MarkdownLinks } from "../../components/Markdown";
import { SourceViewer } from "../../components/SourceViewer";
import { ErrorText } from "../../components/common/ErrorText";
import { useT } from "../../i18n";
import type { MessageKey } from "../../i18n/en";

export function DiscussView({
  api,
  discussionId,
  revision,
  onDeleted,
}: {
  api: DiscussApi;
  discussionId: string;
  revision: number;
  onDeleted: () => void;
}) {
  const t = useT();
  const [discussion, setDiscussion] = useState<Discussion | null>(null),
    [error, setError] = useState("");
  const [round, setRound] = useState(1),
    [busy, setBusy] = useState(false),
    [actionError, setActionError] = useState("");
  const [retry, setRetry] = useState<string | null>(null),
    [deleting, setDeleting] = useState(false);
  const [source, setSource] = useState<SourceTarget | null>(null),
    [logs, setLogs] = useState<string | null>(null);
  useEffect(() => {
    setDiscussion(null);
    setError("");
    setRound(1);
    setRetry(null);
    setDeleting(false);
    setSource(null);
    setLogs(null);
    setActionError("");
  }, [discussionId]);
  useEffect(() => {
    let disposed = false;
    api
      .get(discussionId)
      .then((d) => {
        if (!disposed) {
          setDiscussion(d);
          setError("");
        }
      })
      .catch((e) => {
        if (!disposed) setError(String(e));
      });
    return () => {
      disposed = true;
    };
  }, [api, discussionId, revision]);
  const openSource = useCallback((target: SourceTarget) => setSource(target), []);
  const reader = useMemo(
    () => ({
      readSource: (path: string, start?: number, end?: number) => api.readSource(discussionId, path, start, end),
      listTree: (dir?: string) => api.listTree(discussionId, dir),
    }),
    [api, discussionId],
  );
  if (error)
    return (
      <div className="discuss-main-empty">
        <ErrorText text={error} alert className="discuss-error" />
      </div>
    );
  if (!discussion || discussion.discussionId !== discussionId)
    return (
      <div className="discuss-main-empty" role="status">
        {t("loading")}
      </div>
    );
  const d = discussion,
    running = d.status === "running";
  async function act(fn: () => Promise<unknown>) {
    setBusy(true);
    setActionError("");
    try {
      await fn();
    } catch (e) {
      setActionError(String(e));
    } finally {
      setBusy(false);
    }
  }
  const logTask = d.tasks.find((task) => task.id === logs);
  return (
    <div className="discuss-view" data-testid="discuss-view">
      <header className="discuss-head">
        <div className="discuss-title">
          <h1 title={d.topic}>{d.title}</h1>
          <small>
            {d.participants.map((p) => p.name).join(" · ")} · {d.projectName ?? t("discussNoProjectLabel")}
          </small>
        </div>
        <div className="discuss-actions">
          {running && (
            <button type="button" disabled={busy} onClick={() => void act(() => api.cancel(discussionId))}>
              <Square size={13} />
              {t("discussStop")}
            </button>
          )}
          <button
            type="button"
            className="chat-icon-button"
            disabled={busy || running}
            title={t("discussDelete")}
            aria-label={t("discussDelete")}
            onClick={() => setDeleting(true)}
          >
            <Trash2 size={15} />
          </button>
        </div>
      </header>
      {deleting && (
        <div className="discuss-banner" role="alertdialog" aria-label={t("discussDelete")}>
          <span>{t("discussDeleteConfirm")}</span>
          <button type="button" disabled={busy} onClick={() => setDeleting(false)}>
            {t("cancel")}
          </button>
          <button
            type="button"
            disabled={busy}
            onClick={() =>
              void act(async () => {
                await api.remove(discussionId);
                onDeleted();
              })
            }
          >
            {t("discussDelete")}
          </button>
        </div>
      )}
      {retry && (
        <div className="discuss-banner" role="alertdialog" aria-label={t("discussRetry")}>
          <span>{t("discussRetryConfirm")}</span>
          <button type="button" disabled={busy} onClick={() => setRetry(null)}>
            {t("cancel")}
          </button>
          <button
            type="button"
            className="chat-primary-button"
            disabled={busy || running}
            onClick={() =>
              void act(async () => {
                setDiscussion(await api.retry(discussionId, retry, 1));
                setRetry(null);
              })
            }
          >
            {t("discussConfirmRetry")}
          </button>
        </div>
      )}
      {actionError && <ErrorText text={actionError} alert className="discuss-error" />}
      {d.status !== "ready" && (
        <p className="discuss-state" data-status={d.status} role="status">
          {running ? <LoaderCircle size={14} className="chat-spin" /> : null}
          {t(`discussState_${d.status}` as MessageKey)}
        </p>
      )}
      {d.tasks.some((task) => task.stale) && <p className="discuss-banner">{t("discussStaleHint")}</p>}
      <nav className="discuss-tabs" role="tablist" aria-label={t("discussRounds")}>
        {[1, 2, 3].map((r) => (
          <button key={r} type="button" role="tab" aria-selected={round === r} onClick={() => setRound(r)}>
            {t(`discussRound${r}` as MessageKey)}
            <small>
              {d.tasks.filter((task) => task.round === r && task.status === "done" && !task.stale).length}/
              {d.tasks.filter((task) => task.round === r).length}
            </small>
          </button>
        ))}
      </nav>
      {round === 3 && d.tasks.some((task) => task.round === 2 && (task.status !== "done" || task.stale)) && (
        <p className="discuss-banner">
          {t("discussMissing", {
            names: d.tasks
              .filter((task) => task.round === 2 && (task.status !== "done" || task.stale))
              .map((task) => d.participants.find((p) => p.id === task.participantId)?.name ?? task.participantId)
              .join(", "),
          })}
        </p>
      )}
      {logTask && (
        <JobLogs
          key={logTask.id}
          api={api}
          discussionId={discussionId}
          task={logTask}
          running={running}
          onClose={() => setLogs(null)}
        />
      )}
      <div className="discuss-body">
        <div className="discuss-scroll" role="tabpanel" aria-label={t(`discussRound${round}` as MessageKey)}>
          <p className="discuss-topic">{d.topic}</p>
          <div className="discuss-grid" data-round={round}>
            {d.tasks
              .filter((task) => task.round === round)
              .map((task) => (
                <TaskCard
                  key={task.id}
                  api={api}
                  discussionId={discussionId}
                  task={task}
                  participant={d.participants.find((p) => p.id === task.participantId)!}
                  disabled={busy || running}
                  onRetry={() => setRetry(task.id)}
                  onLogs={() => setLogs(task.id)}
                  onSource={openSource}
                />
              ))}
          </div>
        </div>
        {source && <SourceViewer reader={reader} target={source} onOpen={setSource} onClose={() => setSource(null)} />}
      </div>
    </div>
  );
}
function TaskCard({
  api,
  discussionId,
  task,
  participant,
  disabled,
  onRetry,
  onLogs,
  onSource,
}: {
  api: DiscussApi;
  discussionId: string;
  task: DiscussTask;
  participant: Discussion["participants"][number];
  disabled: boolean;
  onRetry: () => void;
  onLogs: () => void;
  onSource: (target: SourceTarget) => void;
}) {
  const t = useT();
  const [content, setContent] = useState<DiscussReport | null>(null),
    [error, setError] = useState("");
  const lastAttempt = task.attempts[task.attempts.length - 1];
  useEffect(() => {
    let disposed = false;
    setContent(null);
    setError("");
    if (task.status === "done")
      api
        .readReport(discussionId, task.id)
        .then((r) => {
          if (!disposed) setContent(r);
        })
        .catch((e) => {
          if (!disposed) setError(String(e));
        });
    return () => {
      disposed = true;
    };
  }, [api, discussionId, task.id, task.status, lastAttempt]);
  const links = useMemo<MarkdownLinks>(
    () => ({
      onSource: (target) => onSource(parseSourceTarget(target)),
      onWiki: () => undefined,
      sourceStatus: (target) => content?.refs.find((r) => r.target === target)?.status,
    }),
    [content, onSource],
  );
  return (
    <section className="discuss-card" data-task={task.id} data-status={task.status}>
      <header>
        <div>
          <strong>{participant.name}</strong>
          <small>
            {participant.id}
            {participant.model ? ` · ${participant.model}` : ""}
          </small>
        </div>
        <span className="discuss-chip" data-status={task.status}>
          {t(`discussStatus_${task.status}` as MessageKey)}
        </span>
      </header>
      {task.stale && <p className="discuss-stale">{t("discussOutdated")}</p>}
      {task.error && <ErrorText text={task.error} className="discuss-error" />}
      {task.warnings.length > 0 && (
        <ul className="discuss-warnings">
          {task.warnings.map((w) => (
            <li key={w}>{w}</li>
          ))}
        </ul>
      )}
      {task.status === "pending" && <p className="discuss-note">{t("discussWaiting")}</p>}
      {task.status === "running" && (
        <p className="discuss-note" role="status">
          <LoaderCircle size={14} className="chat-spin" />
          {t("discussWorking")}
        </p>
      )}
      {error && <ErrorText text={error} alert className="discuss-error" />}
      {content && (
        <article className="discuss-report">
          <Markdown text={content.markdown} links={links} />
        </article>
      )}
      <footer>
        {retryable(task) && (
          <button type="button" disabled={disabled} onClick={onRetry}>
            <RotateCcw size={13} />
            {t(task.stale ? "discussRegenerate" : "discussRetry")}
          </button>
        )}
        {lastAttempt && (
          <button type="button" onClick={onLogs}>
            <ScrollText size={13} />
            {t("discussLogs")}
          </button>
        )}
      </footer>
    </section>
  );
}
function JobLogs({
  api,
  discussionId,
  task,
  running,
  onClose,
}: {
  api: DiscussApi;
  discussionId: string;
  task: DiscussTask;
  running: boolean;
  onClose: () => void;
}) {
  const t = useT();
  const [job, setJob] = useState(task.attempts[task.attempts.length - 1] ?? "");
  const [stream, setStream] = useState<DiscussLogStream>("stdout");
  const [text, setText] = useState(""),
    [offset, setOffset] = useState(0),
    [more, setMore] = useState(false),
    [error, setError] = useState("");
  const [tick, setTick] = useState(0);
  useEffect(() => {
    if (!running) return;
    const timer = setInterval(() => setTick((n) => n + 1), 1500);
    return () => clearInterval(timer);
  }, [running]);
  useEffect(() => {
    let disposed = false;
    setText("");
    setMore(false);
    api
      .readLogs(discussionId, job, stream, 0)
      .then((c) => {
        if (!disposed) {
          setText(c.text);
          setOffset(c.nextOffset);
          setMore(c.hasMore);
          setError("");
        }
      })
      .catch((e) => {
        if (!disposed) setError(String(e));
      });
    return () => {
      disposed = true;
    };
  }, [api, discussionId, job, stream, tick]);
  async function loadMore() {
    try {
      const c = await api.readLogs(discussionId, job, stream, offset);
      setText((s) => s + c.text);
      setOffset(c.nextOffset);
      setMore(c.hasMore);
    } catch (e) {
      setError(String(e));
    }
  }
  return (
    <section className="discuss-logs" aria-label={t("discussLogs")}>
      <header>
        <select aria-label={t("discussAttempt")} value={job} onChange={(e) => setJob(e.target.value)}>
          {task.attempts.map((id, i) => (
            <option key={id} value={id}>
              {t("discussAttempt")} {i + 1}
            </option>
          ))}
        </select>
        <select
          aria-label={t("discussLogStream")}
          value={stream}
          onChange={(e) => setStream(e.target.value as DiscussLogStream)}
        >
          {["stdout", "stderr", "prompt", "record"].map((s) => (
            <option key={s}>{s}</option>
          ))}
        </select>
        <button type="button" onClick={onClose}>
          {t("close")}
        </button>
      </header>
      {error ? <ErrorText text={error} alert className="discuss-error" /> : <pre>{text || t("discussLogsEmpty")}</pre>}
      {more && (
        <button type="button" onClick={() => void loadMore()}>
          {t("discussMoreLogs")}
        </button>
      )}
    </section>
  );
}
