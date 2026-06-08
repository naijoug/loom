import { useEffect, useState } from "react";
import { Circle, Folder, FolderPlus, Plus, Search, Settings } from "lucide-react";
import type { ProjectSummary, TaskStatus } from "../../domain";
import { useProjectBridge } from "../../hooks/useProjectBridge";
import { useAppState } from "../../state/AppStateContext";
import { NewTaskModal } from "../Board";
import { AddProjectModal } from "./AddProjectModal";
import "./Sidebar.css";

function statusClass(status: TaskStatus) {
  if (status === "completed") {
    return "done";
  }
  if (status === "blocked" || status === "cancelled") {
    return "blocked";
  }
  if (status === "debugging" || status === "fixing" || status === "verifying") {
    return "testing";
  }
  if (status === "implementing" || status === "reviewing") {
    return "running";
  }
  return "todo";
}

export function Navigation() {
  const { state, dispatch } = useAppState();
  const { loadRecentProjects } = useProjectBridge();
  const [newTaskProject, setNewTaskProject] = useState<ProjectSummary | null>(null);
  const [addProjectOpen, setAddProjectOpen] = useState(false);

  useEffect(() => {
    void loadRecentProjects();
  }, [loadRecentProjects]);

  return (
    <div className="sidebar-nav-container">
      <nav className="sidebar-nav-section">
        <div className="sidebar-search">
          <Search size={14} />
          <span>Search tasks...</span>
        </div>
        <button
          type="button"
          className="project-add-button"
          disabled={state.app.isLoadingProjects}
          title="Add project"
          onClick={() => setAddProjectOpen(true)}
        >
          <FolderPlus size={15} />
          <span>Add project</span>
        </button>
        {state.app.projectError && <div className="nav-error">{state.app.projectError}</div>}
        <ul className="nav-list">
          {state.projects.recent.map((project) => {
            const active = project.id === state.app.activeProjectId;
            const projectTasks = active ? state.tasks : [];

            return (
              <li className="project-nav-item" key={`${project.id}:${project.path}`}>
                <a
                  href={`#${project.id}`}
                  className={`nav-item${active ? " active" : ""}`}
                  onClick={(event) => {
                    event.preventDefault();
                    dispatch({ type: "projects/selected", projectId: project.id });
                  }}
                >
                  <Folder size={16} className={`nav-icon${active ? " active-icon" : ""}`} />
                  <span>{project.name}</span>
                  <span className="project-task-count">{active ? projectTasks.length : ""}</span>
                  <button
                    type="button"
                    className="project-inline-add"
                    aria-label={`New task in ${project.name}`}
                    title={`New task in ${project.name}`}
                    onClick={(event) => {
                      event.preventDefault();
                      event.stopPropagation();
                      dispatch({ type: "projects/selected", projectId: project.id });
                      setNewTaskProject(project);
                    }}
                  >
                    <Plus size={13} />
                  </button>
                </a>
                {active && projectTasks.length > 0 && (
                  <ul className="task-nav-list">
                    {projectTasks.map((task) => {
                      const selected = task.id === state.app.selectedTaskId;

                      return (
                        <li key={task.id}>
                          <button
                            type="button"
                            className={`task-nav-item${selected ? " selected" : ""}`}
                            onClick={() => dispatch({ type: "tasks/selected", taskId: task.id })}
                          >
                            <Circle size={8} className={`task-dot task-${statusClass(task.status)}`} />
                            <span>{task.title}</span>
                          </button>
                        </li>
                      );
                    })}
                  </ul>
                )}
              </li>
            );
          })}
          {state.projects.recent.length === 0 && (
            <li className="nav-empty">No recent projects</li>
          )}
        </ul>
      </nav>

      <div className="sidebar-footer">
        <button
          type="button"
          className="sidebar-settings-entry"
          onClick={() => dispatch({ type: "app/viewSelected", view: "settings" })}
        >
          <Settings size={15} />
          <span>Settings</span>
        </button>
      </div>

      {newTaskProject && (
        <NewTaskModal project={newTaskProject} onClose={() => setNewTaskProject(null)} />
      )}

      {addProjectOpen && <AddProjectModal onClose={() => setAddProjectOpen(false)} />}
    </div>
  );
}
