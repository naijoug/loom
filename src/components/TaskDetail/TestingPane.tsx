import { useMemo, useState } from "react";
import {
  AlertTriangle,
  CheckCircle2,
  ClipboardCheck,
  RefreshCw,
  Send,
  Wrench,
} from "lucide-react";
import type { CommandRun, ProjectSummary, Task } from "../../domain";
import { useCommandBridge } from "../../hooks/useCommandBridge";
import { useTaskBridge } from "../../hooks/useTaskBridge";
import { useAppState } from "../../state/AppStateContext";
import { parseCommandLine } from "../../utils/commandLine";
import { Button } from "../common/Button";
import { TerminalCard } from "./TerminalCard";
import "./TaskDetail.css";

interface TestingPaneProps {
  project: ProjectSummary;
  task: Task;
}

interface CommandSlot {
  id: "frontend" | "backend";
  title: string;
  command: string;
  endpoint: string;
  tone: "frontend" | "backend";
}

function preferredFrontendCommand(project: ProjectSummary) {
  return (
    project.suggestedCommands.find((command) =>
      /\b(pnpm|npm|yarn)\s+(dev|run\s+dev)\b/.test(command),
    ) ?? "pnpm dev"
  );
}

function preferredBackendCommand(project: ProjectSummary) {
  if (project.detectedStacks.includes("Tauri")) {
    return "cargo tauri dev";
  }

  return (
    project.suggestedCommands.find((command) => /\b(cargo|go|python|uv|bun)\b/.test(command)) ??
    "cargo tauri dev"
  );
}

function latestRunForCommand(runs: CommandRun[], taskId: string, command: string) {
  return runs
    .filter((run) => run.taskId === taskId && run.command === command)
    .sort((left, right) => right.startedAtMs - left.startedAtMs)[0];
}

function latestFailedRun(runs: CommandRun[], taskId: string) {
  return runs
    .filter((run) => run.taskId === taskId && (run.status === "failed" || run.errorSummary?.failed))
    .sort((left, right) => right.startedAtMs - left.startedAtMs)[0];
}

function cycleLabel(run: CommandRun, index: number) {
  const started = new Date(run.startedAtMs).toLocaleTimeString("en-US", {
    hour: "2-digit",
    minute: "2-digit",
  });
  return `Cycle ${index + 1} · ${run.status} · ${started}`;
}

function errorFinding(run?: CommandRun) {
  if (!run?.errorSummary) {
    return "No failing command has been captured for this task yet.";
  }

  return (
    run.errorSummary.matchedLines[0] ??
    run.errorSummary.stderrTail[run.errorSummary.stderrTail.length - 1] ??
    `Command exited with ${run.errorSummary.exitCode ?? "an error"}`
  );
}

