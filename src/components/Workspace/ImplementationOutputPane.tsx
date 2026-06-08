import { ClipboardCheck, FileText, Send, ShieldCheck, Terminal } from "lucide-react";
import { useMemo, useState, type FormEvent } from "react";
import { useTaskBridge } from "../../hooks/useTaskBridge";
import { useAppState } from "../../state/AppStateContext";
import { Button } from "../common/Button";
import "./Workspace.css";

function summarizeCommand(command: string) {
  return command.length > 180 ? `${command.slice(0, 180)}...` : command;
}

export function ImplementationOutputPane() {
  const { state } = useAppState();
  const { appendFeedback } = useTaskBridge();
  const project = state.projects.current;
  const task = state.tasks.find((candidate) => candidate.id === state.app.selectedTaskId) ?? null;
  const activeTodo = useMemo(
    () => task?.planTodos.find((todo) => todo.id === state.app.selectedTodoId) ?? task?.planTodos[0] ?? null,
    [state.app.selectedTodoId, task],
  );
  const activeTodoIndex = useMemo(
    () => (activeTodo && task ? task.planTodos.findIndex((todo) => todo.id === activeTodo.id) : -1),
    [activeTodo, task],
  );
  const activeRun = useMemo(() => {
    if (!task) {
      return null;
    }

    return (
      state.commandRuns.find((run) => run.id === state.app.activeCommandRunId) ??
      state.commandRuns
        .filter((run) => run.taskId === task.id)
        .sort((left, right) => right.startedAtMs - left.startedAtMs)[0] ??
      null
    );
  }, [state.app.activeCommandRunId, state.commandRuns, task]);
  const activeRunLogs = useMemo(() => {
    if (!activeRun) {
      return "";
    }

    const logs = state.commandLogs[activeRun.id] ?? [];
    if (logs.length === 0) {
      return activeRun.status === "running"
        ? "Agent process started. Waiting for output..."
        : "No live logs were captured for this run.";
    }

    return logs
      .map((entry) => `${entry.stream.toUpperCase()} ${entry.line}`)
      .join("\n");
  }, [activeRun, state.commandLogs]);
  const reviewChecklist = useMemo(() => {
    if (!task || !activeTodo) {
      return [];
    }

    return [
      `Scope: only implement todo ${activeTodoIndex + 1} and keep unrelated files unchanged.`,
      `Evidence: run the smallest relevant verification and attach command output to the task record.`,
      `Review: compare the diff against ${activeTodo.planRef ?? task.finalPlanPath ?? "the confirmed final plan"}.`,
      "Handoff: record accepted risks, blockers, or the next repair loop before selecting another todo.",
    ];
  }, [activeTodo, activeTodoIndex, task]);
  const [note, setNote] = useState("");

  async function handleGuidanceSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();

    if (!project || !task || !activeTodo || !note.trim()) {
      return;
    }

    const scopedGuidance = `Implementation guidance for todo ${activeTodoIndex + 1}: ${activeTodo.title}\n\n${note.trim()}`;
    const updated = await appendFeedback(project.path, task.id, undefined, scopedGuidance);
    if (updated) {
      setNote("");
    }
  }

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

            <div className="implementation-review-card">
              <div className="implementation-review-header">
                <ShieldCheck size={15} />
                Review handoff checklist
              </div>
              <ol className="implementation-review-list">
                {reviewChecklist.map((item) => (
                  <li key={item}>{item}</li>
                ))}
              </ol>
            </div>

            <pre className="terminal-output implementation-terminal">
{`$ loom-agent --todo "${activeTodo.title}"
reading final plan: ${task?.finalPlanPath ?? "pending"}
execution boundary: todo ${activeTodoIndex + 1}/${task?.planTodos.length ?? 0}
selected todo: ${activeTodo.description}
status: ${activeTodo.status}
review handoff: scope, evidence, diff review, blockers
next checkpoint: run targeted verification and request review`}
            </pre>

            {activeRun && (
              <div className="implementation-review-card">
                <div className="implementation-review-header">
                  <Terminal size={15} />
                  Agent run · {activeRun.status}
                </div>
                <div className="implementation-brief-copy">
                  {summarizeCommand(activeRun.command)}
                  {typeof activeRun.exitCode === "number" ? ` · exit ${activeRun.exitCode}` : ""}
                </div>
                <pre className="terminal-output implementation-terminal">{activeRunLogs}</pre>
              </div>
            )}
          </>
        )}
      </div>

      <form className="implementation-composer" onSubmit={handleGuidanceSubmit}>
        <input
          className="feedback-input"
          value={note}
          onChange={(event) => setNote(event.target.value)}
          placeholder="Add scoped implementation guidance or @review-agent"
          disabled={!project || !task || !activeTodo}
        />
        <Button
          type="submit"
          variant="primary"
          iconRight={<Send size={14} />}
          disabled={!project || !task || !activeTodo || !note.trim()}
        >
          Send
        </Button>
      </form>
    </div>
  );
}
