import type { TaskStatus } from "../domain";
import type { StepStatus } from "../components/Header/TaskFlowStep";
import type { TaskStage } from "../components/Header/TaskFlow";

// Public, UI-facing workflow stages. The header step bar, `viewedStage` review
// state, and pane routing all speak this vocabulary. Underlying `TaskStatus`
// values are folded into these four ids by `stageOf`.
export type WorkflowStageId = "planning" | "implementing" | "testing" | "done";

const flowOrder: WorkflowStageId[] = ["planning", "implementing", "testing", "done"];

const STAGE_LABELS: Record<WorkflowStageId, string> = {
  planning: "Planning",
  implementing: "Implementing",
  testing: "Testing",
  done: "Done",
};

// Past-tense labels for stages the task has already cleared, so a completed
// step reads as finished ("Implemented") rather than still in progress
// ("Implementing"). Current/pending stages keep the present-tense labels.
const STAGE_DONE_LABELS: Record<WorkflowStageId, string> = {
  planning: "Planned",
  implementing: "Implemented",
  testing: "Tested",
  done: "Done",
};

const statusToFlowStep: Record<TaskStatus, WorkflowStageId> = {
  drafting_requirements: "planning",
  planning: "planning",
  plan_review: "planning",
  ready_to_implement: "implementing",
  implementing: "implementing",
  reviewing: "implementing",
  debugging: "testing",
  fixing: "testing",
  verifying: "testing",
  completed: "done",
  // blocked/cancelled keep their historical Testing grouping; refining their
  // stage attribution is tracked separately (see plan risk #4).
  blocked: "testing",
  cancelled: "testing",
};

const runningStatuses = new Set<TaskStatus>([
  "planning",
  "implementing",
  "debugging",
  "fixing",
  "verifying",
]);

/** Single source of truth: which workflow stage a task status belongs to. */
export function stageOf(status: TaskStatus | null): WorkflowStageId {
  return status ? statusToFlowStep[status] : "planning";
}

function stepStatus(activeIndex: number, index: number, running: boolean): StepStatus {
  if (index < activeIndex) {
    return "done";
  }

  if (index === activeIndex) {
    return running ? "running" : "active";
  }

  return "pending";
}

/**
 * Build the header step bar.
 *
 * `viewedStage` is the stage the user is currently looking at (null = follow
 * the task's real stage). The returned `viewing` flag highlights that stage,
 * while `status`/running animation still reflects the task's actual progress.
 * `clickable` marks stages the task has already reached (current + completed);
 * stages not yet reached are disabled.
 */
export function deriveTaskStages(
  status: TaskStatus | null,
  viewedStage: WorkflowStageId | null = null,
): TaskStage[] {
  const activeStage = stageOf(status);
  const activeIndex = flowOrder.indexOf(activeStage);
  const running = status ? runningStatuses.has(status) : false;
  const effectiveStage: WorkflowStageId | null = status ? viewedStage ?? activeStage : null;

  return flowOrder.map((id, index) => {
    const stepState = status ? stepStatus(activeIndex, index, running) : "pending";

    return {
      id,
      label: stepState === "done" ? STAGE_DONE_LABELS[id] : STAGE_LABELS[id],
      status: stepState,
      viewing: effectiveStage === id,
      clickable: status ? index <= activeIndex : false,
    };
  });
}
