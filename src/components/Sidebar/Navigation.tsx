import { useEffect, useState, type FormEvent } from "react";
import { Bot, Folder, Plus } from "lucide-react";
import { useAgentBridge } from "../../hooks/useAgentBridge";
import { useProjectBridge } from "../../hooks/useProjectBridge";
import { useAppState } from "../../state/AppStateContext";
import "./Sidebar.css";

export function Navigation() {
  const { state, dispatch } = useAppState();
  const { loadAgents, createAgent } = useAgentBridge();
  const { loadRecentProjects, registerProject } = useProjectBridge();
  const [projectPath, setProjectPath] = useState("");
  const [agentCommand, setAgentCommand] = useState("");

  useEffect(() => {
    void loadRecentProjects();
    void loadAgents();
  }, [loadAgents, loadRecentProjects]);

  async function handleProjectSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const trimmedPath = projectPath.trim();

    if (!trimmedPath) {
      return;
    }

    const project = await registerProject(trimmedPath);

    if (project) {
      setProjectPath("");
    }
  }

  async function handleAgentSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const command = agentCommand.trim();

    if (!command) {
      return;
    }

    const agent = await createAgent({
      name: command,
      command,
      args: [],
      workingDirectoryPolicy: "project_root",
      capabilities: ["planning", "implementation", "review", "debugging", "testing"],
      adapterType: "cli",
      canWriteFiles: true,
      canRunCommands: true,
      enabled: true,
    });

    if (agent) {
      setAgentCommand("");
    }
  }

  return (
    <div className="sidebar-nav-container">
      <nav className="sidebar-nav-section">
        <h3 className="nav-heading">PROJECTS</h3>
        <form className="project-form" onSubmit={handleProjectSubmit}>
          <input
            className="project-path-input"
            value={projectPath}
            onChange={(event) => setProjectPath(event.target.value)}
            placeholder="/path/to/project"
            spellCheck={false}
          />
          <button
            type="submit"
            className="project-add-button"
            disabled={state.app.isLoadingProjects || projectPath.trim().length === 0}
            title="Add project"
          >
            <Plus size={14} />
          </button>
        </form>
        {state.app.projectError && <div className="nav-error">{state.app.projectError}</div>}
        <ul className="nav-list">
          {state.projects.recent.map((project) => {
            const active = project.id === state.app.activeProjectId;

            return (
              <li key={`${project.id}:${project.path}`}>
                <a
                  href={`#${project.id}`}
                  className={`nav-item${active ? " active" : ""}`}
                  onClick={() => dispatch({ type: "projects/selected", projectId: project.id })}
                >
                  <Folder size={16} className={`nav-icon${active ? " active-icon" : ""}`} />
                  <span>{project.name}</span>
                </a>
              </li>
            );
          })}
          {state.projects.recent.length === 0 && (
            <li className="nav-empty">No recent projects</li>
          )}
        </ul>
      </nav>

      <nav className="sidebar-nav-section">
        <h3 className="nav-heading">AGENTS</h3>
        <form className="project-form" onSubmit={handleAgentSubmit}>
          <input
            className="project-path-input"
            value={agentCommand}
            onChange={(event) => setAgentCommand(event.target.value)}
            placeholder="codex"
            spellCheck={false}
          />
          <button
            type="submit"
            className="project-add-button"
            disabled={state.app.isLoadingAgents || agentCommand.trim().length === 0}
            title="Add agent"
          >
            <Plus size={14} />
          </button>
        </form>
        {state.app.agentError && <div className="nav-error">{state.app.agentError}</div>}
        <ul className="nav-list">
          {state.agents.map((agent) => (
            <li key={agent.id}>
              <a href={`#${agent.id}`} className="nav-item">
                <Bot size={16} className="nav-icon" />
                <span>{agent.name}</span>
                <span className={`nav-status ${agent.available ? "status-ok" : "status-bad"}`}>
                  {agent.enabled ? (agent.available ? "ready" : "missing") : "off"}
                </span>
              </a>
            </li>
          ))}
          {state.agents.length === 0 && <li className="nav-empty">No agents configured</li>}
        </ul>
      </nav>
    </div>
  );
}
