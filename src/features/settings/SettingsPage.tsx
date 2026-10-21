import { useEffect, useState, type FormEvent } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { Activity, ArrowLeft, Bot, FlaskConical, Info, Monitor, Moon, Palette, RefreshCw, Plus, Sun } from "lucide-react";
import { invokeCommand, TAURI_COMMANDS } from "../../api";
import { useTheme } from "../../contexts/ThemeContext";
import { useLocale, useT } from "../../i18n";
import type { AgentConfig, AgentDiagnostic, HealthCheckResult } from "../../domain";
import { useAgentCatalog } from "../../hooks/useAgentCatalog";
import { hasTauriRuntime } from "../../hooks/runtime";
import { useAppState } from "../../state/AppStateContext";
import { UpdateButton } from "../app-update/UpdateProvider";
import { defaultAgentDraft, draftFromAgent, isBuiltInAgent } from "./agentDraft";
import { agentHealth } from "../chat/AgentPicker";
import { setInlineCardsEnabled, useInlineCardsEnabled } from "../chat/openui/inlineCardsSetting";
import "../chat/chat.css";
import "./settings.css";

type SettingsTab = "agents" | "appearance" | "experiments" | "diagnostics" | "about";
const tabIcons = { agents: Bot, appearance: Palette, experiments: FlaskConical, diagnostics: Activity, about: Info } as const;
const themeIcons = { system: Monitor, dark: Moon, light: Sun } as const;
export function SettingsPage({ onBack, initialTab = "agents" }: { onBack: () => void; initialTab?: SettingsTab }) {
  const t = useT();
  const { state } = useAppState();
  const { themeMode, setThemeMode, settingsError } = useTheme();
  const { locale, setLocale } = useLocale();
  const { loadAgents, diagnoseAgents, createAgent, updateAgent, deleteAgent, setAgentEnabled } = useAgentCatalog();
  const [tab, setTab] = useState<SettingsTab>(initialTab);
  const [diagnostics, setDiagnostics] = useState<AgentDiagnostic[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [version, setVersion] = useState("");
  const [editing, setEditing] = useState<string | null>(null);
  const [formOpen, setFormOpen] = useState(false);
  const [draft, setDraft] = useState(defaultAgentDraft);
  const [args, setArgs] = useState("");
  const [includeLogs, setIncludeLogs] = useState(false);
  const inlineCards = useInlineCardsEnabled();
  useEffect(() => {
    void loadAgents();
    void diagnoseAgents().then(setDiagnostics).catch(error => setError(String(error)));
    if (hasTauriRuntime()) void invokeCommand<HealthCheckResult>(TAURI_COMMANDS.healthCheck).then(result => setVersion(result.version)).catch(error => setError(String(error)));
  }, [loadAgents, diagnoseAgents]);
  async function detect() {
    setBusy(true); setError("");
    try { await loadAgents(); setDiagnostics(await diagnoseAgents()); }
    catch (error) { setError(String(error)); } finally { setBusy(false); }
  }
  async function testAgent(id: string) {
    setBusy(true); setError("");
    try { const result = await diagnoseAgents(id); setDiagnostics(previous => [...previous.filter(item => item.agentId !== id), ...result]); }
    catch (error) { setError(String(error)); } finally { setBusy(false); }
  }
  function edit(agent?: AgentConfig) {
    setEditing(agent?.id ?? null); setDraft(agent ? draftFromAgent(agent) : defaultAgentDraft());
    setArgs(agent?.args.join("\n") ?? ""); setFormOpen(true);
  }
  async function submit(event: FormEvent) {
    event.preventDefault(); setBusy(true);
    try {
      const input = { ...draft, name: draft.name.trim(), command: draft.command.trim(), args: args.split(/\r?\n/).map(value => value.trim()).filter(Boolean) };
      const result = editing ? await updateAgent(editing, input) : await createAgent(input);
      if (result) { setFormOpen(false); setDiagnostics(await diagnoseAgents()); }
    } catch (error) { setError(String(error)); } finally { setBusy(false); }
  }
  async function exportDiagnostics() {
    const project = state.projects.current;
    if (!project || !hasTauriRuntime()) return;
    try {
      const path = await save({ defaultPath: `${project.name}-diagnostics.json`, filters: [{ name: "JSON", extensions: ["json"] }] });
      if (!path) return;
      setNotice(await invokeCommand<string>(TAURI_COMMANDS.exportDiagnosticBundle, { input: { projectPath: project.path, targetPath: path, includeLogTails: includeLogs } }));
    } catch (error) { setError(String(error)); }
  }
  const description = tab === "agents" ? t("agentsDesc") : tab === "appearance" ? t("appearanceDesc") : tab === "experiments" ? t("experimentsDesc") : tab === "diagnostics" ? t("diagnosticsHint") : t("aboutHint");
  return <div className="chat-settings">
    <aside><button type="button" className="settings-back" onClick={onBack}><ArrowLeft size={15} aria-hidden="true" />{t("back")}</button><h1>{t("settings")}</h1>
      <nav>{(["agents", "appearance", "experiments", "diagnostics", "about"] as const).map(id => {
        const Icon = tabIcons[id];
        return <button type="button" key={id} aria-current={tab === id ? "page" : undefined} onClick={() => setTab(id)}><Icon size={15} aria-hidden="true" />{t(id)}</button>;
      })}</nav>
    </aside>
    <main><div className="settings-content" data-tab={tab}>
      <header className="settings-header"><div><h2>{t(tab)}</h2><p>{description}</p></div>
        {tab === "agents" && <div className="settings-actions">
          <button type="button" onClick={() => void detect()} disabled={busy}><RefreshCw size={14} aria-hidden="true" className={busy ? "chat-spin" : ""} />{t("detect")}</button>
          <button type="button" onClick={() => edit()}><Plus size={14} aria-hidden="true" />{t("customCli")}</button></div>}
      </header>
      {(error || state.app.agentError || settingsError) && <p role="alert" className="chat-error">{error || state.app.agentError || settingsError}</p>}
      {tab === "agents" && <>
        <div className="settings-table-wrap"><table><thead><tr><th>{t("agents")}</th><th>{t("status")}</th><th>{t("path")}</th><th>{t("capabilities")}</th><th>{t("enabled")}</th><th /></tr></thead>
          <tbody>{state.agents.filter(agent => agent.adapterType !== "dummy").map(agent => {
            const diag = diagnostics.find(item => item.agentId === agent.id);
            return <tr key={agent.id}><td><strong>{agent.name}</strong><small><code>{diag?.version ?? t("unknown")}</code></small></td>
              <td title={diag?.detail}><span className="settings-status"><span className="chat-health-dot" data-health={agentHealth(agent, diag)} aria-hidden="true" />{t(diag?.status === "unavailable" ? "cannotStart" : agent.available ? "installed" : "unavailable")}</span></td>
              <td><code>{diag?.resolvedPath ?? agent.command}</code></td>
              <td>{[diag?.capabilities?.structuredStream && t("structuredStream"), diag?.capabilities?.toolEvents && t("toolEvents"), diag?.capabilities?.nativeResume && t("nativeResume")].filter(Boolean).join(", ") || t("historyReplay")}</td>
              <td><label className="settings-switch"><input type="checkbox" role="switch" aria-label={`${t("enabled")} ${agent.name}`} checked={agent.enabled} disabled={busy} onChange={event => void setAgentEnabled(agent.id, event.target.checked)} /><span aria-hidden="true" /></label></td>
              <td className="settings-row-actions"><button type="button" onClick={() => edit(agent)}>{t("edit")}</button><button type="button" disabled={busy} onClick={() => void testAgent(agent.id)}>{t("test")}</button>
                {!isBuiltInAgent(agent) && <button type="button" onClick={() => { if (window.confirm(t("removeAgentWarning"))) void deleteAgent(agent.id); }}>{t("remove")}</button>}</td></tr>;
          })}</tbody></table></div>
        {formOpen && <form onSubmit={event => void submit(event)} className="settings-card settings-agent-form"><h3>{editing ? t("edit") : t("customCli")}</h3>
          <label>{t("name")}<input required value={draft.name} onChange={event => setDraft({ ...draft, name: event.target.value })} /></label>
          <label>{t("executable")}<input required value={draft.command} onChange={event => setDraft({ ...draft, command: event.target.value })} /></label>
          {draft.adapterType === "cli" && <><label>{t("arguments")}<textarea rows={4} value={args} onChange={event => setArgs(event.target.value)} placeholder="{prompt}" /></label>
            <label>{t("outputFormat")}<select value="plain" disabled><option value="plain">{t("plainText")}</option></select></label><p>{t("customHint")}</p></>}
          <div className="settings-form-actions"><button type="button" onClick={() => setFormOpen(false)}>{t("cancel")}</button><button type="submit" className="settings-primary" disabled={busy}>{t("save")}</button></div>
        </form>}
        <section className="settings-card"><h3>{t("workModes")}</h3><p>{t("cliPermissions")}</p>{!diagnostics.some(diag => diag.capabilities) ? <p className="settings-empty">{t("workModesEmpty")}</p> : <div className="settings-table-wrap settings-modes"><table><thead><tr><th>{t("agents")}</th><th data-mode="research">{t("research")}</th><th data-mode="code">{t("code")}</th><th>{t("autoRun")}</th></tr></thead><tbody>
          {diagnostics.filter(diag => diag.capabilities).map(diag => <tr key={diag.agentId}><td>{state.agents.find(agent => agent.id === diag.agentId)?.name}</td>{["explore", "ask", "auto"].map(mode => <td key={mode}><code>{diag.capabilities?.modes.find(item => item.mode === mode)?.policy}</code></td>)}</tr>)}
        </tbody></table></div>}</section>
      </>}
      {tab === "appearance" && <>
        <section className="settings-card"><h3>{t("language")}</h3><div className="settings-choices" role="radiogroup" aria-label={t("language")}>{(["en", "zh-CN"] as const).map(value => <button type="button" role="radio" aria-checked={locale === value} key={value} onClick={() => void setLocale(value).catch(error => setError(String(error)))}><span className="settings-ring" aria-hidden="true" />{t(value === "en" ? "english" : "chinese")}</button>)}</div></section>
        <section className="settings-card"><h3>{t("theme")}</h3><div className="settings-choices" role="radiogroup" aria-label={t("theme")}>{(["system", "dark", "light"] as const).map(value => {
          const Icon = themeIcons[value];
          return <button type="button" role="radio" aria-checked={themeMode === value} key={value} onClick={() => setThemeMode(value)}><Icon size={15} aria-hidden="true" />{t(value)}</button>;
        })}</div></section>
      </>}
      {tab === "experiments" && <section className="settings-card settings-toggle-row"><div><h3>{t("inlineCards")}</h3><p>{t("inlineCardsHint")}</p></div>
        <label className="settings-switch"><input type="checkbox" role="switch" aria-label={t("inlineCards")} checked={inlineCards} onChange={event => void setInlineCardsEnabled(event.target.checked).catch(error => setError(String(error)))} /><span aria-hidden="true" /></label></section>}
      {tab === "diagnostics" && <section className="settings-card"><label className="settings-check"><input type="checkbox" checked={includeLogs} onChange={event => setIncludeLogs(event.target.checked)} />{t("includeLogs")}</label><p><button type="button" className="settings-primary" disabled={!state.projects.current || !hasTauriRuntime()} onClick={() => void exportDiagnostics()}>{t("exportDiagnostics")}</button></p>{notice && <p role="status"><code>{notice}</code></p>}</section>}
      {tab === "about" && <section className="settings-card"><h3>Loom {version}</h3><UpdateButton manual /></section>}
    </div></main>
  </div>;
}
