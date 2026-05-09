import { useEffect } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Folder, FolderPlus } from "lucide-react";
import { useProjectBridge } from "../../hooks/useProjectBridge";
import { hasTauriRuntime } from "../../hooks/runtime";
import { useAppState } from "../../state/AppStateContext";
import "./Sidebar.css";

export function Navigation() {
  const { state, dispatch } = useAppState();
  const { loadRecentProjects, registerProject } = useProjectBridge();

  useEffect(() => {
    void loadRecentProjects();
  }, [loadRecentProjects]);

  async function handleAddProject() {
    if (!hasTauriRuntime()) {
      dispatch({
        type: "projects/loadFailed",
        error: "Project selection requires the Tauri desktop runtime.",
      });
      return;
    }

    const selected = await open({
      directory: true,
      multiple: false,
      title: "Select a project folder",
    });

    if (typeof selected !== "string") {
      return;
    }

    await registerProject(selected);
  }

  return (
    <div className="sidebar-nav-container">
      <nav className="sidebar-nav-section">
        <div className="nav-heading-row">
          <h3 className="nav-heading">PROJECTS</h3>
          <button
            type="button"
            className="project-add-button"
            disabled={state.app.isLoadingProjects}
            title="Add project"
            onClick={handleAddProject}
          >
            <FolderPlus size={16} />
          </button>
        </div>
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
    </div>
  );
}
