import { useEffect, useRef, useState } from "react";
import { Check, ChevronDown } from "lucide-react";
import type { ChatPermissionMode } from "../../domain";
import { useT } from "../../i18n";
import { PermissionIcon } from "./icons";
import { PERMISSION_LABELS } from "./permissionMode";

const choices = [
  { mode: "explore", label: PERMISSION_LABELS.explore, hint: "readOnlyHint" },
  { mode: "ask", label: PERMISSION_LABELS.ask, hint: "askHint" },
  { mode: "auto", label: PERMISSION_LABELS.auto, hint: "autoHint" },
] as const;

/** Permission tiers enforced by the CLI: Read-only, Ask (per-turn confirm) and Auto; the page still confirms Auto. */
export function ModePicker({ value, onChange, disabled }: {
  value: ChatPermissionMode; onChange: (mode: ChatPermissionMode) => void; disabled: boolean;
}) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const current = choices.find(choice => choice.mode === value) ?? choices[0];
  useEffect(() => {
    if (!open) return;
    const close = (event: MouseEvent) => { if (!ref.current?.contains(event.target as Node)) setOpen(false); };
    document.addEventListener("mousedown", close);
    ref.current?.querySelector<HTMLButtonElement>('[role="menuitemradio"][aria-checked="true"]')?.focus();
    return () => document.removeEventListener("mousedown", close);
  }, [open]);
  const icon = (mode: ChatPermissionMode) => <PermissionIcon mode={mode} size={13} />;
  return <div className="chat-agent-picker chat-mode-picker" ref={ref} onKeyDown={event => {
    if (event.key === "Escape" && open) { event.stopPropagation(); setOpen(false); ref.current?.querySelector<HTMLButtonElement>("button")?.focus(); }
    if (open && (event.key === "ArrowDown" || event.key === "ArrowUp")) {
      const buttons = Array.from(ref.current?.querySelectorAll<HTMLButtonElement>('[role="menuitemradio"]') ?? []);
      event.preventDefault();
      const focused = buttons.indexOf(document.activeElement as HTMLButtonElement);
      buttons[(focused + (event.key === "ArrowDown" ? 1 : -1) + buttons.length) % buttons.length]?.focus();
    }
  }}>
    <button type="button" className="chat-agent-button" data-mode={value} aria-label={`${t("permissions")}: ${t(current.label)}`} title={t(current.hint)}
      aria-haspopup="menu" aria-expanded={open} disabled={disabled} onClick={() => setOpen(!open)}>
      {icon(current.mode)}<span className="chat-agent-name">{t(current.label)}</span><ChevronDown size={13} aria-hidden="true" />
    </button>
    {open && <div className="chat-agent-menu chat-mode-menu" role="menu" aria-label={t("permissions")}>
      {choices.map(choice => <button type="button" role="menuitemradio" key={choice.mode} data-mode={choice.mode} aria-checked={value === choice.mode}
        onClick={() => { setOpen(false); if (choice.mode !== value) onChange(choice.mode); }}>
        <span className="chat-agent-check" aria-hidden="true">{value === choice.mode && <Check size={14} strokeWidth={2.5} />}</span>
        <span className="chat-agent-item">
          <span className="chat-agent-item-head">{icon(choice.mode)}<strong>{t(choice.label)}</strong></span>
          <small>{t(choice.hint)}</small>
        </span>
      </button>)}
    </div>}
  </div>;
}
