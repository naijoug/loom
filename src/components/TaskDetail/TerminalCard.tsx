import { Square, Terminal } from "lucide-react";
import type { CommandLogEvent, CommandRun } from "../../domain";
import { Button } from "../common/Button";

interface TerminalCardProps {
  title: string;
  command: string;
  endpoint: string;
  run?: CommandRun;
  logs: CommandLogEvent[];
  tone: "frontend" | "backend";
  onRun: () => void;
  onStop: () => void;
  disabled?: boolean;
}

function statusText(run?: CommandRun) {
  if (!run) {
    return "idle";
  }

  if (typeof run.exitCode === "number") {
    return `${run.status} ${run.exitCode}`;
  }

  return run.status;
}

function statusClass(run?: CommandRun) {
  if (!run) {
    return "idle";
  }

  if (run.status === "succeeded" || run.status === "running") {
    return "ok";
  }

  if (run.status === "failed" || run.status === "cancelled") {
    return "err";
  }

  return "idle";
}

function streamClass(stream: CommandLogEvent["stream"]) {
  return stream === "stderr" ? "terminal-log-error" : "terminal-log-normal";
}

export function TerminalCard({
  title,
  command,
  endpoint,
  run,
  logs,
  tone,
  onRun,
  onStop,
  disabled,
}: TerminalCardProps) {
  const running = run?.status === "running";

  return (
    <section className="testing-terminal-card">
      <div className="testing-terminal-header">
        <span className={`testing-terminal-dot terminal-dot-${tone}`} />
        <span className="testing-terminal-title">{title}</span>
        <span className="testing-terminal-command">{command}</span>
        <span className="testing-terminal-endpoint">{endpoint}</span>
        <span className={`testing-status-pill testing-status-${statusClass(run)}`}>
          <span />
          {statusText(run)}
        </span>
        <Button
          type="button"
          variant="ghost"
          iconLeft={running ? <Square size={13} /> : <Terminal size={13} />}
          disabled={disabled}
          onClick={running ? onStop : onRun}
        >
          {running ? "Stop" : "Run"}
        </Button>
      </div>
      <div className="testing-terminal-body">
        {logs.length === 0 ? (
          <div className="testing-terminal-empty">
            No logs yet. Start this command to stream output into this terminal.
          </div>
        ) : (
          logs.map((entry, index) => (
            <div
              className={`testing-terminal-line ${streamClass(entry.stream)}`}
              key={`${entry.runId}-${entry.timestampMs}-${entry.stream}-${index}`}
            >
              <span>{entry.stream.toUpperCase()}</span>
              <code>{entry.line}</code>
            </div>
          ))
        )}
      </div>
    </section>
  );
}
