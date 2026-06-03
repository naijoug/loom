import {
  Activity,
  ClipboardCheck,
  MessagesSquare,
  Radar,
  ShieldCheck,
  TerminalSquare,
} from "lucide-react";
import { ImplementationPane } from "./ImplementationPane";
import { ImplementationOutputPane } from "./ImplementationOutputPane";
import { PlanningWizard } from "./PlanningWizard";
import { DebugPane } from "./DebugPane";
import type { Task, TaskStatus } from "../../domain";
import { useAppState } from "../../state/AppStateContext";
import "./Workspace.css";

const STAGE_GROUPS: Array<{
  id: string;
  title: string;
  statuses: TaskStatus[];
  icon: typeof MessagesSquare;
}> = [
  {
    id: "plan",
    title: "Discuss + plan",
    statuses: ["drafting_requirements", "planning", "plan_review"],
    icon: MessagesSquare,
  },
  {
    id: "build",
    title: "Implement + review",
    statuses: ["ready_to_implement", "implementing", "reviewing"],
    icon: ClipboardCheck,
  },
  {
    id: "debug",
    title: "Debug + repair",
    statuses: ["debugging", "fixing"],
    icon: TerminalSquare,
  },
  {
    id: "verify",
    title: "Verify + summarize",
    statuses: ["verifying", "completed", "blocked", "cancelled"],
    icon: ShieldCheck,
  },
];

function formatStatus(status: TaskStatus | null) {
  if (!status) {
    return "no task";
  }

  return status.split("_").join(" ");
}

function stageState(taskStatus: TaskStatus | null, statuses: TaskStatus[]) {
  if (!taskStatus) {
    return "pending";
  }

  if (statuses.includes(taskStatus)) {
    return ["blocked", "cancelled"].includes(taskStatus) ? "blocked" : "active";
  }

  const activeIndex = STAGE_GROUPS.findIndex((group) => group.statuses.includes(taskStatus));
  const groupIndex = STAGE_GROUPS.findIndex((group) => group.statuses === statuses);

  return groupIndex < activeIndex ? "done" : "pending";
}

function latestRunSummary(task: Task | null) {
  const latestRun = task?.commandRuns
    .slice()
    .sort((left, right) => right.startedAtMs - left.startedAtMs)[0];

  if (!latestRun) {
    return "No verification run yet";
  }

  const exit = typeof latestRun.exitCode === "number" ? ` · exit ${latestRun.exitCode}` : "";
  return `${latestRun.status}${exit}`;
}

function CommandOverview({ task }: { task: Task | null }) {
  const completedTodos = task?.planTodos.filter((todo) => todo.status === "done").length ?? 0;
  const totalTodos = task?.planTodos.length ?? 0;
  const failedInvocations =
    task?.agentInvocations.filter((invocation) => invocation.status === "failed").length ?? 0;
  const successfulInvocations =
    task?.agentInvocations.filter((invocation) => invocation.status === "succeeded").length ?? 0;

  return (
    <div className="command-overview">
      <div className="command-overview-header">
        <div>
          <div className="command-eyebrow">Current operating state</div>
          <h2>{task?.title ?? "Create a development task"}</h2>
        </div>
        <span className={`status-pill status-${task?.status ?? "none"}`}>
          <Activity size={14} />
          {formatStatus(task?.status ?? null)}
        </span>
      </div>

      <div className="stage-grid">
        {STAGE_GROUPS.map((group) => {
          const Icon = group.icon;
          const state = stageState(task?.status ?? null, group.statuses);

          return (
            <div className={`stage-card stage-${state}`} key={group.id}>
              <Icon size={16} />
              <span>{group.title}</span>
            </div>
          );
        })}
      </div>

      <div className="telemetry-grid">
        <div>
          <span>Agent calls</span>
          <strong>{successfulInvocations}/{task?.agentInvocations.length ?? 0}</strong>
        </div>
        <div>
          <span>Implementation</span>
          <strong>{completedTodos}/{totalTodos}</strong>
        </div>
        <div>
          <span>Review risk</span>
          <strong>{failedInvocations > 0 ? `${failedInvocations} failed` : "clear"}</strong>
        </div>
        <div>
          <span>Latest run</span>
          <strong>{latestRunSummary(task)}</strong>
        </div>
      </div>
    </div>
  );
}

export function WorkspaceSplit() {
  const { state } = useAppState();
  const task = state.tasks.find((candidate) => candidate.id === state.app.selectedTaskId) ?? null;
  const project = state.projects.current;
  const inExecution =
    task?.status === "ready_to_implement" ||
    task?.status === "implementing" ||
    task?.status === "reviewing" ||
    task?.status === "debugging" ||
    task?.status === "fixing" ||
    task?.status === "verifying";

  if (!project) {
    return (
      <div className="workspace-empty-state redesigned-empty">
        <Radar size={28} />
        <div className="workspace-empty-title">先打开一个项目</div>
        <div className="workspace-empty-copy">
          Loom 会识别项目、加载 Agent，并把计划、互评、人工决策和最终文档记录下来。
        </div>
      </div>
    );
  }

  if (!inExecution) {
    return <PlanningWizard />;
  }

  return (
    <div className="command-center">
      <section className="command-column mission-column">
        <div className="column-label">Mission setup</div>
        <ImplementationPane />
      </section>

      <section className="command-column command-column-main">
        <CommandOverview task={task} />
        <ImplementationOutputPane />
      </section>

      <section className="command-column evidence-column">
        <div className="column-label">Evidence + repair</div>
        <DebugPane />
      </section>
    </div>
  );
}
