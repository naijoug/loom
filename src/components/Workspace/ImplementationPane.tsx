import { CheckCircle2, Circle, Play, PlayCircle } from "lucide-react";
import { useTaskBridge } from "../../hooks/useTaskBridge";
import { useAppState } from "../../state/AppStateContext";
import "./Workspace.css";

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

          return (
            <div key={todo.id} className={`todo-row${active ? " active" : ""}`}>
              <button
                type="button"
                className="todo-start-button"
                onClick={() => {
                  if (taskId) {
                    void startTodo(projectPath, taskId, todo.id);
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
              </button>
              <button
                type="button"
                className="todo-complete-button"
                title="Mark todo done after verification evidence is captured"
                disabled={todo.status === "done"}
                onClick={() => {
                  if (taskId) {
                    void completeTodo(projectPath, taskId, todo.id);
                  }
                }}
              >
                {todo.status === "done" ? <CheckCircle2 size={15} /> : <Circle size={10} />}
              </button>
            </div>
          );
        })}
      </div>
    </div>
  );
}
