import { useState } from "react";
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
  return <header className="chat-session-header" data-tauri-drag-region>
    <div><small>{projectName} /</small>
      {renaming ? <input aria-label={t("sessionTitle")} value={title} autoFocus onChange={event => setTitle(event.target.value)} onKeyDown={event => {
        if (event.nativeEvent.isComposing || event.nativeEvent.keyCode === 229) return;
        if (event.key === "Enter") { if (event.currentTarget.value.trim()) onRename(event.currentTarget.value.trim()); setRenaming(false); }
        if (event.key === "Escape") setRenaming(false);
      }} onBlur={() => setRenaming(false)} /> : <h1>{session.title}</h1>}
    </div>
    <ModeSwitch value={session.permissionMode} onChange={onMode} disabled={running} />
    <button type="button" disabled={exporting} onClick={onExport}>{t(exporting ? "exporting" : "export")}</button>
    <details className="chat-session-menu"><summary aria-label={t("chat")}>···</summary><div>
      <button type="button" onClick={() => { setTitle(session.title); setRenaming(true); }}>{t("rename")}</button>
      <button type="button" disabled={running} onClick={onClearResume}>{t("clearResume")}</button>
      <button type="button" disabled={running} onClick={onArchive}>{t(session.status === "archived" ? "restore" : "archive")}</button>
    </div></details>
  </header>;
}
