import { openPath } from "@tauri-apps/plugin-opener";
import { useEffect, useMemo, useState, type FormEvent, type ReactNode } from "react";
import {
  ArrowLeft,
  Bot,
  CheckCircle2,
  ExternalLink,
  Info,
  Palette,
  Pencil,
  Plus,
  RefreshCw,
  ShieldCheck,
  SlidersHorizontal,
  Trash2,
  XCircle,
} from "lucide-react";
import { invokeCommand, TAURI_COMMANDS } from "../../api";
import { useTheme } from "../../contexts/ThemeContext";
import {
  DEFAULT_APP_SETTINGS,
  EMPTY_PROJECT_AGENT_PREFERENCES,
  type AgentAdapterType,
  type AgentCapability,
  type AgentConfig,
  type AgentConfigInput,
  type AgentDiagnostic,
  type AppSettings,
  type HealthCheckResult,
  type ProjectAgentPreferences,
  type TerminalSlot,
} from "../../domain";
import { useAgentBridge } from "../../hooks/useAgentBridge";
import { hasTauriRuntime } from "../../hooks/runtime";
import { useSettingsBridge } from "../../hooks/useSettingsBridge";
import { useProjectPreferencesBridge } from "../../hooks/useProjectPreferencesBridge";
import { useTerminalBridge } from "../../hooks/useTerminalBridge";
import { useAppState } from "../../state/AppStateContext";
import {
  applyTerminalSlotDraft,
  blankTerminalSlotDraft,
  draftFromTerminalSlot,
  resolveTerminalSlotCwd,
  type TerminalSlotDraft,
} from "../../utils/terminalSlots";
import { Button } from "../common/Button";
import "./SettingsPage.css";

interface SettingsPageProps {
  onBack: () => void;
  initialTab?: SettingsTab;
}

type SettingsTab = "general" | "appearance" | "agents" | "safety" | "about";

const cliAdapterTypes = new Set(["codex_cli", "claude_code_cli", "dummy", "cli"]);
const capabilityOptions: AgentCapability[] = [
  "planning",
  "implementation",
  "review",
  "debugging",
  "testing",
  "documentation",
];
const adapterOptions: Array<{ value: AgentAdapterType; label: string; command: string; name: string }> = [
  { value: "codex_cli", label: "Codex CLI", command: "codex", name: "Codex" },
  { value: "claude_code_cli", label: "Claude Code", command: "claude", name: "Claude Code" },
  { value: "cli", label: "Custom CLI", command: "", name: "Custom Agent" },
  { value: "dummy", label: "Dummy/Test", command: "dummy", name: "Dummy Agent" },
];
const settingsTabs: Array<{ id: SettingsTab; label: string; icon: ReactNode }> = [
  { id: "general", label: "General", icon: <SlidersHorizontal size={15} /> },
  { id: "appearance", label: "Appearance", icon: <Palette size={15} /> },
  { id: "agents", label: "Agents", icon: <Bot size={15} /> },
  { id: "safety", label: "Commands & Safety", icon: <ShieldCheck size={15} /> },
  { id: "about", label: "About", icon: <Info size={15} /> },
];

function defaultAgentDraft(): AgentConfigInput {
  return {
    name: "Custom Agent",
    command: "",
    args: [],
    workingDirectoryPolicy: "project_root",
    capabilities: ["planning", "implementation", "review"],
    adapterType: "cli",
    canWriteFiles: true,
    canRunCommands: true,
    enabled: true,
  };
}

function draftFromAgent(agent: AgentConfig): AgentConfigInput {
  return {
    name: agent.name,
    command: agent.command,
    args: agent.args,
    workingDirectoryPolicy: agent.workingDirectoryPolicy,
    capabilities: agent.capabilities,
    adapterType: agent.adapterType,
    canWriteFiles: agent.canWriteFiles,
    canRunCommands: agent.canRunCommands,
    enabled: agent.enabled,
  };
}

function isBuiltInAgent(agent: AgentConfig) {
  return ["agent-codex", "agent-claude", "agent-dummy"].includes(agent.id);
}

function profileSummary(agent: AgentConfig) {
  switch (agent.adapterType) {
    case "codex_cli":
      return "codex exec --cd {projectPath} --sandbox read-only -";
    case "claude_code_cli":
      return "claude -p --output-format text";
    case "dummy":
      return "test fixture only";
    default:
      return agent.args.length > 0 ? `${agent.command} ${agent.args.join(" ")}` : agent.command;
  }
}

