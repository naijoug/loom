import { useEffect, useMemo, useState, type FormEvent, type ReactNode } from "react";
import {
  ArrowLeft,
  Bell,
  Bot,
  CheckCircle2,
  Info,
  Palette,
  Pencil,
  Plus,
  ShieldCheck,
  SlidersHorizontal,
  Trash2,
  XCircle,
} from "lucide-react";
import { useTheme } from "../../contexts/ThemeContext";
import type { AgentAdapterType, AgentCapability, AgentConfig, AgentConfigInput } from "../../domain";
import { useAgentBridge } from "../../hooks/useAgentBridge";
import { useAppState } from "../../state/AppStateContext";
import { Button } from "../common/Button";
import "./SettingsPage.css";

interface SettingsPageProps {
  onBack: () => void;
  initialTab?: SettingsTab;
}

type SettingsTab = "general" | "appearance" | "agents" | "safety" | "notifications" | "about";

const cliAdapterTypes = new Set(["codex_cli", "claude_code_cli", "amp_cli", "dummy", "cli"]);
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
  { value: "amp_cli", label: "Amp CLI", command: "amp", name: "Amp" },
  { value: "cli", label: "Custom CLI", command: "", name: "Custom Agent" },
  { value: "dummy", label: "Dummy/Test", command: "dummy", name: "Dummy Agent" },
];
const settingsTabs: Array<{ id: SettingsTab; label: string; icon: ReactNode }> = [
  { id: "general", label: "General", icon: <SlidersHorizontal size={15} /> },
  { id: "appearance", label: "Appearance", icon: <Palette size={15} /> },
  { id: "agents", label: "Agents", icon: <Bot size={15} /> },
  { id: "safety", label: "Commands & Safety", icon: <ShieldCheck size={15} /> },
  { id: "notifications", label: "Notifications", icon: <Bell size={15} /> },
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
  return ["agent-codex", "agent-claude", "agent-amp", "agent-dummy"].includes(agent.id);
}

