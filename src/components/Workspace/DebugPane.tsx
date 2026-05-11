import { useMemo, useState, type FormEvent } from "react";
import { Send, Square, Terminal } from "lucide-react";
import { useCommandBridge } from "../../hooks/useCommandBridge";
import { useTaskBridge } from "../../hooks/useTaskBridge";
import { useAppState } from "../../state/AppStateContext";
import { parseCommandLine, type ParsedCommandLine } from "../../utils/commandLine";
import { Button } from "../common/Button";
import "./Workspace.css";

export function DebugPane() {
  const { state } = useAppState();
  const { startCommandRun, stopCommandRun } = useCommandBridge();
  const { appendFeedback, generateRepairContext } = useTaskBridge();
  const project = state.projects.current;
  const task = state.tasks.find((candidate) => candidate.id === state.app.selectedTaskId) ?? null;
  const activeRun =
    state.commandRuns.find((run) => run.id === state.app.activeCommandRunId) ??
    (task
      ? state.commandRuns
          .filter((run) => run.taskId === task.id)
          .sort((left, right) => right.startedAtMs - left.startedAtMs)[0]
      : undefined);
  const [command, setCommand] = useState("pnpm build");
  const [feedback, setFeedback] = useState("");
  const [commandParseError, setCommandParseError] = useState<string | null>(null);
  const errorSummaryLines = activeRun?.errorSummary
    ? [
        `Error summary${typeof activeRun.errorSummary.exitCode === "number" ? ` (exit ${activeRun.errorSummary.exitCode})` : ""}`,
        ...activeRun.errorSummary.matchedLines.slice(0, 8),
        ...(activeRun.errorSummary.matchedLines.length === 0
          ? activeRun.errorSummary.stderrTail.slice(-8)
          : []),
      ]
    : [];
  const logText = useMemo(() => {
    if (state.commandLogs.length === 0) {
      return "No command logs yet.";
    }

    return state.commandLogs
      .map((entry) => {
        const time = new Date(entry.timestampMs).toLocaleTimeString("en-US", {
          hour: "2-digit",
          minute: "2-digit",
          second: "2-digit",
        });
        return `${time} ${entry.stream.toUpperCase()} ${entry.line}`;
      })
      .join("\n");
  }, [state.commandLogs]);

  async function handleRunCommand(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();

    if (!project || !task || !command.trim()) {
      return;
    }

    let parsed: ParsedCommandLine;
    try {
      parsed = parseCommandLine(command);
      setCommandParseError(null);
    } catch (error) {
      setCommandParseError(error instanceof Error ? error.message : "Invalid command line");
      return;
    }

    if (!parsed.program) {
      return;
    }

    await startCommandRun({
      program: parsed.program,
      args: parsed.args,
      cwd: project.path,
      taskId: task.id,
    });
  }

  async function handleStopCommand() {
    if (activeRun) {
      await stopCommandRun(activeRun.id);
    }
  }

  async function handleFeedbackSubmit() {
    if (!project || !task || !feedback.trim()) {
      return;
    }

    const updated = await appendFeedback(project.path, task.id, activeRun?.id, feedback.trim());
    if (updated) {
      setFeedback("");
      await generateRepairContext(project.path, task.id);
    }
  }

  return (
    <div className="pane-container">
      <div className="pane-header">
        <span className="pane-header-title">DEBUG / LOGS</span>
        <form className="debug-controls" onSubmit={handleRunCommand}>
          <input
            className="command-input"
            value={command}
            onChange={(event) => setCommand(event.target.value)}
            disabled={!project || !task}
          />
          <Button
            type="submit"
            variant="primary"
            iconLeft={<Terminal size={14} />}
            disabled={!project || !task || !command.trim() || activeRun?.status === "running"}
          >
            Run
          </Button>
          <Button
            type="button"
            variant="ghost"
            iconLeft={<Square size={14} />}
            disabled={activeRun?.status !== "running"}
            onClick={handleStopCommand}
          >
            Stop
          </Button>
        </form>
      </div>

      <div className="terminal-container">
        {commandParseError && <div className="terminal-line error-text">{commandParseError}</div>}
        {state.app.commandError && <div className="terminal-line error-text">{state.app.commandError}</div>}
        {activeRun && (
          <div className="terminal-line secondary-text">
            [{activeRun.status}] {activeRun.command}
            {typeof activeRun.exitCode === "number" ? ` exit=${activeRun.exitCode}` : ""}
          </div>
        )}
        {errorSummaryLines.length > 0 && (
          <pre className="terminal-output error-summary-preview">{errorSummaryLines.join("\n")}</pre>
        )}
        <pre className="terminal-output">{logText}</pre>
        {task?.repairContextPreview && (
          <pre className="terminal-output repair-preview">{task.repairContextPreview}</pre>
        )}
      </div>

      <div className="feedback-container">
        <div className="feedback-title">Manual Feedback</div>
        <div className="feedback-input-wrapper">
          <input 
            type="text" 
            className="feedback-input" 
            value={feedback}
            onChange={(event) => setFeedback(event.target.value)}
            placeholder="The database connection is failing because the env var is missing."
          />
          <Button
            type="button"
            variant="primary"
            iconRight={<Send size={14} />}
            className="submit-btn"
            disabled={!project || !task || !feedback.trim()}
            onClick={handleFeedbackSubmit}
          >
            Submit
          </Button>
        </div>
      </div>
    </div>
  );
}
