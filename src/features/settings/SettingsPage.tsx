import { lazy, Suspense, useContext, useEffect, useState, type FormEvent } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import {
  Activity,
  BookOpen,
  Bot,
  FlaskConical,
  Info,
  KeyRound,
  Monitor,
  Moon,
  Palette,
  RefreshCw,
  Plus,
  Sun,
} from "lucide-react";
import { invokeCommand, TAURI_COMMANDS } from "../../api";
import { useTheme } from "../../contexts/ThemeContext";
import { useLocale, useT } from "../../i18n";
import type { AgentConfig, HealthCheckResult } from "../../domain";
import { useAgentCatalog } from "../../hooks/useAgentCatalog";
import { hasTauriRuntime } from "../../hooks/runtime";
import { useAppState } from "../../state/AppStateContext";
import { UpdateButton } from "../../components/app-update/UpdateProvider";
import { apiAgentDraft, defaultAgentDraft, draftFromAgent, isBuiltInAgent } from "./agentDraft";
import { agentHealth, agentProvider, agentStatusKey } from "../../components/AgentPicker";
import { PROVIDER_KINDS, providerHost, providerKindKey } from "./providerDraft";
import { ProvidersPanel } from "./ProvidersPanel";
import { setInlineCardsEnabled, useInlineCardsEnabled } from "../../hooks/inlineCardsSetting";
import "./settings.css";
import { SkillsPreviewContext } from "../skills/previewContext";

const SkillsPanel = lazy(() => import("../skills/SkillsPanel").then((module) => ({ default: module.SkillsPanel })));