function profileSummary(agent: AgentConfig) {
  switch (agent.adapterType) {
    case "codex_cli":
      return "codex exec --cd {projectPath} --sandbox read-only -";
    case "claude_code_cli":
      return "claude -p --permission-mode plan --output-format text";
    case "amp_cli":
      return "amp -x";
    case "dummy":
      return "test fixture only";
    default:
      return agent.args.length > 0 ? `${agent.command} ${agent.args.join(" ")}` : agent.command;
  }
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

function SelectPill({ value }: { value: string }) {
  return (
    <span className="settings-select-pill">
      {value}
      <span>▾</span>
    </span>
  );
}

function Segmented({ values, active }: { values: string[]; active: number }) {
  return (
    <span className="settings-segmented">
      {values.map((value, index) => (
        <b className={index === active ? "on" : ""} key={value}>
          {value}
        </b>
      ))}
    </span>
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
  onClick?: () => void;
}) {
  return (
    <button
      type="button"
      className={`theme-card ${active ? "on" : ""}`}
      onClick={onClick}
      disabled={!onClick}
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

function policyControl(value: "Ask" | "Allow" | "Block") {
  return (
    <Segmented values={["Ask", "Allow", "Block"]} active={["Ask", "Allow", "Block"].indexOf(value)} />
  );
}

export function SettingsPage({ onBack, initialTab = "general" }: SettingsPageProps) {
  const { theme, toggleTheme } = useTheme();
  const { state } = useAppState();
  const { createAgent, deleteAgent, loadAgents, setAgentEnabled, updateAgent } = useAgentBridge();
  const [activeTab, setActiveTab] = useState<SettingsTab>(initialTab);
  const [agentDraft, setAgentDraft] = useState<AgentConfigInput>(defaultAgentDraft);
  const [argsText, setArgsText] = useState("");
  const [editingAgentId, setEditingAgentId] = useState<string | null>(null);

  useEffect(() => {
    void loadAgents();
  }, [loadAgents]);

  const cliAgents = state.agents.filter((agent) => cliAdapterTypes.has(agent.adapterType));
  const selectedAdapter = adapterOptions.find((option) => option.value === agentDraft.adapterType);
  const editingAgent = editingAgentId
    ? cliAgents.find((agent) => agent.id === editingAgentId) ?? null
    : null;
  const suggestedCommands = useMemo(() => {
    const commands = state.projects.current?.suggestedCommands ?? [];
    return commands.length > 0
      ? commands
      : ["pnpm install", "pnpm dev", "pnpm exec tsc --noEmit", "pnpm build"];
  }, [state.projects.current?.suggestedCommands]);
  const availableAgents = cliAgents.filter((agent) => agent.available);
  const phaseCoverage = new Set(cliAgents.flatMap((agent) => agent.capabilities));

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

  function renderGeneral() {
    return (
      <>
        <div className="setsec-title">General</div>
        <SettingCard title="Workspace">
          <Field title="Workspace name" control={<span className="settings-input-pill">Loom</span>} />
          <Field
            title="Default project location"
            description="New projects open from here."
            control={
              <span className="settings-row-control">
                <span className="settings-chip mono">~/Workspace</span>
                <button className="settings-small-btn" type="button">
                  Change
                </button>
              </span>
            }
          />
        </SettingCard>
        <SettingCard title="Startup">
          <Field title="Restore last session on launch" description="Reopen projects, tasks and panels." control={<Switch on />} />
          <Field title="Reopen last active task" control={<Switch on />} />
          <Field title="Launch Loom at login" control={<Switch on={false} />} />
        </SettingCard>
        <SettingCard title="Behavior">
          <Field title="Max parallel agents" description="How many agent sessions can run at once." control={<Segmented values={["1", "2", "3"]} active={2} />} />
          <Field title="Confirm before running commands" control={<Switch on />} />
          <Field title="Language" control={<SelectPill value="English (US)" />} />
        </SettingCard>
        <SettingCard title="Privacy & data">
          <Field title="Share anonymous usage data" control={<Switch on={false} />} />
          <Field
            title="Local cache"
            description="Logs, analysis and session history · 248 MB"
            control={<button className="settings-small-btn" type="button">Clear cache</button>}
          />
        </SettingCard>
      </>
    );
  }

  function renderAppearance() {
    return (
      <>
        <div className="setsec-title">Appearance</div>
        <SettingCard title="Theme">
          <div className="theme-grid">
            <ThemePreview label="Light" mode="light" active={theme === "light"} onClick={theme === "dark" ? toggleTheme : undefined} />
            <ThemePreview label="Dark" mode="dark" active={theme === "dark"} onClick={theme === "light" ? toggleTheme : undefined} />
            <ThemePreview label="System" mode="system" active={false} />
          </div>
        </SettingCard>
        <SettingCard title="Accent color">
          <Field
            title="Highlight & primary action color"
            description="Active state, links and primary buttons."
            control={
              <div className="settings-swatches">
                {["#3485D1", "#0D9488", "#7C5CFC", "#2E9E5B", "#D97706"].map((color, index) => (
                  <span className={`settings-swatch ${index === 0 ? "on" : ""}`} style={{ background: color }} key={color} />
                ))}
              </div>
            }
          />
        </SettingCard>
        <SettingCard title="Typography">
          <Field title="Interface font" description="Navigation, labels and body text." control={<SelectPill value="System (SF Pro)" />} />
          <Field title="Monospace font" description="Logs, commands, paths and code." control={<SelectPill value="JetBrains Mono" />} />
          <Field
            title="Font size"
            description="Base UI text size."
            control={
              <span className="settings-row-control">
                <Segmented values={["Small", "Default", "Large"]} active={1} />
                <span className="settings-chip mono">14px</span>
              </span>
            }
          />
          <Field title="Density" description="Row height and padding for dense work." control={<Segmented values={["Compact", "Cozy"]} active={0} />} />
          <div className="settings-font-preview">
            The quick brown fox jumps. <span>$ pnpm tauri dev -- log search 100k+ lines</span>
          </div>
        </SettingCard>
      </>
    );
  }

  function renderAgents() {
    return (
      <>
        <div className="settings-page-heading">
          <div>
            <div className="setsec-title">Agents</div>
            <div className="settings-sub">Local AI coding agents available to orchestrate</div>
          </div>
          <Button type="button" variant="primary" iconLeft={<Plus size={14} />} onClick={() => setEditingAgentId(null)}>
            Add Agent
          </Button>
        </div>
        <div className="settings-tiles">
          <div className="settings-tile"><span>Total</span><strong>{cliAgents.length}</strong></div>
          <div className="settings-tile"><span>Available</span><strong className="ok">{availableAgents.length}</strong></div>
          <div className="settings-tile"><span>Unreachable</span><strong className="err">{cliAgents.length - availableAgents.length}</strong></div>
          <div className="settings-tile"><span>Coverage</span><strong>{Math.min(phaseCoverage.size, 4)}/4</strong></div>
        </div>
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
                  <th>Default</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {cliAgents.map((agent) => (
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
                      <span className={`settings-pill ${agent.available ? "ok" : "err"}`}>
                        {agent.available ? <CheckCircle2 size={13} /> : <XCircle size={13} />}
                        {agent.available ? "Available" : "Missing"}
                      </span>
                    </td>
                    <td>{agent.capabilities.includes("implementation") ? <span className="settings-tag primary">IMPLEMENT</span> : <span className="settings-dim">None</span>}</td>
                    <td>
                      <div className="settings-agent-actions">
                        <button type="button" className="settings-icon-control" title={`Edit ${agent.name}`} onClick={() => handleEditAgent(agent)}>
                          <Pencil size={14} />
                        </button>
                        <button
                          type="button"
                          className="settings-text-control"
                          disabled={!agent.available}
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
                ))}
              </tbody>
            </table>
          </div>
          <form className="settings-agent-form" onSubmit={handleCreateAgent}>
            <div className="settings-form-header">
              <div>
                <div className="settings-row-title">{editingAgent ? `Edit ${editingAgent.name}` : "Add custom Agent"}</div>
                <div className="settings-row-desc">Store reusable CLI profiles behind the adapter interface.</div>
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

  function renderSafety() {
    return (
      <>
        <div className="setsec-title">Commands & Safety</div>
        <SettingCard title="Command presets (defaults)">
          <div className="settings-table-wrap">
            <table className="settings-agent-table settings-command-table">
              <tbody>
                {suggestedCommands.map((command, index) => (
                  <tr key={`${command}-${index}`}>
                    <td>{index === 0 ? "Primary" : `Preset ${index + 1}`}</td>
                    <td><span className="settings-chip mono">{command}</span></td>
                    <td><span className="settings-dim">From project analysis / MVP defaults</span></td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </SettingCard>
        <SettingCard title="High-risk action policy">
          <Field title="Delete files" control={policyControl("Ask")} />
          <Field title="Reset git" control={policyControl("Block")} />
          <Field title="Install dependencies" control={policyControl("Ask")} />
          <Field title="Network & production access" control={policyControl("Block")} />
        </SettingCard>
        <SettingCard title="Execution guards">
          <Field title="Command timeout" description="Kill long-running commands after..." control={<SelectPill value="10 min" />} />
          <Field
            title="Allowed working directories"
            control={
              <span className="settings-row-control wrap">
                <span className="settings-chip mono">project root</span>
                <span className="settings-chip mono">~/.loom/tmp</span>
                <span className="settings-chip dashed">+ Add</span>
              </span>
            }
          />
          <Field title="Redact secrets from logs" description="Mask tokens, keys and env values." control={<Switch on />} />
        </SettingCard>
      </>
    );
  }

  function renderNotifications() {
    return (
      <>
        <div className="setsec-title">Notifications</div>
        <SettingCard title="Notify me when">
          <Field title="Task completed" control={<Switch on />} />
          <Field title="Task blocked or errored" control={<Switch on />} />
          <Field title="Review needed" control={<Switch on />} />
          <Field title="Test failed" control={<Switch on />} />
          <Field title="Agent is waiting for input" control={<Switch on />} />
        </SettingCard>
        <SettingCard title="Delivery">
          <Field title="Desktop notifications" control={<Switch on />} />
          <Field title="Play sound" control={<Switch on />} />
          <Field
            title="Quiet hours"
            description="Silence notifications overnight."
            control={<span className="settings-row-control"><Switch on={false} /><SelectPill value="22:00 - 08:00" /></span>}
          />
        </SettingCard>
      </>
    );
  }

  function renderAbout() {
    return (
      <>
        <div className="setsec-title">About</div>
        <SettingCard title="Loom">
          <div className="settings-about-head">
            <div className="settings-bigmark">L</div>
            <div>
              <h2>Loom</h2>
              <p>Multi-agent local development workbench</p>
              <div className="settings-tag-row">
                <span className="settings-chip mono">v0.4.0</span>
                <span className="settings-chip mono">build 2026.06.05</span>
                <span className="settings-tag ok">Up to date</span>
              </div>
            </div>
            <button className="settings-small-btn" type="button">Check for updates</button>
          </div>
        </SettingCard>
        <SettingCard title="Update channel">
          <Field title="Channel" control={<Segmented values={["Stable", "Beta"]} active={0} />} />
          <Field title="Auto-install updates" control={<Switch on />} />
        </SettingCard>
        <SettingCard title="Resources">
          {["Documentation", "Changelog", "GitHub repository", "Report an issue", "License (MIT)"].map((item) => (
            <div className="settings-link-row" key={item}>
              <span>{item}</span>
              <span>↗</span>
            </div>
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
    notifications: renderNotifications,
    about: renderAbout,
  }[activeTab]();

  return (
    <div className="settings-page">
      <div className="settings-topbar">
        <button type="button" className="settings-back-link" aria-label="Back" title="Back" onClick={onBack}>
          <ArrowLeft size={14} />
        </button>
        <div className="settings-crumb">Settings</div>
        <div className="settings-topbar-spacer" />
        <span className="settings-segmented">
          <b className={theme === "light" ? "on" : ""} onClick={theme === "dark" ? toggleTheme : undefined}>Light</b>
          <b className={theme === "dark" ? "on" : ""} onClick={theme === "light" ? toggleTheme : undefined}>Dark</b>
        </span>
      </div>

      <main className="settings-main">
        <div className="settings-columns">
          <nav className="settings-nav">
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
          </nav>

          <div className="settings-body">{body}</div>
        </div>
      </main>
    </div>
  );
}
