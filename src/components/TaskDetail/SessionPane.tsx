import { useEffect, useMemo, useRef, useState, type FormEvent } from "react";
import {
  CheckCircle2,
  Circle,
  FileText,
  Play,
  PlayCircle,
  Send,
  Terminal,
} from "lucide-react";
import type {
  AgentConfig,
  CommandRun,
  PlanTodoItem,
  PlanTodoStatus,
  ProjectSummary,
  Task,
  TerminalSlot,
} from "../../domain";
import { DEFAULT_APP_SETTINGS } from "../../domain";
import { useAgentBridge } from "../../hooks/useAgentBridge";
import { useCommandBridge } from "../../hooks/useCommandBridge";
import { useSettingsBridge } from "../../hooks/useSettingsBridge";
import { useTaskBridge } from "../../hooks/useTaskBridge";
import { useTerminalBridge } from "../../hooks/useTerminalBridge";
import { useAppState } from "../../state/AppStateContext";
import {
  buildAgentCommandInvocation,
  buildImplementationPrompt,
  buildRepairPrompt,
  buildResumeRepairPrompt,
  hasImplementationCapability,
} from "../../utils/agentRun";
import {
  DEFAULT_LOOP_BUDGET,
  MAX_AUTO_REPAIR_ATTEMPTS,
  decideAfterRepair,
  decideAfterValidation,
  escalationNotice,
  isTimedOut,
} from "../../utils/loopPolicy";
import { detectDangerousCommand, parseCommandLine } from "../../utils/commandLine";
import { resolveTerminalSlotCwd } from "../../utils/terminalSlots";
import { Button } from "../common/Button";
import { TaskTimeline } from "./TaskTimeline";
import "./TaskDetail.css";

interface SessionPaneProps {
  project: ProjectSummary;
  task: Task;
  readOnly?: boolean;
}

const TODO_STATUS_LABELS: Record<PlanTodoStatus, string> = {
  pending: "Pending",
  implementing: "Implementing",
  done: "Done",
  blocked: "Blocked",
};
interface AutoImplementationLoop {
  loopId: string;
  todoId: string;
  validationCommand: string;
  validationCwd: string;
  repairAttempts: number;
  lastFailureFingerprint?: string;
  repeatedFailureCount: number;
  startedAtMs: number;
  status: "validating" | "repairing" | "passed" | "escalated";
}

function agentInitials(agent?: AgentConfig | null) {
  if (!agent) {
    return "Ag";
  }

  return agent.name
    .split(/\s+/)
    .map((part) => part[0])
    .join("")
    .slice(0, 2);
}

function summarizeCommand(command: string) {
  return command.length > 140 ? `${command.slice(0, 140)}...` : command;
}

