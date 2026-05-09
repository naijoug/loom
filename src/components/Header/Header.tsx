import { TaskFlow, type TaskStage } from "./TaskFlow";
import { Button } from "../common/Button";
import "./Header.css";

// Temporary mock data for UI visual completion
const mockStages: TaskStage[] = [
  { id: "planning", label: "Planning", status: "done" },
  { id: "implementing", label: "Implementing", status: "active" },
  { id: "debugging", label: "Debugging", status: "pending" },
];

export function Header() {
  return (
    <div className="main-header">
      <div className="header-breadcrumb">
        sample-project / MVP Core
      </div>
      
      <div className="header-flow">
        <TaskFlow stages={mockStages} />
      </div>

      <div className="header-actions">
        <Button variant="danger">Stop Task</Button>
      </div>
    </div>
  );
}
