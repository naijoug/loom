import { Check, Loader, Circle } from "lucide-react";
import "./Header.css";

export type StepStatus = "done" | "active" | "pending";

interface TaskFlowStepProps {
  label: string;
  status: StepStatus;
}

export function TaskFlowStep({ label, status }: TaskFlowStepProps) {
  return (
    <div className="task-flow-step">
      {status === "done" && <Check size={14} className="step-icon done-icon" />}
      {status === "active" && <Loader size={14} className="step-icon active-icon spin-anim" />}
      {status === "pending" && <Circle size={14} className="step-icon pending-icon" />}
      <span className={`step-label status-${status}`}>{label}</span>
    </div>
  );
}
