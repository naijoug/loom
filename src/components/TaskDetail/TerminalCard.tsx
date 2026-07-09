import { useEffect, useRef, type MutableRefObject } from "react";
import { listen } from "@tauri-apps/api/event";
import { MessageSquarePlus, Pencil, Square, Terminal as TerminalIcon, X } from "lucide-react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import type { CommandLogEvent, CommandRun, PtyOutputEvent } from "../../domain";
import { usePtyBridge } from "../../hooks/usePtyBridge";
import { Button } from "../common/Button";

// "pty" → live dev-server in a real terminal (xterm, streams loom://pty-output);
// "logs" → one-shot piped command whose stdout/stderr lines drive errorSummary.
type TerminalMode = "pty" | "logs";

interface TerminalCardProps {
  title: string;
  command: string;
  endpoint: string;
  run?: CommandRun;
  mode: TerminalMode;
  logs?: CommandLogEvent[];
  tone: "frontend" | "validation";
  emptyMessage: string;
  onRun: () => void;
  onStop: () => void;
  onEdit?: () => void;
  onRemove?: () => void;
  onQuote?: (text: string, command: string) => void;
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

function formatTime(timestampMs?: number) {
  if (!timestampMs) {
    return "not finished";
  }

  return new Date(timestampMs).toLocaleTimeString("en-US", {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

/** xterm-backed body for the live PTY (Preview) slot. */
function PtyTerminal({
  run,
  selectionRef,
}: {
  run?: CommandRun;
  selectionRef: MutableRefObject<(() => string) | null>;
}) {
  const { writePty, resizePty } = usePtyBridge();
  const containerRef = useRef<HTMLDivElement>(null);
  const termRef = useRef<Terminal | null>(null);
  // Latest run id / running flag for the long-lived output listener + handlers.
  const runIdRef = useRef<string | undefined>(run?.id);
  const runningRef = useRef(false);
  const running = run?.status === "running";

  useEffect(() => {
    runIdRef.current = run?.id;
  }, [run?.id]);

  useEffect(() => {
    runningRef.current = running;
  }, [running]);

  // Create the xterm instance once and wire input / resize / output streaming.
  useEffect(() => {
    if (!containerRef.current) {
      return;
    }

    const term = new Terminal({
      convertEol: false,
      cursorBlink: false,
      fontFamily: '"Geist Mono", ui-monospace, SFMono-Regular, Menlo, monospace',
      fontSize: 12,
      scrollback: 5000,
      theme: { background: "#15181d", foreground: "#e6e9ef" },
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(containerRef.current);
    try {
      fit.fit();
    } catch {
      // container may be 0-sized on first paint; ResizeObserver re-fits
    }
    termRef.current = term;
    selectionRef.current = () => term.getSelection();

    const dataSub = term.onData((data) => {
      if (runIdRef.current && runningRef.current) {
        void writePty(runIdRef.current, data);
      }
    });

    const observer = new ResizeObserver(() => {
      try {
        fit.fit();
      } catch {
        return;
      }
      if (runIdRef.current && runningRef.current) {
        void resizePty(runIdRef.current, term.rows, term.cols);
      }
    });
    observer.observe(containerRef.current);

    const unlisten = listen<PtyOutputEvent>("loom://pty-output", (event) => {
      if (event.payload.runId === runIdRef.current) {
        term.write(new Uint8Array(event.payload.bytes));
      }
    });

    return () => {
      dataSub.dispose();
      observer.disconnect();
      void unlisten.then((remove) => remove());
      term.dispose();
      termRef.current = null;
      selectionRef.current = null;
    };
  }, [writePty, resizePty, selectionRef]);

  // On a fresh run, clear the previous output and sync the pty to the fitted size.
  useEffect(() => {
    if (!running || !run?.id || !termRef.current) {
      return;
    }
    termRef.current.clear();
    void resizePty(run.id, termRef.current.rows, termRef.current.cols);
  }, [run?.id, running, resizePty]);

  return <div ref={containerRef} className="testing-terminal-xterm" />;
}

export function TerminalCard({
  title,
  command,
  endpoint,
  run,
  mode,
  logs = [],
  tone,
  emptyMessage,
  onRun,
  onStop,
  onEdit,
  onRemove,
  onQuote,
  disabled,
}: TerminalCardProps) {
  const running = run?.status === "running";
  const ptySelectionRef = useRef<(() => string) | null>(null);

  function handleQuote() {
    const text =
      mode === "pty"
        ? ptySelectionRef.current?.() ?? ""
        : window.getSelection()?.toString() ?? "";
    onQuote?.(text, command);
  }

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
          iconLeft={running ? <Square size={13} /> : <TerminalIcon size={13} />}
          disabled={disabled}
          onClick={running ? onStop : onRun}
        >
          {running ? "停止" : "运行"}
        </Button>
        {onQuote && (
          <button
            type="button"
            className="testing-terminal-icon-btn"
            title="Quote selected log lines to the agent"
            onClick={handleQuote}
          >
            <MessageSquarePlus size={13} />
          </button>
        )}
        {onEdit && (
          <button
            type="button"
            className="testing-terminal-icon-btn"
            title="Edit terminal"
            disabled={disabled || running}
            onClick={onEdit}
          >
            <Pencil size={13} />
          </button>
        )}
        {onRemove && (
          <button
            type="button"
            className="testing-terminal-icon-btn"
            title="Remove terminal"
            disabled={disabled || running}
            onClick={onRemove}
          >
            <X size={13} />
          </button>
        )}
      </div>
      <div className={`testing-terminal-body${mode === "pty" ? " is-pty" : ""}`}>
        {mode === "pty" ? (
          <PtyTerminal run={run} selectionRef={ptySelectionRef} />
        ) : (
          <>
            {run && (
              <div className={`testing-terminal-run-meta meta-${statusClass(run)}`}>
                <span>{statusText(run)}</span>
                <span>started {formatTime(run.startedAtMs)}</span>
                <span>ended {formatTime(run.endedAtMs)}</span>
              </div>
            )}
            {logs.length === 0 ? (
              <div className="testing-terminal-empty">{emptyMessage}</div>
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
          </>
        )}
      </div>
    </section>
  );
}
