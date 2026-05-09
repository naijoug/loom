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

const runningStatuses = new Set<TaskStatus>([
  "planning",
  "implementing",
  "debugging",
  "fixing",
  "verifying",
]);

function stepStatus(activeIndex: number, index: number, running: boolean): StepStatus {
  if (index < activeIndex) {
    return "done";
  }

  if (index === activeIndex) {
    return running ? "running" : "active";
  }

  return "pending";
}

export function deriveTaskStages(status: TaskStatus | null): TaskStage[] {
  if (!status) {
    return [
      { id: "planning", label: "Planning", status: "pending" },
      { id: "implementing", label: "Implementing", status: "pending" },
      { id: "debugging", label: "Debugging", status: "pending" },
    ];
  }

  const activeStep = status ? statusToFlowStep[status] : "planning";
  const activeIndex = flowOrder.indexOf(activeStep);
  const running = runningStatuses.has(status);

  return [
    { id: "planning", label: "Planning", status: stepStatus(activeIndex, 0, running) },
    { id: "implementing", label: "Implementing", status: stepStatus(activeIndex, 1, running) },
    { id: "debugging", label: "Debugging", status: stepStatus(activeIndex, 2, running) },
  ];
}