function shortTimestamp(timestampMs?: number) {
  if (!timestampMs) {
    return "not available";
  }
  return new Date(timestampMs).toLocaleString("en-US", {
    hour: "2-digit",
    minute: "2-digit",
    year: "numeric",
    month: "short",
    day: "2-digit",
  });
}

function SettingCard({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="setcard">
      <div className="setcard-title">{title}</div>
      {children}
    </section>
  );
}

function Field({
  title,
  description,
  control,
}: {
  title: string;
  description?: string;
  control: ReactNode;
}) {
  return (
    <div className="settings-field-row">
      <div>
        <div className="settings-field-title">{title}</div>
        {description && <div className="settings-field-desc">{description}</div>}
      </div>
      <div className="settings-field-control">{control}</div>
    </div>
  );
}

function Switch({ on }: { on: boolean }) {
  return <span className={`settings-switch ${on ? "on" : ""}`} />;
}

function ToggleControl({
  checked,
  disabled,
  onChange,
}: {
  checked: boolean;
  disabled?: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <button
      type="button"
      className="settings-toggle-control"
      aria-pressed={checked}
      disabled={disabled}
      onClick={() => onChange(!checked)}
    >
      <Switch on={checked} />
    </button>
  );
}

function ThemePreview({
  label,
  active,
  mode,
  onClick,
}: {
  label: string;
  active: boolean;
  mode: "light" | "dark" | "system";
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      className={`theme-card ${active ? "on" : ""}`}
      onClick={onClick}
    >
      <div className={`theme-preview theme-preview-${mode}`}>
        <div className="theme-preview-side" />
        <div className="theme-preview-main">
          <span />
          <span />
          <span />
        </div>
      </div>
      <div className="theme-card-caption">
        {label}
        <span />
      </div>
    </button>
  );
}

