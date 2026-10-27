import { useContext, useEffect, useState } from "react";
import { GraduationCap, LoaderCircle, Plus } from "lucide-react";
import { tauriLearnApi, type LearnStudySummary } from "../../api/learn";
import { hasTauriRuntime } from "../../hooks/runtime";
import { useT } from "../../i18n";
import { useAppState } from "../../state/AppStateContext";
import { NewStudyForm } from "./NewStudyForm";
import { LearnPreviewContext } from "./previewContext";
import { StudyView } from "./StudyView";
import "./learn.css";

export function LearnPage() {
  const t = useT();
  const { state } = useAppState();
  const preview = useContext(LearnPreviewContext);
  const api = preview ?? tauriLearnApi;
  const available = !!preview || hasTauriRuntime();
  const [studies, setStudies] = useState<LearnStudySummary[]>([]);
  const [listError, setListError] = useState("");
  const [selected, setSelected] = useState<string | null>(() => { try { return localStorage.getItem("loom-learn-study"); } catch { return null; } });
  const [newStudy, setNewStudy] = useState(false);
  const [revision, setRevision] = useState(0);
  useEffect(() => { try { if (selected) localStorage.setItem("loom-learn-study", selected); else localStorage.removeItem("loom-learn-study"); } catch { /* per-device convenience only */ } }, [selected]);
  useEffect(() => {
    if (!available) return;
    let disposed = false;
    api.list().then(list => {
      if (disposed) return;
      setStudies(list.studies);
      setListError(list.storageErrors.join("\n"));
      setSelected(current => current && list.studies.some(item => item.studyId === current) ? current : list.studies[0]?.studyId ?? null);
    }).catch(reason => { if (!disposed) setListError(String(reason)); });
    return () => { disposed = true; };
  }, [api, available, revision]);
  useEffect(() => {
    if (!available) return;
    let unlisten: (() => void) | undefined, disposed = false;
    void api.subscribe(() => setRevision(value => value + 1)).then(stop => { if (disposed) stop(); else unlisten = stop; });
    return () => { disposed = true; unlisten?.(); };
  }, [api, available]);
  const creating = newStudy || selected === null;
  const select = (id: string | null) => { setNewStudy(id === null); setSelected(id ?? selected); };
  return <div className="chat-page learn-page" data-testid="learn-page">
    <aside className="chat-project-tree learn-sidebar">
      <header data-tauri-drag-region />
      <div className="chat-project-heading"><span>{t("learnStudies")}</span>
        <button type="button" className="chat-icon-button" aria-label={t("learnNewStudy")} title={t("learnNewStudy")} aria-pressed={creating} onClick={() => select(null)}><Plus size={15} /></button></div>
      <nav aria-label={t("learnStudies")}><ul className="learn-study-list">
        {studies.map(study => <li key={study.studyId}>
          <button type="button" className="chat-tree-session" aria-current={!creating && study.studyId === selected ? "page" : undefined} onClick={() => select(study.studyId)}>
            <span className="learn-study-status" data-status={study.status} aria-hidden="true" />
            <span className="chat-tree-title">{study.owner}/{study.repo}</span>
            {study.status === "running" ? <LoaderCircle size={12} className="chat-spin" role="img" aria-label={t("running")} />
              : <small>{study.pagesTotal ? `${study.pagesReady}/${study.pagesTotal}` : t(study.status === "failed" ? "error" : "learnStatus_pending")}</small>}
          </button>
        </li>)}
        {!studies.length && <li className="chat-tree-empty">{t("learnNoStudies")}</li>}
      </ul></nav>
      {listError && <p className="learn-error" role="alert">{listError}</p>}
    </aside>
    <main className="chat-main learn-main">
      <div className="chat-window-bar" data-tauri-drag-region />
      {!available ? <div className="learn-main-empty"><span className="chat-empty-mark"><GraduationCap size={24} /></span><p>{t("learnDesktopOnly")}</p></div>
        : creating ? <NewStudyForm api={api} agents={state.agents} diagnostics={state.agentDiagnostics} onCreated={id => { select(id); setRevision(value => value + 1); }} />
        : <StudyView api={api} studyId={selected!} revision={revision} onDeleted={() => { setSelected(null); setRevision(value => value + 1); }} />}
    </main>
  </div>;
}
