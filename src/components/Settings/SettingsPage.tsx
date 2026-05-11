import { useEffect } from "react";
import { ArrowLeft, Bot, Braces, CheckCircle2, Moon, Sun, Terminal, XCircle } from "lucide-react";
import { useTheme } from "../../contexts/ThemeContext";
import type { AgentConfig } from "../../domain";
import { useAgentBridge } from "../../hooks/useAgentBridge";
import { useAppState } from "../../state/AppStateContext";
import "./SettingsPage.css";

interface SettingsPageProps {
  onBack: () => void;
}

const cliAdapterTypes = new Set(["codex_cli", "claude_code_cli", "amp_cli", "dummy"]);

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
  const { loadAgents, setAgentEnabled } = useAgentBridge();

  useEffect(() => {
    void loadAgents();
  }, [loadAgents]);

  const cliAgents = state.agents.filter((agent) => cliAdapterTypes.has(agent.adapterType));

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
                    <button
                      type="button"
                      className="settings-text-control"
                      disabled={!agent.available}
                      onClick={() => void setAgentEnabled(agent.id, !agent.enabled)}
                    >
                      {agent.enabled ? "Disable" : "Enable"}
                    </button>
                  </div>
                </div>
              ))}
              {state.app.agentError && (
                <div className="settings-card-row">
                  <div className="settings-inline-error">{state.app.agentError}</div>
                </div>
              )}
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