type SettingsTab = "skills" | "agents" | "providers" | "appearance" | "experiments" | "diagnostics" | "about";
const tabIcons = {
  skills: BookOpen,
  agents: Bot,
  providers: KeyRound,
  appearance: Palette,
  experiments: FlaskConical,
  diagnostics: Activity,
  about: Info,
} as const;
const themeIcons = { system: Monitor, dark: Moon, light: Sun } as const;
export function SettingsPage() {
  const t = useT();
  const skillsPreview = useContext(SkillsPreviewContext);
  const { state } = useAppState();
  const { themeMode, setThemeMode, settingsError } = useTheme();
  const { locale, setLocale } = useLocale();
  const { loadAgents, diagnoseAgents, createAgent, updateAgent, deleteAgent, setAgentEnabled } = useAgentCatalog();
  const [tab, setTab] = useState<SettingsTab>("agents");
  const diagnostics = state.agentDiagnostics;
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [version, setVersion] = useState("");
  const [editing, setEditing] = useState<string | null>(null);
  const [formOpen, setFormOpen] = useState(false);
  const [draft, setDraft] = useState(defaultAgentDraft);
  const [args, setArgs] = useState("");
  const [includeLogs, setIncludeLogs] = useState(false);
  const [addingProvider, setAddingProvider] = useState(false);
  const providers = state.providers.providers;
  const inlineCards = useInlineCardsEnabled();
  useEffect(() => {
    void loadAgents();
    void diagnoseAgents().catch((error) => setError(String(error)));
    if (hasTauriRuntime())
      void invokeCommand<HealthCheckResult>(TAURI_COMMANDS.healthCheck)
        .then((result) => setVersion(result.version))
        .catch((error) => setError(String(error)));
  }, [loadAgents, diagnoseAgents]);
  async function detect() {
    setBusy(true);
    setError("");
    try {
      await loadAgents();
      await diagnoseAgents();
    } catch (error) {
      setError(String(error));
    } finally {
      setBusy(false);
    }
  }
  async function testAgent(id: string) {
    setBusy(true);
    setError("");
    try {
      await diagnoseAgents(id);
    } catch (error) {
      setError(String(error));
    } finally {
      setBusy(false);
    }
  }
  function edit(agent?: AgentConfig, api = false) {
    setEditing(agent?.id ?? null);
    setDraft(agent ? draftFromAgent(agent) : api ? apiAgentDraft(providers[0]) : defaultAgentDraft());
    setArgs(agent?.args.join("\n") ?? "");
    setFormOpen(true);
  }
  async function submit(event: FormEvent) {
    event.preventDefault();
    setBusy(true);
    try {
      const input = {
        ...draft,
        name: draft.name.trim(),
        command: draft.command.trim(),
        args: args
          .split(/\r?\n/)
          .map((value) => value.trim())
          .filter(Boolean),
      };
      const result = editing ? await updateAgent(editing, input) : await createAgent(input);
      if (result) {
        setFormOpen(false);
        await diagnoseAgents();
      }
    } catch (error) {
      setError(String(error));
    } finally {
      setBusy(false);
    }
  }
  async function exportDiagnostics() {
    const project = state.projects.current;
    if (!project || !hasTauriRuntime()) return;
    try {
      const path = await save({
        defaultPath: `${project.name}-diagnostics.json`,
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (!path) return;
      setNotice(
        await invokeCommand<string>(TAURI_COMMANDS.exportDiagnosticBundle, {
          input: { projectPath: project.path, targetPath: path, includeLogTails: includeLogs },
        }),
      );
    } catch (error) {
      setError(String(error));
    }
  }
  const description =
    tab === "skills"
      ? t("skillsSettingsDesc")
      : tab === "agents"
        ? t("agentsDesc")
        : tab === "providers"
          ? t("providersDesc")
          : tab === "appearance"
            ? t("appearanceDesc")
            : tab === "experiments"
              ? t("experimentsDesc")
              : tab === "diagnostics"
                ? t("diagnosticsHint")
                : t("aboutHint");
  return (
    <div className="chat-settings">
      <aside data-tauri-drag-region>
        <h1>{t("settings")}</h1>
        <nav>
          {(["agents", "providers", "skills", "appearance", "experiments", "diagnostics", "about"] as const).map(
            (id) => {
              const Icon = tabIcons[id];
              return (
                <button
                  type="button"
                  key={id}
                  aria-current={tab === id ? "page" : undefined}
                  onClick={() => setTab(id)}
                >
                  <Icon size={15} aria-hidden="true" />
                  {t(id)}
                </button>
              );
            },
          )}
        </nav>
      </aside>
      <main className={tab === "skills" ? "settings-skills-main" : undefined}>
        <div className="settings-content" data-tab={tab}>
          <header className="settings-header">
            <div>
              <h2>{t(tab)}</h2>
              <p>{description}</p>
            </div>
            {tab === "agents" && (
              <div className="settings-actions">
                <button type="button" onClick={() => void detect()} disabled={busy}>
                  <RefreshCw size={14} aria-hidden="true" className={busy ? "chat-spin" : ""} />
                  {t("detect")}
                </button>
                <button type="button" onClick={() => edit()}>
                  <Plus size={14} aria-hidden="true" />
                  {t("customCli")}
                </button>
                <button type="button" onClick={() => edit(undefined, true)}>
                  <Plus size={14} aria-hidden="true" />
                  {t("directApi")}
                </button>
              </div>
            )}
            {tab === "providers" && (
              <div className="settings-actions">
                <button type="button" className="settings-primary" onClick={() => setAddingProvider(true)}>
                  <Plus size={14} aria-hidden="true" />
                  {t("addProvider")}
                </button>
              </div>
            )}
          </header>
          {(error || state.app.agentError || settingsError) && (
            <p role="alert" className="chat-error">
              {error || state.app.agentError || settingsError}
            </p>
          )}
          {tab === "skills" && (
            <Suspense fallback={<p role="status">{t("loading")}</p>}>
              <SkillsPanel
                api={skillsPreview ?? undefined}
                desktop={skillsPreview ? true : undefined}
                projectName={state.projects.current?.name}
                projectPath={state.projects.current?.path ?? null}
              />
            </Suspense>
          )}
          {tab === "providers" && <ProvidersPanel adding={addingProvider} onAddingChange={setAddingProvider} />}
          {tab === "agents" && (
            <>
              <div className="settings-table-wrap">
                <table>
                  <thead>
                    <tr>
                      <th>{t("agents")}</th>
                      <th>{t("status")}</th>
                      <th>{t("modelSource")}</th>
                      <th>{t("path")}</th>
                      <th>{t("capabilities")}</th>
                      <th>{t("enabled")}</th>
                      <th />
                    </tr>
                  </thead>
                  <tbody>
                    {state.agents
                      .filter((agent) => agent.adapterType !== "dummy")
                      .map((agent) => {
                        const diag = diagnostics.find((item) => item.agentId === agent.id);
                        const provider = agentProvider(agent, providers);
                        const api = agent.adapterType === "api";
                        return (
                          <tr key={agent.id}>
                            <td>
                              <strong>{agent.name}</strong>
                              <small>
                                <code>{api ? t("directApi") : (diag?.version ?? t("unknown"))}</code>
                              </small>
                            </td>
                            <td title={diag?.detail}>
                              <span className="settings-status">
                                <span
                                  className="chat-health-dot"
                                  data-health={agentHealth(agent, diag, provider)}
                                  aria-hidden="true"
                                />
                                {t(agentStatusKey(agent, diag) ?? "installed")}
                              </span>
                            </td>
                            <td>{provider?.name ?? agent.providerId ?? t("ownLogin")}</td>
                            <td>
                              <code title={api ? provider?.baseUrl : undefined}>
                                {api ? providerHost(provider?.baseUrl) : (diag?.resolvedPath ?? agent.command)}
                              </code>
                            </td>
                            <td>
                              {api
                                ? t("textOnly")
                                : [
                                    diag?.capabilities?.structuredStream && t("structuredStream"),
                                    diag?.capabilities?.toolEvents && t("toolEvents"),
                                    diag?.capabilities?.nativeResume && t("nativeResume"),
                                  ]
                                    .filter(Boolean)
                                    .join(", ") || t("historyReplay")}
                            </td>
                            <td>
                              <label className="settings-switch">
                                <input
                                  type="checkbox"
                                  role="switch"
                                  aria-label={`${t("enabled")} ${agent.name}`}
                                  checked={agent.enabled}
                                  disabled={busy}
                                  onChange={(event) => void setAgentEnabled(agent.id, event.target.checked)}
                                />
                                <span aria-hidden="true" />
                              </label>
                            </td>
                            <td className="settings-row-actions">
                              <button type="button" onClick={() => edit(agent)}>
                                {t("edit")}
                              </button>
                              <button type="button" disabled={busy} onClick={() => void testAgent(agent.id)}>
                                {t("test")}
                              </button>
                              {!isBuiltInAgent(agent) && (
                                <button
                                  type="button"
                                  onClick={() => {
                                    if (window.confirm(t("removeAgentWarning"))) void deleteAgent(agent.id);
                                  }}
                                >
                                  {t("remove")}
                                </button>
                              )}
                            </td>
                          </tr>
                        );
                      })}
                  </tbody>
                </table>
              </div>
              {formOpen && (
                <form onSubmit={(event) => void submit(event)} className="settings-card settings-agent-form">
                  <h3>{editing ? t("edit") : draft.adapterType === "api" ? t("directApi") : t("customCli")}</h3>
                  <label>
                    {t("name")}
                    <input
                      required
                      value={draft.name}
                      onChange={(event) => setDraft({ ...draft, name: event.target.value })}
                    />
                  </label>
                  {draft.adapterType !== "api" && (
                    <label>
                      {t("executable")}
                      <input
                        required
                        value={draft.command}
                        onChange={(event) => setDraft({ ...draft, command: event.target.value })}
                      />
                    </label>
                  )}
                  {draft.adapterType === "api" ? (
                    <>
                      <label>
                        {t("modelSource")}
                        <select
                          required
                          value={draft.providerId ?? ""}
                          onChange={(event) => {
                            const next = providers.find((item) => item.id === event.target.value);
                            const auto = apiAgentDraft(providers.find((item) => item.id === draft.providerId));
                            // Keep a name the user typed; follow the provider while it is still the default.
                            setDraft({
                              ...draft,
                              providerId: next?.id ?? null,
                              name: draft.name === auto.name ? apiAgentDraft(next).name : draft.name,
                            });
                          }}
                        >
                          {!providers.length && <option value="">{t("directApiNeedsProvider")}</option>}
                          {providers.map((item) => (
                            <option key={item.id} value={item.id}>
                              {item.name} · {t(providerKindKey(item.kind))}
                            </option>
                          ))}
                        </select>
                      </label>
                      <p>{t("directApiHint")}</p>
                    </>
                  ) : (
                    (() => {
                      const diag = diagnostics.find((item) => item.agentId === editing);
                      // Custom CLIs take no provider; an undiagnosed built-in lists every kind and the backend checks.
                      const kinds =
                        draft.adapterType === "cli" ? [] : (diag?.capabilities?.providerKinds ?? PROVIDER_KINDS);
                      const compatible = providers.filter((item) => kinds.includes(item.kind));
                      const incompatible = providers.filter((item) => !kinds.includes(item.kind));
                      return (
                        <>
                          <div className="settings-field">
                            <span>{t("modelSource")}</span>
                            <span className="settings-inline-field">
                              <select
                                aria-label={t("modelSource")}
                                value={draft.providerId ?? ""}
                                disabled={!kinds.length}
                                onChange={(event) => setDraft({ ...draft, providerId: event.target.value || null })}
                              >
                                <option value="">
                                  {t("ownLoginOption", { command: draft.command.trim() || draft.name })}
                                </option>
                                {compatible.map((item) => (
                                  <option key={item.id} value={item.id}>
                                    {item.name} · {t(providerKindKey(item.kind))}
                                  </option>
                                ))}
                                {incompatible.length > 0 && (
                                  <optgroup label={t("providerIncompatible")}>
                                    {incompatible.map((item) => (
                                      <option key={item.id} value={item.id} disabled>
                                        {item.name} · {t(providerKindKey(item.kind))}
                                      </option>
                                    ))}
                                  </optgroup>
                                )}
                              </select>
                              <button type="button" onClick={() => setTab("providers")}>
                                {t("manageProviders")}
                              </button>
                            </span>
                          </div>
                          <p>{kinds.length ? t("providerInjectHint") : t("providerNotSupported")}</p>
                        </>
                      );
                    })()
                  )}
                  {draft.adapterType === "cli" && (
                    <>
                      <label>
                        {t("arguments")}
                        <textarea
                          rows={4}
                          value={args}
                          onChange={(event) => setArgs(event.target.value)}
                          placeholder="{prompt}"
                        />
                      </label>
                      <label>
                        {t("outputFormat")}
                        <select value="plain" disabled>
                          <option value="plain">{t("plainText")}</option>
                        </select>
                      </label>
                      <p>{t("customHint")}</p>
                    </>
                  )}
                  <div className="settings-form-actions">
                    <button type="button" onClick={() => setFormOpen(false)}>
                      {t("cancel")}
                    </button>
                    <button
                      type="submit"
                      className="settings-primary"
                      disabled={busy || (draft.adapterType === "api" && !draft.providerId)}
                    >
                      {t("save")}
                    </button>
                  </div>
                </form>
              )}
              <section className="settings-card">
                <h3>{t("permissions")}</h3>
                <p>{t("cliPermissions")}</p>
                {!diagnostics.some((diag) => diag.capabilities) ? (
                  <p className="settings-empty">{t("permissionsEmpty")}</p>
                ) : (
                  <div className="settings-table-wrap settings-modes">
                    <table>
                      <thead>
                        <tr>
                          <th>{t("agents")}</th>
                          <th data-mode="explore">{t("readOnly")}</th>
                          <th data-mode="ask">{t("ask")}</th>
                          <th data-mode="auto">{t("auto")}</th>
                        </tr>
                      </thead>
                      <tbody>
                        {diagnostics
                          .filter((diag) => diag.capabilities)
                          .map((diag) => (
                            <tr key={diag.agentId}>
                              <td>{state.agents.find((agent) => agent.id === diag.agentId)?.name}</td>
                              {["explore", "ask", "auto"].map((mode) => (
                                <td key={mode}>
                                  <code>{diag.capabilities?.modes.find((item) => item.mode === mode)?.policy}</code>
                                </td>
                              ))}
                            </tr>
                          ))}
                      </tbody>
                    </table>
                  </div>
                )}
              </section>
            </>
          )}
          {tab === "appearance" && (
            <>
              <section className="settings-card">
                <h3>{t("language")}</h3>
                <div className="settings-choices" role="radiogroup" aria-label={t("language")}>
                  {(["en", "zh-CN"] as const).map((value) => (
                    <button
                      type="button"
                      role="radio"
                      aria-checked={locale === value}
                      key={value}
                      onClick={() => void setLocale(value).catch((error) => setError(String(error)))}
                    >
                      <span className="settings-ring" aria-hidden="true" />
                      {t(value === "en" ? "english" : "chinese")}
                    </button>
                  ))}
                </div>
              </section>
              <section className="settings-card">
                <h3>{t("theme")}</h3>
                <div className="settings-choices" role="radiogroup" aria-label={t("theme")}>
                  {(["system", "dark", "light"] as const).map((value) => {
                    const Icon = themeIcons[value];
                    return (
                      <button
                        type="button"
                        role="radio"
                        aria-checked={themeMode === value}
                        key={value}
                        onClick={() => setThemeMode(value)}
                      >
                        <Icon size={15} aria-hidden="true" />
                        {t(value)}
                      </button>
                    );
                  })}
                </div>
              </section>
            </>
          )}
          {tab === "experiments" && (
            <section className="settings-card settings-toggle-row">
              <div>
                <h3>{t("inlineCards")}</h3>
                <p>{t("inlineCardsHint")}</p>
              </div>
              <label className="settings-switch">
                <input
                  type="checkbox"
                  role="switch"
                  aria-label={t("inlineCards")}
                  checked={inlineCards}
                  onChange={(event) =>
                    void setInlineCardsEnabled(event.target.checked).catch((error) => setError(String(error)))
                  }
                />
                <span aria-hidden="true" />
              </label>
            </section>
          )}
          {tab === "diagnostics" && (
            <section className="settings-card">
              <label className="settings-check">
                <input
                  type="checkbox"
                  checked={includeLogs}
                  onChange={(event) => setIncludeLogs(event.target.checked)}
                />
                {t("includeLogs")}
              </label>
              <p>
                <button
                  type="button"
                  className="settings-primary"
                  disabled={!state.projects.current || !hasTauriRuntime()}
                  onClick={() => void exportDiagnostics()}
                >
                  {t("exportDiagnostics")}
                </button>
              </p>
              {notice && (
                <p role="status">
                  <code>{notice}</code>
                </p>
              )}
            </section>
          )}
          {tab === "about" && (
            <section className="settings-card">
              <h3>Loom {version}</h3>
              <UpdateButton manual />
            </section>
          )}
        </div>
      </main>
    </div>
  );
}
