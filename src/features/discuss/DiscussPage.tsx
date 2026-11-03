import { useContext, useEffect, useState } from "react";
import { LoaderCircle, Plus, MessagesSquare } from "lucide-react";
import { tauriDiscussApi, type DiscussionSummary } from "../../api/discuss";
import { hasTauriRuntime } from "../../hooks/runtime";
import { useT } from "../../i18n";
import { useAppState } from "../../state/AppStateContext";
import { NewDiscussForm } from "./NewDiscussForm";
import { DiscussPreviewContext } from "./previewContext";
import { DiscussView } from "./DiscussView";
import "./discuss.css";

export function DiscussPage() {
  const t = useT();
  const { state } = useAppState();
  const preview = useContext(DiscussPreviewContext);
  const api = preview ?? tauriDiscussApi;
  const available = !!preview || hasTauriRuntime();
  const [items, setItems] = useState<DiscussionSummary[]>([]);
  const [listError, setListError] = useState("");
  const [selected, setSelected] = useState<string | null>(() => { try { return localStorage.getItem("loom-discuss-item"); } catch { return null; } });
  const [creating, setCreating] = useState(false);
  const [revision, setRevision] = useState(0);
  useEffect(() => { try { if (selected) localStorage.setItem("loom-discuss-item", selected); else localStorage.removeItem("loom-discuss-item"); } catch { /* per-device convenience only */ } }, [selected]);
  useEffect(() => {
    if (!available) return;
    let disposed = false;
    api.list().then(list => {
      if (disposed) return;
      setItems(list.items);
      setListError(list.storageErrors.join("\n"));
      setSelected(current => current && list.items.some(item => item.discussionId === current) ? current : list.items[0]?.discussionId ?? null);
    }).catch(reason => { if (!disposed) setListError(String(reason)); });
    return () => { disposed = true; };
  }, [api, available, revision]);
  useEffect(() => {
    if (!available) return;
    let unlisten: (() => void) | undefined, disposed = false;
    void api.subscribe(() => setRevision(value => value + 1)).then(stop => { if (disposed) stop(); else unlisten = stop; }).catch(reason => { if (!disposed) setListError(String(reason)); });
    return () => { disposed = true; unlisten?.(); };
  }, [api, available]);
  const showForm = creating || selected === null;
  const select = (id: string | null) => { setCreating(id === null); if (id) setSelected(id); };
  return <div className="chat-page discuss-page" data-testid="discuss-page">
    <aside className="chat-project-tree discuss-sidebar">
      <header data-tauri-drag-region />
      <div className="chat-project-heading"><span>{t("discussHistory")}</span>
        <button type="button" className="chat-icon-button" aria-label={t("discussNew")} title={t("discussNew")} aria-pressed={showForm} onClick={() => select(null)}><Plus size={15} /></button></div>
      <nav aria-label={t("discussHistory")}><ul className="discuss-list">
        {items.map(item => <li key={item.discussionId}>
          <button type="button" className="chat-tree-session" aria-current={!showForm && item.discussionId === selected ? "page" : undefined} onClick={() => select(item.discussionId)}>
            <span className="discuss-status" data-status={item.status} aria-hidden="true" />
            <span className="chat-tree-title" title={item.title}>{item.title}</span>
            {item.status === "running" ? <LoaderCircle size={12} className="chat-spin" role="img" aria-label={t("running")} />
              : <small>{item.projectName ?? t("discussNoProjectLabel")}</small>}
          </button>
        </li>)}
        {!items.length && <li className="chat-tree-empty">{t("discussEmpty")}</li>}
      </ul></nav>
      {listError && <p className="discuss-error" role="alert">{listError}</p>}
    </aside>
    <main className="chat-main discuss-main">
      <div className="chat-window-bar" data-tauri-drag-region />
      {!available ? <div className="discuss-main-empty"><span className="chat-empty-mark"><MessagesSquare size={24} /></span><p>{t("discussDesktopOnly")}</p></div>
        : showForm ? <NewDiscussForm api={api} agents={state.agents} diagnostics={state.agentDiagnostics} projects={state.projects.recent} onCreated={id => { setCreating(false); setSelected(id); setRevision(value => value + 1); }} />
        : <DiscussView api={api} discussionId={selected!} revision={revision} onDeleted={() => { setSelected(null); setRevision(value => value + 1); }} />}
    </main>
  </div>;
}
