import { Send, Terminal } from "lucide-react";
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
            <div className="conversation-message agent-message">
              <div className="message-author">
                <Terminal size={14} />
                codex · implementing
              </div>
              <div>
                Preparing implementation for "{activeTodo.title}". This pane will stream the primary Agent
                output, command evidence, and review notes for the selected todo.
              </div>
            </div>

            <pre className="terminal-output implementation-terminal">
{`$ run-agent --todo "${activeTodo.title}"
reading final plan...
selected todo: ${activeTodo.description}
status: ${activeTodo.status}`}
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
