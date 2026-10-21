import { useEffect, useRef, useState } from "react";
import { Check, ChevronDown, Folder, FolderPlus } from "lucide-react";
import type { ProjectSummary } from "../../domain";
import { useT } from "../../i18n";

/** Where a new chat starts; an existing chat keeps its project, so the composer only shows this before the first send. */
export function ProjectPicker({ projects, value, onChange, onAdd, disabled }: {
  projects: ProjectSummary[]; value: string; onChange: (project: ProjectSummary) => void; onAdd: () => void; disabled: boolean;
}) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const current = projects.find(project => project.path === value);
  useEffect(() => {
    if (!open) return;
    const close = (event: MouseEvent) => { if (!ref.current?.contains(event.target as Node)) setOpen(false); };
    document.addEventListener("mousedown", close);
    ref.current?.querySelector<HTMLButtonElement>('[role="menuitemradio"][aria-checked="true"], [role="menuitem"]')?.focus();
    return () => document.removeEventListener("mousedown", close);
  }, [open]);
  return <div className="chat-agent-picker chat-project-picker" ref={ref} onKeyDown={event => {
    if (event.key === "Escape" && open) { event.stopPropagation(); setOpen(false); ref.current?.querySelector<HTMLButtonElement>("button")?.focus(); }
    if (open && (event.key === "ArrowDown" || event.key === "ArrowUp")) {
      const buttons = Array.from(ref.current?.querySelectorAll<HTMLButtonElement>('[role^="menuitem"]') ?? []);
      event.preventDefault();
      const focused = buttons.indexOf(document.activeElement as HTMLButtonElement);
      buttons[(focused + (event.key === "ArrowDown" ? 1 : -1) + buttons.length) % buttons.length]?.focus();
    }
  }}>
    <button type="button" className="chat-agent-button" aria-label={`${t("project")}: ${current?.name ?? value}`} title={value}
      aria-haspopup="menu" aria-expanded={open} disabled={disabled} onClick={() => setOpen(!open)}>
      <Folder size={13} aria-hidden="true" /><span className="chat-agent-name">{current?.name ?? value}</span><ChevronDown size={13} aria-hidden="true" />
    </button>
    {open && <div className="chat-agent-menu chat-project-menu" role="menu" aria-label={t("project")}>
      {projects.map(project => <button type="button" role="menuitemradio" key={project.path} aria-checked={project.path === value}
        onClick={() => { setOpen(false); if (project.path !== value) onChange(project); }}>
        <span className="chat-agent-check" aria-hidden="true">{project.path === value && <Check size={14} strokeWidth={2.5} />}</span>
        <span className="chat-agent-item">
          <span className="chat-agent-item-head"><strong>{project.name}</strong></span>
          <small title={project.path}>{project.path}</small>
        </span>
      </button>)}
      <button type="button" role="menuitem" className="chat-project-add" onClick={() => { setOpen(false); onAdd(); }}>
        <span className="chat-agent-check" aria-hidden="true"><FolderPlus size={14} /></span>
        <span className="chat-agent-item"><span className="chat-agent-item-head"><strong>{t("addProject")}</strong></span></span>
      </button>
    </div>}
  </div>;
}
