import { useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Folder, X } from "lucide-react";
import { useProjectBridge } from "../../hooks/useProjectBridge";
import { hasTauriRuntime } from "../../hooks/runtime";
import { useAppState } from "../../state/AppStateContext";
import { Button } from "../common/Button";
import "./Sidebar.css";

interface AddProjectModalProps {
  onClose: () => void;
}

function displayPath(path: string) {
  return path.replace(/^\/Users\/[^/]+/, "~");
}

export function AddProjectModal({ onClose }: AddProjectModalProps) {
  const { state, dispatch } = useAppState();
  const { registerProject } = useProjectBridge();
  const [selectedPath, setSelectedPath] = useState(
    state.projects.current?.path ?? state.projects.recent[0]?.path ?? "",
  );

  const detectedProject = useMemo(
    () =>
      state.projects.recent.find((project) => project.path === selectedPath) ??
      state.projects.current ??
      state.projects.recent[0] ??
      null,
    [selectedPath, state.projects.current, state.projects.recent],
  );
  const detectedStacks = detectedProject?.detectedStacks.length
    ? detectedProject.detectedStacks
    : ["Vite + React", "Tauri", "Rust"];
  const recentProjects = state.projects.recent.slice(0, 3);

  async function handleBrowse() {
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

    if (typeof selected === "string") {
      setSelectedPath(selected);
    }
  }

  async function handleAddProject() {
    const trimmedPath = selectedPath.trim();
    if (!trimmedPath) {
      return;
    }

    const project = await registerProject(trimmedPath);
    if (project) {
      onClose();
    }
  }

  return (
    <div className="project-modal-backdrop" role="presentation">
      <div className="project-modal" role="dialog" aria-modal="true" aria-labelledby="add-project-title">
        <div className="project-modal-header">
          <h2 id="add-project-title">Add project</h2>
          <button type="button" className="project-modal-close" aria-label="Close" onClick={onClose}>
            <X size={16} />
          </button>
        </div>

        <div className="project-modal-body">
          <section className="project-modal-section">
            <div className="project-modal-label">Open a local directory</div>
            <div className="project-path-row">
              <div className={`project-path-field ${selectedPath ? "" : "empty"}`}>
                {selectedPath ? displayPath(selectedPath) : "~/Workspace/naijoug/project"}
              </div>
              <Button type="button" variant="ghost" onClick={handleBrowse}>
                Browse...
              </Button>
            </div>
          </section>

          <section className="project-modal-section">
            <div className="project-modal-label">Recent</div>
            <div className="project-recent-list">
              {recentProjects.map((project) => (
                <button
                  type="button"
                  className={`project-recent-row ${project.path === selectedPath ? "active" : ""}`}
                  key={`${project.id}:${project.path}`}
                  onClick={() => setSelectedPath(project.path)}
                >
                  <Folder size={14} />
                  <span>{displayPath(project.path)}</span>
                </button>
              ))}
              {recentProjects.length === 0 && (
                <div className="project-recent-empty">No recent projects yet</div>
              )}
            </div>
          </section>

          <section className="project-detected-card">
            <div className="project-detected-head">
              <span>Detected</span>
              <span className="project-detected-status">analyzed</span>
            </div>
            <div className="project-stack-row">
              {detectedStacks.slice(0, 4).map((stack) => (
                <span className="project-stack-chip" key={stack}>
                  {stack}
                </span>
              ))}
            </div>
            <div className="project-meta-line">
              <span>main</span>
              <span>{detectedProject?.isGitRepository ? "git repository" : "local directory"}</span>
              <span>{detectedProject?.suggestedCommands[0] ?? "pnpm dev"}</span>
            </div>
          </section>

          {state.app.projectError && <div className="project-modal-error">{state.app.projectError}</div>}
        </div>

        <div className="project-modal-actions">
          <Button type="button" variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button
            type="button"
            variant="primary"
            disabled={!selectedPath.trim() || state.app.isLoadingProjects}
            onClick={handleAddProject}
          >
            Add project
          </Button>
        </div>
      </div>
    </div>
  );
}
