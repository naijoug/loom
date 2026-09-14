import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { type CSSProperties, type MouseEvent, useEffect, useState } from "react";
import {
  ChevronRight,
  Circle,
  Folder,
  FolderOpen,
  FolderPlus,
  LayoutGrid,
  MessageSquare,
  Plus,
  Search,
  Settings,
  Trash2,
  X,
} from "lucide-react";
import type { ProjectSummary, TaskStatus } from "../../domain";
import { useProjectBridge } from "../../hooks/useProjectBridge";
import { hasTauriRuntime } from "../../hooks/runtime";
import { useTaskBridge } from "../../hooks/useTaskBridge";
import { useAppState } from "../../state/AppStateContext";
import { Button } from "../common/Button";
import { AddProjectModal } from "./AddProjectModal";
import "./Sidebar.css";

interface TaskMenuState {
  taskId: string;
  projectPath: string;
  x: number;
  y: number;
}

interface ProjectMenuState {
  projectId: string;
  x: number;
  y: number;
}

type ProjectMenuAction = "reveal" | "remove";

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

function toErrorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function contextMenuPosition(event: MouseEvent) {
  return {
    x: Math.max(8, Math.min(event.clientX, window.innerWidth - 236)),
    y: Math.max(8, Math.min(event.clientY, window.innerHeight - 228)),
  };
}

