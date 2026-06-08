import { useMemo, useState } from "react";
import {
  AlertCircle,
  CheckCircle2,
  Circle,
  Filter,
  ListTodo,
  Play,
  Plus,
  Radio,
  SlidersHorizontal,
} from "lucide-react";
import type { Task, TaskStatus } from "../../domain";
import { useTaskBridge } from "../../hooks/useTaskBridge";
import { useAppState } from "../../state/AppStateContext";
import { Button } from "../common/Button";
import { NewTaskModal } from "./NewTaskModal";
import "./Board.css";

interface BoardGroup {
  id: string;
  title: string;
  statuses: TaskStatus[];
  icon: typeof Circle;
}

const BOARD_GROUPS: BoardGroup[] = [
  {
    id: "in-progress",
    title: "In Progress",
    statuses: ["implementing", "reviewing"],
    icon: Radio,
  },
  {
    id: "todo",
    title: "Todo",
    statuses: ["drafting_requirements", "planning", "plan_review", "ready_to_implement"],
    icon: ListTodo,
  },
  {
    id: "testing",
    title: "Testing",
    statuses: ["debugging", "fixing", "verifying"],
    icon: AlertCircle,
  },
  {
    id: "blocked",
    title: "Blocked",
    statuses: ["blocked", "cancelled"],
    icon: AlertCircle,
  },
  {
    id: "done",
    title: "Done",
    statuses: ["completed"],
    icon: CheckCircle2,
  },
];

function taskShortId(task: Task) {
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

function taskTag(task: Task) {
  if (task.primaryAgentId?.includes("claude")) {
    return "backend";
  }
  if (task.primaryAgentId?.includes("codex")) {
    return "ui";
  }
  if (task.status === "blocked") {
    return "perf";
  }
  if (task.status === "completed") {
    return "state";
  }
  return task.status === "debugging" || task.status === "verifying" || task.status === "fixing"
    ? "test"
    : "ui";
}

function agentInitial(task: Task) {
  if (!task.primaryAgentId) {
    return "·";
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
  return "other";
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
  const [newTaskOpen, setNewTaskOpen] = useState(false);
  const groupedTasks = useMemo(
    () =>
      BOARD_GROUPS.map((group) => ({
        ...group,
        tasks: state.tasks.filter((task) => group.statuses.includes(task.status)),
      })).filter((group) => group.tasks.length > 0),
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
      <section className="board-panel">
        <div className="board-toolbar">
          <div className="board-title-row">
            <h1>{project.name === "speaker" ? "Children Three Kingdoms · audio" : "Add log search & filter"}</h1>
            <span className="board-chip">{taskCount} tasks</span>
          </div>
          <div className="board-toolbar-actions">
            <span className="board-view-toggle">
              <b>List</b>
              <span>Board</span>
            </span>
            <button type="button" className="board-chip board-toolbar-button">
              <Filter size={13} />
              Filter
            </button>
            <button type="button" className="board-chip board-toolbar-button">
              <SlidersHorizontal size={13} />
              Status
            </button>
            <Button variant="ghost" iconLeft={<Plus size={14} />} onClick={() => setNewTaskOpen(true)}>
              New task
            </Button>
          </div>
        </div>

        <div className="board-groups">
          {groupedTasks.map((group) => {
            const Icon = group.icon;

            return (
              <section className="board-group" key={group.id}>
                <div className="board-group-header">
                  <div className="board-group-title">
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
                      <button
                        type="button"
                        className="board-task-row"
                        key={task.id}
                        onClick={() => dispatch({ type: "tasks/selected", taskId: task.id })}
                      >
                        <span className={`board-status-dot status-${task.status}`} />
                        <span className="board-task-id">{taskShortId(task)}</span>
                        <span className="board-priority" aria-hidden="true">
                          <i />
                          <i />
                          <i />
                        </span>
                        <span className="board-task-title">{task.title}</span>
                        <span className="board-task-spacer" />
                        <span className="board-task-meta">
                          <span className="board-tag">{taskTag(task)}</span>
                          {canRun && (
                            <span
                              className="board-run-button"
                              role="button"
                              tabIndex={0}
                              onClick={(event) => {
                                event.stopPropagation();
                                void handleRunTask(task);
                              }}
                              onKeyDown={(event) => {
                                if (event.key === "Enter" || event.key === " ") {
                                  event.preventDefault();
                                  event.stopPropagation();
                                  void handleRunTask(task);
                                }
                              }}
                            >
                              <Play size={13} />
                              Run
                            </span>
                          )}
                          <span className={`board-agent ${agentClass(task)}`}>
                            {agentInitial(task)}
                          </span>
                        </span>
                      </button>
                    );
                  })}
                </div>
              </section>
            );
          })}
        </div>
      </section>

      {newTaskOpen && <NewTaskModal project={project} onClose={() => setNewTaskOpen(false)} />}
    </div>
  );
}