export function SettingsPage({ onBack, initialTab = "general" }: SettingsPageProps) {
  const { theme, themeMode, setThemeMode } = useTheme();
  const { state } = useAppState();
  const { createAgent, deleteAgent, diagnoseAgents, loadAgents, setAgentEnabled, updateAgent } = useAgentBridge();
  const { loadSettings, saveSettings } = useSettingsBridge();
  const { loadProjectAgentPreferences, saveProjectAgentPreferences } = useProjectPreferencesBridge();
  const { listTerminalSlots, saveTerminalSlots, suggestTerminalSlots } = useTerminalBridge();
  const [activeTab, setActiveTab] = useState<SettingsTab>(initialTab);
  const [agentDraft, setAgentDraft] = useState<AgentConfigInput>(defaultAgentDraft);
  const [argsText, setArgsText] = useState("");
  const [editingAgentId, setEditingAgentId] = useState<string | null>(null);
  const [appSettings, setAppSettings] = useState<AppSettings>(DEFAULT_APP_SETTINGS);
  const [settingsError, setSettingsError] = useState<string | null>(null);
  const [projectPreferences, setProjectPreferences] = useState<ProjectAgentPreferences>({
    ...EMPTY_PROJECT_AGENT_PREFERENCES,
  });
  const [projectPreferencesError, setProjectPreferencesError] = useState<string | null>(null);
  const [agentDiagnostics, setAgentDiagnostics] = useState<AgentDiagnostic[]>([]);
  const [health, setHealth] = useState<HealthCheckResult | null>(null);
  const [slots, setSlots] = useState<TerminalSlot[]>([]);
  const [slotsLoaded, setSlotsLoaded] = useState(false);
  const [slotDraft, setSlotDraft] = useState<TerminalSlotDraft | null>(null);
  const [slotError, setSlotError] = useState<string | null>(null);

  const project = state.projects.current;

  useEffect(() => {
    void loadAgents();
    void diagnoseAgents().then(setAgentDiagnostics);
  }, [diagnoseAgents, loadAgents]);

  useEffect(() => {
    let cancelled = false;
    void loadSettings()
      .then((loaded) => {
        if (!cancelled) {
          setAppSettings(loaded);
        }
      })
      .catch((error: unknown) => {
        if (!cancelled) {
          setSettingsError(error instanceof Error ? error.message : String(error));
        }
      });
    return () => {
      cancelled = true;
    };
  }, [loadSettings]);

  useEffect(() => {
    let cancelled = false;
    setProjectPreferencesError(null);
    if (!project) {
      setProjectPreferences({ ...EMPTY_PROJECT_AGENT_PREFERENCES });
      return;
    }
    void loadProjectAgentPreferences(project.path)
      .then((loaded) => {
        if (!cancelled) {
          setProjectPreferences(loaded);
        }
      })
      .catch((error: unknown) => {
        if (!cancelled) {
          setProjectPreferencesError(error instanceof Error ? error.message : String(error));
        }
      });
    return () => {
      cancelled = true;
    };
  }, [project, loadProjectAgentPreferences]);

  useEffect(() => {
    if (!hasTauriRuntime()) {
      setHealth({
        status: "ok",
        app: "Loom",
        version: "preview",
        backend: "tauri",
        timestampMs: Date.now(),
      });
      return;
    }

    let cancelled = false;
    void invokeCommand<HealthCheckResult>(TAURI_COMMANDS.healthCheck).then((result) => {
      if (!cancelled) {
        setHealth(result);
      }
    });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    setSlotsLoaded(false);
    setSlotDraft(null);
    setSlotError(null);

    if (!project) {
      setSlots([]);
      setSlotsLoaded(true);
      return;
    }

    void listTerminalSlots(project.path).then(async (loaded) => {
      const next = loaded.length > 0 ? loaded : await suggestTerminalSlots(project.path);
      if (!cancelled) {
        setSlots(next);
        setSlotsLoaded(true);
      }
    });

    return () => {
      cancelled = true;
    };
  }, [project, listTerminalSlots, suggestTerminalSlots]);

  const cliAgents = state.agents.filter((agent) => cliAdapterTypes.has(agent.adapterType));
  const selectedAdapter = adapterOptions.find((option) => option.value === agentDraft.adapterType);
  const editingAgent = editingAgentId
    ? cliAgents.find((agent) => agent.id === editingAgentId) ?? null
    : null;
  const availableAgents = cliAgents.filter((agent) => agent.available);
  const phaseCoverage = useMemo(() => new Set(cliAgents.flatMap((agent) => agent.capabilities)), [cliAgents]);

  function updateAppSettings(patch: Partial<AppSettings>) {
    const next = { ...appSettings, ...patch };
    setAppSettings(next);
    setSettingsError(null);
    void saveSettings(next)
      .then((saved) => setAppSettings(saved))
      .catch((error: unknown) => {
        setSettingsError(error instanceof Error ? error.message : String(error));
      });
  }

  function updateProjectPreferences(patch: Partial<ProjectAgentPreferences>) {
    if (!project) {
      return;
    }
    const next = { ...projectPreferences, ...patch };
    setProjectPreferences(next);
    setProjectPreferencesError(null);
    void saveProjectAgentPreferences(project.path, next)
      .then((saved) => setProjectPreferences(saved))
      .catch((error: unknown) => {
        setProjectPreferencesError(error instanceof Error ? error.message : String(error));
      });
  }

  function toggleProjectPreference(
    key: "planningAgentIds" | "reviewAgentIds",
    agentId: string,
  ) {
    const current = projectPreferences[key];
    updateProjectPreferences({
      [key]: current.includes(agentId)
        ? current.filter((id) => id !== agentId)
        : [...current, agentId],
    });
  }

  function handleAdapterChange(adapterType: AgentAdapterType) {
    const adapter = adapterOptions.find((option) => option.value === adapterType);

    setAgentDraft((current) => ({
      ...current,
      adapterType,
      name: current.name.trim() && current.name !== selectedAdapter?.name ? current.name : adapter?.name ?? current.name,
      command: adapterType === "cli" ? current.command : adapter?.command ?? current.command,
      canWriteFiles: adapterType === "dummy" ? false : current.canWriteFiles,
      canRunCommands: adapterType === "dummy" ? false : current.canRunCommands,
    }));
  }

  function toggleCapability(capability: AgentCapability) {
    setAgentDraft((current) => {
      const hasCapability = current.capabilities.includes(capability);
      const capabilities = hasCapability
        ? current.capabilities.filter((value) => value !== capability)
        : [...current.capabilities, capability];

      return { ...current, capabilities };
    });
  }

  async function handleCreateAgent(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();

    if (!agentDraft.name.trim() || !agentDraft.command.trim() || agentDraft.capabilities.length === 0) {
      return;
    }

    const input = {
      ...agentDraft,
      name: agentDraft.name.trim(),
      command: agentDraft.command.trim(),
      args: argsText
        .split(/\r?\n/)
        .map((arg) => arg.trim())
        .filter(Boolean),
    };
    const result = editingAgentId
      ? await updateAgent(editingAgentId, input)
      : await createAgent(input);

    if (result) {
      setAgentDraft(defaultAgentDraft());
      setArgsText("");
      setEditingAgentId(null);
    }
  }

  function handleEditAgent(agent: AgentConfig) {
    if (isBuiltInAgent(agent)) {
      return;
    }
    setEditingAgentId(agent.id);
    setAgentDraft(draftFromAgent(agent));
    setArgsText(agent.args.join("\n"));
    setActiveTab("agents");
  }

  function handleCancelEdit() {
    setEditingAgentId(null);
    setAgentDraft(defaultAgentDraft());
    setArgsText("");
  }

  async function persistSlots(next: TerminalSlot[]) {
    if (!project) {
      return;
    }
    setSlotError(null);
    const saved = await saveTerminalSlots(project.path, next);
    if (saved) {
      setSlots(saved);
    } else {
      setSlotError("Terminal slots could not be saved in this environment.");
    }
  }

  function saveSlotDraft() {
    if (!slotDraft) {
      return;
    }
    const next = applyTerminalSlotDraft(slots, slotDraft);
    if (!next) {
      return;
    }
    void persistSlots(next);
    setSlotDraft(null);
  }

  function removeSlot(slot: TerminalSlot) {
    void persistSlots(slots.filter((candidate) => candidate.id !== slot.id));
  }

  async function resetSlotsToDetected() {
    if (!project) {
      return;
    }
    const detected = await suggestTerminalSlots(project.path);
    await persistSlots(detected);
  }

  function openResource(path: string) {
    if (!hasTauriRuntime()) {
      return;
    }
    void openPath(path);
  }

  function renderGeneral() {
    return (
      <>
        <SettingCard title="Current project">
          <Field
            title="Project"
            description={project ? project.path : "Open a local project before configuring project-specific settings."}
            control={<span className="settings-input-pill">{project?.name ?? "No project open"}</span>}
          />
          <Field
            title="Repository"
            description={project?.gitBranch ? `Current branch: ${project.gitBranch}` : "Git branch is shown when the current project is a git repository."}
            control={<span className={`settings-tag ${project?.isGitRepository ? "ok" : ""}`}>{project?.isGitRepository ? "Git" : "Not git"}</span>}
          />
          <Field
            title="Loom project store"
            description=".loom stores local task history, logs, terminal slots, and validation evidence."
            control={<span className={`settings-tag ${project?.loomDirReady ? "ok" : ""}`}>{project?.loomDirReady ? "Ready" : "Not initialized"}</span>}
          />
        </SettingCard>
        <SettingCard title="Runtime model">
          <Field
            title="Data location"
            description="Agent profiles and app settings are stored locally in the desktop app data directory."
            control={<span className="settings-chip">Local only</span>}
          />
          <Field
            title="Project stacks"
            description="Detected stacks drive command suggestions and terminal slot defaults."
            control={
              <span className="settings-row-control wrap">
                {(project?.detectedStacks.length ? project.detectedStacks : ["Not detected"]).map((stack) => (
                  <span className="settings-chip" key={stack}>{stack}</span>
                ))}
              </span>
            }
          />
        </SettingCard>
      </>
    );
  }

  function renderAppearance() {
    return (
      <>
        <SettingCard title="Theme">
          <div className="theme-grid">
            <ThemePreview label="Light" mode="light" active={themeMode === "light"} onClick={() => setThemeMode("light")} />
            <ThemePreview label="Dark" mode="dark" active={themeMode === "dark"} onClick={() => setThemeMode("dark")} />
            <ThemePreview label="System" mode="system" active={themeMode === "system"} onClick={() => setThemeMode("system")} />
          </div>
        </SettingCard>
        <SettingCard title="Interface">
          <Field
            title="Effective theme"
            description="System mode follows the operating system until you choose Light or Dark explicitly."
            control={<span className="settings-chip">{theme}</span>}
          />
          <Field
            title="Typography and density"
            description="Loom currently uses the bundled interface fonts and fixed workbench density."
            control={<span className="settings-chip">Built in</span>}
          />
        </SettingCard>
      </>
    );
  }

  function renderAgents() {
    const stageAgents = (capabilities: AgentCapability[]) => cliAgents.filter(
      (agent) => agent.enabled && agent.capabilities.some((capability) => capabilities.includes(capability)),
    );
    const agentSelect = (
      value: string | undefined,
      capabilities: AgentCapability[],
      onChange: (agentId: string | undefined) => void,
    ) => (
      <select
        className="settings-stage-select"
        value={value ?? ""}
        disabled={!project}
        onChange={(event) => onChange(event.target.value || undefined)}
      >
        <option value="">No default</option>
        {stageAgents(capabilities).map((agent) => (
          <option key={agent.id} value={agent.id}>{agent.name}{agent.available ? "" : " (unavailable)"}</option>
        ))}
      </select>
    );

    return (
      <>
        <div className="settings-page-heading">
          <div>
            <div className="settings-sub">Local AI coding agents available to orchestrate</div>
          </div>
          <div className="settings-heading-actions">
            <Button type="button" variant="ghost" iconLeft={<RefreshCw size={14} />} onClick={() => {
              void loadAgents();
              void diagnoseAgents().then(setAgentDiagnostics);
            }}>
              Refresh diagnostics
            </Button>
            <Button
              type="button"
              variant="primary"
              iconLeft={<Plus size={14} />}
              onClick={() => {
                handleCancelEdit();
                setActiveTab("agents");
              }}
            >
              Add Agent
            </Button>
          </div>
        </div>
        <div className="settings-tiles">
          <div className="settings-tile"><span>Total</span><strong>{cliAgents.length}</strong></div>
          <div className="settings-tile"><span>Available</span><strong className="ok">{availableAgents.length}</strong></div>
          <div className="settings-tile"><span>Unreachable</span><strong className="err">{cliAgents.length - availableAgents.length}</strong></div>
          <div className="settings-tile"><span>Coverage</span><strong>{Math.min(phaseCoverage.size, 4)}/4</strong></div>
        </div>
        <SettingCard title="Project stage defaults">
          <Field
            title="Project scope"
            description={project ? `Saved in ${project.path}/.loom/agent-preferences.json and applied to new tasks.` : "Open a project to configure stage defaults."}
            control={<span className="settings-chip">{project?.name ?? "No project"}</span>}
          />
          <Field
            title="Planning Agents"
            description="Agents selected automatically for a new planning discussion."
            control={
              <span className="settings-row-control wrap">
                {stageAgents(["planning"]).map((agent) => (
                  <label className="settings-check" key={agent.id}>
                    <input
                      type="checkbox"
                      disabled={!project}
                      checked={projectPreferences.planningAgentIds.includes(agent.id)}
                      onChange={() => toggleProjectPreference("planningAgentIds", agent.id)}
                    />
                    <span>{agent.name}</span>
                  </label>
                ))}
              </span>
            }
          />
          <Field
            title="Implementation Agent"
            description="Primary Agent inherited by new tasks when one is not selected explicitly."
            control={agentSelect(projectPreferences.implementationAgentId, ["implementation"], (implementationAgentId) => updateProjectPreferences({ implementationAgentId }))}
          />
          <Field
            title="Review Agents"
            description="Collaborators assigned to implementation and plan review."
            control={
              <span className="settings-row-control wrap">
                {stageAgents(["review"]).map((agent) => (
                  <label className="settings-check" key={agent.id}>
                    <input
                      type="checkbox"
                      disabled={!project}
                      checked={projectPreferences.reviewAgentIds.includes(agent.id)}
                      onChange={() => toggleProjectPreference("reviewAgentIds", agent.id)}
                    />
                    <span>{agent.name}</span>
                  </label>
                ))}
              </span>
            }
          />
          <Field
            title="Debugging Agent"
            description="Default Agent for repair and feedback-driven debugging."
            control={agentSelect(projectPreferences.debuggingAgentId, ["debugging", "implementation"], (debuggingAgentId) => updateProjectPreferences({ debuggingAgentId }))}
          />
          <Field
            title="Testing Agent"
            description="Default Agent for validation failures and test analysis."
            control={agentSelect(projectPreferences.testingAgentId, ["testing"], (testingAgentId) => updateProjectPreferences({ testingAgentId }))}
          />
          <Field
            title="Documentation Agent"
            description="Default Agent for task summaries and documentation work."
            control={agentSelect(projectPreferences.documentationAgentId, ["documentation"], (documentationAgentId) => updateProjectPreferences({ documentationAgentId }))}
          />
          {projectPreferencesError && <div className="settings-inline-error">{projectPreferencesError}</div>}
        </SettingCard>
        <SettingCard title="Installed agents">
          <div className="settings-table-wrap">
            <table className="settings-agent-table">
              <thead>
                <tr>
                  <th>Agent & executable</th>
                  <th>Adapter</th>
                  <th>Capabilities</th>
                  <th>Permissions</th>
                  <th>Status</th>
                  <th>Implementation</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {cliAgents.map((agent) => {
                  const diagnostic = agentDiagnostics.find((candidate) => candidate.agentId === agent.id);
                  return (
                  <tr key={agent.id}>
                    <td>
                      <strong>{agent.name}</strong>
                      <code>{profileSummary(agent)}</code>
                    </td>
                    <td><span className="settings-chip">{agent.adapterType}</span></td>
                    <td>
                      <div className="settings-tag-row">
                        {agent.capabilities.map((capability) => (
                          <span className="settings-tag" key={capability}>{capability}</span>
                        ))}
                      </div>
                    </td>
                    <td>
                      <span className="settings-perm"><Switch on={agent.canWriteFiles} /> File</span>
                      <span className="settings-perm"><Switch on={agent.canRunCommands} /> Cmd</span>
                    </td>
                    <td>
                      <span className={`settings-pill ${diagnostic?.status === "ready" ? "ok" : "err"}`} title={diagnostic?.detail}>
                        {diagnostic?.status === "ready" ? <CheckCircle2 size={13} /> : <XCircle size={13} />}
                        {diagnostic?.status ?? (agent.available ? "available" : "missing")}
                      </span>
                      {diagnostic?.version && <code title={diagnostic.resolvedPath}>{diagnostic.version}</code>}
                      {diagnostic?.resolvedPath && <span className="settings-diagnostic-path" title={diagnostic.resolvedPath}>{diagnostic.resolvedPath}</span>}
                    </td>
                    <td>{agent.capabilities.includes("implementation") ? <span className="settings-tag primary">Capable</span> : <span className="settings-dim">No</span>}</td>
                    <td>
                      <div className="settings-agent-actions">
                        {isBuiltInAgent(agent) ? (
                          <span className="settings-dim">Built-in</span>
                        ) : (
                          <button type="button" className="settings-icon-control" title={`Edit ${agent.name}`} onClick={() => handleEditAgent(agent)}>
                            <Pencil size={14} />
                          </button>
                        )}
                        <button
                          type="button"
                          className="settings-text-control"
                          onClick={() => void setAgentEnabled(agent.id, !agent.enabled)}
                        >
                          {agent.enabled ? "Disable" : "Enable"}
                        </button>
                        {!isBuiltInAgent(agent) && (
                          <button type="button" className="settings-icon-control danger" title={`Delete ${agent.name}`} onClick={() => void deleteAgent(agent.id)}>
                            <Trash2 size={14} />
                          </button>
                        )}
                      </div>
                    </td>
                  </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
          <form className="settings-agent-form" onSubmit={handleCreateAgent}>
            <div className="settings-form-header">
              <div>
                <div className="settings-row-title">{editingAgent ? `Edit ${editingAgent.name}` : "Add custom Agent"}</div>
                <div className="settings-row-desc">Store reusable CLI profiles behind the adapter interface. Built-in profiles can only be enabled or disabled.</div>
              </div>
              <div className="settings-form-actions">
                {editingAgent && (
                  <button type="button" className="settings-text-control" onClick={handleCancelEdit}>
                    Cancel
                  </button>
                )}
                <Button type="submit" variant="primary" disabled={!agentDraft.name.trim() || !agentDraft.command.trim()}>
                  {editingAgent ? "Save Agent" : "Add Agent"}
                </Button>
              </div>
            </div>
            <div className="settings-form-grid">
              <label className="settings-form-field">
                <span>Name</span>
                <input value={agentDraft.name} onChange={(event) => setAgentDraft((current) => ({ ...current, name: event.target.value }))} />
              </label>
              <label className="settings-form-field">
                <span>Adapter</span>
                <select value={agentDraft.adapterType} onChange={(event) => handleAdapterChange(event.target.value as AgentAdapterType)}>
                  {adapterOptions.map((option) => (
                    <option key={option.value} value={option.value}>{option.label}</option>
                  ))}
                </select>
              </label>
              <label className="settings-form-field">
                <span>Command</span>
                <input value={agentDraft.command} onChange={(event) => setAgentDraft((current) => ({ ...current, command: event.target.value }))} />
              </label>
              <label className="settings-form-field wide">
                <span>Args, one per line</span>
                <textarea value={argsText} onChange={(event) => setArgsText(event.target.value)} placeholder="Leave blank for the adapter default." />
              </label>
            </div>
            <div className="settings-check-row">
              {capabilityOptions.map((capability) => (
                <label className="settings-check" key={capability}>
                  <input type="checkbox" checked={agentDraft.capabilities.includes(capability)} onChange={() => toggleCapability(capability)} />
                  <span>{capability}</span>
                </label>
              ))}
            </div>
            <div className="settings-check-row">
              <label className="settings-check">
                <input type="checkbox" checked={agentDraft.canWriteFiles} disabled={agentDraft.adapterType === "dummy"} onChange={(event) => setAgentDraft((current) => ({ ...current, canWriteFiles: event.target.checked }))} />
                <span>Can write files</span>
              </label>
              <label className="settings-check">
                <input type="checkbox" checked={agentDraft.canRunCommands} disabled={agentDraft.adapterType === "dummy"} onChange={(event) => setAgentDraft((current) => ({ ...current, canRunCommands: event.target.checked }))} />
                <span>Can run commands</span>
              </label>
              <label className="settings-check">
                <input type="checkbox" checked={agentDraft.enabled} onChange={(event) => setAgentDraft((current) => ({ ...current, enabled: event.target.checked }))} />
                <span>Enable immediately</span>
              </label>
            </div>
          </form>
          {state.app.agentError && <div className="settings-inline-error">{state.app.agentError}</div>}
        </SettingCard>
      </>
    );
  }

  function renderSlotEditor() {
    if (!slotDraft) {
      return null;
    }

    return (
      <form className="settings-slot-editor" onSubmit={(event) => {
        event.preventDefault();
        saveSlotDraft();
      }}>
        <div className="settings-form-grid">
          <label className="settings-form-field">
            <span>Name</span>
            <input value={slotDraft.name} onChange={(event) => setSlotDraft({ ...slotDraft, name: event.target.value })} />
          </label>
          <label className="settings-form-field">
            <span>Kind</span>
            <select value={slotDraft.kind} onChange={(event) => setSlotDraft({ ...slotDraft, kind: event.target.value as TerminalSlot["kind"] })}>
              <option value="preview">Preview</option>
              <option value="validation">Validation</option>
            </select>
          </label>
          <label className="settings-form-field">
            <span>Working directory</span>
            <input value={slotDraft.cwd} placeholder="project root" onChange={(event) => setSlotDraft({ ...slotDraft, cwd: event.target.value })} />
          </label>
          <label className="settings-form-field wide">
            <span>Command</span>
            <input value={slotDraft.command} placeholder="pnpm test" onChange={(event) => setSlotDraft({ ...slotDraft, command: event.target.value })} />
          </label>
        </div>
        <div className="settings-form-actions">
          <button type="button" className="settings-text-control" onClick={() => setSlotDraft(null)}>Cancel</button>
          <Button type="submit" variant="primary" disabled={!slotDraft.name.trim() || !slotDraft.command.trim()}>
            Save slot
          </Button>
        </div>
      </form>
    );
  }

  function renderSafety() {
    return (
      <>
        <SettingCard title="Project terminal slots">
          {!project ? (
            <div className="settings-empty-state">Open a project to configure preview and validation commands.</div>
          ) : (
            <>
              <div className="settings-card-toolbar">
                <div className="settings-row-desc">
                  Slots are saved to <code>.loom/terminal-slots.json</code> and are used by the Testing cockpit and implementation auto-validation.
                </div>
                <div className="settings-heading-actions">
                  <Button type="button" variant="ghost" iconLeft={<RefreshCw size={14} />} onClick={() => void resetSlotsToDetected()}>
                    Reset to detected
                  </Button>
                  <Button type="button" variant="primary" iconLeft={<Plus size={14} />} onClick={() => setSlotDraft(blankTerminalSlotDraft())}>
                    Add slot
                  </Button>
                </div>
              </div>
              <div className="settings-table-wrap">
                <table className="settings-agent-table settings-command-table">
                  <thead>
                    <tr>
                      <th>Name</th>
                      <th>Command</th>
                      <th>Kind</th>
                      <th>Working directory</th>
                      <th />
                    </tr>
                  </thead>
                  <tbody>
                    {slots.map((slot) => (
                      <tr key={slot.id}>
                        <td><strong>{slot.name}</strong></td>
                        <td><span className="settings-chip mono">{slot.command || "Not configured"}</span></td>
                        <td><span className="settings-tag">{slot.kind}</span></td>
                        <td><span className="settings-chip mono">{resolveTerminalSlotCwd(project.path, slot)}</span></td>
                        <td>
                          <div className="settings-agent-actions">
                            <button type="button" className="settings-icon-control" title={`Edit ${slot.name}`} onClick={() => setSlotDraft(draftFromTerminalSlot(slot))}>
                              <Pencil size={14} />
                            </button>
                            <button type="button" className="settings-icon-control danger" title={`Delete ${slot.name}`} onClick={() => removeSlot(slot)}>
                              <Trash2 size={14} />
                            </button>
                          </div>
                        </td>
                      </tr>
                    ))}
                    {slotsLoaded && slots.length === 0 && (
                      <tr>
                        <td colSpan={5}><span className="settings-dim">No slots configured yet.</span></td>
                      </tr>
                    )}
                  </tbody>
                </table>
              </div>
              {renderSlotEditor()}
              {slotError && <div className="settings-inline-error">{slotError}</div>}
            </>
          )}
        </SettingCard>
        <SettingCard title="Execution guards">
          <Field
            title="Confirm dependency changes"
            description="Prompts before dependency installs. Destructive file/Git commands and production-like targets always require a one-time backend approval."
            control={
              <ToggleControl
                checked={appSettings.confirmBeforeCommands}
                onChange={(checked) => updateAppSettings({ confirmBeforeCommands: checked })}
              />
            }
          />
          <Field
            title="Command timeout"
            description="One-shot validation commands stop after this many seconds. Preview terminals run until stopped manually."
            control={
              <input
                className="settings-number-input"
                type="number"
                min={5}
                max={3600}
                value={appSettings.commandTimeoutSeconds}
                onChange={(event) => setAppSettings((current) => ({ ...current, commandTimeoutSeconds: Number(event.target.value) }))}
                onBlur={(event) => updateAppSettings({ commandTimeoutSeconds: Number(event.target.value) })}
              />
            }
          />
          <Field
            title="Redact secrets from logs"
            description="Command text and output are always passed through the backend redaction rules before persistence."
            control={<span className="settings-tag ok">Always on</span>}
          />
          {settingsError && <div className="settings-inline-error">{settingsError}</div>}
        </SettingCard>
      </>
    );
  }

  function renderAbout() {
    const resources = project
      ? [
          { label: "Requirements document", path: `${project.path}/docs/requirements.md` },
          { label: "Plans index", path: `${project.path}/docs/PLANS.md` },
        ]
      : [];

    return (
      <>
        <SettingCard title="Loom">
          <div className="settings-about-head">
            <div className="settings-bigmark">L</div>
            <div>
              <h2>{health?.app ?? "Loom"}</h2>
              <p>Multi-agent local development workbench</p>
              <div className="settings-tag-row">
                <span className="settings-chip mono">v{health?.version ?? "unknown"}</span>
                <span className="settings-chip mono">{health?.backend ?? "backend pending"}</span>
                <span className="settings-tag ok">{health?.status ?? "loading"}</span>
              </div>
            </div>
          </div>
        </SettingCard>
        <SettingCard title="Backend health">
          <Field title="Status" description="Reported by the Tauri health_check command." control={<span className="settings-tag ok">{health?.status ?? "Loading"}</span>} />
          <Field title="Last checked" control={<span className="settings-chip mono">{shortTimestamp(health?.timestampMs)}</span>} />
        </SettingCard>
        <SettingCard title="Resources">
          {resources.length === 0 ? (
            <div className="settings-empty-state">Open a project to reveal local docs from its workspace.</div>
          ) : resources.map((item) => (
            <button
              type="button"
              className="settings-link-row"
              disabled={!hasTauriRuntime()}
              onClick={() => openResource(item.path)}
              key={item.path}
            >
              <span>{item.label}</span>
              <ExternalLink size={13} />
            </button>
          ))}
        </SettingCard>
      </>
    );
  }

  const body = {
    general: renderGeneral,
    appearance: renderAppearance,
    agents: renderAgents,
    safety: renderSafety,
    about: renderAbout,
  }[activeTab]();

  return (
    <div className="settings-page">
      <div className="settings-topbar" data-tauri-drag-region>
        <div className="settings-topbar-spacer" data-tauri-drag-region />
      </div>

      <main className="settings-main">
        <div className="settings-columns">
          <nav className="settings-nav">
            <button type="button" className="settings-nav-back" aria-label="Back" onClick={onBack}>
              <span className="settings-nav-icon">
                <ArrowLeft size={15} />
              </span>
              Settings
            </button>
            <div className="settings-nav-items">
              {settingsTabs.map((tab) => (
                <button
                  type="button"
                  className={`settings-nav-item ${tab.id === activeTab ? "active" : ""}`}
                  onClick={() => setActiveTab(tab.id)}
                  key={tab.id}
                >
                  <span className="settings-nav-icon">{tab.icon}</span>
                  {tab.label}
                </button>
              ))}
            </div>
          </nav>

          <div className="settings-body">{body}</div>
        </div>
      </main>
    </div>
  );
}
