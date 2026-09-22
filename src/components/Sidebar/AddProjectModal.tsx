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
      (state.projects.current?.path === selectedPath ? state.projects.current : null) ??
      null,
    [selectedPath, state.projects.current, state.projects.recent],
  );
  const detectedStacks = detectedProject?.detectedStacks ?? [];
  const recentProjects = state.projects.recent.slice(0, 3);

  async function handleBrowse() {
    if (!hasTauriRuntime()) {
      dispatch({
        type: "projects/loadFailed",
        error: "选择项目需要在 Tauri 桌面应用中进行。",
      });
      return;
    }

    const selected = await open({
      directory: true,
      multiple: false,
      title: "选择项目文件夹",
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
          <h2 id="add-project-title">添加项目</h2>
          <button type="button" className="project-modal-close" aria-label="关闭" onClick={onClose}>
            <X size={16} />
          </button>
        </div>

        <div className="project-modal-body">
          <section className="project-modal-section">
            <div className="project-modal-label">打开本地目录</div>
            <div className="project-path-row">
              <div className={`project-path-field ${selectedPath ? "" : "empty"}`}>
                {selectedPath ? displayPath(selectedPath) : "请选择本地项目文件夹"}
              </div>
              <Button type="button" variant="ghost" onClick={handleBrowse}>
                浏览…
              </Button>
            </div>
          </section>

          <section className="project-modal-section">
            <div className="project-modal-label">最近项目</div>
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
                <div className="project-recent-empty">暂无最近项目</div>
              )}
            </div>
          </section>

          <section className="project-detected-card">
            <div className="project-detected-head">
              <span>检测结果</span>
              <span className="project-detected-status">{detectedProject ? "已保存记录" : "待检测"}</span>
            </div>
            {detectedProject ? <><div className="project-stack-row">
              {detectedStacks.slice(0, 4).map((stack) => (
                <span className="project-stack-chip" key={stack}>
                  {stack}
                </span>
              ))}
              {detectedStacks.length === 0 ? <span>未识别到预设技术栈</span> : null}
            </div>
            <div className="project-meta-line">
              {detectedProject.gitBranch ? <span>{detectedProject.gitBranch}</span> : null}
              <span>{detectedProject.isGitRepository ? "Git 仓库" : "本地目录"}</span>
              <span>{detectedProject.suggestedCommands[0] ?? "未检测到启动命令"}</span>
            </div>
            </> : <p className="project-recent-empty">添加时将检测所选目录的技术栈、Git 状态和可用命令。</p>}
          </section>

          {state.app.projectError && <div className="project-modal-error">{state.app.projectError}</div>}
        </div>

        <div className="project-modal-actions">
          <Button type="button" variant="ghost" onClick={onClose}>
            取消
          </Button>
          <Button
            type="button"
            variant="primary"
            disabled={!selectedPath.trim() || state.app.isLoadingProjects}
            onClick={handleAddProject}
          >
            添加项目
          </Button>
        </div>
      </div>
    </div>
  );
}
