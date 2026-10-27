import { useEffect, useRef, useState } from "react";
import { ChevronRight, File, Folder, FolderTree, X } from "lucide-react";
import type { SourceReader, SourceTarget, SourceText, TreeEntry } from "../api/sourceFiles";
import { useT } from "../i18n";
import "./SourceViewer.css";

/** Read-only view of a feature's source root (Learn checkout, Research project) with the cited range highlighted. `reader` must be stable per root. */
export function SourceViewer({ reader, target, onOpen, onClose }: {
  reader: SourceReader; target: SourceTarget | null; onOpen: (target: SourceTarget) => void; onClose: () => void;
}) {
  const t = useT();
  const [source, setSource] = useState<SourceText | null>(null);
  const [error, setError] = useState("");
  const [browsing, setBrowsing] = useState(!target);
  const [dir, setDir] = useState("");
  const [entries, setEntries] = useState<TreeEntry[]>([]);
  const highlight = useRef<HTMLDivElement>(null);
  useEffect(() => { if (target) setBrowsing(false); }, [target]);
  useEffect(() => {
    if (!target) { setSource(null); return; }
    let disposed = false;
    setError("");
    reader.readSource(target.path, target.startLine, target.endLine)
      .then(result => { if (!disposed) setSource(result); })
      .catch(reason => { if (!disposed) { setSource(null); setError(String(reason)); } });
    return () => { disposed = true; };
  }, [reader, target]);
  // Scroll only the code pane: scrollIntoView would also shift the host's overflow-hidden columns and hide its header.
  useEffect(() => {
    const line = highlight.current, pane = line?.closest<HTMLElement>(".source-code");
    if (line && pane) pane.scrollTop += line.getBoundingClientRect().top - pane.getBoundingClientRect().top - (pane.clientHeight - line.offsetHeight) / 2;
  }, [source]);
  useEffect(() => {
    if (!browsing) return;
    let disposed = false;
    reader.listTree(dir).then(result => { if (!disposed) setEntries(result); }).catch(reason => { if (!disposed) setError(String(reason)); });
    return () => { disposed = true; };
  }, [reader, dir, browsing]);
  const lines = source ? source.text.replace(/\n$/, "").split("\n") : [];
  const start = target?.startLine ?? 0, end = target?.endLine ?? start;
  const crumbs = dir ? dir.split("/") : [];
  return <aside className="source-viewer" aria-label={t("sourceViewer")}>
    <header>
      <button type="button" className="chat-icon-button" aria-pressed={browsing} aria-label={t("sourceBrowse")} title={t("sourceBrowse")} onClick={() => setBrowsing(!browsing)}><FolderTree size={15} /></button>
      <code title={target?.path}>{browsing ? `/${dir}` : target ? `${target.path}${start ? `:${start}${end > start ? `-${end}` : ""}` : ""}` : t("sourceViewer")}</code>
      <button type="button" className="chat-icon-button" aria-label={t("close")} title={t("close")} onClick={onClose}><X size={15} /></button>
    </header>
    {error && <p className="source-error" role="alert">{error}</p>}
    {browsing ? <div className="source-tree">
      <nav className="source-crumbs"><button type="button" onClick={() => setDir("")}>{t("sourceRoot")}</button>
        {crumbs.map((part, index) => <span key={index}><ChevronRight size={12} aria-hidden="true" /><button type="button" onClick={() => setDir(crumbs.slice(0, index + 1).join("/"))}>{part}</button></span>)}</nav>
      <ul>{entries.map(entry => <li key={entry.path}>
        <button type="button" disabled={entry.kind === "symlink"} onClick={() => entry.kind === "dir" ? setDir(entry.path) : onOpen({ path: entry.path })}>
          {entry.kind === "dir" ? <Folder size={14} aria-hidden="true" /> : <File size={14} aria-hidden="true" />}<span>{entry.name}</span>
        </button></li>)}</ul>
    </div> : source ? <div className="source-code" role="region" aria-label={source.path}>
      {source.truncated && <p className="source-note">{t("sourceTruncated", { first: source.firstLine, last: source.firstLine + lines.length - 1, total: source.totalLines })}</p>}
      {start > source.totalLines && <p className="source-error">{t("sourceRangeMissing", { total: source.totalLines })}</p>}
      <pre>{lines.map((line, index) => {
        const number = source.firstLine + index;
        const marked = start > 0 && number >= start && number <= end;
        return <div key={number} className="source-code-line" data-marked={marked || undefined} ref={marked && number === start ? highlight : undefined}>
          <span className="source-code-number" aria-hidden="true">{number}</span><code>{line || " "}</code>
        </div>;
      })}</pre>
    </div> : !error && <p className="source-note">{t("sourcePick")}</p>}
  </aside>;
}
