import { ChevronRight } from "lucide-react";
import { TaskFlowStep, type StepStatus } from "./TaskFlowStep";
import "./Header.css";

export interface TaskStage {
  id: string;
  label: string;
  status: StepStatus;
}

interface TaskFlowProps {
  stages: TaskStage[];
}

export function TaskFlow({ stages }: TaskFlowProps) {
  return (
    <div className="task-flow-container">
      {stages.map((stage, index) => (
        <div key={stage.id} className="task-flow-node">
          <TaskFlowStep label={stage.label} status={stage.status} />
          {index < stages.length - 1 && (
            <ChevronRight size={14} className="step-separator" />
          )}
        </div>
      ))}
    </div>
  );
}
