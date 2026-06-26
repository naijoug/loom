import { History, Radar } from "lucide-react";
import { PlanningChat } from "../Planning";
import { DonePane, SessionPane, TestingPane } from "../TaskDetail";
import { useAppState } from "../../state/AppStateContext";
import { stageOf, type WorkflowStageId } from "../../state/selectors";
import "./Workspace.css";

const STAGE_LABELS: Record<WorkflowStageId, string> = {
  planning: "Planning",
  implementing: "Implementing",
  testing: "Testing",
  done: "Done",
};

export function WorkspaceSplit() {
  const { state, dispatch } = useAppState();
  const task = state.tasks.find((candidate) => candidate.id === state.app.selectedTaskId) ?? null;
  const project = state.projects.current;

  if (!project) {
    return (
      <div className="workspace-empty-state redesigned-empty">
        <Radar size={28} />
        <div className="workspace-empty-title">Open a project to get started</div>
        <div className="workspace-empty-copy">
          Loom will analyze the project, load agents, and keep plans, reviews,
          human decisions, and final documents in one place.
        </div>
      </div>
    );
  }

  const currentStage = stageOf(task?.status ?? null);
  const effectiveStage: WorkflowStageId = state.app.viewedStage ?? currentStage;
  // Read-only review means the user is looking at a stage other than the one
  // the task is actually in. All side-effecting controls are disabled.
  const readOnly = task !== null && state.app.viewedStage !== null && effectiveStage !== currentStage;

  let pane: JSX.Element;
  switch (effectiveStage) {
    case "done":
      pane = task ? <DonePane project={project} task={task} readOnly={readOnly} /> : <PlanningChat />;
      break;
    case "testing":
      pane = task ? <TestingPane project={project} task={task} readOnly={readOnly} /> : <PlanningChat />;
      break;
    case "implementing":
      pane = task ? <SessionPane project={project} task={task} readOnly={readOnly} /> : <PlanningChat />;
      break;
    case "planning":
    default:
      pane = <PlanningChat readOnly={readOnly} />;
      break;
  }

  if (!readOnly) {
    return pane;
  }

  return (
    <div className="stage-review-wrap">
      <div className="stage-review-banner">
        <History size={14} />
        <span>
          Viewing the <b>{STAGE_LABELS[effectiveStage]}</b> stage. The task is
          currently in {STAGE_LABELS[currentStage]}.
        </span>
        <button
          type="button"
          className="stage-review-return"
          onClick={() => dispatch({ type: "app/stageViewed", stage: null })}
        >
          Return to current stage
        </button>
      </div>
      <div className="stage-review-content">{pane}</div>
    </div>
  );
}