export function TestingPane({ project, task }: TestingPaneProps) {
  const { state } = useAppState();
  const { startCommandRun, stopCommandRun } = useCommandBridge();
  const { appendFeedback, completeTask, generateRepairContext } = useTaskBridge();
  const [frontendRunId, setFrontendRunId] = useState<string | null>(null);
  const [backendRunId, setBackendRunId] = useState<string | null>(null);
  const [feedback, setFeedback] = useState("");
  const [commandError, setCommandError] = useState<string | null>(null);

  const slots = useMemo<CommandSlot[]>(
    () => [
      {
        id: "frontend",
        title: "Frontend",
        command: preferredFrontendCommand(project),
        endpoint: "localhost:1420",
        tone: "frontend",
      },
      {
        id: "backend",
        title: "Backend",
        command: preferredBackendCommand(project),
        endpoint: project.detectedStacks.includes("Tauri") ? "tauri runtime" : "local service",
        tone: "backend",
      },
    ],
    [project],
  );

  const frontendRun =
    state.commandRuns.find((run) => run.id === frontendRunId) ??
    latestRunForCommand(state.commandRuns, task.id, slots[0].command);
  const backendRun =
    state.commandRuns.find((run) => run.id === backendRunId) ??
    latestRunForCommand(state.commandRuns, task.id, slots[1].command);
  const taskRuns = state.commandRuns
    .filter((run) => run.taskId === task.id)
    .sort((left, right) => left.startedAtMs - right.startedAtMs);
  const failedRun = latestFailedRun(state.commandRuns, task.id);
  const hasRunningRun = [frontendRun, backendRun].some((run) => run?.status === "running");

  async function runSlot(slot: CommandSlot) {
    try {
      const parsed = parseCommandLine(slot.command);
      setCommandError(null);
      const run = await startCommandRun({
        program: parsed.program,
        args: parsed.args,
        cwd: project.path,
        taskId: task.id,
      });

      if (!run) {
        return;
      }

      if (slot.id === "frontend") {
        setFrontendRunId(run.id);
      } else {
        setBackendRunId(run.id);
      }
    } catch (error) {
      setCommandError(error instanceof Error ? error.message : "Invalid command");
    }
  }

  async function stopSlot(run?: CommandRun) {
    if (run) {
      await stopCommandRun(run.id);
    }
  }

  async function runAll() {
    for (const slot of slots) {
      const run = slot.id === "frontend" ? frontendRun : backendRun;
      if (run?.status !== "running") {
        await runSlot(slot);
      }
    }
  }

  async function handleGenerateRepairContext() {
    await generateRepairContext(project.path, task.id);
  }

  async function handleFeedbackSubmit() {
    if (!feedback.trim()) {
      return;
    }

    const updated = await appendFeedback(project.path, task.id, failedRun?.id, feedback.trim());
    if (updated) {
      setFeedback("");
      await generateRepairContext(project.path, task.id);
    }
  }

  async function handleAccept() {
    await completeTask(project.path, task.id);
  }

  return (
    <div className="testing-pane">
      <div className="testing-header">
        <div className="testing-stepper">
          <span className="testing-step step-done">Implement</span>
          <span className="testing-step step-active">Test</span>
          <span className="testing-step">Done</span>
        </div>
        <span className={`testing-status-pill ${failedRun ? "testing-status-err" : "testing-status-ok"}`}>
          <span />
          {failedRun ? "Detected error" : task.status === "verifying" ? "Ready to accept" : "Testing"}
        </span>
        <Button type="button" variant="ghost" iconLeft={<RefreshCw size={14} />} onClick={runAll}>
          Re-run
        </Button>
      </div>

      <div className="testing-grid">
        <div className="testing-terminals">
          <TerminalCard
            {...slots[0]}
            run={frontendRun}
            logs={frontendRun ? state.commandLogs[frontendRun.id] ?? [] : []}
            onRun={() => runSlot(slots[0])}
            onStop={() => stopSlot(frontendRun)}
            disabled={false}
          />
          <TerminalCard
            {...slots[1]}
            run={backendRun}
            logs={backendRun ? state.commandLogs[backendRun.id] ?? [] : []}
            onRun={() => runSlot(slots[1])}
            onStop={() => stopSlot(backendRun)}
            disabled={false}
          />
          {(commandError || state.app.commandError) && (
            <div className="testing-inline-error">{commandError ?? state.app.commandError}</div>
          )}
        </div>

        <aside className="debug-agent-panel">
          <div className="debug-agent-header">
            <div className="debug-agent-avatar">Cl</div>
            <span>Debug agent</span>
            <div className="debug-agent-mode">
              <b>Auto</b>
              <span>Manual</span>
            </div>
          </div>

          <div className="debug-agent-body">
            <section className="debug-card debug-card-error">
              <div className="debug-card-label">
                <AlertTriangle size={14} />
                Detected error
              </div>
              <strong>{failedRun ? failedRun.command : "Waiting for a failing run"}</strong>
              <p>{errorFinding(failedRun)}</p>
            </section>

            <section className="debug-card">
              <div className="debug-card-label">
                <Wrench size={14} />
                Proposed fix
              </div>
              <pre className="debug-repair-preview">
                {task.repairContextPreview ||
                  "Generate a repair handoff after a failure to package logs, todo context, and human feedback."}
              </pre>
              <Button
                type="button"
                variant="primary"
                iconLeft={<ClipboardCheck size={14} />}
                onClick={handleGenerateRepairContext}
              >
                Prepare repair handoff
              </Button>
            </section>

            <section className="debug-card">
              <div className="debug-card-label">Test cycles</div>
              <div className="testing-cycle-list">
                {taskRuns.length === 0 ? (
                  <span>No command cycles yet.</span>
                ) : (
                  taskRuns.map((run, index) => (
                    <div className={`testing-cycle cycle-${run.status}`} key={run.id}>
                      <span />
                      <div>
                        <strong>{cycleLabel(run, index)}</strong>
                        <p>{run.errorSummary?.matchedLines[0] ?? run.command}</p>
                      </div>
                    </div>
                  ))
                )}
              </div>
            </section>

            <section className="debug-card">
              <div className="debug-card-label">Manual feedback</div>
              <div className="testing-feedback-row">
                <input
                  value={feedback}
                  onChange={(event) => setFeedback(event.target.value)}
                  placeholder="Add context for the next fix cycle."
                />
                <Button
                  type="button"
                  variant="ghost"
                  iconRight={<Send size={14} />}
                  disabled={!feedback.trim()}
                  onClick={handleFeedbackSubmit}
                >
                  Send
                </Button>
              </div>
            </section>

            {task.status === "verifying" && (
              <Button
                type="button"
                variant="primary"
                iconLeft={<CheckCircle2 size={14} />}
                disabled={hasRunningRun}
                onClick={handleAccept}
              >
                Accept / Done
              </Button>
            )}
          </div>
        </aside>
      </div>
    </div>
  );
}
