import { Circle, Play, PlayCircle } from "lucide-react";
import { useAppState } from "../../state/AppStateContext";
import "./Workspace.css";

export function ImplementationPane() {
  const { state, dispatch } = useAppState();
  const task = state.tasks.find((candidate) => candidate.id === state.app.selectedTaskId) ?? null;
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

          return (
            <button
              type="button"
              key={todo.id}
              className={`todo-row${active ? " active" : ""}`}
              onClick={() => {
                if (taskId) {
                  dispatch({ type: "tasks/todoSelected", taskId, todoId: todo.id });
                }
              }}
            >
              <span className="todo-start-icon" title="Start implementation">
                {active ? <PlayCircle size={16} /> : <Play size={16} />}
              </span>
              <span className="todo-copy">
                <span className="todo-title">{todo.title}</span>
                <span className="todo-description">{todo.description}</span>
              </span>
              <Circle size={10} className={`todo-status-dot status-${todo.status}`} />
            </button>
          );
        })}
      </div>
    </div>
  );
}
