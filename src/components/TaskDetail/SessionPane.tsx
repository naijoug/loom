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
import { useAgentCatalog } from "../../hooks/useAgentCatalog";
import { useCommandBridge } from "../../hooks/useCommandBridge";
import { useProjectPreferencesBridge } from "../../hooks/useProjectPreferencesBridge";
import { useTaskBridge } from "../../hooks/useTaskBridge";
import { useTerminalBridge } from "../../hooks/useTerminalBridge";
import { useAppState } from "../../state/AppStateContext";
import { implementationReviewGate } from "../../utils/implementationReview";
import {
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
import { parseCommandLine } from "../../utils/commandLine";
import { resolveTerminalSlotCwd } from "../../utils/terminalSlots";
import { Button } from "../common/Button";
import { WORKFLOW_COPY } from "../../copy/workflow";
import { ImplementationReviewPanel } from "./ImplementationReviewPanel";
import { TaskTimeline } from "./TaskTimeline";
import "./TaskDetail.css";

interface SessionPaneProps {
  project: ProjectSummary;
  task: Task;
  readOnly?: boolean;
}

const TODO_STATUS_LABELS: Record<PlanTodoStatus, string> = {
  pending: "待办",
  implementing: "实施中",
  done: "完成",
  blocked: "阻塞",
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
    return "";
  }

  if (agent.adapterType === "codex_cli" || agent.id.includes("codex")) {
    return "Co";
  }
  if (agent.adapterType === "claude_code_cli" || agent.id.includes("claude")) {
    return "Cl";
  }

  return agent.name
    .split(/\s+/)
    .map((part) => part[0])
    .join("")
    .slice(0, 2);
}

function agentClass(agent?: AgentConfig | null) {
  if (!agent) {
    return "empty";
  }
  if (agent.adapterType === "codex_cli" || agent.id.includes("codex")) {
    return "codex";
  }
  if (agent.adapterType === "claude_code_cli" || agent.id.includes("claude")) {
    return "claude";
  }
  return "hermes";
}

function summarizeCommand(command: string) {
  return command.length > 140 ? `${command.slice(0, 140)}...` : command;
}

function matchesDebuggingStatus(status: Task["status"]) {
  return status === "debugging" || status === "fixing" || status === "verifying";
}

