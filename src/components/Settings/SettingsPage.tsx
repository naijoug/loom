import { useEffect, useState, type FormEvent } from "react";
import {
  ArrowLeft,
  Bot,
  Braces,
  CheckCircle2,
  Moon,
  Pencil,
  Plus,
  Sun,
  Terminal,
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
}

const cliAdapterTypes = new Set(["codex_cli", "claude_code_cli", "amp_cli", "dummy"]);
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

export function SettingsPage({ onBack }: SettingsPageProps) {
  const { theme, toggleTheme } = useTheme();
  const { state } = useAppState();
  const { createAgent, deleteAgent, loadAgents, setAgentEnabled, updateAgent } = useAgentBridge();
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
  }

  function handleCancelEdit() {
    setEditingAgentId(null);
    setAgentDraft(defaultAgentDraft());
    setArgsText("");
  }

  return (
    <div className="settings-page">
      <aside className="settings-sidebar">
        <button
          type="button"
          className="settings-back-link"
          aria-label="Back"
          title="Back"
          onClick={onBack}
        >
          <ArrowLeft size={14} />
        </button>

        <nav className="settings-nav">
          <div className="settings-nav-item active">
            {theme === "dark" ? <Moon size={15} /> : <Sun size={15} />}
            Appearance
          </div>
          <div className="settings-nav-item">
            <Bot size={15} />
            Agents
          </div>
        </nav>
      </aside>

      <main className="settings-main">
        <div className="settings-content">
          <h1 className="settings-title">Appearance</h1>

          <section className="settings-section">
            <h2 className="settings-section-title">Theme</h2>
            <div className="settings-card">
              <div className="settings-card-row split">
                <div className="settings-row-copy">
                  <div className="settings-row-title">Color mode</div>
                  <div className="settings-row-desc">Switch between light and dark UI.</div>
                </div>
                <button
                  type="button"
                  className="settings-icon-control"
                  aria-label={`Switch to ${theme === "dark" ? "light" : "dark"} theme`}
                  title={`Switch to ${theme === "dark" ? "light" : "dark"} theme`}
                  onClick={toggleTheme}
                >
                  {theme === "dark" ? <Moon size={16} /> : <Sun size={16} />}
                </button>
              </div>
            </div>
          </section>

          <section className="settings-section">
            <h2 className="settings-section-title">Agent CLI profiles</h2>
            <div className="settings-card">
              {cliAgents.map((agent) => (
                <div className="settings-card-row split" key={agent.id}>
                  <div className="settings-agent-profile">
                    <Terminal size={15} className="settings-row-icon" />
                    <div className="settings-row-copy">
                      <div className="settings-row-title">{agent.name}</div>
                      <div className="settings-row-desc">
                        Adapter <code>{agent.adapterType}</code>, command <code>{agent.command}</code>,
                        profile <code>{profileSummary(agent)}</code>.
                      </div>
                    </div>
                  </div>
                  <div className="settings-agent-actions">
                    <span
                      className={`settings-status-pill ${agent.available ? "available" : "missing"}`}
                    >
                      {agent.available ? <CheckCircle2 size={13} /> : <XCircle size={13} />}
                      {agent.available ? "Available" : "Missing"}
                    </span>
                    {isBuiltInAgent(agent) && (
                      <span className="settings-status-pill">Built-in</span>
                    )}
                    <button
                      type="button"
                      className="settings-text-control"
                      disabled={!agent.available}
                      onClick={() => void setAgentEnabled(agent.id, !agent.enabled)}
                    >
                      {agent.enabled ? "Disable" : "Enable"}
                    </button>
                    {!isBuiltInAgent(agent) && (
                      <>
                        <button
                          type="button"
                          className="settings-icon-control"
                          aria-label={`Edit ${agent.name}`}
                          title={`Edit ${agent.name}`}
                          onClick={() => handleEditAgent(agent)}
                        >
                          <Pencil size={14} />
                        </button>
                        <button
                          type="button"
                          className="settings-icon-control danger"
                          aria-label={`Delete ${agent.name}`}
                          title={`Delete ${agent.name}`}
                          onClick={() => {
                            if (window.confirm(`Delete Agent "${agent.name}"?`)) {
                              void deleteAgent(agent.id);
                              if (editingAgentId === agent.id) {
                                handleCancelEdit();
                              }
                            }
                          }}
                        >
                          <Trash2 size={14} />
                        </button>
                      </>
                    )}
                  </div>
                </div>
              ))}
              {state.app.agentError && (
                <div className="settings-card-row">
                  <div className="settings-inline-error">{state.app.agentError}</div>
                </div>
              )}
              <form className="settings-agent-form" onSubmit={handleCreateAgent}>
                <div className="settings-form-header">
                  <div>
                    <div className="settings-row-title">
                      {editingAgent ? `Edit ${editingAgent.name}` : "Add another Agent"}
                    </div>
                    <div className="settings-row-desc">
                      {editingAgent
                        ? "Update a custom CLI profile. Built-in Agent profiles stay fixed and can be disabled instead."
                        : "Add multiple CLI profiles, then use them together in planning or select one for a todo handoff."}
                    </div>
                  </div>
                  <div className="settings-form-actions">
                    {editingAgent && (
                      <Button type="button" variant="ghost" onClick={handleCancelEdit}>
                        Cancel
                      </Button>
                    )}
                    <Button
                      type="submit"
                      variant="primary"
                      iconLeft={<Plus size={14} />}
                      disabled={
                        state.app.isLoadingAgents ||
                        !agentDraft.name.trim() ||
                        !agentDraft.command.trim() ||
                        agentDraft.capabilities.length === 0
                      }
                    >
                      {editingAgent ? "Save Agent" : "Add Agent"}
                    </Button>
                  </div>
                </div>
                <div className="settings-form-grid">
                  <label className="settings-field">
                    <span>Name</span>
                    <input
                      value={agentDraft.name}
                      onChange={(event) =>
                        setAgentDraft((current) => ({ ...current, name: event.target.value }))
                      }
                      placeholder="Planner Codex"
                    />
                  </label>
                  <label className="settings-field">
                    <span>Adapter</span>
                    <select
                      value={agentDraft.adapterType}
                      onChange={(event) => handleAdapterChange(event.target.value as AgentAdapterType)}
                    >
                      {adapterOptions.map((option) => (
                        <option key={option.value} value={option.value}>
                          {option.label}
                        </option>
                      ))}
                    </select>
                  </label>
                  <label className="settings-field">
                    <span>Command</span>
                    <input
                      value={agentDraft.command}
                      onChange={(event) =>
                        setAgentDraft((current) => ({ ...current, command: event.target.value }))
                      }
                      placeholder="codex"
                    />
                  </label>
                  <label className="settings-field settings-field-wide">
                    <span>Args, one per line</span>
                    <textarea
                      value={argsText}
                      onChange={(event) => setArgsText(event.target.value)}
                      placeholder={
                        "Leave blank for the adapter default.\nPlanning supports {promptFile}; todo execution supports {prompt} and {projectPath}."
                      }
                    />
                  </label>
                </div>
                <div className="settings-check-row">
                  {capabilityOptions.map((capability) => (
                    <label className="settings-check" key={capability}>
                      <input
                        type="checkbox"
                        checked={agentDraft.capabilities.includes(capability)}
                        onChange={() => toggleCapability(capability)}
                      />
                      <span>{capability}</span>
                    </label>
                  ))}
                </div>
                <div className="settings-check-row">
                  <label className="settings-check">
                    <input
                      type="checkbox"
                      checked={agentDraft.canWriteFiles}
                      disabled={agentDraft.adapterType === "dummy"}
                      onChange={(event) =>
                        setAgentDraft((current) => ({
                          ...current,
                          canWriteFiles: event.target.checked,
                        }))
                      }
                    />
                    <span>Can write files</span>
                  </label>
                  <label className="settings-check">
                    <input
                      type="checkbox"
                      checked={agentDraft.canRunCommands}
                      disabled={agentDraft.adapterType === "dummy"}
                      onChange={(event) =>
                        setAgentDraft((current) => ({
                          ...current,
                          canRunCommands: event.target.checked,
                        }))
                      }
                    />
                    <span>Can run commands</span>
                  </label>
                  <label className="settings-check">
                    <input
                      type="checkbox"
                      checked={agentDraft.enabled}
                      onChange={(event) =>
                        setAgentDraft((current) => ({ ...current, enabled: event.target.checked }))
                      }
                    />
                    <span>Enable immediately</span>
                  </label>
                </div>
              </form>
              <div className="settings-card-row">
                <Terminal size={15} className="settings-row-icon" />
                <div className="settings-row-copy">
                  <div className="settings-row-title">Codex CLI</div>
                  <div className="settings-row-desc">
                    Adapter <code>codex_cli</code>, command <code>codex</code>, default planning
                    call <code>codex exec --cd {"{projectPath}"} --sandbox read-only -</code>.
                  </div>
                </div>
              </div>
              <div className="settings-card-row">
                <Terminal size={15} className="settings-row-icon" />
                <div className="settings-row-copy">
                  <div className="settings-row-title">Claude Code CLI</div>
                  <div className="settings-row-desc">
                    Adapter <code>claude_code_cli</code>, command <code>claude</code>, default
                    planning call <code>claude -p --permission-mode plan --output-format text</code>.
                  </div>
                </div>
              </div>
              <div className="settings-card-row">
                <Terminal size={15} className="settings-row-icon" />
                <div className="settings-row-copy">
                  <div className="settings-row-title">Amp CLI</div>
                  <div className="settings-row-desc">
                    Adapter <code>amp_cli</code>, command <code>amp</code>, default planning call{" "}
                    <code>amp -x</code>.
                  </div>
                </div>
              </div>
              <div className="settings-card-row">
                <Braces size={15} className="settings-row-icon" />
                <div className="settings-row-copy">
                  <div className="settings-row-title">Argument placeholders</div>
                  <div className="settings-row-desc">
                    Custom args can use <code>{"{promptFile}"}</code> for the generated planning
                    prompt file and <code>{"{projectPath}"}</code> for the selected project root.
                  </div>
                </div>
              </div>
            </div>
          </section>
        </div>
      </main>
    </div>
  );
}
