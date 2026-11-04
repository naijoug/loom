import { useEffect, useRef, useState } from "react";
import { Check, ChevronDown } from "lucide-react";
import type { AgentConfig, AgentDiagnostic, ModelProvider } from "../domain";
import { useT } from "../i18n";
import "./AgentPicker.css";

/** Direct-API agents are never picked by default: most features need tools they lack. */
export function defaultAgentId(agents: AgentConfig[], previous?: string | null): string {
  const available = agents.filter(
    (agent) => agent.enabled && agent.available && agent.adapterType !== "dummy" && agent.adapterType !== "api",
  );
  return (
    available.find((agent) => agent.id === previous)?.id ??
    available.find((agent) => ["claude_code_cli", "codex_cli"].includes(agent.adapterType))?.id ??
    available[0]?.id ??
    ""
  );
}
/** ok: installed and runnable; unknown: not diagnosed yet; down: missing or cannot start. */
/** Diagnostic states in which an Agent cannot run a turn. */
export function isAgentUnusable(status: AgentDiagnostic["status"] | undefined): boolean {
  return status === "missing" || status === "unavailable" || status === "broken";
}
/** Short status label: why an Agent cannot start, or its version when it can. */
export function agentStatusKey(
  agent: AgentConfig,
  diagnostic?: AgentDiagnostic,
): "cannotStart" | "brokenInstall" | "unavailable" | undefined {
  if (diagnostic?.status === "unavailable") return "cannotStart";
  if (diagnostic?.status === "broken") return "brokenInstall";
  return agent.available ? undefined : "unavailable";
}
/** A provider without a usable key makes its Agent unusable even when the CLI itself is fine. */
export function agentHealth(
  agent: AgentConfig | undefined,
  diagnostic?: AgentDiagnostic,
  provider?: ModelProvider,
): "ok" | "unknown" | "down" {
  if (!agent?.available || isAgentUnusable(diagnostic?.status) || provider?.configured === false) return "down";
  return diagnostic ? "ok" : "unknown";
}
export function agentProvider(agent: AgentConfig | undefined, providers: ModelProvider[]): ModelProvider | undefined {
  return agent?.providerId ? providers.find((provider) => provider.id === agent.providerId) : undefined;
}
/** Button label: "Executor · Source" for a CLI on a provider; direct-API names already say their source. */
export function agentButtonLabel(agent: AgentConfig, providers: ModelProvider[]): string {
  if (agent.adapterType === "api" || !agent.providerId) return agent.name;
  return `${agent.name} · ${agentProvider(agent, providers)?.name ?? agent.providerId}`;
}
/** A feature's eligible Agents plus the direct-API ones, which the picker lists greyed out when tools are needed. */
export function pickerAgents(eligible: AgentConfig[], all: AgentConfig[]): AgentConfig[] {
  return [...eligible, ...all.filter((agent) => agent.adapterType === "api" && !eligible.includes(agent))];
}
/** Model override for one run; the UI passes only the name, the backend builds the CLI arguments. */
export interface AgentModelInput {
  value: string;
  onChange: (model: string) => void;
  invalid?: boolean;
}
/** True when the agent's diagnosis says it accepts a per-run model. */
export function agentSupportsModel(diagnostics: AgentDiagnostic[], agentId: string): boolean {
  return !!diagnostics.find((item) => item.agentId === agentId)?.capabilities?.modelSelection;
}
/**
 * Shared Agent menu: availability and diagnosis per Agent; unusable ones are listed with the reason but cannot be picked.
 * `model` adds a model field, shown only for Agents with `modelSelection` (Chat leaves it out).
 * `placement="down"` opens the menu below the button (forms); the default opens upward (composer).
 * `providers` names each Agent's model source; `needsTools` greys out direct-API Agents (the backend refuses them too).
 */