export function Navigation() {
  const { state, dispatch } = useAppState();
  const { loadRecentProjects, removeRecentProject } = useProjectBridge();
  const { deleteTask } = useTaskBridge();
  const [addProjectOpen, setAddProjectOpen] = useState(false);
  const [taskMenu, setTaskMenu] = useState<TaskMenuState | null>(null);
  const [projectMenu, setProjectMenu] = useState<ProjectMenuState | null>(null);
  const [confirmTaskId, setConfirmTaskId] = useState<string | null>(null);
  const [confirmProjectId, setConfirmProjectId] = useState<string | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [removingProject, setRemovingProject] = useState(false);
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
    if (!taskMenu && !projectMenu) {
      return;
    }

    const close = () => {
      setTaskMenu(null);
      setProjectMenu(null);
    };
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        close();
      }
    };

    window.addEventListener("click", close);
    window.addEventListener("keydown", closeOnEscape);
    window.addEventListener("resize", close);
    window.addEventListener("scroll", close, true);
    return () => {
      window.removeEventListener("click", close);
      window.removeEventListener("keydown", closeOnEscape);
      window.removeEventListener("resize", close);
      window.removeEventListener("scroll", close, true);
    };
  }, [projectMenu, taskMenu]);

  const confirmTask =
    state.tasks.find((task) => task.id === confirmTaskId) ??
    Object.values(state.taskCache)
      .flat()
      .find((task) => task.id === confirmTaskId) ??
    null;
  const menuProject =
    state.projects.recent.find((project) => project.id === projectMenu?.projectId) ?? null;
  const confirmProject =
    state.projects.recent.find((project) => project.id === confirmProjectId) ??
    (state.projects.current?.id === confirmProjectId ? state.projects.current : null);

  async function handleDeleteConfirmed() {
    if (!confirmTaskId) {
      return;
    }

    setDeleting(true);
    await deleteTask(confirmTask?.projectPath ?? state.projects.current?.path ?? null, confirmTaskId);
    setDeleting(false);
    setConfirmTaskId(null);
  }

  async function handleRemoveProjectConfirmed() {
    if (!confirmProject) {
      return;
    }

    setRemovingProject(true);
    const removed = await removeRecentProject(confirmProject);
    setRemovingProject(false);
    if (removed) {
      setConfirmProjectId(null);
    }
  }

  async function handleProjectMenuAction(action: ProjectMenuAction, project: ProjectSummary) {
    setProjectMenu(null);

    if (action === "remove") {
      setConfirmProjectId(project.id);
      return;
    }

    if (!hasTauriRuntime()) {
      return;
    }

    try {
      await revealItemInDir(project.path);
    } catch (error) {
      dispatch({ type: "projects/loadFailed", error: toErrorMessage(error) });
    }
  }

  return (
    <div className="sidebar-nav-container">
      <nav className="sidebar-nav-section">
        <div className="sidebar-search">
          <Search size={14} />
          <span>搜索任务…</span>
        </div>
        <button
          type="button"
          className="project-add-button"
          disabled={state.app.isLoadingProjects}
          title="添加项目"
          onClick={() => setAddProjectOpen(true)}
        >
          <FolderPlus size={15} />
          <span>添加项目</span>
        </button>
        {state.app.projectError && <div className="nav-error">{state.app.projectError}</div>}
        <ul className="nav-list">
          {state.projects.recent.map((project) => {
            const active = project.id === state.app.activeProjectId;
            const cachedProjectTasks = state.taskCache[project.path];
            // Source tasks are ordered by createdAtMs; the sidebar shows the
            // most recently touched task first. Copy before sorting so we never
            // mutate state.
            const projectTasks = (cachedProjectTasks ?? (active ? state.tasks : []))
              .slice()
              .sort((left, right) => right.updatedAtMs - left.updatedAtMs);
            const hasTasks = projectTasks.length > 0;
            const hasLoadedTasks = cachedProjectTasks !== undefined || active;
            const expanded = hasTasks && !collapsedIds.has(project.id);

            return (
              <li className="project-nav-item" key={`${project.id}:${project.path}`}>
                <div
                  className={`nav-item${active ? " active" : ""}`}
                  onContextMenu={(event) => {
                    event.preventDefault();
                    setTaskMenu(null);
                    setProjectMenu({
                      projectId: project.id,
                      ...contextMenuPosition(event),
                    });
                  }}
                >
                  <button
                    type="button"
                    className="project-select-area"
                    aria-expanded={expanded}
                    onClick={() => {
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
                    <span className="project-task-count">{hasLoadedTasks ? projectTasks.length : ""}</span>
                  </button>
                  <button
                    type="button"
                    className="project-inline-add"
                    aria-label={`在 ${project.name} 中新建任务`}
                    title={`在 ${project.name} 中新建任务`}
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
                </div>
                {expanded && (
                  <div className="task-nav-collapse">
                    <ul className="task-nav-list">
                      <li>
                        <button
                          type="button"
                          className={`task-nav-item all-tasks${
                            active && state.app.currentView === "chat" ? " selected" : ""
                          }`}
                          onClick={() => {
                            if (!active) {
                              dispatch({ type: "projects/selected", projectId: project.id });
                            }
                            dispatch({ type: "app/viewSelected", view: "chat" });
                          }}
                        >
                          <MessageSquare size={12} className="task-allboard-icon" />
                          <span>对话</span>
                        </button>
                      </li>
                      <li>
                        <button
                          type="button"
                          className={`task-nav-item all-tasks${
                            active && state.app.currentView === "board" ? " selected" : ""
                          }`}
                          onClick={() => {
                            if (!active) {
                              dispatch({ type: "projects/selected", projectId: project.id });
                            }
                            dispatch({ type: "app/viewSelected", view: "board" });
                          }}
                        >
                          <LayoutGrid size={12} className="task-allboard-icon" />
                          <span>任务看板</span>
                        </button>
                      </li>
                      {projectTasks.map((task, index) => {
                        const selected = task.id === state.app.selectedTaskId;

                        return (
                          <li key={task.id} style={{ "--task-index": index } as CSSProperties}>
                            <button
                              type="button"
                              className={`task-nav-item${selected ? " selected" : ""}`}
                              onClick={() => {
                                if (!active) {
                                  dispatch({ type: "projects/selected", projectId: project.id });
                                }
                                dispatch({ type: "tasks/selected", taskId: task.id });
                              }}
                              onContextMenu={(event) => {
                                event.preventDefault();
                                setProjectMenu(null);
                                setTaskMenu({
                                  taskId: task.id,
                                  projectPath: task.projectPath,
                                  ...contextMenuPosition(event),
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
            <li className="nav-empty">暂无最近项目</li>
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
          <span>设置</span>
        </button>
      </div>

      {addProjectOpen && <AddProjectModal onClose={() => setAddProjectOpen(false)} />}

      {menuProject && projectMenu && (
        <div
          className="context-menu project-context-menu"
          style={{ top: projectMenu.y, left: projectMenu.x }}
          role="menu"
          aria-label={`${menuProject.name} 项目操作`}
          onClick={(event) => event.stopPropagation()}
          onContextMenu={(event) => event.preventDefault()}
        >
          <button
            type="button"
            className="context-menu-item"
            role="menuitem"
            onClick={() => void handleProjectMenuAction("reveal", menuProject)}
          >
            <FolderOpen size={13} />
            在 Finder 中显示
          </button>
          <div className="context-menu-divider" role="separator" />
          <button
            type="button"
            className="context-menu-item danger"
            role="menuitem"
            onClick={() => void handleProjectMenuAction("remove", menuProject)}
            title="仅从 Loom 侧边栏移除"
          >
            <X size={13} />
            从侧边栏移除
          </button>
        </div>
      )}

      {taskMenu && (
        <div
          className="context-menu task-context-menu"
          style={{ top: taskMenu.y, left: taskMenu.x }}
          role="menu"
          onClick={(event) => event.stopPropagation()}
          onContextMenu={(event) => event.preventDefault()}
        >
          <button
            type="button"
            className="context-menu-item danger"
            role="menuitem"
            onClick={() => {
              setConfirmTaskId(taskMenu.taskId);
              setTaskMenu(null);
            }}
          >
            <Trash2 size={13} />
            删除任务
          </button>
        </div>
      )}

      {confirmProject && (
        <div className="confirm-backdrop" role="presentation">
          <div className="confirm-modal" role="dialog" aria-modal="true">
            <h2>移除项目？</h2>
            <p>
              “{confirmProject.name}”会从 Loom 侧边栏移除；本地文件夹及其中的 .loom 数据不会删除。
            </p>
            <div className="confirm-modal-actions">
              <Button
                variant="ghost"
                onClick={() => setConfirmProjectId(null)}
                disabled={removingProject}
              >
                取消
              </Button>
              <Button
                variant="danger"
                onClick={() => void handleRemoveProjectConfirmed()}
                disabled={removingProject}
              >
                {removingProject ? "正在移除…" : "移除"}
              </Button>
            </div>
          </div>
        </div>
      )}

      {confirmTask && (
        <div className="confirm-backdrop" role="presentation">
          <div className="confirm-modal" role="dialog" aria-modal="true">
            <h2>删除任务？</h2>
            <p>
              “{confirmTask.title}”及其规划证据将被永久删除，且无法撤销。
            </p>
            <div className="confirm-modal-actions">
              <Button variant="ghost" onClick={() => setConfirmTaskId(null)} disabled={deleting}>
                取消
              </Button>
              <Button variant="danger" onClick={() => void handleDeleteConfirmed()} disabled={deleting}>
                {deleting ? "正在删除…" : "删除"}
              </Button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