export function SessionPane({ project, task, readOnly = false }: SessionPaneProps) {
  const { state } = useAppState();
  const { loadAgents } = useAgentBridge();
  const { startCommandRun, stopCommandRun } = useCommandBridge();
  const {
    appendFeedback,
    buildImplementationContext,
    completeTodo,
    generateRepairContext,
    markReadyForTesting,
    startTodo,
  } = useTaskBridge();
  const { loadSettings } = useSettingsBridge();
  const { listTerminalSlots, suggestTerminalSlots } = useTerminalBridge();
  const implementationAgents = useMemo(
    () => state.agents.filter((agent) => hasImplementationCapability(agent)),
    [state.agents],
  );
  const [selectedAgentId, setSelectedAgentId] = useState(task.primaryAgentId ?? "");
  const [guidance, setGuidance] = useState("");
  const [autoValidate, setAutoValidate] = useState(false);
  const [validationSlots, setValidationSlots] = useState<TerminalSlot[]>([]);
  const [settings, setSettings] = useState(DEFAULT_APP_SETTINGS);
  const [validationNotice, setValidationNotice] = useState<string | null>(null);
  const [autoLoop, setAutoLoop] = useState<AutoImplementationLoop | null>(null);
  const handledLoopRunsRef = useRef<Set<string>>(new Set());
  const selectedAgent =
    implementationAgents.find((agent) => agent.id === selectedAgentId) ??
    implementationAgents.find((agent) => agent.id === task.primaryAgentId) ??
    implementationAgents[0] ??
    null;
  const activeTodo =
    task.planTodos.find((todo) => todo.id === state.app.selectedTodoId) ??
    task.planTodos.find((todo) => todo.status === "implementing") ??
    task.planTodos[0] ??
    null;
  const commandRunning = state.commandRuns.some(
    (run) => run.taskId === task.id && run.status === "running",
  );
  const latestRun =
    state.commandRuns
      .filter((run) => run.taskId === task.id)
      .sort((left, right) => right.startedAtMs - left.startedAtMs)[0] ?? null;
  const latestRunLogs = latestRun ? state.commandLogs[latestRun.id] ?? [] : [];
  const allTodosDone =
    task.planTodos.length > 0 && task.planTodos.every((todo) => todo.status === "done");
  const canMarkReadyForTesting = !readOnly && task.status === "reviewing" && allTodosDone;
  const preferredValidationSlot = useMemo(
    () => validationSlots.find((slot) => slot.kind === "validation" && slot.command.trim()),
    [validationSlots],
  );
  const loopRuns = useMemo(
    () =>
      autoLoop
        ? state.commandRuns
            .filter((run) => run.taskId === task.id && run.loopId === autoLoop.loopId)
            .sort((left, right) => left.startedAtMs - right.startedAtMs)
        : [],
    [autoLoop, state.commandRuns, task.id],
  );
  const completedLoopRuns = loopRuns.filter((run) => run.status !== "running");
  const latestCompletedLoopRun = completedLoopRuns[completedLoopRuns.length - 1] ?? null;
  const runningLoopRun = loopRuns.find((run) => run.status === "running") ?? null;
  const latestAgentResumeCommand =
    state.commandRuns
      .filter(
        (run) =>
          run.taskId === task.id &&
          run.intent === "agent_action" &&
          run.status === "succeeded" &&
          run.resumeCommand,
      )
      .sort((left, right) => right.startedAtMs - left.startedAtMs)[0]?.resumeCommand ?? null;

  useEffect(() => {
    void loadAgents();
  }, [loadAgents]);

  useEffect(() => {
    let cancelled = false;
    void loadSettings().then((loaded) => {
      if (!cancelled) {
        setSettings(loaded);
      }
    });
    return () => {
      cancelled = true;
    };
  }, [loadSettings]);

  useEffect(() => {
    if (!selectedAgentId && selectedAgent) {
      setSelectedAgentId(selectedAgent.id);
    }
  }, [selectedAgent, selectedAgentId]);

  useEffect(() => {
    let cancelled = false;
    void listTerminalSlots(project.path).then(async (loaded) => {
      const slots = loaded.length > 0 ? loaded : await suggestTerminalSlots(project.path);
      if (!cancelled) {
        setValidationSlots(slots);
      }
    });
    return () => {
      cancelled = true;
    };
  }, [project.path, listTerminalSlots, suggestTerminalSlots]);

  function slotCwd(slot: { cwd?: string }) {
    return resolveTerminalSlotCwd(project.path, slot);
  }

  function confirmDangerousCommand(command: string) {
    if (!settings.confirmBeforeCommands) {
      return true;
    }
    const finding = detectDangerousCommand(command);
    if (!finding) {
      return true;
    }
    return window.confirm(
      `Run potentially risky command?\n\n${command}\n\n${finding.detail}`,
    );
  }

  async function startAutoValidationRun(
    loop: AutoImplementationLoop,
    iteration: number,
    attempt: number,
  ) {
    if (!confirmDangerousCommand(loop.validationCommand)) {
      setAutoLoop((current) => (current?.loopId === loop.loopId ? { ...current, status: "escalated" } : current));
      setValidationNotice("Auto validation stopped because the command was not confirmed.");
      return null;
    }

    try {
      const parsed = parseCommandLine(loop.validationCommand);
      if (!parsed.program) {
        setAutoLoop((current) => (current?.loopId === loop.loopId ? { ...current, status: "escalated" } : current));
        setValidationNotice("The configured validation command is empty.");
        return null;
      }

      const run = await startCommandRun({
        program: parsed.program,
        args: parsed.args,
        cwd: loop.validationCwd,
        taskId: task.id,
        intent: "validation",
        loopId: loop.loopId,
        iteration,
        attempt,
      });
      setValidationNotice(
        run ? `Auto validation started: ${loop.validationCommand}` : "Auto validation failed to start.",
      );
      if (!run) {
        setAutoLoop((current) =>
          current?.loopId === loop.loopId ? { ...current, status: "escalated" } : current,
        );
      }
      return run;
    } catch (error) {
      setAutoLoop((current) => (current?.loopId === loop.loopId ? { ...current, status: "escalated" } : current));
      setValidationNotice(error instanceof Error ? error.message : "Invalid validation command.");
      return null;
    }
  }

  async function startAutoRepairRun(loop: AutoImplementationLoop, failedRun: CommandRun, attempt: number) {
    if (!selectedAgent) {
      setAutoLoop((current) => (current?.loopId === loop.loopId ? { ...current, status: "escalated" } : current));
      setValidationNotice("Auto repair stopped because no implementation agent is available.");
      return;
    }

    setAutoLoop((current) =>
      current?.loopId === loop.loopId
        ? { ...current, repairAttempts: attempt, status: "repairing" }
        : current,
    );
    const refreshed = await generateRepairContext(project.path, task.id);
    const repairNote = `Auto repair attempt ${attempt}/${MAX_AUTO_REPAIR_ATTEMPTS}: validation failed in \`${failedRun.command}\`.`;
    const prompt = latestAgentResumeCommand
      ? buildResumeRepairPrompt(refreshed ?? task, repairNote)
      : buildRepairPrompt(refreshed ?? task, "", repairNote);
    const invocation = buildAgentCommandInvocation(
      selectedAgent,
      project.path,
      prompt,
      latestAgentResumeCommand,
    );
    const run = await startCommandRun({
      program: invocation.program,
      args: invocation.args,
      cwd: project.path,
      taskId: task.id,
      intent: "agent_action",
      loopId: loop.loopId,
      iteration: attempt,
      attempt,
    });
    setValidationNotice(
      run
        ? invocation.resumed
          ? `Auto repair attempt ${attempt}/${MAX_AUTO_REPAIR_ATTEMPTS} resumed the previous agent session.`
          : `Auto repair attempt ${attempt}/${MAX_AUTO_REPAIR_ATTEMPTS} started.`
        : "Auto repair failed to start.",
    );
    if (!run) {
      setAutoLoop((current) =>
        current?.loopId === loop.loopId ? { ...current, status: "escalated" } : current,
      );
    }
  }

  useEffect(() => {
    if (
      !autoValidate ||
      readOnly ||
      !autoLoop ||
      autoLoop.status === "passed" ||
      autoLoop.status === "escalated"
    ) {
      return;
    }

    const tick = () => {
      const timedOut = isTimedOut(
        {
          repairAttempts: autoLoop.repairAttempts,
          repeatedFailureCount: autoLoop.repeatedFailureCount,
          lastFailureFingerprint: autoLoop.lastFailureFingerprint,
          startedAtMs: autoLoop.startedAtMs,
        },
        DEFAULT_LOOP_BUDGET,
        Date.now(),
      );
      if (!timedOut) {
        return;
      }

      setAutoLoop((current) =>
        current?.loopId === autoLoop.loopId ? { ...current, status: "escalated" } : current,
      );
      setValidationNotice(escalationNotice("timeout", DEFAULT_LOOP_BUDGET));
      if (runningLoopRun) {
        void stopCommandRun(runningLoopRun.id, "timeout");
      }
    };

    tick();
    const timer = window.setInterval(tick, 1_000);
    return () => window.clearInterval(timer);
  }, [autoValidate, autoLoop, readOnly, runningLoopRun, stopCommandRun]);

  useEffect(() => {
    if (
      !autoValidate ||
      readOnly ||
      !autoLoop ||
      autoLoop.status === "passed" ||
      autoLoop.status === "escalated" ||
      !latestCompletedLoopRun
    ) {
      return;
    }
    if (handledLoopRunsRef.current.has(latestCompletedLoopRun.id)) {
      return;
    }
    handledLoopRunsRef.current.add(latestCompletedLoopRun.id);

    const loopProgress = {
      repairAttempts: autoLoop.repairAttempts,
      repeatedFailureCount: autoLoop.repeatedFailureCount,
      lastFailureFingerprint: autoLoop.lastFailureFingerprint,
      startedAtMs: autoLoop.startedAtMs,
    };

    if (latestCompletedLoopRun.intent === "validation") {
      const decision = decideAfterValidation(
        loopProgress,
        latestCompletedLoopRun,
        DEFAULT_LOOP_BUDGET,
        Date.now(),
      );
      if (decision.kind === "pass") {
        setAutoLoop({ ...autoLoop, status: "passed" });
        setValidationNotice("Auto validation passed. This todo has passing evidence.");
        return;
      }
      if (decision.kind === "escalate") {
        setAutoLoop({
          ...autoLoop,
          lastFailureFingerprint: decision.fingerprint ?? autoLoop.lastFailureFingerprint,
          repeatedFailureCount: decision.repeatedFailureCount ?? autoLoop.repeatedFailureCount,
          status: "escalated",
        });
        setValidationNotice(escalationNotice(decision.reason, DEFAULT_LOOP_BUDGET));
        return;
      }

      setAutoLoop({
        ...autoLoop,
        lastFailureFingerprint: decision.fingerprint,
        repeatedFailureCount: decision.repeatedFailureCount,
        repairAttempts: decision.attempt,
        status: "repairing",
      });
      void startAutoRepairRun(autoLoop, latestCompletedLoopRun, decision.attempt);
      return;
    }

    if (latestCompletedLoopRun.intent === "agent_action") {
      const decision = decideAfterRepair(
        loopProgress,
        latestCompletedLoopRun,
        DEFAULT_LOOP_BUDGET,
        Date.now(),
      );
      if (decision.kind === "escalate") {
        setAutoLoop({ ...autoLoop, status: "escalated" });
        setValidationNotice(escalationNotice(decision.reason, DEFAULT_LOOP_BUDGET));
        return;
      }
      setAutoLoop({ ...autoLoop, status: "validating" });
      void startAutoValidationRun(autoLoop, autoLoop.repairAttempts + 1, autoLoop.repairAttempts);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [autoValidate, autoLoop, latestCompletedLoopRun?.id, readOnly]);

  async function handleStartTodo(todo: PlanTodoItem, todoIndex: number) {
    if (!selectedAgent) {
      return;
    }

    const updatedTask = await startTodo(project.path, task.id, todo.id, selectedAgent.id);
    const context = await buildImplementationContext(project.path, task.id, todo.id);
    const prompt = context?.prompt ?? buildImplementationPrompt(updatedTask ?? task, todo, todoIndex);
    const invocation = buildAgentCommandInvocation(selectedAgent, project.path, prompt);
    await startCommandRun({
      program: invocation.program,
      args: invocation.args,
      cwd: project.path,
      taskId: task.id,
      intent: "agent_action",
    });
  }

  async function handleCompleteTodo(todo: PlanTodoItem) {
    if (autoValidate && commandRunning) {
      setValidationNotice("Wait for the active command run to finish before auto validation.");
      return;
    }

    const updatedTask = await completeTodo(project.path, task.id, todo.id);
    if (!autoValidate || !updatedTask) {
      return;
    }

    if (!preferredValidationSlot) {
      setValidationNotice("No validation command is configured for this project.");
      return;
    }

    const loop: AutoImplementationLoop = {
      loopId: `loop-${task.id}-${todo.id}-${Date.now()}`,
      todoId: todo.id,
      validationCommand: preferredValidationSlot.command.trim(),
      validationCwd: slotCwd(preferredValidationSlot),
      repairAttempts: 0,
      repeatedFailureCount: 0,
      startedAtMs: Date.now(),
      status: "validating",
    };
    handledLoopRunsRef.current = new Set();
    setAutoLoop(loop);
    setValidationNotice(null);
    await startAutoValidationRun(loop, 1, 0);
  }

  async function handleGuidanceSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();

    if (!guidance.trim()) {
      return;
    }

    const updated = await appendFeedback(project.path, task.id, latestRun?.id, guidance.trim());
    if (updated) {
      setGuidance("");
    }
  }

  async function handleMarkReadyForTesting() {
    await markReadyForTesting(project.path, task.id);
  }

  return (
    <div className="session-pane">
      <div className="session-header">
        <span className="testing-status-pill testing-status-ok">
          <span />
          {selectedAgent?.name ?? "No implementation Agent"}
        </span>
        <div className="testing-mode-toggle" role="group" aria-label="Implementation validation mode">
          <button
            type="button"
            className={autoValidate ? "" : "active"}
            disabled={readOnly}
            onClick={() => {
              setAutoValidate(false);
              setAutoLoop(null);
              setValidationNotice(null);
            }}
          >
            Manual
          </button>
          <button
            type="button"
            className={autoValidate ? "active" : ""}
            disabled={readOnly}
            onClick={() => setAutoValidate(true)}
          >
            Auto
          </button>
        </div>
      </div>

      <div className="session-grid">
        <section className="session-main-panel">
          <div className="session-thread">
            <article className="session-turn">
              <div className="session-avatar user">ME</div>
              <div className="session-turn-body">
                <div className="session-name">Task</div>
                <p>{task.title}</p>
                {activeTodo && <p>{activeTodo.description}</p>}
              </div>
            </article>

            <article className="session-turn">
              <div className="session-avatar agent">{agentInitials(selectedAgent)}</div>
              <div className="session-turn-body">
                <div className="session-name">
                  {selectedAgent?.name ?? "Implementation Agent"}
                  <span>agent loop</span>
                </div>
                <div className="session-thinking">
                  Reading the confirmed plan and preparing a scoped implementation run for the selected todo.
                </div>

                <div className="session-tool">
                  <div className="session-tool-header">
                    <FileText size={14} />
                    <b>Read</b>
                    <span>{activeTodo?.planRef ?? task.finalPlanPath ?? "confirmed plan"}</span>
                    <em>scope</em>
                  </div>
                  <pre>{task.finalPlan ?? "Confirm a plan before implementation."}</pre>
                </div>

                {latestRun && (
                  <div className="session-tool">
                    <div className="session-tool-header">
                      <Terminal size={14} />
                      <b>Run</b>
                      <span>{summarizeCommand(latestRun.command)}</span>
                      <em>{latestRun.status}</em>
                    </div>
                    <pre>
                      {latestRunLogs.length > 0
                        ? latestRunLogs.map((entry) => `${entry.stream.toUpperCase()} ${entry.line}`).join("\n")
                        : "Agent process started. Waiting for output..."}
                    </pre>
                  </div>
                )}

                <div className="session-tool">
                  <div className="session-tool-header">
                    <FileText size={14} />
                    <b>Timeline</b>
                    <span>{autoLoop?.loopId ?? "task history"}</span>
                    <em>trace</em>
                  </div>
                  <TaskTimeline task={task} maxItems={5} />
                </div>

                <p className="session-agent-copy">
                  {activeTodo
                    ? `Ready to implement "${activeTodo.title}". Start the todo, capture command evidence, then mark it done after review.`
                    : "Confirm a plan to generate implementation todos."}
                </p>
                {validationNotice && <p className="session-agent-copy">{validationNotice}</p>}
              </div>
            </article>
          </div>

          <form className="session-composer" onSubmit={handleGuidanceSubmit}>
            <input
              value={guidance}
              onChange={(event) => setGuidance(event.target.value)}
              placeholder={`Steer ${selectedAgent?.name ?? "the Agent"} - add a constraint, answer, or approve next step...`}
              disabled={!selectedAgent || readOnly}
            />
            <Button
              type="submit"
              variant="primary"
              iconRight={<Send size={14} />}
              disabled={!guidance.trim() || readOnly}
            >
              Send
            </Button>
          </form>
        </section>

        <aside className="session-task-panel">
          <div className="session-task-header">
            <span>Task</span>
            <span className="testing-status-pill testing-status-ok">
              <span />
              In Progress
            </span>
          </div>
          <div className="session-task-body">
            <div>
              <div className="debug-card-label">Assignee</div>
              <label className="session-assignee">
                <div className="session-avatar agent">{agentInitials(selectedAgent)}</div>
                <select
                  value={selectedAgent?.id ?? ""}
                  disabled={implementationAgents.length === 0 || commandRunning || readOnly}
                  onChange={(event) => setSelectedAgentId(event.target.value)}
                >
                  {implementationAgents.map((agent) => (
                    <option value={agent.id} key={agent.id}>
                      {agent.name}
                    </option>
                  ))}
                </select>
              </label>
            </div>

            <div>
              <div className="debug-card-label">Subtasks</div>
              <div className="session-subtasks">
                {task.planTodos.length === 0 && <span>No todos yet.</span>}
                {task.planTodos.map((todo, todoIndex) => {
                  const active = todo.id === activeTodo?.id;
                  const done = todo.status === "done";

                  return (
                    <div className={`session-subtask ${active ? "active" : ""}`} key={todo.id}>
                      <button
                        type="button"
                        className="session-subtask-main"
                        disabled={done || !selectedAgent || commandRunning || readOnly}
                        onClick={() => void handleStartTodo(todo, todoIndex)}
                      >
                        {todo.status === "implementing" ? <PlayCircle size={15} /> : <Play size={15} />}
                        <span className={done ? "done" : ""}>{todo.title}</span>
                        <em>{TODO_STATUS_LABELS[todo.status]}</em>
                      </button>
                      <button
                        type="button"
                        className="session-subtask-check"
                        disabled={done || readOnly || (autoValidate && commandRunning)}
                        onClick={() => void handleCompleteTodo(todo)}
                      >
                        {done ? <CheckCircle2 size={14} /> : <Circle size={10} />}
                      </button>
                    </div>
                  );
                })}
              </div>
            </div>

            <div>
              <div className="debug-card-label">Files changed</div>
              <div className="session-file-list">
                {task.agentInvocations.slice(-3).map((invocation) => (
                  <div key={invocation.id}>
                    <span>{invocation.evidenceRef ?? invocation.agentName}</span>
                    <b>{invocation.status}</b>
                  </div>
                ))}
                {task.agentInvocations.length === 0 && <span>No file evidence recorded yet.</span>}
              </div>
            </div>

            <Button
              type="button"
              variant="primary"
              disabled={!canMarkReadyForTesting}
              onClick={handleMarkReadyForTesting}
            >
              Mark ready for testing
            </Button>
          </div>
        </aside>
      </div>
    </div>
  );
}
