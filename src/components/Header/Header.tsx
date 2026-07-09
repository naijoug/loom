import { TaskFlow, type TaskStage } from "./TaskFlow";
import { useAppState } from "../../state/AppStateContext";
import { deriveTaskStages, stageOf, type WorkflowStageId } from "../../state/selectors";
import "./Header.css";

export function Header() {
  const { state, dispatch } = useAppState();
  const currentTask = state.tasks.find((task) => task.id === state.app.selectedTaskId) ?? null;
  const currentProject = state.projects.current;
  const stages: TaskStage[] = deriveTaskStages(currentTask?.status ?? null, state.app.viewedStage);
  const isBoard = state.app.currentView === "board";
  const currentStage = stageOf(currentTask?.status ?? null);
  const currentPage = isBoard ? "任务看板" : currentTask?.title ?? "未选择任务";

  function handleSelectStage(stage: WorkflowStageId) {
    // Clicking the task's real stage clears the review state (follow live);
    // clicking an earlier stage enters read-only review of that stage.
    dispatch({ type: "app/stageViewed", stage: stage === currentStage ? null : stage });
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
    </div>
  );
}
