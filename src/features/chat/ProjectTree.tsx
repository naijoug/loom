import { useEffect, useState } from "react";
import { ChevronRight, FolderPlus, Plus, Settings, PanelLeftClose } from "lucide-react";
import { chatListProjectGroups } from "../../api/chatClient";
import type { ChatProjectGroup } from "../../domain/chat";
import type { AgentConfig, ProjectSummary } from "../../domain";
import { useT } from "../../i18n";
import { mockChatStore } from "./mockStore";
import { toWorkMode } from "./workMode";

export function ProjectTree({ projects, agents, projectPath, sessionId, useBackend, revision, onSelect, onAdd, onSettings, onCollapse }: {
  projects: ProjectSummary[]; agents: AgentConfig[]; projectPath: string | null; sessionId: string | null;
  useBackend: boolean; revision: number; onSelect: (project: ProjectSummary, session: string | null) => void;
  onAdd: () => void; onSettings: () => void; onCollapse: () => void;
}) {
  const t = useT();
  const [groups, setGroups] = useState<ChatProjectGroup[]>([]);
  const [error, setError] = useState("");
  const [search, setSearch] = useState("");
  const [archived, setArchived] = useState(false);
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>(() => {
    try { return JSON.parse(localStorage.getItem("loom-project-collapsed") ?? "{}"); } catch { return {}; }
  });
  const expanded = projects.filter(project => collapsed[project.path] === false || (collapsed[project.path] === undefined && project.path === projectPath)).map(project => project.path);
  const expandedKey = JSON.stringify(expanded);
  useEffect(() => {
    let disposed = false;
    const read = useBackend ? chatListProjectGroups(expanded) : Promise.resolve(projects.map(project => {
      const sessions = mockChatStore.list(project.path);
      return { project, sessionCount: sessions.length, sessions: expanded.includes(project.path) ? sessions : [] };
    }));
    void read.then(result => { if (!disposed) { setGroups(result); setError(""); } }).catch(error => { if (!disposed) setError(String(error)); });
    return () => { disposed = true; };
  // String key keeps project expansion stable across renders.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projects, expandedKey, revision, useBackend]);
  function toggle(path: string) {
    const next = { ...collapsed, [path]: expanded.includes(path) };
    setCollapsed(next); localStorage.setItem("loom-project-collapsed", JSON.stringify(next));
  }
  return <aside className="chat-project-tree" aria-label={t("projects")}>
    <header><strong>Loom</strong><button type="button" aria-label={t("collapseSidebar")} onClick={onCollapse}><PanelLeftClose size={16} /></button></header>
    <div className="chat-project-heading"><span>{t("projects")}</span><button type="button" aria-label={t("addProject")} onClick={onAdd}><FolderPlus size={16} /></button></div>
    <input type="search" placeholder={t("searchChats")} aria-label={t("searchChats")} value={search} onChange={event => setSearch(event.target.value)} />
    <label className="chat-archive-filter"><input type="checkbox" checked={archived} onChange={event => setArchived(event.target.checked)} />{t("archived")}</label>
    {error && <p role="alert" className="chat-error">{error}</p>}
    <nav>{projects.map(project => {
      const group = groups.find(group => group.project.path === project.path);
      const open = expanded.includes(project.path);
      return <section key={project.path}>
        <div className="chat-project-row" data-active={projectPath === project.path}>
          <button type="button" title={project.path} aria-expanded={open} onClick={() => toggle(project.path)}><ChevronRight size={14} className={open ? "expanded" : ""} /><strong>{project.name}</strong><small>{group?.sessionCount ?? ""}</small></button>
          <button type="button" aria-label={`${t("newSession")} · ${project.name}`} onClick={() => {
            const next = { ...collapsed, [project.path]: false }; setCollapsed(next); localStorage.setItem("loom-project-collapsed", JSON.stringify(next)); onSelect(project, null);
          }}><Plus size={14} /></button>
        </div>
        {group?.storageError && <p role="alert" className="chat-storage-error">{group.storageError}</p>}
        {open && <ul>{group?.sessions.filter(session => (session.status === "archived") === archived && `${session.title} ${session.preview ?? ""}`.toLowerCase().includes(search.toLowerCase())).map(session =>
          <li key={session.id}><button type="button" className="chat-tree-session" aria-current={projectPath === project.path && sessionId === session.id ? "page" : undefined}
            title={session.storageError ?? session.title} onClick={() => onSelect(project, session.id)}>
            <span aria-hidden="true">{session.turnStatus === "streaming" ? "◌" : toWorkMode(session.permissionMode) === "research" ? "⌕" : "‹›"}</span>
            <span>{session.title}<small>{agents.find(agent => agent.id === session.agentId)?.name ?? session.agentId}</small></span>
            {session.needsAttention && <span aria-label={t("error")}>•</span>}
          </button></li>)}</ul>}
      </section>;
    })}</nav>
    <footer><button type="button" onClick={onSettings}><Settings size={15} />{t("settings")}</button></footer>
  </aside>;
}