export function AgentPicker({
  agents,
  diagnostics,
  value,
  onChange,
  disabled,
  model,
  placement = "up",
  note,
  providers = [],
  needsTools = false,
}: {
  agents: AgentConfig[];
  diagnostics: AgentDiagnostic[];
  value: string;
  onChange: (id: string) => void;
  disabled: boolean;
  model?: AgentModelInput;
  placement?: "up" | "down";
  /** Footer line in the menu, e.g. what switching does in this feature. */ note?: string;
  providers?: ModelProvider[];
  needsTools?: boolean;
}) {
  const t = useT();
  const showModel = !!model && agentSupportsModel(diagnostics, value);
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const current = agents.find((agent) => agent.id === value);
  const defaultModel = agentProvider(current, providers)?.model;
  useEffect(() => {
    if (!open) return;
    const close = (event: MouseEvent) => {
      if (!ref.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", close);
    ref.current?.querySelector<HTMLButtonElement>('[role="menuitemradio"]:not(:disabled)')?.focus();
    return () => document.removeEventListener("mousedown", close);
  }, [open]);
  return (
    <>
      <div
        className="chat-agent-picker"
        ref={ref}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.stopPropagation();
            setOpen(false);
            ref.current?.querySelector<HTMLButtonElement>("button")?.focus();
          }
          if (event.key === "ArrowDown" || event.key === "ArrowUp") {
            const buttons = Array.from(
              ref.current?.querySelectorAll<HTMLButtonElement>('[role="menuitemradio"]:not(:disabled)') ?? [],
            );
            if (!buttons.length) return;
            event.preventDefault();
            const focused = buttons.indexOf(document.activeElement as HTMLButtonElement);
            buttons[(focused + (event.key === "ArrowDown" ? 1 : -1) + buttons.length) % buttons.length]?.focus();
          }
        }}
      >
        <button
          type="button"
          className="chat-agent-button"
          aria-label={t("selectAgent")}
          aria-haspopup="menu"
          aria-expanded={open}
          disabled={disabled}
          onClick={() => setOpen(!open)}
        >
          <span
            className="chat-health-dot"
            data-health={agentHealth(
              current,
              diagnostics.find((item) => item.agentId === value),
              agentProvider(current, providers),
            )}
            aria-hidden="true"
          />
          <span className="chat-agent-name">{current ? agentButtonLabel(current, providers) : t("selectAgent")}</span>
          <ChevronDown size={13} aria-hidden="true" />
        </button>
        {open && (
          <div className="chat-agent-menu" role="menu" aria-label={t("agents")} data-placement={placement}>
            {agents
              .filter((agent) => agent.enabled && agent.adapterType !== "dummy")
              .map((agent) => {
                const diagnostic = diagnostics.find((item) => item.agentId === agent.id);
                const caps = diagnostic?.capabilities;
                const provider = agentProvider(agent, providers);
                const api = agent.adapterType === "api";
                const health = agentHealth(agent, diagnostic, provider);
                const blocked = api && needsTools;
                const sourceName = provider?.name ?? agent.providerId;
                const capabilities = [
                  caps?.structuredStream && t("structuredStream"),
                  caps?.toolEvents && t("toolEvents"),
                  caps?.nativeResume && t("nativeResume"),
                ]
                  .filter(Boolean)
                  .join(", ");
                // Second line: why it cannot be picked, else its model source then its capabilities.
                const detail = blocked
                  ? t("agentNeedsTools")
                  : provider && !provider.configured
                    ? t("providerMissingKey", { name: provider.name })
                    : api
                      ? [sourceName, t("textOnly")].filter(Boolean).join(" · ")
                      : [sourceName ?? t("ownLogin"), capabilities].filter(Boolean).join(" · ");
                return (
                  <button
                    type="button"
                    role="menuitemradio"
                    aria-checked={value === agent.id}
                    key={agent.id}
                    disabled={health === "down" || blocked}
                    data-blocked={blocked || undefined}
                    onClick={() => {
                      onChange(agent.id);
                      setOpen(false);
                    }}
                    title={diagnostic?.detail}
                  >
                    <span className="chat-agent-check" aria-hidden="true">
                      {value === agent.id && <Check size={14} strokeWidth={2.5} />}
                    </span>
                    <span className="chat-agent-item">
                      <span className="chat-agent-item-head">
                        <strong>{agent.name}</strong>
                        <code>
                          {(() => {
                            const key = agentStatusKey(agent, diagnostic);
                            return key ? t(key) : api ? t("directApi") : (diagnostic?.version ?? t("unknown"));
                          })()}
                        </code>
                      </span>
                      {detail && <small>{detail}</small>}
                    </span>
                  </button>
                );
              })}
            {note && <p className="chat-agent-note">{note}</p>}
          </div>
        )}
      </div>
      {showModel && (
        <input
          className="agent-model-input"
          value={model.value}
          placeholder={defaultModel ? t("agentModelDefault", { model: defaultModel }) : t("agentModelPlaceholder")}
          aria-label={t("agentModel")}
          spellCheck={false}
          aria-invalid={model.invalid || undefined}
          disabled={disabled}
          onChange={(event) => model.onChange(event.target.value)}
        />
      )}
    </>
  );
}
