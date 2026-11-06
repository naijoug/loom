import { useContext, useEffect, useState } from "react";
import { LoaderCircle, Plus, Telescope } from "lucide-react";
import { tauriResearchApi, type ResearchSummary } from "../../api/research";
import { targetResearchApi } from "../../api/researchTargets";
import { hasTauriRuntime } from "../../hooks/runtime";
import { useT } from "../../i18n";
import { useAppState } from "../../state/AppStateContext";
import { NewResearchForm } from "./NewResearchForm";
import { ResearchPreviewContext, ResearchTargetPreviewContext } from "./previewContext";
import { ResearchView } from "./ResearchView";
import "./research.css";

export function ResearchPage() {
  const t = useT();
  const { state } = useAppState();
  const preview = useContext(ResearchPreviewContext);
  const targetPreview = useContext(ResearchTargetPreviewContext);
  const api = preview ?? tauriResearchApi;
  const targets = targetPreview ?? targetResearchApi;
  const available = !!preview || hasTauriRuntime();
  const [canStart, setCanStart] = useState(Boolean(targetPreview));
  const [items, setItems] = useState<ResearchSummary[]>([]);
  const [listError, setListError] = useState("");
  const [selected, setSelected] = useState<string | null>(() => { try { return localStorage.getItem("loom-research-item"); } catch { return null; } });
  const [creating, setCreating] = useState(false);
  const [revision, setRevision] = useState(0);
  useEffect(() => { try { if (selected) localStorage.setItem("loom-research-item", selected); else localStorage.removeItem("loom-research-item"); } catch { /* per-device convenience only */ } }, [selected]);
  useEffect(() => {
    if (!available) return;
    let disposed = false;
    api.list().then(list => {
      if (disposed) return;
      setItems(list.items);
      setListError(list.storageErrors.join("\n"));
      setSelected(current => current && list.items.some(item => item.researchId === current) ? current : list.items[0]?.researchId ?? null);
    }).catch(reason => { if (!disposed) setListError(String(reason)); });
    return () => { disposed = true; };
  }, [api, available, revision]);
  useEffect(() => {
    if (!available) return;
    let unlisten: (() => void) | undefined, disposed = false;
    void api.subscribe(() => setRevision(value => value + 1)).then(stop => { if (disposed) stop(); else unlisten = stop; });
    return () => { disposed = true; unlisten?.(); };
  }, [api, available]);
  useEffect(() => {
    if (targetPreview) { setCanStart(true); return; }
    if (!available || preview) return;
    let disposed = false;
    targetResearchApi.capabilities().then(caps => { if (!disposed) setCanStart(caps.singleShot); }).catch(() => { if (!disposed) setCanStart(false); });
    return () => { disposed = true; };
  }, [available, preview, targetPreview]);
  const showForm = creating || selected === null;
  const dot = (status: string) => ["queued", "preflight", "collecting", "analyzing", "reporting", "cancelling", "running"].includes(status) ? "running" : status === "completed" || status === "ready" ? "ready" : "failed";
  const select = (id: string | null) => { setCreating(id === null); if (id) setSelected(id); };
  return <div className="chat-page research-page" data-testid="research-page">
    <aside className="chat-project-tree research-sidebar">
      <header data-tauri-drag-region />
      <div className="chat-project-heading"><span>{t("researchHistory")}</span>
        <button type="button" className="chat-icon-button" aria-label={t("researchNew")} title={t("researchNew")} aria-pressed={showForm} onClick={() => select(null)}><Plus size={15} /></button></div>
      <nav aria-label={t("researchHistory")}><ul className="research-list">
        {items.map(item => <li key={item.researchId}>
          <button type="button" className="chat-tree-session" aria-current={!showForm && item.researchId === selected ? "page" : undefined} onClick={() => select(item.researchId)}>
            <span className="research-status" data-status={dot(item.status)} aria-hidden="true" />
            <span className="chat-tree-title" title={item.title}>{item.title}</span>
            {dot(item.status) === "running" ? <LoaderCircle size={12} className="chat-spin" role="img" aria-label={t("running")} />
              : <small>{item.workflow === "target" ? t("researchTargetLabel") : item.projectName ?? t("researchNoProjectLabel")}</small>}
          </button>
        </li>)}
        {!items.length && <li className="chat-tree-empty">{t("researchEmpty")}</li>}
      </ul></nav>
      {listError && <p className="research-error" role="alert">{listError}</p>}
    </aside>
    <main className="chat-main research-main">
      <div className="chat-window-bar" data-tauri-drag-region />
      {!available ? <div className="research-main-empty"><span className="chat-empty-mark"><Telescope size={24} /></span><p>{t("researchDesktopOnly")}</p></div>
        : showForm ? <NewResearchForm api={api} targets={targets} targetsEnabled={canStart} agents={state.agents} diagnostics={state.agentDiagnostics} projects={state.projects.recent} onCreated={id => { setCreating(false); setSelected(id); setRevision(value => value + 1); }} />
        : <ResearchView api={api} targets={targets} canStart={canStart} researchId={selected!} revision={revision} onDeleted={() => { setSelected(null); setRevision(value => value + 1); }} />}
    </main>
  </div>;
}
