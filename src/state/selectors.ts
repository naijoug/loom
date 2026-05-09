import type { TaskStatus } from "../domain";
import type { StepStatus } from "../components/Header/TaskFlowStep";
import type { TaskStage } from "../components/Header/TaskFlow";

const flowOrder = ["planning", "implementing", "debugging"] as const;

const statusToFlowStep: Record<TaskStatus, (typeof flowOrder)[number]> = {
  drafting_requirements: "planning",
  planning: "planning",
  plan_review: "planning",
  ready_to_implement: "implementing",
  implementing: "implementing",
  reviewing: "implementing",
  debugging: "debugging",
  fixing: "debugging",
  verifying: "debugging",
  completed: "debugging",
  blocked: "debugging",
  cancelled: "debugging",
};

function stepStatus(activeIndex: number, index: number): StepStatus {
  if (index < activeIndex) {
    return "done";
  }

  if (index === activeIndex) {
    return "active";
  }

  return "pending";
}

export function deriveTaskStages(status: TaskStatus | null): TaskStage[] {
  const activeStep = status ? statusToFlowStep[status] : "planning";
  const activeIndex = flowOrder.indexOf(activeStep);

  return [
    { id: "planning", label: "Planning", status: stepStatus(activeIndex, 0) },
    { id: "implementing", label: "Implementing", status: stepStatus(activeIndex, 1) },
    { id: "debugging", label: "Debugging", status: stepStatus(activeIndex, 2) },
  ];
}
