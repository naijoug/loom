import { ClipboardCheck, FileText, Send, Terminal } from "lucide-react";
import { useMemo, useState } from "react";
import { useAppState } from "../../state/AppStateContext";
import { Button } from "../common/Button";
import "./Workspace.css";

export function ImplementationOutputPane() {
  const { state } = useAppState();
  const task = state.tasks.find((candidate) => candidate.id === state.app.selectedTaskId) ?? null;
  const activeTodo = useMemo(
    () => task?.planTodos.find((todo) => todo.id === state.app.selectedTodoId) ?? task?.planTodos[0] ?? null,
    [state.app.selectedTodoId, task],
  );
  const activeTodoIndex = useMemo(
    () => (activeTodo && task ? task.planTodos.findIndex((todo) => todo.id === activeTodo.id) : -1),
    [activeTodo, task],
  );
  const [note, setNote] = useState("");

  return (
    <div className="pane-container">
      <div className="pane-header">
        <span className="pane-header-title">AGENT OUTPUT</span>
      </div>

      <div className="implementation-output">
        {!activeTodo && (
          <div className="planning-empty">Select a todo item to start implementation.</div>
        )}

        {activeTodo && (
          <>
            <div className="implementation-brief">
              <div className="implementation-brief-header">
                <ClipboardCheck size={15} />
                Todo {activeTodoIndex + 1} of {task?.planTodos.length ?? 0} · {activeTodo.status}
              </div>
              <div className="implementation-brief-title">{activeTodo.title}</div>
              <div className="implementation-brief-copy">{activeTodo.description}</div>
              {(activeTodo.planRef || task?.finalPlanPath) && (
                <div className="implementation-plan-ref">
                  <FileText size={13} />
                  {activeTodo.planRef ?? task?.finalPlanPath}
                </div>
              )}
            </div>

            <div className="conversation-message agent-message">
              <div className="message-author">
                <Terminal size={14} />
                primary agent · ready for scoped execution
              </div>
              <div>
                Use this todo as the execution boundary: implement only the selected slice, capture command
                evidence, then hand off to review before moving to the next item.
              </div>
            </div>

            <pre className="terminal-output implementation-terminal">
{`$ loom-agent --todo "${activeTodo.title}"
reading final plan: ${task?.finalPlanPath ?? "pending"}
execution boundary: todo ${activeTodoIndex + 1}/${task?.planTodos.length ?? 0}
selected todo: ${activeTodo.description}
status: ${activeTodo.status}
next checkpoint: run targeted verification and request review`}
            </pre>
          </>
        )}
      </div>

      <div className="implementation-composer">
        <input
          className="feedback-input"
          value={note}
          onChange={(event) => setNote(event.target.value)}
          placeholder="Add implementation guidance or @review-agent"
        />
        <Button type="button" variant="primary" iconRight={<Send size={14} />} disabled={!note.trim()}>
          Send
        </Button>
      </div>
    </div>
  );
}
