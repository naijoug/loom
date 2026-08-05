import { History, Radar } from "lucide-react";
import { PlanningChat } from "../Planning";
import { DonePane, SessionPane, TestingPane } from "../TaskDetail";
import { useAppState } from "../../state/AppStateContext";
import { stageOf, type WorkflowStageId } from "../../state/selectors";
import "./Workspace.css";

const STAGE_LABELS: Record<WorkflowStageId, string> = {
  planning: "规划",
  implementing: "实施",
  testing: "测试验收",
  done: "完成",
};

const TASK_STATUS_LABELS: Record<string, string> = {
  paused: "已暂停",
  blocked: "已阻塞",
  cancelled: "已取消",
};

export function WorkspaceSplit() {
  const { state, dispatch } = useAppState();
  const task = state.tasks.find((candidate) => candidate.id === state.app.selectedTaskId) ?? null;
  const project = state.projects.current;

  if (!project) {
    return (
      <div className="workspace-empty-state redesigned-empty">
        <Radar size={28} />
        <div className="workspace-empty-title">打开项目以开始使用</div>
        <div className="workspace-empty-copy">
          Loom 会分析项目、加载 Agent，并集中保存计划、Review、人工决策与最终文档。
        </div>
      </div>
    );
  }

  const flowStatus =
    task?.status === "blocked" && task.lifecycle?.resumeStatus
      ? task.lifecycle.resumeStatus
      : task?.status ?? null;
  const currentStage = stageOf(flowStatus);
  const effectiveStage: WorkflowStageId = state.app.viewedStage ?? currentStage;
  // Read-only review means the user is looking at a stage other than the one
  // the task is actually in. All side-effecting controls are disabled.
  const stageReviewReadOnly =
    task !== null && state.app.viewedStage !== null && effectiveStage !== currentStage;
  const lifecycleReadOnly = Boolean(
    task?.lifecycle?.paused || task?.status === "blocked" || task?.status === "cancelled",
  );
  const readOnly = stageReviewReadOnly || lifecycleReadOnly;

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
        {lifecycleReadOnly ? (
          <span>
            任务当前<b>{TASK_STATUS_LABELS[task?.lifecycle?.paused ? "paused" : task?.status ?? ""] ?? task?.status}</b>。{" "}
            {task?.status === "cancelled"
              ? "历史记录仍可只读查看。"
              : "请先从顶部恢复任务，再更改工作流状态。"}
          </span>
        ) : (
          <>
            <span>
              正在回看<b>{STAGE_LABELS[effectiveStage]}</b>阶段；任务当前位于{STAGE_LABELS[currentStage]}阶段。
            </span>
            <button
              type="button"
              className="stage-review-return"
              onClick={() => dispatch({ type: "app/stageViewed", stage: null })}
            >
              返回当前阶段
            </button>
          </>
        )}
      </div>
      <div className="stage-review-content">{pane}</div>
    </div>
  );
}
