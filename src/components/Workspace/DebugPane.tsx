import { useEffect, useMemo, useState, type FormEvent, type ReactNode } from "react";
import { Send, Square, Terminal, Wrench } from "lucide-react";
import { useCommandBridge } from "../../hooks/useCommandBridge";
import { useTaskBridge } from "../../hooks/useTaskBridge";
import { useAppState } from "../../state/AppStateContext";
import { parseCommandLine, type ParsedCommandLine } from "../../utils/commandLine";
import { Button } from "../common/Button";
import "./Workspace.css";

function renderHighlightedLogLine(line: string, normalizedFilter: string): ReactNode {
  if (!normalizedFilter) {
    return line;
  }

  const lowerLine = line.toLowerCase();
  const segments: ReactNode[] = [];
  let cursor = 0;
  let matchIndex = lowerLine.indexOf(normalizedFilter, cursor);

  while (matchIndex !== -1) {
    if (matchIndex > cursor) {
      segments.push(line.slice(cursor, matchIndex));
    }

    const matchEnd = matchIndex + normalizedFilter.length;
    segments.push(
      <mark className="log-filter-highlight" key={`${matchIndex}-${matchEnd}`}>
        {line.slice(matchIndex, matchEnd)}
      </mark>,
    );
    cursor = matchEnd;
    matchIndex = lowerLine.indexOf(normalizedFilter, cursor);
  }

  if (cursor < line.length) {
    segments.push(line.slice(cursor));
  }

  return segments;
}

function countTextMatches(line: string, normalizedFilter: string): number {
  if (!normalizedFilter) {
    return 0;
  }

  const lowerLine = line.toLowerCase();
  let count = 0;
  let cursor = 0;
  let matchIndex = lowerLine.indexOf(normalizedFilter, cursor);

  while (matchIndex !== -1) {
    count += 1;
    cursor = matchIndex + normalizedFilter.length;
    matchIndex = lowerLine.indexOf(normalizedFilter, cursor);
  }

  return count;
}

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
  const [logFilter, setLogFilter] = useState("");
  const [logStreamFilter, setLogStreamFilter] = useState<"all" | "stdout" | "stderr">("all");
  const [commandParseError, setCommandParseError] = useState<string | null>(null);

  useEffect(() => {
    setLogFilter("");
    setLogStreamFilter("all");
  }, [activeRun?.id]);

  const errorSummaryLines = activeRun?.errorSummary
    ? [
        `Error summary${typeof activeRun.errorSummary.exitCode === "number" ? ` (exit ${activeRun.errorSummary.exitCode})` : ""}`,
        ...activeRun.errorSummary.matchedLines.slice(0, 8),
        ...(activeRun.errorSummary.matchedLines.length === 0
          ? activeRun.errorSummary.stderrTail.slice(-8)
          : []),
      ]
    : [];
  const scopedLogs = useMemo(() => {
    if (!activeRun) {
      return state.commandLogs;
    }

    return state.commandLogs.filter((entry) => entry.runId === activeRun.id);
  }, [activeRun, state.commandLogs]);

  const visibleLogs = useMemo(() => {
    const normalizedFilter = logFilter.trim().toLowerCase();

    return scopedLogs.filter((entry) => {
      const matchesStream = logStreamFilter === "all" || entry.stream === logStreamFilter;
      const matchesText = !normalizedFilter || entry.line.toLowerCase().includes(normalizedFilter);
      return matchesStream && matchesText;
    });
  }, [logFilter, logStreamFilter, scopedLogs]);

  const textMatchCount = useMemo(() => {
    const normalizedFilter = logFilter.trim().toLowerCase();

    if (!normalizedFilter) {
      return 0;
    }

    return visibleLogs.reduce(
      (total, entry) => total + countTextMatches(entry.line, normalizedFilter),
      0,
    );
  }, [logFilter, visibleLogs]);

  const hasActiveLogFilters = logFilter.trim().length > 0 || logStreamFilter !== "all";

  const logText = useMemo<ReactNode>(() => {
    if (scopedLogs.length === 0) {
      return activeRun ? "No logs for the selected command run yet." : "No command logs yet.";
    }

    if (visibleLogs.length === 0) {
      const textFilter = logFilter.trim();
      const streamLabel = logStreamFilter === "all" ? "any stream" : logStreamFilter.toUpperCase();
      return textFilter
        ? `No ${streamLabel} log lines match "${textFilter}".`
        : `No ${streamLabel} log lines for the selected command run.`;
    }

    const normalizedFilter = logFilter.trim().toLowerCase();

    return visibleLogs.map((entry, index) => {
      const time = new Date(entry.timestampMs).toLocaleTimeString("en-US", {
        hour: "2-digit",
        minute: "2-digit",
        second: "2-digit",
      });
      const prefix = `${time} ${entry.stream.toUpperCase()} `;

      return (
        <span key={`${entry.runId}-${entry.timestampMs}-${entry.stream}-${index}`}>
          {prefix}
          {renderHighlightedLogLine(entry.line, normalizedFilter)}
          {index < visibleLogs.length - 1 ? "\n" : null}
        </span>
      );
    });
  }, [activeRun, logFilter, logStreamFilter, scopedLogs.length, visibleLogs]);

  const logFilterSummary = useMemo(() => {
    const streamLabel = logStreamFilter === "all" ? "all streams" : logStreamFilter.toUpperCase();
    const lineSummary = `${visibleLogs.length}/${scopedLogs.length} ${streamLabel}`;

    if (!logFilter.trim()) {
      return lineSummary;
    }

    return `${lineSummary} · ${textMatchCount} ${textMatchCount === 1 ? "match" : "matches"}`;
  }, [logFilter, logStreamFilter, scopedLogs.length, textMatchCount, visibleLogs.length]);

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

  async function handleGenerateRepairContext() {
    if (!project || !task) {
      return;
    }

    await generateRepairContext(project.path, task.id);
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
        <div className="log-filter-row">
          <input
            type="search"
            className="log-filter-input"
            value={logFilter}
            onChange={(event) => setLogFilter(event.target.value)}
            placeholder="Filter visible logs by text"
            aria-label="Filter command logs by text"
          />
          <select
            className="log-filter-select"
            value={logStreamFilter}
            onChange={(event) => setLogStreamFilter(event.target.value as "all" | "stdout" | "stderr")}
            aria-label="Filter command logs by stream"
          >
            <option value="all">All streams</option>
            <option value="stdout">STDOUT</option>
            <option value="stderr">STDERR</option>
          </select>
          <button
            type="button"
            className="log-filter-clear"
            disabled={!hasActiveLogFilters}
            onClick={() => {
              setLogFilter("");
              setLogStreamFilter("all");
            }}
          >
            Clear
          </button>
          <span className="log-filter-summary">{logFilterSummary}</span>
        </div>
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
        <div className="feedback-header">
          <div>
            <div className="feedback-title">Manual Feedback</div>
            <div className="feedback-subtitle">
              Package the latest failed command, selected todo, and feedback for the next Agent fix.
            </div>
          </div>
          <Button
            type="button"
            variant="ghost"
            iconLeft={<Wrench size={14} />}
            disabled={!project || !task}
            onClick={handleGenerateRepairContext}
          >
            Repair Handoff
          </Button>
        </div>
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
