import { TaskFlow, type TaskStage } from "./TaskFlow";
import { useAppState } from "../../state/AppStateContext";
import { deriveTaskStages } from "../../state/selectors";
import "./Header.css";

export function Header() {
  const { state } = useAppState();
  const currentTask = state.tasks.find((task) => task.id === state.app.selectedTaskId) ?? null;
  const currentProject = state.projects.current;
  const stages: TaskStage[] = deriveTaskStages(currentTask?.status ?? null);
  const isBoard = state.app.currentView === "board";
  const breadcrumb = currentProject
    ? isBoard
      ? `${currentProject.name} › Tasks`
      : `${currentProject.name} › ${currentTask?.title ?? "No active task"}`
    : "No project selected";

  return (
    <div className="main-header" data-tauri-drag-region>
      <div className="header-breadcrumb" data-tauri-drag-region>
        {breadcrumb}
      </div>
      
      {currentProject && !isBoard && (
        <div className="header-flow">
          <TaskFlow stages={stages} />
        </div>
      )}
    </div>
  );
}