export function SessionPane({ project, task, readOnly = false }: SessionPaneProps) {
  const { state } = useAppState();
  const { loadAgents, prepareAgentInvocation } = useAgentCatalog();
  const { startCommandRun, stopCommandRun } = useCommandBridge();
  const { loadProjectAgentPreferences } = useProjectPreferencesBridge();
  const {
    appendFeedback,
    buildImplementationContext,
    completeTodo,
    generateRepairContext,
    markReadyForTesting,
    startTodo,
  } = useTaskBridge();
  const { listTerminalSlots, suggestTerminalSlots } = useTerminalBridge();
  const implementationAgents = useMemo(
    () => state.agents.filter((agent) => hasImplementationCapability(agent)),
    [state.agents],
  );
  const [selectedAgentId, setSelectedAgentId] = useState(task.primaryAgentId ?? "");
  const [primaryAgentSwitchReason, setPrimaryAgentSwitchReason] = useState("");
  const [guidance, setGuidance] = useState("");
  const [autoValidate, setAutoValidate] = useState(false);
  const [validationSlots, setValidationSlots] = useState<TerminalSlot[]>([]);
  const [validationNotice, setValidationNotice] = useState<string | null>(null);
  const [autoLoop, setAutoLoop] = useState<AutoImplementationLoop | null>(null);
  const handledLoopRunsRef = useRef<Set<string>>(new Set());
  const selectedAgent =
    implementationAgents.find((agent) => agent.id === selectedAgentId) ??
    implementationAgents.find((agent) => agent.id === task.primaryAgentId) ??
    implementationAgents[0] ??
    null;
  const switchingPrimaryAgent = Boolean(
    task.primaryAgentId && selectedAgent && task.primaryAgentId !== selectedAgent.id,
  );
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
  const reviewGate = implementationReviewGate(task);
  const canMarkReadyForTesting =
    !readOnly && task.status === "reviewing" && allTodosDone && reviewGate.ready;
  const markReadyBlocker = readOnly
    ? "当前正在回看历史阶段。"
    : !allTodosDone
      ? "先完成全部实施子任务。"
      : task.status !== "reviewing"
        ? "完成子任务后运行独立实施 Review。"
        : !reviewGate.ready
          ? reviewGate.detail
          : null;
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
    if (!matchesDebuggingStatus(task.status)) {
      return;
    }
    let cancelled = false;
    void loadProjectAgentPreferences(project.path)
      .then((preferences) => {
        if (!cancelled && preferences.debuggingAgentId) {
          setSelectedAgentId(preferences.debuggingAgentId);
        }
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [loadProjectAgentPreferences, project.path, task.status]);

  useEffect(() => {
    if (!selectedAgentId && selectedAgent) {
      setSelectedAgentId(selectedAgent.id);
    }
  }, [selectedAgent, selectedAgentId]);

  useEffect(() => {
    if (task.status === "reviewing" && task.primaryAgentId) {
      setSelectedAgentId(task.primaryAgentId);
      setPrimaryAgentSwitchReason("");
    }
  }, [task.primaryAgentId, task.status]);

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

  async function startAutoValidationRun(
    loop: AutoImplementationLoop,
    iteration: number,
    attempt: number,
  ) {
    try {
      const parsed = parseCommandLine(loop.validationCommand);
      if (!parsed.program) {
        setAutoLoop((current) => (current?.loopId === loop.loopId ? { ...current, status: "escalated" } : current));
        setValidationNotice("已配置的验证命令为空。请先在设置中补充命令。");
        return null;
      }

      const run = await startCommandRun({
        program: parsed.program,
        args: parsed.args,
        cwd: loop.validationCwd,
        projectPath: project.path,
        taskId: task.id,
        intent: "validation",
        loopId: loop.loopId,
        iteration,
        attempt,
      });
      setValidationNotice(
        run ? `自动验证已启动：${loop.validationCommand}` : "自动验证启动失败。",
      );
      if (!run) {
        setAutoLoop((current) =>
          current?.loopId === loop.loopId ? { ...current, status: "escalated" } : current,
        );
      }
      return run;
    } catch (error) {
      setAutoLoop((current) => (current?.loopId === loop.loopId ? { ...current, status: "escalated" } : current));
      setValidationNotice(error instanceof Error ? error.message : "验证命令无效。请检查配置。");
      return null;
    }
  }

  async function startAutoRepairRun(loop: AutoImplementationLoop, failedRun: CommandRun, attempt: number) {
    if (!selectedAgent) {
      setAutoLoop((current) => (current?.loopId === loop.loopId ? { ...current, status: "escalated" } : current));
      setValidationNotice("自动修复已停止：没有可用的实施 Agent。");
      return;
    }

    setAutoLoop((current) =>
      current?.loopId === loop.loopId
        ? { ...current, repairAttempts: attempt, status: "repairing" }
        : current,
    );
    const refreshed = await generateRepairContext(project.path, task.id);
    const repairNote = `自动修复 ${attempt}/${MAX_AUTO_REPAIR_ATTEMPTS}：验证命令 \`${failedRun.command}\` 失败。`;
    const prompt = latestAgentResumeCommand
      ? buildResumeRepairPrompt(refreshed ?? task, repairNote)
      : buildRepairPrompt(refreshed ?? task, "", repairNote);
    const invocation = await prepareAgentInvocation({
      projectPath: project.path,
      taskId: task.id,
      agentId: selectedAgent.id,
      stage: "debugging",
      prompt,
      resumeCommand: latestAgentResumeCommand ?? undefined,
    });
    if (!invocation) {
      setAutoLoop((current) =>
        current?.loopId === loop.loopId ? { ...current, status: "escalated" } : current,
      );
      setValidationNotice("自动修复已停止：后端拒绝了 Agent 调用。");
      return;
    }
    const run = await startCommandRun({
      program: invocation.program,
      args: invocation.args,
      cwd: invocation.cwd,
      projectPath: project.path,
      taskId: task.id,
      agentId: selectedAgent.id,
      intent: "agent_action",
      loopId: loop.loopId,
      iteration: attempt,
      attempt,
    });
    setValidationNotice(
      run
        ? invocation.resumed
          ? `自动修复 ${attempt}/${MAX_AUTO_REPAIR_ATTEMPTS} 已恢复上一次 Agent 会话。`
          : `自动修复 ${attempt}/${MAX_AUTO_REPAIR_ATTEMPTS} 已启动。`
        : "自动修复启动失败。",
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
        setValidationNotice("自动验证已通过，当前子任务已有通过证据。");
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

    const updatedTask = await startTodo(
      project.path,
      task.id,
      todo.id,
      selectedAgent.id,
      switchingPrimaryAgent ? primaryAgentSwitchReason.trim() : undefined,
    );
    const context = await buildImplementationContext(project.path, task.id, todo.id);
    const prompt = context?.prompt ?? buildImplementationPrompt(updatedTask ?? task, todo, todoIndex);
    const invocation = await prepareAgentInvocation({
      projectPath: project.path,
      taskId: task.id,
      agentId: selectedAgent.id,
      stage: "implementation",
      prompt,
    });
    if (!invocation) {
      return;
    }
    await startCommandRun({
      program: invocation.program,
      args: invocation.args,
      cwd: invocation.cwd,
      projectPath: project.path,
      taskId: task.id,
      agentId: selectedAgent.id,
      intent: "agent_action",
    });
  }

  async function handleCompleteTodo(todo: PlanTodoItem) {
    if (autoValidate && commandRunning) {
      setValidationNotice("请等待当前命令结束后再运行自动验证。");
      return;
    }

    const updatedTask = await completeTodo(project.path, task.id, todo.id);
    if (!autoValidate || !updatedTask) {
      return;
    }

    if (!preferredValidationSlot) {
      setValidationNotice("当前项目尚未配置验证命令。");
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
        <div className="session-header-left">
          <span className={`loom-agent-avatar ${agentClass(selectedAgent)}`}>
            {agentInitials(selectedAgent)}
          </span>
          <span className="session-agent-name">{selectedAgent?.name ?? "未选择实施 Agent"}</span>
          <span className={`session-status-pill ${commandRunning ? "is-running" : ""}`}>
            <span className="status-pulse-dot" aria-hidden="true" />
            {commandRunning ? "执行中" : "待命"}
          </span>
          <span className="session-elapsed loom-mono">
            {latestRun ? summarizeCommand(latestRun.command) : "等待启动"}
          </span>
        </div>
        <div className="testing-mode-toggle" role="group" aria-label="实施验证模式">
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
            手动
          </button>
          <button
            type="button"
            className={autoValidate ? "active" : ""}
            disabled={readOnly}
            onClick={() => setAutoValidate(true)}
          >
            自动
          </button>
        </div>
      </div>

      <div className="session-grid">
        <section className="session-main-panel">
          <div className="session-thread">
            <article className="session-turn">
              <div className="session-avatar user">你</div>
              <div className="session-turn-body">
                <div className="session-name">任务</div>
                <p>{task.title}</p>
                {activeTodo && <p>{activeTodo.description}</p>}
              </div>
            </article>

            <article className="session-turn">
              <div className={`loom-agent-avatar session-avatar agent ${agentClass(selectedAgent)}`}>
                {agentInitials(selectedAgent)}
              </div>
              <div className="session-turn-body">
                <div className="session-name">
                  {selectedAgent?.name ?? "实施 Agent"}
                  <span>Agent 循环</span>
                </div>
                <div className="session-thinking think-block">
                  正在读取已确认计划，并为当前子任务准备限定范围的实施运行。
                </div>

                <div className="session-tool tool-card">
                  <div className="session-tool-header">
                    <FileText size={14} />
                    <b>读取</b>
                    <span>{activeTodo?.planRef ?? task.finalPlanPath ?? "已确认计划"}</span>
                    <em>范围</em>
                  </div>
                  <pre>{task.finalPlan ?? "实施前请先确认计划。"}</pre>
                </div>

                {latestRun && (
                  <div className="session-tool tool-card">
                    <div className="session-tool-header">
                      <Terminal size={14} />
                      <b>运行</b>
                      <span>{summarizeCommand(latestRun.command)}</span>
                      <em>{latestRun.status}</em>
                    </div>
                    <pre>
                      {latestRunLogs.length > 0
                        ? latestRunLogs.map((entry) => `${entry.stream.toUpperCase()} ${entry.line}`).join("\n")
                        : "Agent 进程已启动，正在等待输出…"}
                    </pre>
                  </div>
                )}

                <div className="session-tool tool-card">
                  <div className="session-tool-header">
                    <FileText size={14} />
                    <b>时间线</b>
                    <span>{autoLoop?.loopId ?? "任务历史"}</span>
                    <em>轨迹</em>
                  </div>
                  <TaskTimeline task={task} maxItems={5} />
                </div>

                <p className="session-agent-copy">
                  {activeTodo
                    ? `可以实施“${activeTodo.title}”。启动子任务、保留命令证据，并在 Review 后标记完成。`
                    : "确认计划后会生成实施子任务。"}
                </p>
                {validationNotice && <p className="session-agent-copy">{validationNotice}</p>}
              </div>
            </article>
          </div>

          <form className="session-composer" onSubmit={handleGuidanceSubmit}>
            <input
              value={guidance}
              onChange={(event) => setGuidance(event.target.value)}
              placeholder={`给 ${selectedAgent?.name ?? "Agent"} 补充约束、答复或下一步指示…`}
              disabled={!selectedAgent || readOnly}
            />
            <Button
              type="submit"
              variant="ghost"
              iconRight={<Send size={14} />}
              disabled={!guidance.trim() || readOnly}
            >
              发送
            </Button>
          </form>
        </section>

        <aside className="session-task-panel files-panel">
          <div className="session-task-header">
            <span>任务</span>
            <span className="testing-status-pill testing-status-ok">
              <span />
              进行中
            </span>
          </div>
          <div className="session-task-body">
            <div>
              <div className="debug-card-label">实施 Agent</div>
              <label className="session-assignee">
                <div className={`loom-agent-avatar session-avatar agent ${agentClass(selectedAgent)}`}>
                  {agentInitials(selectedAgent)}
                </div>
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
              {switchingPrimaryAgent && (
                <input
                  className="session-agent-switch-reason"
                  value={primaryAgentSwitchReason}
                  placeholder="说明切换主 Agent 的原因"
                  disabled={commandRunning || readOnly}
                  onChange={(event) => setPrimaryAgentSwitchReason(event.target.value)}
                />
              )}
            </div>

            <div>
              <div className="debug-card-label">子任务</div>
              <div className="session-subtasks">
                {task.planTodos.length === 0 && <span>暂无子任务。</span>}
                {task.planTodos.map((todo, todoIndex) => {
                  const active = todo.id === activeTodo?.id;
                  const done = todo.status === "done";

                  return (
                    <div className={`session-subtask ${active ? "active" : ""}`} key={todo.id}>
                      <button
                        type="button"
                        className="session-subtask-main"
                        disabled={
                          done ||
                          !selectedAgent ||
                          commandRunning ||
                          readOnly ||
                          (switchingPrimaryAgent && primaryAgentSwitchReason.trim().length < 5)
                        }
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
              <div className="debug-card-label">文件变更</div>
              <div className="session-file-list">
                {task.agentInvocations.slice(-3).map((invocation) => (
                  <div key={invocation.id}>
                    <span>{invocation.evidenceRef ?? invocation.agentName}</span>
                    <b>{invocation.status}</b>
                  </div>
                ))}
                {task.agentInvocations.length === 0 && <span>尚无文件证据。</span>}
              </div>
            </div>

            <ImplementationReviewPanel project={project} task={task} readOnly={readOnly} />

            <Button
              type="button"
              variant="primary"
              disabled={!canMarkReadyForTesting}
              onClick={handleMarkReadyForTesting}
            >
              {WORKFLOW_COPY.actions.markReadyForTesting}
            </Button>
            {markReadyBlocker && (
              <div className="testing-accept-note">
                <strong>{WORKFLOW_COPY.blockers.prefix}</strong>{markReadyBlocker}
              </div>
            )}
          </div>
        </aside>
      </div>
    </div>
  );
}
