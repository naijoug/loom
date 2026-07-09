import { useMemo } from "react";
import {
  AlertCircle,
  CheckCircle2,
  Circle,
  Filter,
  ListTodo,
  Play,
  Plus,
  Radio,
} from "lucide-react";
import type { Task, TaskStatus } from "../../domain";
import { useTaskBridge } from "../../hooks/useTaskBridge";
import { useAppState } from "../../state/AppStateContext";
import { Button } from "../common/Button";
import "./Board.css";

interface BoardGroup {
  id: string;
  title: string;
  statuses: TaskStatus[];
  icon: typeof Circle;
  tone: "progress" | "todo" | "testing" | "blocked" | "done";
}

const BOARD_GROUPS: BoardGroup[] = [
  {
    id: "in-progress",
    title: "进行中",
    statuses: ["implementing", "reviewing"],
    icon: Radio,
    tone: "progress",
  },
  {
    id: "todo",
    title: "待办",
    statuses: ["drafting_requirements", "planning", "plan_review", "ready_to_implement"],
    icon: ListTodo,
    tone: "todo",
  },
  {
    id: "testing",
    title: "测试中",
    statuses: ["debugging", "fixing", "verifying"],
    icon: AlertCircle,
    tone: "testing",
  },
  {
    id: "blocked",
    title: "阻塞",
    statuses: ["blocked", "cancelled"],
    icon: AlertCircle,
    tone: "blocked",
  },
  {
    id: "done",
    title: "已完成",
    statuses: ["completed"],
    icon: CheckCircle2,
    tone: "done",
  },
];

function taskShortId(task: Task) {
  // Fixed short ids for the standalone planning preview fixtures only; real
  // tasks fall through to the id-derived path below.
  const previewIds: Record<string, string> = {
    "task-preview-implementing": "LOOM-12",
    "task-preview-planning": "LOOM-13",
    "task-preview-testing": "LOOM-08",
    "task-preview-done": "LOOM-10",
  };
  if (previewIds[task.id]) {
    return previewIds[task.id];
  }

  const parts = task.id.split("-");
  if (parts[0] === "loom" || parts[0] === "spk") {
    return task.id.toUpperCase();
  }
  if (parts.length >= 3) {
    return parts.slice(0, 2).join("-").toUpperCase();
  }

  return task.id.toUpperCase();
}

function agentInitial(task: Task) {
  if (!task.primaryAgentId) {
    return "";
  }
  if (task.primaryAgentId.includes("codex")) {
    return "Co";
  }
  if (task.primaryAgentId.includes("claude")) {
    return "Cl";
  }
  return task.primaryAgentId.slice(0, 2).toUpperCase();
}

function agentClass(task: Task) {
  if (!task.primaryAgentId) {
    return "empty";
  }
  if (task.primaryAgentId.includes("codex")) {
    return "codex";
  }
  if (task.primaryAgentId.includes("claude")) {
    return "claude";
  }
  if (task.primaryAgentId.includes("hermes")) {
    return "hermes";
  }
  return "other";
}

function statusTone(status: TaskStatus) {
  if (status === "completed") {
    return "done";
  }
  if (status === "blocked" || status === "cancelled") {
    return "blocked";
  }
  if (status === "debugging" || status === "fixing" || status === "verifying") {
    return "testing";
  }
  if (status === "implementing" || status === "reviewing") {
    return "progress";
  }
  return "todo";
}

function statusLabel(status: TaskStatus) {
  const labels: Record<TaskStatus, string> = {
    drafting_requirements: "需求",
    planning: "规划",
    plan_review: "评审",
    ready_to_implement: "可实施",
    implementing: "实施",
    reviewing: "复核",
    debugging: "调试",
    fixing: "修复",
    verifying: "验收",
    completed: "完成",
    blocked: "阻塞",
    cancelled: "取消",
  };
  return labels[status];
}

function priorityLevel(task: Task) {
  if (task.status === "blocked" || task.status === "cancelled") {
    return "high";
  }
  if (task.status === "debugging" || task.status === "fixing") {
    return "medium";
  }
  if (task.status === "implementing" || task.status === "reviewing") {
    return "medium";
  }
  return "low";
}

