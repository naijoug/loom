import { Ban, CircleStop, Pause, Play } from "lucide-react";
import { useState } from "react";
import { TaskFlow, type TaskStage } from "./TaskFlow";
import { useTaskBridge } from "../../hooks/useTaskBridge";
import { useAppState } from "../../state/AppStateContext";
import { deriveTaskStages, stageOf, type WorkflowStageId } from "../../state/selectors";
import "./Header.css";

export function Header() {
  const { state, dispatch } = useAppState();
  const { blockTask, cancelTask, pauseTask, resumeTask } = useTaskBridge();
  const [lifecycleBusy, setLifecycleBusy] = useState(false);
  const currentTask = state.tasks.find((task) => task.id === state.app.selectedTaskId) ?? null;
  const currentProject = state.projects.current;
  const flowStatus =
    currentTask?.status === "blocked" && currentTask.lifecycle?.resumeStatus
      ? currentTask.lifecycle.resumeStatus
      : currentTask?.status ?? null;
  const stages: TaskStage[] = deriveTaskStages(flowStatus, state.app.viewedStage);
  const isBoard = state.app.currentView === "board";
  const currentStage = stageOf(flowStatus);
  const currentPage = isBoard ? "任务看板" : currentTask?.title ?? "未选择任务";

  function handleSelectStage(stage: WorkflowStageId) {
    // Clicking the task's real stage clears the review state (follow live);
    // clicking an earlier stage enters read-only review of that stage.
    dispatch({ type: "app/stageViewed", stage: stage === currentStage ? null : stage });
  }

  async function runLifecycleAction(action: () => Promise<unknown>) {
    setLifecycleBusy(true);
    try {
      await action();
    } finally {
      setLifecycleBusy(false);
    }
  }

  function handlePause() {
    if (!currentProject || !currentTask) return;
    const reason = window.prompt("Why are you pausing this task?", "Paused by user");
    if (reason === null) return;
    void runLifecycleAction(() => pauseTask(currentProject.path, currentTask.id, reason));
  }

  function handleBlock() {
    if (!currentProject || !currentTask) return;
    const reason = window.prompt("What is blocking this task?", "External dependency blocked");
    if (reason === null) return;
    void runLifecycleAction(() => blockTask(currentProject.path, currentTask.id, reason));
  }

  function handleResume() {
    if (!currentProject || !currentTask) return;
    void runLifecycleAction(() => resumeTask(currentProject.path, currentTask.id));
  }

  function handleCancel() {
    if (!currentProject || !currentTask) return;
    if (!window.confirm(`Cancel task “${currentTask.title}”? Running commands will be stopped.`)) {
      return;
    }
    void runLifecycleAction(() =>
      cancelTask(currentProject.path, currentTask.id, "Cancelled by user"),
    );
  }

  return (
    <div className="main-header" data-tauri-drag-region>
      <div className="hd-left header-breadcrumb" data-tauri-drag-region>
        {currentProject ? (
          <nav className="hd-breadcrumb" aria-label="面包屑">
            <button
              type="button"
              className="hd-crumb hd-crumb-link breadcrumb-project"
              onClick={() => dispatch({ type: "app/viewSelected", view: "board" })}
            >
              {currentProject.name}
            </button>
            <span className="hd-crumb-sep breadcrumb-sep">›</span>
            <span className="hd-crumb hd-crumb-current breadcrumb-current">{currentPage}</span>
          </nav>
        ) : (
          <span className="hd-crumb hd-crumb-current">未选择项目</span>
        )}
      </div>

      {currentProject && !isBoard && (
        <div className="hd-stepper header-flow">
          <TaskFlow stages={stages} onSelect={handleSelectStage} />
        </div>
      )}

      {currentProject &&
        currentTask &&
        !isBoard &&
        currentTask.status !== "completed" &&
        currentTask.status !== "cancelled" && (
        <div className="header-lifecycle-actions" aria-label="Task lifecycle controls">
          {currentTask.lifecycle?.paused || currentTask.status === "blocked" ? (
            <button type="button" onClick={handleResume} disabled={lifecycleBusy} title="Resume task">
              <Play size={13} /> Resume
            </button>
          ) : (
            <>
              <button type="button" onClick={handlePause} disabled={lifecycleBusy} title="Pause task">
                <Pause size={13} /> Pause
              </button>
              <button type="button" onClick={handleBlock} disabled={lifecycleBusy} title="Mark blocked">
                <Ban size={13} /> Block
              </button>
            </>
          )}
          <button className="danger" type="button" onClick={handleCancel} disabled={lifecycleBusy} title="Cancel task">
            <CircleStop size={13} /> Cancel
          </button>
        </div>
      )}
    </div>
  );
}
