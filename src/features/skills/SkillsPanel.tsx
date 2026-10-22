import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { BookOpen, FolderPlus, GitBranch, LoaderCircle, RefreshCw, Search, Trash2 } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { skillsApi, type SkillsApi, type SkillDetail, type SkillsLibrary, type SkillsOperation } from "../../api/skills";
import { hasTauriRuntime } from "../../hooks/runtime";
import { useT } from "../../i18n";
import { Markdown } from "../chat/MarkdownView";
import { mergeOperation, operationActionKey, skillBody, skillsError, visibleSkills } from "./model";
import { InstallPanel } from "./InstallPanel";
import { InstalledSkills } from "./InstalledSkills";
import "./skills.css";

const emptyLibrary: SkillsLibrary = { sources: [], skills: [], issues: [] };

export function SkillsPanel({ projectName, projectPath = null, api = skillsApi, desktop = hasTauriRuntime() }: {
  projectName?: string; projectPath?: string | null; api?: SkillsApi; desktop?: boolean;
}) {
  const t = useT();
  const [library, setLibrary] = useState<SkillsLibrary>(emptyLibrary);
  const [sourceId, setSourceId] = useState("");
  const [query, setQuery] = useState("");
  const [selectedId, setSelectedId] = useState("");
  const [detail, setDetail] = useState<SkillDetail | null>(null);
  const [detailBusy, setDetailBusy] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [detailError, setDetailError] = useState("");
  const [localPath, setLocalPath] = useState("");
  const [url, setUrl] = useState("");
  const [adding, setAdding] = useState(false);
  const [tab, setTab] = useState<"overview" | "markdown" | "files" | "install">("overview");
  const [view, setView] = useState<"library" | "installed">("library");
  const [installRevision, setInstallRevision] = useState(0);
  const [operations, setOperations] = useState<SkillsOperation[]>([]);
  const [requestedImport, setRequestedImport] = useState<string | null>(null);
  const libraryRequest = useRef(0);
  const detailRequest = useRef(0);
  const mounted = useRef(false);
  const terminalSeen = useRef(new Set<string>());
  const searchInput = useRef<HTMLInputElement>(null);
  const filtered = useMemo(() => visibleSkills(library, sourceId, query), [library, sourceId, query]);
  const activeId = filtered.some(skill => skill.id === selectedId) ? selectedId : filtered[0]?.id ?? "";
  const source = library.sources.find(item => item.id === sourceId);

  useEffect(() => {
    if (sourceId && !library.sources.some(item => item.id === sourceId)) setSourceId("");
  }, [library.sources, sourceId]);
  useEffect(() => {
    const operation = operations.find(item => item.operationId === requestedImport);
    if (!operation || operation.phase === "running") return;
    if (operation.phase === "committed" && library.sources.some(item => item.id === operation.sourceId)) {
      setSourceId(operation.sourceId); setSelectedId(""); setQuery(""); setRequestedImport(null);
    } else if (operation.phase !== "committed") setRequestedImport(null);
  }, [operations, requestedImport, library.sources]);

  const load = useCallback(async () => {
    const request = ++libraryRequest.current;
    try {
      const next = await api.list();
      if (mounted.current && request === libraryRequest.current) { setLibrary(next); setError(""); }
    } catch (failure) { if (mounted.current && request === libraryRequest.current) setError(skillsError(failure)); }
  }, [api]);

  useEffect(() => {
    mounted.current = true;
    let stopped = false; let unlisten: (() => void) | undefined;
    let polling = false;
    const receive = (operation: SkillsOperation) => {
      if (stopped) return;
      setOperations(previous => mergeOperation(previous, operation));
      if (operation.phase !== "running" && !terminalSeen.current.has(operation.operationId)) {
        terminalSeen.current.add(operation.operationId);
        if (operation.phase === "committed" && ["import", "refresh"].includes(operation.action)) void load();
        if (["install", "uninstall", "recover"].includes(operation.action)) setInstallRevision(value => value + 1);
      }
    };
    const poll = async () => {
      if (stopped || polling) return;
      polling = true;
      try { for (const operation of await api.operations()) receive(operation); }
      catch (failure) { if (!stopped) setError(skillsError(failure)); }
      finally { polling = false; }
    };
    if (desktop) {
      void api.subscribe(receive).then(stop => { if (stopped) stop(); else unlisten = stop; }).catch(failure => { if (!stopped) setError(skillsError(failure)); });
      void load(); void poll();
    }
    const timer = desktop ? window.setInterval(() => { void poll(); }, 1500) : undefined;
    return () => { stopped = true; mounted.current = false; libraryRequest.current++; detailRequest.current++; unlisten?.(); if (timer !== undefined) window.clearInterval(timer); };
  }, [api, desktop, load]);

  useEffect(() => {
    const request = ++detailRequest.current;
    setDetail(null); setDetailError("");
    if (!activeId || !desktop) { setDetailBusy(false); return; }
    setDetailBusy(true);
    void api.detail(activeId).then(next => {
      if (mounted.current && request === detailRequest.current) setDetail(next);
    }).catch(failure => { if (mounted.current && request === detailRequest.current) setDetailError(skillsError(failure)); })
      .finally(() => { if (mounted.current && request === detailRequest.current) setDetailBusy(false); });
  }, [activeId, api, desktop, library]);

  useEffect(() => setTab("overview"), [activeId]);

  async function action(work: () => Promise<void>) {
    if (busy) return;
    setBusy(true); setError("");
    try { await work(); } catch (failure) { if (mounted.current) setError(skillsError(failure)); }
    finally { if (mounted.current) setBusy(false); }
  }
  async function adopt(next: SkillsLibrary) {
    libraryRequest.current++;
    if (mounted.current) { setLibrary(next); setAdding(false); }
  }
  async function addLocal() {
    await action(async () => {
      const knownSources = new Set(library.sources.map(item => item.id));
      const next = await api.addLocal(localPath.trim());
      await adopt(next);
      if (mounted.current) { setSourceId(next.sources.find(item => !knownSources.has(item.id))?.id ?? next.sources.find(item => item.localPath === localPath.trim())?.id ?? sourceId); setQuery(""); setLocalPath(""); }
    });
  }
  async function importRepo() {
    await action(async () => {
      const operation = await api.importRepository(url.trim());
      if (mounted.current) { setRequestedImport(operation.operationId); setOperations(previous => mergeOperation(previous, operation)); setAdding(false); setUrl(""); }
    });
  }
  async function browse() {
    await action(async () => {
      const selected = await open({ directory: true, multiple: false, title: t("skillsChooseFolder") });
      if (typeof selected === "string" && mounted.current) setLocalPath(selected);
    });
  }
  const validDetail = detail?.summary.id === activeId ? detail : null;
  return <div className="skills-page" data-testid="skills-panel">
    {projectName && <div className="skills-context">{t("skillsProjectContext", { project: projectName })}</div>}
    {!desktop && <p className="skills-notice" role="status">{t("skillsDesktopOnly")}</p>}
    {error && <p className="skills-error" role="alert">{error}<button type="button" onClick={() => { void load(); }}>{t("skillsRetry")}</button></p>}
    <nav className="skills-view-tabs" aria-label={t("skills")}><button type="button" aria-pressed={view === "library"} onClick={() => setView("library")}>{t("skillsLibraryView")}</button><button type="button" aria-pressed={view === "installed"} disabled={!desktop} onClick={() => setView("installed")}>{t("skillsInstalledView")}</button></nav>
    {view === "installed" ? <InstalledSkills api={api} projectPath={projectPath} revision={installRevision} /> : <div className="skills-workspace">
      <aside className="skills-sources" aria-label={t("skillsSources")}>
        <div className="skills-section-heading"><h2>{t("skillsSources")}</h2><button type="button" aria-label={t("skillsAddSource")} disabled={!desktop || busy} onClick={() => setAdding(!adding)}><FolderPlus size={17} /></button></div>
        <button type="button" aria-pressed={!sourceId} onClick={() => setSourceId("")}>{t("skillsAll")}<span>{library.skills.length}</span></button>
        {library.sources.map(item => <button type="button" key={item.id} aria-pressed={sourceId === item.id} onClick={() => { setSourceId(item.id); setQuery(""); }}><span className="skills-source-name">{item.kind === "git" && <GitBranch size={14} />}{item.name}</span><span>{library.skills.filter(skill => skill.sourceId === item.id).length}</span></button>)}
        {!library.sources.length && <p>{t("skillsSourceHint")}</p>}
        {source && <div className="skills-source-actions"><code>{source.git?.url ?? source.localPath}</code>{source.revision && <small>{source.revision.slice(0, 12)}</small>}
          <button type="button" disabled={busy || operations.some(op => op.sourceId === source.id && op.phase === "running")} onClick={() => { void action(async () => { const operation = await api.refresh(source.id); if (mounted.current) setOperations(previous => mergeOperation(previous, operation)); }); }}><RefreshCw size={14} />{t("skillsRefresh")}</button>
          <button type="button" disabled={busy} onClick={() => { void action(async () => { await adopt(await api.remove(source.id)); if (mounted.current) setSourceId(""); }); }}><Trash2 size={14} />{t("skillsRemoveSource")}</button><small>{t("skillsRemoveHint")}</small></div>}
      </aside>
      <main className="skills-library">
        {adding && <section className="skills-add-source" aria-label={t("skillsAddSource")}>
          <h2>{t("skillsAddSource")}</h2><div className="skills-source-field"><label htmlFor="skills-local-path">{t("skillsLocalFolder")}</label><div><input id="skills-local-path" value={localPath} onChange={event => setLocalPath(event.target.value)} placeholder="/path/to/skills" /><button type="button" disabled={busy} onClick={() => { void browse(); }}>{t("browse")}</button><button type="button" disabled={busy || !localPath.trim()} onClick={() => { void addLocal(); }}>{t("skillsAdd")}</button></div></div>
          <div className="skills-source-field"><label htmlFor="skills-git-url">{t("skillsGitUrl")}</label><div><input id="skills-git-url" value={url} onChange={event => setUrl(event.target.value)} placeholder="https://github.com/owner/repository" /><button type="button" disabled={busy || !url.trim()} onClick={() => { void importRepo(); }}>{t("skillsImport")}</button></div></div>
          <p>{t("skillsRefreshHint")}</p>
        </section>}
        <label className="skills-search"><Search size={17} /><input ref={searchInput} value={query} onChange={event => setQuery(event.target.value)} placeholder={t("skillsSearch")} aria-label={t("skillsSearch")} /><span>{filtered.length}</span></label>
        {!!library.issues.length && <details className="skills-issues"><summary>{t("skillsIssues", { count: library.issues.length })}</summary>{library.issues.map((issue, index) => <p key={index}><strong>{library.sources.find(item => item.id === issue.sourceId)?.name}</strong> {issue.relativePath}: {issue.message}</p>)}</details>}
        <div className="skills-columns">
          <nav className="skills-list" aria-label={t("skillsList")}>
            {filtered.map(skill => <button key={skill.id} type="button" aria-current={activeId === skill.id ? "true" : undefined} onClick={() => setSelectedId(skill.id)}><strong>{skill.title}</strong><span>{skill.description}</span><small>{skill.category} · {library.sources.find(item => item.id === skill.sourceId)?.name}</small></button>)}
            {!filtered.length && <div className="skills-empty"><BookOpen size={30} /><h2>{t(query ? "skillsNoMatches" : "skillsEmpty")}</h2><p>{t(query ? "skillsSearchHint" : "skillsSourceHint")}</p>{query && <button type="button" onClick={() => setQuery("")}>{t("skillsClearSearch")}</button>}</div>}
          </nav>
          <article className="skills-detail" aria-busy={detailBusy}>
            {detailBusy && <p role="status"><LoaderCircle size={16} className="chat-spin" />{t("loading")}</p>}
            {detailError && <p className="skills-error" role="alert">{detailError}</p>}
            {validDetail ? <><header><small>{validDetail.summary.category}</small><h2>{validDetail.summary.title}</h2><p>{validDetail.summary.description}</p><code>{validDetail.summary.relativePath}</code></header>
              <nav className="skills-tabs" aria-label={t("skillsDetailTabs")}>{(["overview", "markdown", "files", "install"] as const).map(value => <button type="button" aria-pressed={tab === value} key={value} onClick={() => setTab(value)}>{t(value === "overview" ? "skillsOverview" : value === "markdown" ? "skillsMarkdown" : value === "files" ? "skillsFiles" : "skillsInstallTab")}</button>)}</nav>
              {tab === "overview" && <div className="skills-markdown"><Markdown text={skillBody(validDetail.content)} /></div>}
              {tab === "markdown" && <pre>{validDetail.content}</pre>}
              {tab === "install" && <InstallPanel api={api} skillId={activeId} projectPath={projectPath} revision={installRevision} />}
              {tab === "files" && <div>{validDetail.relatedFiles.map(file => <details key={file.relativePath}><summary>{file.relativePath} <small>{file.sizeBytes} B</small></summary>{file.content === null ? <p>{t("skillsNoPreview")}</p> : <pre>{file.content}</pre>}</details>)}{!validDetail.relatedFiles.length && <p>{t("skillsNoFiles")}</p>}</div>}
            </> : !detailBusy && <div className="skills-empty"><p>{t("skillsSelect")}</p></div>}
          </article>
        </div>
      </main>
    </div>}
    {!!operations.length && <section className="skills-operations" aria-label={t("skillsActivity")}><h2>{t("skillsActivity")}</h2>{operations.slice(0, 4).map(operation => <div key={operation.operationId} role="status"><span>{t(operationActionKey(operation.action))} · {library.sources.find(item => item.id === operation.sourceId)?.name ?? t(["install", "uninstall", "recover"].includes(operation.action) ? "skillsManagedInstallations" : "skillsSources")} · {t(operation.phase === "running" ? "skillsWorking" : operation.phase === "committed" ? "skillsDone" : operation.phase === "cancelled" ? "skillsCancelled" : "skillsFailed")}</span>{operation.error && <span className="skills-error">{operation.error.message}</span>}{!!operation.executions?.length && <details><summary>{t("skillsExecutionLog")}</summary>{operation.executions.map((execution, index) => <pre key={index}>{[execution.program + " " + execution.args.join(" "), execution.cwd, t("exitCodeLabel") + ": " + (execution.exitCode ?? "—"), execution.error].filter(Boolean).join("\n")}</pre>)}</details>}{operation.phase === "running" && <button type="button" onClick={() => { void action(() => api.cancel(operation.operationId)); }}>{t("skillsCancel")}</button>}</div>)}</section>}
  </div>;
}
