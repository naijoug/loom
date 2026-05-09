import { ImplementationPane } from "./ImplementationPane";
import { ImplementationOutputPane } from "./ImplementationOutputPane";
import { PlanningPane } from "./PlanningPane";
import { useAppState } from "../../state/AppStateContext";
import "./Workspace.css";

export function WorkspaceSplit() {
  const { state } = useAppState();
  const task = state.tasks.find((candidate) => candidate.id === state.app.selectedTaskId) ?? null;
  const inImplementation = task?.status === "ready_to_implement" || task?.status === "implementing";

  if (!inImplementation) {
    return <PlanningPane />;
  }

  return (
    <div className="workspace-split">
      <div className="workspace-pane pane-left">
        <ImplementationPane />
      </div>
      <div className="workspace-pane pane-right">
        <ImplementationOutputPane />
      </div>
    </div>
  );
}
