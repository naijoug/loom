import { CheckCircle2, Circle, Play, PlayCircle } from "lucide-react";
import type { PlanTodoStatus } from "../../domain/task";
import { useTaskBridge } from "../../hooks/useTaskBridge";
import { useAppState } from "../../state/AppStateContext";
import "./Workspace.css";

const TODO_STATUS_LABELS: Record<PlanTodoStatus, string> = {
  pending: "Pending",
  implementing: "Implementing",
  done: "Done",
  blocked: "Blocked",
};

export function ImplementationPane() {
  const { state } = useAppState();
  const { startTodo, completeTodo } = useTaskBridge();
  const task = state.tasks.find((candidate) => candidate.id === state.app.selectedTaskId) ?? null;
  const projectPath = state.projects.current?.path ?? null;
  const taskId = task?.id ?? null;
  const todos = task?.planTodos ?? [];

  return (
    <div className="pane-container">
      <div className="pane-header">
        <span className="pane-header-title">IMPLEMENTATION TODO</span>
      </div>

      <div className="todo-list-pane">
        {todos.length === 0 && (
          <div className="planning-empty">Confirm a plan to generate implementation todo items.</div>
        )}

        {todos.map((todo) => {
          const active = todo.id === state.app.selectedTodoId || todo.status === "implementing";
          const done = todo.status === "done";
          const blocked = todo.status === "blocked";

          return (
            <div
              key={todo.id}
              className={`todo-row status-${todo.status}${active ? " active" : ""}`}
            >
              <button
                type="button"
                className="todo-start-button"
                disabled={done}
                title={done ? "Todo already completed" : "Start implementation"}
                onClick={() => {
                  if (taskId && !done) {
                    void startTodo(projectPath, taskId, todo.id);
                  }
                }}
              >
                <span className="todo-start-icon" aria-hidden="true">
                  {active ? <PlayCircle size={16} /> : <Play size={16} />}
                </span>
                <span className="todo-copy">
                  <span className="todo-title-line">
                    <span className="todo-title">{todo.title}</span>
                    <span className="todo-status-label">{TODO_STATUS_LABELS[todo.status]}</span>
                  </span>
                  <span className="todo-description">{todo.description}</span>
                </span>
              </button>
              <button
                type="button"
                className="todo-complete-button"
                title={
                  done
                    ? "Todo already completed"
                    : blocked
                      ? "Mark blocked todo done after resolving and verifying it"
                      : "Mark todo done after verification evidence is captured"
                }
                disabled={done}
                onClick={() => {
                  if (taskId) {
                    void completeTodo(projectPath, taskId, todo.id);
                  }
                }}
              >
                {done ? <CheckCircle2 size={15} /> : <Circle size={10} />}
              </button>
            </div>
          );
        })}
      </div>
    </div>
  );
}
