import { useEffect, useRef, useState } from "react";
import { Archive, ArchiveRestore, Download, Ellipsis, LoaderCircle, Pencil, RotateCcw } from "lucide-react";
import type { ChatPermissionMode, ChatSession } from "../../domain";
import { useT } from "../../i18n";
import { ModeSwitch } from "./ModeSwitch";
export function ChatSessionHeader({ session, projectName, running, onMode, onRename, onArchive, onClearResume, onExport, exporting }: {
  session: ChatSession; projectName: string; running: boolean; onMode: (mode: ChatPermissionMode) => void;
  onRename: (title: string) => void; onArchive: () => void; onClearResume: () => void; onExport: () => void; exporting: boolean;
}) {
  const t = useT();
  const [renaming, setRenaming] = useState(false);
  const [title, setTitle] = useState(session.title);
  const menu = useRef<HTMLDetailsElement>(null);
  useEffect(() => {
    const close = (event: MouseEvent) => { if (menu.current?.open && !menu.current.contains(event.target as Node)) menu.current.open = false; };
    document.addEventListener("mousedown", close); return () => document.removeEventListener("mousedown", close);
  }, []);
  const act = (action: () => void) => () => { if (menu.current) menu.current.open = false; action(); };
  return <header className="chat-session-header" data-tauri-drag-region>
    <div className="chat-session-title">
      <span className="chat-crumb">{projectName}<span aria-hidden="true">/</span></span>
      {renaming ? <input aria-label={t("sessionTitle")} value={title} autoFocus onChange={event => setTitle(event.target.value)} onKeyDown={event => {
        if (event.nativeEvent.isComposing || event.nativeEvent.keyCode === 229) return;
        if (event.key === "Enter") { if (event.currentTarget.value.trim()) onRename(event.currentTarget.value.trim()); setRenaming(false); }
        if (event.key === "Escape") setRenaming(false);
      }} onBlur={() => setRenaming(false)} /> : <h1 onDoubleClick={() => { setTitle(session.title); setRenaming(true); }}>{session.title}</h1>}
    </div>
    <div className="chat-session-tools">
      <ModeSwitch value={session.permissionMode} onChange={onMode} disabled={running} />
      <button type="button" className="chat-icon-button" aria-label={t(exporting ? "exporting" : "export")} title={t(exporting ? "exporting" : "export")} disabled={exporting} onClick={onExport}>
        {exporting ? <LoaderCircle size={16} className="chat-spin" aria-hidden="true" /> : <Download size={16} aria-hidden="true" />}
      </button>
      <details className="chat-session-menu" ref={menu}><summary className="chat-icon-button" aria-label={t("moreActions")} title={t("moreActions")}><Ellipsis size={16} aria-hidden="true" /></summary><div role="menu">
        <button type="button" role="menuitem" onClick={act(() => { setTitle(session.title); setRenaming(true); })}><Pencil size={14} aria-hidden="true" />{t("rename")}</button>
        <button type="button" role="menuitem" disabled={running} onClick={act(onClearResume)}><RotateCcw size={14} aria-hidden="true" />{t("clearResume")}</button>
        <button type="button" role="menuitem" disabled={running} onClick={act(onArchive)}>
          {session.status === "archived" ? <ArchiveRestore size={14} aria-hidden="true" /> : <Archive size={14} aria-hidden="true" />}{t(session.status === "archived" ? "restore" : "archive")}</button>
      </div></details>
    </div>
  </header>;
}
