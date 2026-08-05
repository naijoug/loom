import { ChevronDown, Clock3 } from "lucide-react";
import type { CommandLogEvent, CommandRun } from "../../domain";
import { cycleLabel } from "./model";

interface RepairCycleHistoryProps {
  runs: CommandRun[];
  logs: Record<string, CommandLogEvent[]>;
  open: boolean;
  expandedRunId: string | null;
  onToggleOpen: () => void;
  onToggleRun: (runId: string) => void;
}

export function RepairCycleHistory({
  runs,
  logs,
  open,
  expandedRunId,
  onToggleOpen,
  onToggleRun,
}: RepairCycleHistoryProps) {
  return (
    <section className="debug-card">
      <button
        type="button"
        className="debug-card-label testing-cycles-toggle"
        aria-expanded={open}
        onClick={onToggleOpen}
      >
        <Clock3 size={14} />
        测试循环
        <span className="testing-cycles-count">{runs.length}</span>
        <ChevronDown size={14} className={`testing-cycles-chevron${open ? " open" : ""}`} />
      </button>
      {open && (
        <div className="testing-cycle-list">
          {runs.length === 0 ? (
            <span>暂无命令循环。</span>
          ) : (
            runs.map((run, index) => {
              const expanded = expandedRunId === run.id;
              const cycleLogs = logs[run.id] ?? [];
              return (
                <div className={`testing-cycle cycle-${run.status}`} key={run.id}>
                  <span />
                  <div className="testing-cycle-body">
                    <button
                      type="button"
                      className="testing-cycle-toggle"
                      aria-expanded={expanded}
                      onClick={() => onToggleRun(run.id)}
                    >
                      <strong>{cycleLabel(run, index)}</strong>
                      <p>{run.errorSummary?.matchedLines[0] ?? run.command}</p>
                    </button>
                    {expanded && (
                      <pre className="testing-cycle-log">
                        {cycleLogs.length > 0
                          ? cycleLogs.map((entry) => entry.line).join("\n")
                          : "No captured log lines (live terminal output streams to the terminal, not the cycle log)."}
                      </pre>
                    )}
                  </div>
                </div>
              );
            })
          )}
        </div>
      )}
    </section>
  );
}
