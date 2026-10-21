import { useEffect, useRef, useState } from "react";
import { Check, ChevronDown } from "lucide-react";
import type { AgentConfig, AgentDiagnostic } from "../../domain";
import { useT } from "../../i18n";

export function defaultAgentId(agents: AgentConfig[], previous?: string | null): string {
  const available = agents.filter(agent => agent.enabled && agent.available && agent.adapterType !== "dummy");
  return available.find(agent => agent.id === previous)?.id
    ?? available.find(agent => ["claude_code_cli", "codex_cli"].includes(agent.adapterType))?.id
    ?? available[0]?.id ?? "";
}
/** ok: installed and runnable; unknown: not diagnosed yet; down: missing or cannot start. */
export function agentHealth(agent: AgentConfig | undefined, diagnostic?: AgentDiagnostic): "ok" | "unknown" | "down" {
  if (!agent?.available || ["missing", "unavailable"].includes(diagnostic?.status ?? "")) return "down";
  return diagnostic ? "ok" : "unknown";
}
export function AgentPicker({ agents, diagnostics, value, onChange, disabled }: {
  agents: AgentConfig[]; diagnostics: AgentDiagnostic[]; value: string; onChange: (id: string) => void; disabled: boolean;
}) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const current = agents.find(agent => agent.id === value);
  useEffect(() => {
    if (!open) return;
    const close = (event: MouseEvent) => { if (!ref.current?.contains(event.target as Node)) setOpen(false); };
    document.addEventListener("mousedown", close);
    ref.current?.querySelector<HTMLButtonElement>('[role="menuitemradio"]:not(:disabled)')?.focus();
    return () => document.removeEventListener("mousedown", close);
  }, [open]);
  return <div className="chat-agent-picker" ref={ref} onKeyDown={event => {
    if (event.key === "Escape") { event.stopPropagation(); setOpen(false); ref.current?.querySelector<HTMLButtonElement>("button")?.focus(); }
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      const buttons = Array.from(ref.current?.querySelectorAll<HTMLButtonElement>('[role="menuitemradio"]:not(:disabled)') ?? []);
      if (!buttons.length) return;
      event.preventDefault();
      const focused = buttons.indexOf(document.activeElement as HTMLButtonElement);
      buttons[(focused + (event.key === "ArrowDown" ? 1 : -1) + buttons.length) % buttons.length]?.focus();
    }
  }}>
    <button type="button" className="chat-agent-button" aria-label={t("selectAgent")} aria-haspopup="menu" aria-expanded={open} disabled={disabled} onClick={() => setOpen(!open)}>
      <span className="chat-health-dot" data-health={agentHealth(current, diagnostics.find(item => item.agentId === value))} aria-hidden="true" />
      <span className="chat-agent-name">{current?.name ?? t("selectAgent")}</span>
      <ChevronDown size={13} aria-hidden="true" />
    </button>
    {open && <div className="chat-agent-menu" role="menu" aria-label={t("agents")}>
      {agents.filter(agent => agent.enabled && agent.adapterType !== "dummy").map(agent => {
        const diagnostic = diagnostics.find(item => item.agentId === agent.id);
        const caps = diagnostic?.capabilities;
        const health = agentHealth(agent, diagnostic);
        const capabilities = [caps?.structuredStream && t("structuredStream"), caps?.toolEvents && t("toolEvents"), caps?.nativeResume && t("nativeResume")].filter(Boolean).join(", ");
        return <button type="button" role="menuitemradio" aria-checked={value === agent.id} key={agent.id}
          disabled={health === "down"} onClick={() => { onChange(agent.id); setOpen(false); }} title={diagnostic?.detail}>
          <span className="chat-agent-check" aria-hidden="true">{value === agent.id && <Check size={14} strokeWidth={2.5} />}</span>
          <span className="chat-agent-item">
            <span className="chat-agent-item-head"><strong>{agent.name}</strong>
              <code>{diagnostic?.status === "unavailable" ? t("cannotStart") : agent.available ? diagnostic?.version ?? t("unknown") : t("unavailable")}</code></span>
            {capabilities && <small>{capabilities}</small>}
          </span>
        </button>;
      })}
      <p className="chat-agent-note">{t("agentMenuNote")}</p>
    </div>}
  </div>;
}
