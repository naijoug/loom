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

  function handleSelectStage(stage: WorkflowStageId) {
    // Clicking the task's real stage clears the review state (follow live);
    // clicking an earlier stage enters read-only review of that stage.
    dispatch({ type: "app/stageViewed", stage: stage === currentStage ? null : stage });
  }

  return (
    <div className="main-header" data-tauri-drag-region>
      <div className="header-breadcrumb" data-tauri-drag-region>
        {currentProject ? (
          <>
            <button
              type="button"
              className="breadcrumb-project"
              onClick={() => dispatch({ type: "app/viewSelected", view: "board" })}
            >
              {currentProject.name}
            </button>
            <span className="breadcrumb-sep">›</span>
            <span className="breadcrumb-current">
              {isBoard ? "Tasks" : currentTask?.title ?? "No active task"}
            </span>
          </>
        ) : (
          "No project selected"
        )}
      </div>

      {currentProject && !isBoard && (
        <div className="header-flow">
          <TaskFlow stages={stages} onSelect={handleSelectStage} />
        </div>
      )}
    </div>
  );
}