function runnableTodo(task: Task) {
  return (
    task.planTodos.find((todo) => todo.status === "pending") ??
    task.planTodos.find((todo) => todo.status === "implementing") ??
    null
  );
}

export function Board() {
  const { state, dispatch } = useAppState();
  const { startTodo } = useTaskBridge();
  const project = state.projects.current;
  const groupedTasks = useMemo(
    () =>
      BOARD_GROUPS.map((group) => ({
        ...group,
        tasks: state.tasks.filter((task) => group.statuses.includes(task.status)),
      })),
    [state.tasks],
  );
  const taskCount = groupedTasks.reduce((count, group) => count + group.tasks.length, 0);

  async function handleRunTask(task: Task) {
    const todo = runnableTodo(task);
    if (!project || !todo) {
      return;
    }

    await startTodo(project.path, task.id, todo.id, task.primaryAgentId);
    dispatch({ type: "tasks/selected", taskId: task.id });
  }

  if (!project) {
    return (
      <div className="board-empty">
        <ListTodo size={28} />
        <h2>Select a project</h2>
        <p>Open a local project to see its task board.</p>
      </div>
    );
  }

  return (
    <div className="task-board">
      <section className="board-panel board">
        <div className="board-toolbar">
          <div className="board-title-row">
            <div>
              <h1>任务看板</h1>
              <span className="board-sub">{project.name} · {taskCount} 个任务</span>
            </div>
          </div>
          <div className="board-toolbar-actions">
            <div className="board-agent-filter" aria-label="Agent filters">
              <span className="loom-agent-avatar codex" title="Codex">Co</span>
              <span className="loom-agent-avatar claude" title="Claude Code">Cl</span>
              <span className="loom-agent-avatar hermes" title="Hermes">He</span>
            </div>
            <Button variant="ghost" iconLeft={<Filter size={14} />}>
              筛选
            </Button>
            <Button
              variant="ghost"
              iconLeft={<Plus size={14} />}
              onClick={() => dispatch({ type: "tasks/new" })}
            >
              新建任务
            </Button>
          </div>
        </div>

        <div className="board-groups">
          {groupedTasks.map((group) => {
            const Icon = group.icon;

            return (
              <section className="board-group" key={group.id}>
                <div className="board-group-header">
                  <div className={`board-group-title group-${group.tone}`}>
                    <Icon size={15} />
                    <span>{group.title}</span>
                    <span className="board-count">{group.tasks.length}</span>
                  </div>
                </div>

                <div className="board-task-list">
                  {group.tasks.map((task) => {
                    const todo = runnableTodo(task);
                    const canRun = task.status === "ready_to_implement" && Boolean(todo);

                    return (
                      <div className="board-task-row" key={task.id}>
                        <button
                          type="button"
                          className="board-task-open"
                          onClick={() => dispatch({ type: "tasks/selected", taskId: task.id })}
                        >
                          <span className={`board-status-dot status-${statusTone(task.status)}`} />
                          <span className="board-task-id">{taskShortId(task)}</span>
                          <span className={`board-priority priority-${priorityLevel(task)}`} aria-hidden="true">
                            <i />
                            <i />
                            <i />
                          </span>
                          <span className="board-task-title">{task.title}</span>
                          <span className={`board-tag tag-${statusTone(task.status)}`}>
                            {statusLabel(task.status)}
                          </span>
                          <span className={`loom-agent-avatar board-agent ${agentClass(task)}`}>
                            {agentInitial(task)}
                          </span>
                        </button>
                        {canRun && (
                          <button
                            type="button"
                            className="board-run-button"
                            onClick={() => void handleRunTask(task)}
                          >
                            <Play size={13} />
                            Run
                          </button>
                        )}
                      </div>
                    );
                  })}
                  {group.tasks.length === 0 && (
                    <div className="board-empty-group">暂无任务</div>
                  )}
                </div>
              </section>
            );
          })}
        </div>
      </section>
    </div>
  );
}
