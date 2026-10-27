import { useEffect, useRef, useState } from "react";
import { ChevronRight, File, Folder, FolderTree, X } from "lucide-react";
import type { LearnApi, LearnSourceText, LearnTreeEntry } from "../../api/learn";
import { useT } from "../../i18n";

export interface SourceTarget { path: string; startLine?: number; endLine?: number }

/** Read-only view of the study checkout with the cited range highlighted. */
export function SourceViewer({ api, studyId, target, onOpen, onClose }: {
  api: LearnApi; studyId: string; target: SourceTarget | null; onOpen: (target: SourceTarget) => void; onClose: () => void;
}) {
  const t = useT();
  const [source, setSource] = useState<LearnSourceText | null>(null);
  const [error, setError] = useState("");
  const [browsing, setBrowsing] = useState(!target);
  const [dir, setDir] = useState("");
  const [entries, setEntries] = useState<LearnTreeEntry[]>([]);
  const highlight = useRef<HTMLDivElement>(null);
  useEffect(() => { if (target) setBrowsing(false); }, [target]);
  useEffect(() => {
    if (!target) { setSource(null); return; }
    let disposed = false;
    setError("");
    api.readSource(studyId, target.path, target.startLine, target.endLine)
      .then(result => { if (!disposed) setSource(result); })
      .catch(reason => { if (!disposed) { setSource(null); setError(String(reason)); } });
    return () => { disposed = true; };
  }, [api, studyId, target]);
  useEffect(() => { highlight.current?.scrollIntoView({ block: "center" }); }, [source]);
  useEffect(() => {
    if (!browsing) return;
    let disposed = false;
    api.listTree(studyId, dir).then(result => { if (!disposed) setEntries(result); }).catch(reason => { if (!disposed) setError(String(reason)); });
    return () => { disposed = true; };
  }, [api, studyId, dir, browsing]);
  const lines = source ? source.text.replace(/\n$/, "").split("\n") : [];
  const start = target?.startLine ?? 0, end = target?.endLine ?? start;
  const crumbs = dir ? dir.split("/") : [];
  return <aside className="learn-source" aria-label={t("learnSource")}>
    <header>
      <button type="button" className="chat-icon-button" aria-pressed={browsing} aria-label={t("learnBrowseFiles")} title={t("learnBrowseFiles")} onClick={() => setBrowsing(!browsing)}><FolderTree size={15} /></button>
      <code title={target?.path}>{browsing ? `/${dir}` : target ? `${target.path}${start ? `:${start}${end > start ? `-${end}` : ""}` : ""}` : t("learnSource")}</code>
      <button type="button" className="chat-icon-button" aria-label={t("close")} title={t("close")} onClick={onClose}><X size={15} /></button>
    </header>
    {error && <p className="learn-error" role="alert">{error}</p>}
    {browsing ? <div className="learn-tree">
      <nav className="learn-crumbs"><button type="button" onClick={() => setDir("")}>{t("learnRepoRoot")}</button>
        {crumbs.map((part, index) => <span key={index}><ChevronRight size={12} aria-hidden="true" /><button type="button" onClick={() => setDir(crumbs.slice(0, index + 1).join("/"))}>{part}</button></span>)}</nav>
      <ul>{entries.map(entry => <li key={entry.path}>
        <button type="button" disabled={entry.kind === "symlink"} onClick={() => entry.kind === "dir" ? setDir(entry.path) : onOpen({ path: entry.path })}>
          {entry.kind === "dir" ? <Folder size={14} aria-hidden="true" /> : <File size={14} aria-hidden="true" />}<span>{entry.name}</span>
        </button></li>)}</ul>
    </div> : source ? <div className="learn-code" role="region" aria-label={source.path}>
      {source.truncated && <p className="learn-note">{t("learnTruncated", { first: source.firstLine, last: source.firstLine + lines.length - 1, total: source.totalLines })}</p>}
      {start > source.totalLines && <p className="learn-error">{t("learnRangeMissing", { total: source.totalLines })}</p>}
      <pre>{lines.map((line, index) => {
        const number = source.firstLine + index;
        const marked = start > 0 && number >= start && number <= end;
        return <div key={number} className="learn-code-line" data-marked={marked || undefined} ref={marked && number === start ? highlight : undefined}>
          <span className="learn-code-number" aria-hidden="true">{number}</span><code>{line || " "}</code>
        </div>;
      })}</pre>
    </div> : !error && <p className="learn-note">{t("learnPickSource")}</p>}
  </aside>;
}
