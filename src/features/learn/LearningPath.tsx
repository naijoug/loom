import { useCallback, useEffect, useState } from "react";
import {
  ArrowLeft,
  ArrowRight,
  BookOpen,
  ChevronRight,
  CircleCheck,
  CircleDashed,
  CircleDot,
  FileText,
  LoaderCircle,
  Play,
} from "lucide-react";
import type { LearnArchitecture, LearnPageInfo, LearnStageState } from "../../api/learn";
import { useT } from "../../i18n";
import type { MessageKey } from "../../i18n/en";
import { ErrorText } from "../../components/common/ErrorText";

export const LEVELS = [1, 2, 3] as const;
export const LEVEL_LABEL: Record<number, MessageKey> = { 1: "learnLevel1", 2: "learnLevel2", 3: "learnLevel3" };

/** Learning order shared by the outline and the pager: overview, architecture, level 1–3 modules, glossary. */
export function learningOrder(pages: LearnPageInfo[]) {
  return [
    ...pages.filter((page) => page.kind === "overview" || page.kind === "architecture"),
    ...LEVELS.flatMap((level) => pages.filter((page) => page.kind === "module" && page.level === level)),
    ...pages.filter((page) => page.kind === "glossary"),
  ];
}

// Read state is a per-device convenience: storage failures only lose the marks.
const readKey = (studyId: string) => `loom-learn-read:${studyId}`;
function loadRead(studyId: string) {
  try {
    const stored: unknown = JSON.parse(localStorage.getItem(readKey(studyId)) ?? "[]");
    return new Set(Array.isArray(stored) ? stored.filter((id): id is string => typeof id === "string") : []);
  } catch {
    return new Set<string>();
  }
}
export function forgetReadPages(studyId: string) {
  try {
    localStorage.removeItem(readKey(studyId));
  } catch {
    /* per-device convenience only */
  }
}
export function useReadPages(studyId: string) {
  const [read, setRead] = useState(() => loadRead(studyId));
  useEffect(() => setRead(loadRead(studyId)), [studyId]);
  const markRead = useCallback(
    (pageId: string) =>
      setRead((current) => {
        if (current.has(pageId)) return current;
        const next = new Set(current).add(pageId);
        try {
          localStorage.setItem(readKey(studyId), JSON.stringify([...next]));
        } catch {
          /* per-device convenience only */
        }
        return next;
      }),
    [studyId],
  );
  return { read, markRead };
}

export type PageState = "read" | "ready" | "running" | "pending";
export function pageState(page: LearnPageInfo, read: Set<string>, running?: string): PageState {
  if (running === page.pageId) return "running";
  if (!page.ready) return "pending";
  return read.has(page.pageId) ? "read" : "ready";
}
const STATE_LABEL: Record<PageState, MessageKey> = {
  read: "learnRead",
  ready: "learnReady",
  running: "learnLegendRunning",
  pending: "learnNotGenerated",
};

export function PageIcon({ state }: { state: PageState }) {
  const t = useT();
  const props = { size: 14, role: "img", "aria-label": t(STATE_LABEL[state]) };
  if (state === "running") return <LoaderCircle {...props} className="chat-spin learn-icon-running" />;
  if (state === "read") return <CircleCheck {...props} className="learn-icon-read" />;
  if (state === "ready") return <CircleDot {...props} className="learn-icon-ready" />;
  return <CircleDashed {...props} className="learn-tree-todo" />;
}

interface PathProps {
  read: Set<string>;
  running?: string;
  current: string;
  onSelect: (pageId: string) => void;
}

function PageLink({ page, read, running, current, onSelect }: PathProps & { page: LearnPageInfo }) {
  return (
    <button
      type="button"
      className="learn-page-link"
      aria-current={page.pageId === current ? "page" : undefined}
      data-ready={page.ready || undefined}
      data-drawer-close
      onClick={() => onSelect(page.pageId)}
    >
      <PageIcon state={pageState(page, read, running)} />
      <span className="learn-tree-title">{page.title}</span>
    </button>
  );
}

