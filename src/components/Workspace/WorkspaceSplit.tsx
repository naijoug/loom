import {
  Activity,
  Bot,
  ClipboardCheck,
  GitBranch,
  ListChecks,
  MessagesSquare,
  Radar,
  ShieldCheck,
  TerminalSquare,
} from "lucide-react";
import { ImplementationPane } from "./ImplementationPane";
import { ImplementationOutputPane } from "./ImplementationOutputPane";
import { PlanningPane } from "./PlanningPane";
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

function DecisionLedger({ task }: { task: Task | null }) {
  if (!task) {
    return (
      <div className="decision-ledger empty-ledger">
        <Radar size={18} />
        <div>
          <h3>Planning room is idle</h3>
          <p>Describe a goal on the left to invite planning Agents and build the implementation brief.</p>
        </div>
      </div>
    );
  }

  const recentInvocations = task.agentInvocations.slice(-4).reverse();

  return (
    <div className="decision-ledger">
      <div className="ledger-section">
        <div className="ledger-title">
          <ListChecks size={15} />
          Consensus brief
        </div>
        <p>{task.discussionSummary ?? "Waiting for Agents to compare plans, conflicts, risks, and open questions."}</p>
        {task.finalPlanPath && (
          <div className="ledger-ref">
            <GitBranch size={13} />
            {task.finalPlanPath}
          </div>
        )}
      </div>

      <div className="ledger-section">
        <div className="ledger-title">
          <Bot size={15} />
          Recent Agent signals
        </div>
        {recentInvocations.length === 0 && (
          <p>No planning Agent output has been captured for this task yet.</p>
        )}
        {recentInvocations.map((invocation) => (
          <div className="agent-signal" key={invocation.id}>
            <span>{invocation.agentName}</span>
            <strong>{invocation.status}</strong>
            <p>{invocation.outputSummary || invocation.promptSummary}</p>
          </div>
        ))}
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
        <div className="workspace-empty-title">Open a project to start orchestration</div>
        <div className="workspace-empty-copy">
          Loom will map the stack, surface Agent options, and keep every command, review, and repair loop traceable.
        </div>
      </div>
    );
  }

  return (
    <div className="command-center">
      <section className="command-column mission-column">
        <div className="column-label">Mission setup</div>
        {inExecution ? <ImplementationPane /> : <PlanningPane />}
      </section>

      <section className="command-column command-column-main">
        <CommandOverview task={task} />
        {inExecution ? <ImplementationOutputPane /> : <DecisionLedger task={task} />}
      </section>

      <section className="command-column evidence-column">
        <div className="column-label">Evidence + repair</div>
        <DebugPane />
      </section>
    </div>
  );
}
