import { ChevronRight } from "lucide-react";
import { TaskFlowStep, type StepStatus } from "./TaskFlowStep";
import type { WorkflowStageId } from "../../state/selectors";
import "./Header.css";

export interface TaskStage {
  id: WorkflowStageId;
  label: string;
  status: StepStatus;
  viewing: boolean;
  clickable: boolean;
}

interface TaskFlowProps {
  stages: TaskStage[];
  onSelect: (stage: WorkflowStageId) => void;
}

export function TaskFlow({ stages, onSelect }: TaskFlowProps) {
  return (
    <div className="task-flow-container">
      {stages.map((stage, index) => (
        <div key={stage.id} className="task-flow-node">
          <TaskFlowStep
            label={stage.label}
            status={stage.status}
            viewing={stage.viewing}
            clickable={stage.clickable}
            onClick={() => onSelect(stage.id)}
          />
          {index < stages.length - 1 && (
            <ChevronRight size={14} className="step-separator" />
          )}
        </div>
      ))}
    </div>
  );
}