const readCount = (pages: LearnPageInfo[], read: Set<string>) =>
  pages.filter((page) => page.ready && read.has(page.pageId)).length;

/** Left-column outline in learning order with per-level read counts and a status legend. */
export function WikiTree({ pages, ...path }: PathProps & { pages: LearnPageInfo[] }) {
  const t = useT();
  const order = learningOrder(pages);
  const list = (items: LearnPageInfo[]) => (
    <ul>
      {items.map((page) => (
        <li key={page.pageId}>
          <PageLink page={page} {...path} />
        </li>
      ))}
    </ul>
  );
  return (
    <>
      <nav className="learn-wiki-tree" aria-label={t("learnWiki")}>
        {list(order.filter((page) => page.kind === "overview" || page.kind === "architecture"))}
        {LEVELS.map((level) => {
          const modules = order.filter((page) => page.kind === "module" && page.level === level);
          return (
            modules.length > 0 && (
              <section key={level}>
                <h3>
                  <span>{t(LEVEL_LABEL[level])}</span>
                  <span title={t("learnRead")}>
                    {readCount(modules, path.read)}/{modules.length}
                  </span>
                </h3>
                {list(modules)}
              </section>
            )
          );
        })}
        {list(order.filter((page) => page.kind === "glossary"))}
      </nav>
      <div className="learn-legend" aria-hidden="true">
        {(["read", "ready", "running", "pending"] as const).map((state) => (
          <span key={state}>
            <PageIcon state={state} />
            {t(STATE_LABEL[state])}
          </span>
        ))}
      </div>
    </>
  );
}

/** Top of the Overview page: the next page to read, overall progress, and one card per level. */
export function OverviewEntry({
  pages,
  architecture,
  ...path
}: PathProps & { pages: LearnPageInfo[]; architecture?: LearnArchitecture }) {
  const t = useT();
  const order = learningOrder(pages);
  const ready = order.filter((page) => page.ready).length;
  const done = readCount(order, path.read);
  const next =
    order.find((page) => pageState(page, path.read, path.running) === "ready") ??
    order.find((page) => !page.ready && page.pageId !== path.running);
  const glossary = order.find((page) => page.kind === "glossary");
  const percent = (count: number) => `${order.length ? (count / order.length) * 100 : 0}%`;
  return (
    <section className="learn-entry" aria-label={t("learnPath")}>
      <div className="learn-continue">
        <div className="learn-continue-text">
          <small>{next ? t(next.ready ? "learnNextUnread" : "learnNextToGenerate") : t("learnAllRead")}</small>
          {next && <strong>{next.title}</strong>}
          <div className="learn-bar" aria-hidden="true">
            <i className="learn-bar-read" style={{ width: percent(done) }} />
            <i className="learn-bar-ready" style={{ width: percent(ready - done) }} />
          </div>
          <small>{t("learnReadProgress", { read: done, generated: ready, total: order.length })}</small>
        </div>
        {next && (
          <button type="button" className="chat-primary-button" onClick={() => path.onSelect(next.pageId)}>
            {t("learnContinue")}
            <ChevronRight size={14} aria-hidden="true" />
          </button>
        )}
      </div>
      <div className="learn-levels">
        {LEVELS.map((level) => {
          const modules = order.filter((page) => page.kind === "module" && page.level === level);
          if (!modules.length) return null;
          const goals = architecture?.learningPath.filter((step) => step.level === level).map((step) => step.goal);
          return (
            <section key={level} className="learn-level">
              <header>
                <strong>{t(LEVEL_LABEL[level])}</strong>
                <span>{t("learnLevelRead", { read: readCount(modules, path.read), total: modules.length })}</span>
              </header>
              {goals?.length ? <p>{goals.join(" · ")}</p> : null}
              <ul>
                {modules.map((page) => (
                  <li key={page.pageId}>
                    <PageLink page={page} {...path} />
                  </li>
                ))}
              </ul>
            </section>
          );
        })}
      </div>
      {glossary && (
        <button type="button" className="learn-glossary-row" onClick={() => path.onSelect(glossary.pageId)}>
          <BookOpen size={15} aria-hidden="true" />
          <span>{glossary.title}</span>
          <PageIcon state={pageState(glossary, path.read, path.running)} />
          <small>{t(STATE_LABEL[pageState(glossary, path.read, path.running)])}</small>
        </button>
      )}
    </section>
  );
}

