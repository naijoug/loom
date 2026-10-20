import { useEffect, useState, type FormEvent } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { ArrowLeft } from "lucide-react";
import { invokeCommand, TAURI_COMMANDS } from "../../api";
import { useTheme } from "../../contexts/ThemeContext";
import { useLocale, useT } from "../../i18n";
import type { AgentConfig, AgentDiagnostic, HealthCheckResult } from "../../domain";
import { useAgentCatalog } from "../../hooks/useAgentCatalog";
import { hasTauriRuntime } from "../../hooks/runtime";
import { useAppState } from "../../state/AppStateContext";
import { UpdateButton } from "../app-update/UpdateProvider";
import { defaultAgentDraft, draftFromAgent, isBuiltInAgent } from "./agentDraft";
import "./settings.css";

type SettingsTab = "agents" | "appearance" | "diagnostics" | "about";
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
  return <div className="chat-settings">
    <aside><button type="button" onClick={onBack}><ArrowLeft size={15} />{t("back")}</button><h1>{t("settings")}</h1>
      <nav>{(["agents", "appearance", "diagnostics", "about"] as const).map(id => <button type="button" key={id} aria-current={tab === id ? "page" : undefined} onClick={() => setTab(id)}>{t(id)}</button>)}</nav>
    </aside>
    <main>
      <h2>{t(tab)}</h2>{(error || state.app.agentError || settingsError) && <p role="alert" className="chat-error">{error || state.app.agentError || settingsError}</p>}
      {tab === "agents" && <>
        <div className="settings-actions"><button type="button" onClick={() => void detect()} disabled={busy}>{t("detect")}</button><button type="button" onClick={() => edit()}>{t("customCli")}</button></div>
        <div className="settings-table-wrap"><table><thead><tr>{["agents", "installed", "path", "capabilities", "enabled"].map(key => <th key={key}>{t(key as "agents" | "installed" | "path" | "capabilities" | "enabled")}</th>)}<th /></tr></thead>
          <tbody>{state.agents.filter(agent => agent.adapterType !== "dummy").map(agent => {
            const diag = diagnostics.find(item => item.agentId === agent.id);
            return <tr key={agent.id}><td><strong>{agent.name}</strong><small>{diag?.version ?? t("unknown")}</small></td>
              <td title={diag?.detail}>{t(diag?.status === "unavailable" ? "cannotStart" : agent.available ? "installed" : "unavailable")}</td>
              <td><code>{diag?.resolvedPath ?? agent.command}</code></td>
              <td>{[diag?.capabilities?.structuredStream && t("structuredStream"), diag?.capabilities?.toolEvents && t("toolEvents"), diag?.capabilities?.nativeResume && t("nativeResume")].filter(Boolean).join(" · ") || t("historyReplay")}</td>
              <td><input type="checkbox" aria-label={`${t("enabled")} ${agent.name}`} checked={agent.enabled} disabled={busy} onChange={event => void setAgentEnabled(agent.id, event.target.checked)} /></td>
              <td><button type="button" onClick={() => edit(agent)}>{t("edit")}</button><button type="button" disabled={busy} onClick={() => void testAgent(agent.id)}>{t("test")}</button>
                {!isBuiltInAgent(agent) && <button type="button" onClick={() => { if (window.confirm(t("removeAgentWarning"))) void deleteAgent(agent.id); }}>{t("remove")}</button>}</td></tr>;
          })}</tbody></table></div>
        <section><h3>{t("workModes")}</h3><p>{t("cliPermissions")}</p><div className="settings-table-wrap"><table><thead><tr><th>{t("agents")}</th><th>{t("research")}</th><th>{t("code")}</th><th>{t("autoRun")}</th></tr></thead><tbody>
          {diagnostics.filter(diag => diag.capabilities).map(diag => <tr key={diag.agentId}><td>{state.agents.find(agent => agent.id === diag.agentId)?.name}</td>{["explore", "ask", "auto"].map(mode => <td key={mode}><code>{diag.capabilities?.modes.find(item => item.mode === mode)?.policy}</code></td>)}</tr>)}
        </tbody></table></div></section>
        {formOpen && <form onSubmit={event => void submit(event)} className="settings-agent-form"><h3>{editing ? t("edit") : t("customCli")}</h3>
          <label>{t("name")}<input required value={draft.name} onChange={event => setDraft({ ...draft, name: event.target.value })} /></label>
          <label>{t("executable")}<input required value={draft.command} onChange={event => setDraft({ ...draft, command: event.target.value })} /></label>
          {draft.adapterType === "cli" && <><label>{t("arguments")}<textarea rows={4} value={args} onChange={event => setArgs(event.target.value)} placeholder="{prompt}" /></label>
            <label>{t("outputFormat")}<select value="plain" disabled><option value="plain">{t("plainText")}</option></select></label><p>{t("customHint")}</p></>}
          <div><button type="button" onClick={() => setFormOpen(false)}>{t("cancel")}</button><button type="submit" disabled={busy}>{t("save")}</button></div>
        </form>}
      </>}
      {tab === "appearance" && <>
        <section><h3>{t("language")}</h3><div role="radiogroup" aria-label={t("language")}>{(["en", "zh-CN"] as const).map(value => <button type="button" role="radio" aria-checked={locale === value} key={value} onClick={() => void setLocale(value).catch(error => setError(String(error)))}>{t(value === "en" ? "english" : "chinese")}</button>)}</div></section>
        <section><h3>{t("theme")}</h3><div role="radiogroup" aria-label={t("theme")}>{(["system", "dark", "light"] as const).map(value => <button type="button" role="radio" aria-checked={themeMode === value} key={value} onClick={() => setThemeMode(value)}>{t(value)}</button>)}</div></section>
      </>}
      {tab === "diagnostics" && <section><p>{t("diagnosticsHint")}</p><label><input type="checkbox" checked={includeLogs} onChange={event => setIncludeLogs(event.target.checked)} />{t("includeLogs")}</label><p><button type="button" disabled={!state.projects.current || !hasTauriRuntime()} onClick={() => void exportDiagnostics()}>{t("exportDiagnostics")}</button></p>{notice && <p role="status"><code>{notice}</code></p>}</section>}
      {tab === "about" && <section><h3>Loom {version}</h3><p>{t("aboutHint")}</p><UpdateButton manual /></section>}
    </main>
  </div>;
}
