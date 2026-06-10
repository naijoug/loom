import { type CSSProperties, useEffect, useState } from "react";
import { ChevronRight, Circle, Folder, FolderPlus, Plus, Search, Settings, Trash2 } from "lucide-react";
import type { TaskStatus } from "../../domain";
import { useProjectBridge } from "../../hooks/useProjectBridge";
import { useTaskBridge } from "../../hooks/useTaskBridge";
import { useAppState } from "../../state/AppStateContext";
import { Button } from "../common/Button";
import { AddProjectModal } from "./AddProjectModal";
import "./Sidebar.css";

interface TaskMenuState {
  taskId: string;
  x: number;
  y: number;
}

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
  const { deleteTask } = useTaskBridge();
  const [addProjectOpen, setAddProjectOpen] = useState(false);
  const [taskMenu, setTaskMenu] = useState<TaskMenuState | null>(null);
  const [confirmTaskId, setConfirmTaskId] = useState<string | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [collapsedIds, setCollapsedIds] = useState<Set<string>>(new Set());

  function toggleCollapsed(projectId: string) {
    setCollapsedIds((prev) => {
      const next = new Set(prev);
      if (next.has(projectId)) {
        next.delete(projectId);
      } else {
        next.add(projectId);
      }
      return next;
    });
  }

  function expandProject(projectId: string) {
    setCollapsedIds((prev) => {
      if (!prev.has(projectId)) {
        return prev;
      }
      const next = new Set(prev);
      next.delete(projectId);
      return next;
    });
  }

  useEffect(() => {
    void loadRecentProjects();
  }, [loadRecentProjects]);

  useEffect(() => {
    if (!taskMenu) {
      return;
    }

    const close = () => setTaskMenu(null);
    window.addEventListener("click", close);
    window.addEventListener("resize", close);
    return () => {
      window.removeEventListener("click", close);
      window.removeEventListener("resize", close);
    };
  }, [taskMenu]);

  const confirmTask = state.tasks.find((task) => task.id === confirmTaskId) ?? null;

  async function handleDeleteConfirmed() {
    if (!confirmTaskId) {
      return;
    }

    setDeleting(true);
    await deleteTask(state.projects.current?.path ?? null, confirmTaskId);
    setDeleting(false);
    setConfirmTaskId(null);
  }

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
            const hasTasks = projectTasks.length > 0;
            const expanded = active && hasTasks && !collapsedIds.has(project.id);

            return (
              <li className="project-nav-item" key={`${project.id}:${project.path}`}>
                <a
                  href={`#${project.id}`}
                  className={`nav-item${active ? " active" : ""}`}
                  onClick={(event) => {
                    event.preventDefault();
                    if (active) {
                      // Already selected: a second click toggles expand/collapse.
                      toggleCollapsed(project.id);
                    } else {
                      // Selecting a project always opens it expanded.
                      expandProject(project.id);
                      dispatch({ type: "projects/selected", projectId: project.id });
                    }
                  }}
                >
                  <ChevronRight
                    size={14}
                    className={`nav-disclosure${expanded ? " expanded" : ""}${
                      active && !hasTasks ? " hidden" : ""
                    }`}
                  />
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
                      if (project.id !== state.app.activeProjectId) {
                        dispatch({ type: "projects/selected", projectId: project.id });
                      }
                      dispatch({ type: "tasks/new" });
                    }}
                  >
                    <Plus size={13} />
                  </button>
                </a>
                {expanded && (
                  <div className="task-nav-collapse">
                    <ul className="task-nav-list">
                      {projectTasks.map((task, index) => {
                        const selected = task.id === state.app.selectedTaskId;

                        return (
                          <li key={task.id} style={{ "--task-index": index } as CSSProperties}>
                            <button
                              type="button"
                              className={`task-nav-item${selected ? " selected" : ""}`}
                              onClick={() => dispatch({ type: "tasks/selected", taskId: task.id })}
                              onContextMenu={(event) => {
                                event.preventDefault();
                                setTaskMenu({
                                  taskId: task.id,
                                  x: event.clientX,
                                  y: event.clientY,
                                });
                              }}
                            >
                              <Circle size={8} className={`task-dot task-${statusClass(task.status)}`} />
                              <span>{task.title}</span>
                            </button>
                          </li>
                        );
                      })}
                    </ul>
                  </div>
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

      {addProjectOpen && <AddProjectModal onClose={() => setAddProjectOpen(false)} />}

      {taskMenu && (
        <div
          className="task-context-menu"
          style={{ top: taskMenu.y, left: taskMenu.x }}
          role="menu"
          onClick={(event) => event.stopPropagation()}
        >
          <button
            type="button"
            className="task-context-item danger"
            role="menuitem"
            onClick={() => {
              setConfirmTaskId(taskMenu.taskId);
              setTaskMenu(null);
            }}
          >
            <Trash2 size={13} />
            Delete task
          </button>
        </div>
      )}

      {confirmTask && (
        <div className="confirm-backdrop" role="presentation">
          <div className="confirm-modal" role="dialog" aria-modal="true">
            <h2>Delete task?</h2>
            <p>
              “{confirmTask.title}” and its planning evidence will be permanently removed. This
              cannot be undone.
            </p>
            <div className="confirm-modal-actions">
              <Button variant="ghost" onClick={() => setConfirmTaskId(null)} disabled={deleting}>
                Cancel
              </Button>
              <Button variant="danger" onClick={() => void handleDeleteConfirmed()} disabled={deleting}>
                {deleting ? "Deleting…" : "Delete"}
              </Button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
