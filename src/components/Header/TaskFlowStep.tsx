import { Check, Loader, Circle } from "lucide-react";
import "./Header.css";

export type StepStatus = "done" | "active" | "running" | "pending";

interface TaskFlowStepProps {
  label: string;
  status: StepStatus;
  viewing: boolean;
  clickable: boolean;
  onClick: () => void;
}

export function TaskFlowStep({ label, status, viewing, clickable, onClick }: TaskFlowStepProps) {
  return (
    <button
      type="button"
      className={`task-flow-step${viewing ? " viewing" : ""}`}
      disabled={!clickable}
      onClick={onClick}
    >
      {status === "done" && <Check size={14} className="step-icon done-icon" />}
      {status === "active" && <Circle size={14} className="step-icon active-icon" />}
      {status === "running" && <Loader size={14} className="step-icon active-icon spin-anim" />}
      {status === "pending" && <Circle size={14} className="step-icon pending-icon" />}
      <span className={`step-label status-${status}`}>{label}</span>
    </button>
  );
}
