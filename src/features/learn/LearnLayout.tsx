import { useEffect, useRef, useState, type ReactNode } from "react";
import { ChevronsUpDown, LoaderCircle, PanelLeftOpen, Plus } from "lucide-react";
import type { LearnStudySummary } from "../../api/learn";
import { useT } from "../../i18n";
import { useMenuKeys, usePopover } from "./usePopover";

/** Two columns: the left column (switcher + outline) and the main column. Below 760px the left column is a drawer. */
export function LearnLayout({ side, children }: { side: ReactNode; children: ReactNode }) {
  const t = useT();
  const [drawer, setDrawer] = useState(false);
  const toggle = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (!drawer) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || event.defaultPrevented) return;
      setDrawer(false);
      toggle.current?.focus();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [drawer]);
  return (
    <div className="chat-page learn-page" data-testid="learn-page" data-drawer={drawer || undefined}>
      <aside
        className="chat-project-tree learn-sidebar"
        aria-label={t("learnStudies")}
        onClick={(event) => {
          // Choosing a page or study closes the narrow-window drawer.
          if ((event.target as Element).closest("[data-drawer-close]")) setDrawer(false);
        }}
      >
        <header data-tauri-drag-region />
        {side}
      </aside>
      {drawer && <div className="learn-scrim" onClick={() => setDrawer(false)} />}
      <main className="chat-main learn-main">
        <div className="chat-window-bar" data-tauri-drag-region>
          <button
            type="button"
            ref={toggle}
            className="chat-icon-button learn-drawer-toggle"
            aria-label={t("learnContents")}
            title={t("learnContents")}
            aria-expanded={drawer}
            onClick={() => setDrawer(!drawer)}
          >
            <PanelLeftOpen size={16} />
          </button>
        </div>
        {children}
      </main>
    </div>
  );
}

export interface SwitcherProps {
  studies: LearnStudySummary[];
  current?: LearnStudySummary;
  creating: boolean;
  listError: string;
  onSelect: (studyId: string | null) => void;
}

/** Left column top. With a study open: a switcher button whose menu lists every study; otherwise the plain study list. */
export function StudySwitcher({ studies, current, creating, listError, onSelect }: SwitcherProps) {
  const t = useT();
  const menu = usePopover<HTMLDivElement>();
  const keys = useMenuKeys(menu.open);
  const newButton = (
    <button
      type="button"
      className="chat-icon-button"
      aria-label={t("learnNewStudy")}
      title={t("learnNewStudy")}
      aria-pressed={creating}
      data-drawer-close
      onClick={() => onSelect(null)}
    >
      <Plus size={15} />
    </button>
  );
  const error = listError && (
    <p className="learn-error" role="alert">
      {listError}
    </p>
  );
  if (creating || !current)
    return (
      <>
        <div className="chat-project-heading">
          <span>{t("learnStudies")}</span>
          {newButton}
        </div>
        <nav aria-label={t("learnStudies")}>
          <StudyList studies={studies} current={creating ? undefined : current?.studyId} onSelect={onSelect} />
        </nav>
        {error}
      </>
    );
  return (
    <>
      <div className="learn-switcher" ref={menu.ref}>
        <button
          type="button"
          className="learn-switcher-button"
          title={t("learnSwitchStudy")}
          aria-haspopup="menu"
          aria-expanded={menu.open}
          onClick={() => menu.setOpen(!menu.open)}
        >
          <span className="learn-study-status" data-status={current.status} aria-hidden="true" />
          <span className="learn-switcher-text">
            <strong>{current.repo}</strong>
            <small>
              {current.owner}
              {current.commit ? ` · ${current.commit.slice(0, 7)}` : ""}
            </small>
          </span>
          <ChevronsUpDown size={14} aria-hidden="true" />
        </button>
        {newButton}
        {menu.open && (
          <div
            className="learn-switcher-menu"
            role="menu"
            aria-label={t("learnSwitchStudy")}
            ref={keys.list}
            onKeyDown={keys.onKeyDown}
          >
            <StudyList
              studies={studies}
              current={current.studyId}
              menu
              onSelect={(id) => {
                menu.setOpen(false);
                onSelect(id);
              }}
            />
          </div>
        )}
      </div>
      {error}
    </>
  );
}

function StudyList({
  studies,
  current,
  menu,
  onSelect,
}: {
  studies: LearnStudySummary[];
  current?: string;
  menu?: boolean;
  onSelect: (studyId: string) => void;
}) {
  const t = useT();
  return (
    <ul className="learn-study-list" role={menu ? "none" : undefined}>
      {studies.map((study) => (
        <li key={study.studyId} role={menu ? "none" : undefined}>
          <button
            type="button"
            className="chat-tree-session"
            role={menu ? "menuitemradio" : undefined}
            aria-checked={menu ? study.studyId === current : undefined}
            aria-current={!menu && study.studyId === current ? "page" : undefined}
            data-drawer-close
            onClick={() => onSelect(study.studyId)}
          >
            <span className="learn-study-status" data-status={study.status} aria-hidden="true" />
            <span className="chat-tree-title">
              {study.owner}/{study.repo}
            </span>
            {study.status === "running" ? (
              <LoaderCircle size={12} className="chat-spin" role="img" aria-label={t("running")} />
            ) : (
              <small>
                {study.pagesTotal
                  ? `${study.pagesReady}/${study.pagesTotal}`
                  : t(study.status === "failed" ? "error" : "learnStatus_pending")}
              </small>
            )}
          </button>
        </li>
      ))}
      {!studies.length && <li className="chat-tree-empty">{t("learnNoStudies")}</li>}
    </ul>
  );
}