/** Previous / next page in learning order. */
export function Pager({
  pages,
  current,
  onSelect,
}: {
  pages: LearnPageInfo[];
  current: string;
  onSelect: (pageId: string) => void;
}) {
  const t = useT();
  const order = learningOrder(pages);
  const at = order.findIndex((page) => page.pageId === current);
  if (at < 0) return null;
  const prev = order[at - 1],
    next = order[at + 1];
  return (
    <nav className="learn-pager" aria-label={t("learnPager")}>
      {prev ? (
        <button type="button" onClick={() => onSelect(prev.pageId)}>
          <small>
            <ArrowLeft size={12} aria-hidden="true" />
            {t("learnPrev")}
          </small>
          <strong>{prev.title}</strong>
        </button>
      ) : (
        <span />
      )}
      {next ? (
        <button type="button" className="learn-pager-next" onClick={() => onSelect(next.pageId)}>
          <small>
            {t("learnNext")}
            <ArrowRight size={12} aria-hidden="true" />
          </small>
          <strong>{next.title}</strong>
        </button>
      ) : (
        <span />
      )}
    </nav>
  );
}

/** A page that has not been generated: what it will contain, how to generate it, and its neighbours in the graph. */
export function PagePending({
  info,
  state,
  busy,
  agentName,
  remaining,
  dependsOn,
  usedBy,
  read,
  running,
  onGenerate,
  onGenerateAll,
  onSelect,
}: {
  info: LearnPageInfo;
  state?: LearnStageState;
  busy: boolean;
  agentName: string;
  /** Pages "Generate remaining" would run; the button shows only when it is more than this one. */
  remaining: number;
  dependsOn: LearnPageInfo[];
  usedBy: LearnPageInfo[];
  read: Set<string>;
  running?: string;
  onGenerate: () => void;
  onGenerateAll: () => void;
  onSelect: (pageId: string) => void;
}) {
  const t = useT();
  const related = (key: MessageKey, items: LearnPageInfo[]) =>
    items.length > 0 && (
      <section className="learn-related">
        <h4>{t(key)}</h4>
        <div>
          {items.map((page) => (
            <button key={page.pageId} type="button" className="learn-chip" onClick={() => onSelect(page.pageId)}>
              <PageIcon state={pageState(page, read, running)} />
              {page.title}
            </button>
          ))}
        </div>
      </section>
    );
  return (
    <div className="learn-pending">
      {info.level && LEVEL_LABEL[info.level] && <div className="learn-kicker">{t(LEVEL_LABEL[info.level])}</div>}
      <h2>{info.title}</h2>
      <div className="learn-pending-card">
        <span className="learn-pending-mark">
          <FileText size={20} aria-hidden="true" />
        </span>
        {state?.status === "running" ? (
          <p role="status">
            <LoaderCircle size={14} className="chat-spin" aria-hidden="true" /> {t("learnGenerating")}
          </p>
        ) : (
          <>
            {state?.error && <ErrorText text={state.error} className="learn-error" />}
            <p>
              <strong>{t("learnNotGeneratedTitle")}</strong>
              <br />
              {t("learnNotGeneratedDesc")}
            </p>
            <div className="learn-pending-actions">
              <button type="button" className="chat-primary-button" disabled={busy} onClick={onGenerate}>
                <Play size={13} aria-hidden="true" />
                {t("learnGeneratePage")}
              </button>
              {remaining > 1 && (
                <button type="button" className="learn-secondary-button" disabled={busy} onClick={onGenerateAll}>
                  {t("learnGenerateAll", { count: remaining })}
                </button>
              )}
            </div>
            <small>{t("learnGenerateHint", { agent: agentName })}</small>
          </>
        )}
      </div>
      {related("learnDependsOn", dependsOn)}
      {related("learnUsedBy", usedBy)}
    </div>
  );
}
