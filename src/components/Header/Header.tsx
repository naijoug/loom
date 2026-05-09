import { TaskFlow, type TaskStage } from "./TaskFlow";
import { Button } from "../common/Button";
import { useAppState } from "../../state/AppStateContext";
import { deriveTaskStages } from "../../state/selectors";
import "./Header.css";

export function Header() {
  const { state } = useAppState();
  const currentTask = state.tasks.find((task) => task.id === state.app.selectedTaskId) ?? null;
  const currentProject = state.projects.current;
  const stages: TaskStage[] = deriveTaskStages(currentTask?.status ?? null);
  const breadcrumb = currentProject
    ? `${currentProject.name} / ${currentTask?.title ?? "No active task"}`
    : "No project selected";

  return (
    <div className="main-header">
      <div className="header-breadcrumb">
        {breadcrumb}
      </div>
      
      <div className="header-flow">
        <TaskFlow stages={stages} />
      </div>

      <div className="header-actions">
        <Button variant="danger">Stop Task</Button>
      </div>
    </div>
  );
}
