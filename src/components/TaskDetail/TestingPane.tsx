import { useEffect, useMemo, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  PanelRightClose,
  PanelRightOpen,
  RefreshCw,
} from "lucide-react";
import type { CommandRun, ProjectSummary, Task, TerminalSlot } from "../../domain";
import { useAgentBridge } from "../../hooks/useAgentBridge";
import { useCommandBridge } from "../../hooks/useCommandBridge";
import { usePtyBridge } from "../../hooks/usePtyBridge";
import { useProjectPreferencesBridge } from "../../hooks/useProjectPreferencesBridge";
import { useTaskBridge } from "../../hooks/useTaskBridge";
import { useTerminalBridge } from "../../hooks/useTerminalBridge";
import { useAppState } from "../../state/AppStateContext";
import { hasTauriRuntime } from "../../hooks/runtime";
import { FeedbackComposer } from "../../features/testing/FeedbackComposer";
import {
  type AutoTestingLoop,
  type ConversationTurn,
  type QuoteDraft,
  deriveValidationEvidence,
  errorFinding,
  feedbackConversationContent,
  gateStatus,
  latestRunForCommand,
  latestTaskRun as findLatestTaskRun,
  shortTime,
  statusLabel,
} from "../../features/testing/model";
import { RepairCycleHistory } from "../../features/testing/RepairCycleHistory";
import { TerminalGrid } from "../../features/testing/TerminalGrid";
import { TerminalSlotEditor } from "../../features/testing/TerminalSlotEditor";
import { ValidationGate } from "../../features/testing/ValidationGate";
import {
  buildRepairPrompt,
  formatQuotedFeedback,
  hasImplementationCapability,
} from "../../utils/agentRun";
import { parseCommandLine } from "../../utils/commandLine";
import {
  DEFAULT_LOOP_BUDGET,
  MAX_AUTO_REPAIR_ATTEMPTS,
  decideAfterRepair,
  decideAfterValidation,
  escalationNotice,
  isTimedOut,
} from "../../utils/loopPolicy";
import {
  applyTerminalSlotDraft,
  blankTerminalSlotDraft,
  buildDefaultSlots,
  draftFromTerminalSlot,
  resolveTerminalSlotCwd,
  type TerminalSlotDraft,
} from "../../utils/terminalSlots";
import { Button } from "../common/Button";
import "./TaskDetail.css";

interface TestingPaneProps {
  project: ProjectSummary;
  task: Task;
  readOnly?: boolean;
}

export function TestingPane({ project, task, readOnly = false }: TestingPaneProps) {
  const { state } = useAppState();
  const { loadAgents, prepareAgentInvocation } = useAgentBridge();
  const { startCommandRun, stopCommandRun, loadCommandRunLogs } = useCommandBridge();
  const { startPtyRun, stopPtyRun } = usePtyBridge();
  const { loadProjectAgentPreferences } = useProjectPreferencesBridge();
  const { listTerminalSlots, saveTerminalSlots, suggestTerminalSlots } = useTerminalBridge();
  const { appendFeedback, completeTask, generateRepairContext } = useTaskBridge();

  const [slots, setSlots] = useState<TerminalSlot[]>([]);
  const [runIds, setRunIds] = useState<Record<string, string>>({});
  const [draft, setDraft] = useState<TerminalSlotDraft | null>(null);
  const [note, setNote] = useState("");
  const [reproductionSteps, setReproductionSteps] = useState("");
  const [expectedBehavior, setExpectedBehavior] = useState("");
  const [attachmentPaths, setAttachmentPaths] = useState<string[]>([]);
  const [quote, setQuote] = useState<QuoteDraft | null>(null);
  const [autoMode, setAutoMode] = useState(false);
  const [autoLoop, setAutoLoop] = useState<AutoTestingLoop | null>(null);
  const [autoNotice, setAutoNotice] = useState<string | null>(null);
  const [selectedAgentId, setSelectedAgentId] = useState("");
  const [fixRunIds, setFixRunIds] = useState<string[]>([]);
  const [expandedCycle, setExpandedCycle] = useState<string | null>(null);
  const [cyclesOpen, setCyclesOpen] = useState(false);
  const [cockpitOpen, setCockpitOpen] = useState(true);
  const [commandError, setCommandError] = useState<string | null>(null);
  const handledAutoLoopRunsRef = useRef<Set<string>>(new Set());

  const implementationAgents = useMemo(
    () => state.agents.filter(hasImplementationCapability),
    [state.agents],
  );
  const selectedAgent =
    implementationAgents.find((agent) => agent.id === selectedAgentId) ??
    implementationAgents.find((agent) => agent.id === task.primaryAgentId) ??
    implementationAgents[0] ??
    null;

  useEffect(() => {
    void loadAgents();
  }, [loadAgents]);

  useEffect(() => {
    let cancelled = false;
    void loadProjectAgentPreferences(project.path)
      .then((preferences) => {
        const preferredAgentId = preferences.debuggingAgentId ?? preferences.testingAgentId;
        if (!cancelled && preferredAgentId) {
          setSelectedAgentId(preferredAgentId);
        }
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [loadProjectAgentPreferences, project.path]);

  useEffect(() => {
    if (!selectedAgentId && selectedAgent) {
      setSelectedAgentId(selectedAgent.id);
    }
  }, [selectedAgent, selectedAgentId]);

  // Load persisted slots; on first visit seed project-aware defaults (scanned
  // from real manifests, incl. monorepo subdir apps) and persist them.
  useEffect(() => {
    let cancelled = false;
    void listTerminalSlots(project.path).then(async (loaded) => {
      if (cancelled) {
        return;
      }
      if (loaded.length > 0) {
        setSlots(loaded);
        return;
      }
      const suggested = await suggestTerminalSlots(project.path);
      if (cancelled) {
        return;
      }
      const defaults = suggested.length > 0 ? suggested : buildDefaultSlots(project);
      setSlots(defaults);
      void saveTerminalSlots(project.path, defaults);
    });
    return () => {
      cancelled = true;
    };
  }, [project, listTerminalSlots, saveTerminalSlots, suggestTerminalSlots]);

  function persistSlots(next: TerminalSlot[]) {
    setSlots(next);
    void saveTerminalSlots(project.path, next);
  }

  function slotRun(slot: TerminalSlot) {
    const trackedId = runIds[slot.id];
    return (
      (trackedId && state.commandRuns.find((run) => run.id === trackedId)) ||
      latestRunForCommand(state.commandRuns, task.id, slot.command)
    );
  }

  const validationCommands = useMemo(
    () => new Set(slots.filter((slot) => slot.kind === "validation").map((slot) => slot.command)),
    [slots],
  );
  const taskRuns = state.commandRuns
    .filter((run) => run.taskId === task.id)
    .sort((left, right) => left.startedAtMs - right.startedAtMs);
  const visibleLogRunIds = useMemo(() => {
    const ids = new Set<string>();
    for (const slot of slots) {
      const trackedId = runIds[slot.id];
      const run =
        (trackedId && state.commandRuns.find((candidate) => candidate.id === trackedId)) ||
        latestRunForCommand(state.commandRuns, task.id, slot.command);
      if (run) {
        ids.add(run.id);
      }
    }
    if (expandedCycle) {
      ids.add(expandedCycle);
    }
    return [...ids];
  }, [expandedCycle, runIds, slots, state.commandRuns, task.id]);

  useEffect(() => {
    if (!hasTauriRuntime()) {
      return;
    }
    for (const runId of visibleLogRunIds) {
      const run = state.commandRuns.find((candidate) => candidate.id === runId);
      if (
        run &&
        state.commandLogs[runId] === undefined &&
        (run.stdoutLogRef || run.stderrLogRef)
      ) {
        void loadCommandRunLogs(project.path, task.id, runId);
      }
    }
  }, [loadCommandRunLogs, project.path, state.commandLogs, state.commandRuns, task.id, visibleLogRunIds]);
  const validationEvidence = deriveValidationEvidence(state.commandRuns, task.id, slots);
  const failedRun = validationEvidence.failedRun;
  const latestTaskRun = findLatestTaskRun(state.commandRuns, task.id);
  const successfulValidationRun = validationEvidence.successfulRun;
  const latestBlockingFailure = validationEvidence.blockingFailure;
  const hasValidationEvidence = validationEvidence.hasEvidence;
  const hasRunningValidationRun = slots
    .filter((slot) => slot.kind === "validation")
    .some((slot) => slotRun(slot)?.status === "running");
  const hasPassingEvidence = validationEvidence.hasPassingEvidence;
  const gate = gateStatus({
    hasPassingEvidence,
    hasRunningValidationRun,
    latestBlockingFailure,
    hasValidationEvidence,
  });
  const canAccept =
    task.status === "verifying" && hasPassingEvidence && !hasRunningValidationRun && !readOnly;
  const validationCommandLabel =
    [...validationCommands].join("  ·  ") || "No validation command configured";
  const validationSlots = useMemo(
    () => slots.filter((slot) => slot.kind === "validation"),
    [slots],
  );
  const autoLoopRuns = useMemo(
    () =>
      autoLoop
        ? state.commandRuns
            .filter((run) => run.taskId === task.id && run.loopId === autoLoop.loopId)
            .sort((left, right) => left.startedAtMs - right.startedAtMs)
        : [],
    [autoLoop, state.commandRuns, task.id],
  );
  const latestAutoCompletedRun =
    autoLoopRuns.filter((run) => run.status !== "running").slice(-1)[0] ?? null;
  const runningAutoLoopRun = autoLoopRuns.find((run) => run.status === "running") ?? null;

  function slotCwd(slot: TerminalSlot) {
    return resolveTerminalSlotCwd(project.path, slot);
  }

  function validationSlotForRun(run: CommandRun) {
    return (
      validationSlots.find((slot) => slot.command === run.command) ??
      validationSlots[0] ??
      null
    );
  }

  async function startAutoValidationRun(loop: AutoTestingLoop) {
    try {
      const parsed = parseCommandLine(loop.validationCommand);
      if (!parsed.program) {
        setAutoLoop((current) =>
          current?.loopId === loop.loopId ? { ...current, status: "escalated" } : current,
        );
        setAutoNotice("Auto repair stopped because the validation command is empty.");
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
        iteration: loop.repairAttempts + 1,
        attempt: loop.repairAttempts,
      });
      if (run && loop.validationSlotId) {
        setRunIds((prev) => ({ ...prev, [loop.validationSlotId as string]: run.id }));
      }
      setAutoNotice(
        run
          ? `Auto validation started: ${loop.validationCommand}`
          : "Auto validation failed to start.",
      );
      if (!run) {
        setAutoLoop((current) =>
          current?.loopId === loop.loopId ? { ...current, status: "escalated" } : current,
        );
      }
      return run;
    } catch (error) {
      setAutoLoop((current) =>
        current?.loopId === loop.loopId ? { ...current, status: "escalated" } : current,
      );
      setAutoNotice(error instanceof Error ? error.message : "Invalid validation command.");
      return null;
    }
  }

  async function runSlot(slot: TerminalSlot) {
    if (!slot.command.trim()) {
      // Unconfigured slot — open the editor instead of running an empty command.
      setDraft(draftFromTerminalSlot(slot));
      return;
    }
    try {
      const parsed = parseCommandLine(slot.command);
      const cwd = slotCwd(slot);
      setCommandError(null);
      // Preview = long-running dev server → PTY (ANSI colors, no errorSummary).
      // Validation = one-shot check → piped, so its stdout/stderr feed the gate.
      const run =
        slot.kind === "preview"
          ? await startPtyRun({
              program: parsed.program,
              args: parsed.args,
              cwd,
              projectPath: project.path,
              taskId: task.id,
              rows: 24,
              cols: 80,
            })
          : await startCommandRun({
              program: parsed.program,
              args: parsed.args,
              cwd,
              projectPath: project.path,
              taskId: task.id,
              intent: "validation",
            });
      if (run) {
        setRunIds((prev) => ({ ...prev, [slot.id]: run.id }));
      }
    } catch (error) {
      setCommandError(error instanceof Error ? error.message : "Invalid command");
    }
  }

  async function stopSlot(slot: TerminalSlot, run?: CommandRun) {
    if (!run) {
      return;
    }
    if (slot.kind === "preview") {
      await stopPtyRun(run.id);
    } else {
      await stopCommandRun(run.id);
    }
  }

  async function runAllValidations() {
    for (const slot of slots.filter((candidate) => candidate.kind === "validation")) {
      if (slotRun(slot)?.status !== "running") {
        await runSlot(slot);
      }
    }
  }

  function saveDraft() {
    if (!draft) {
      return;
    }
    const next = applyTerminalSlotDraft(slots, draft);
    if (!next) {
      return;
    }
    persistSlots(next);
    setDraft(null);
  }

  function removeSlot(slot: TerminalSlot) {
    persistSlots(slots.filter((candidate) => candidate.id !== slot.id));
  }

  async function resetToDetected() {
    const suggested = await suggestTerminalSlots(project.path);
    persistSlots(suggested.length > 0 ? suggested : buildDefaultSlots(project));
    setRunIds({});
  }

  function handleQuote(text: string, command: string) {
    setQuote({ text, command, failedRunId: latestBlockingFailure?.id });
  }

  async function startFixRun(
    noteText: string,
    quoteDraft: QuoteDraft | null,
    auto?: { loop: AutoTestingLoop; failedRun: CommandRun; attempt: number },
  ) {
    if (!selectedAgent) {
      setCommandError("No implementation agent available to run a fix.");
      if (auto) {
        setAutoLoop((current) =>
          current?.loopId === auto.loop.loopId ? { ...current, status: "escalated" } : current,
        );
        setAutoNotice("Auto repair stopped because no implementation agent is available.");
      }
      return;
    }
    setCommandError(null);
    // Refresh the repair context (packages logs / todo / feedback) so the
    // prompt carries the latest evidence, then hand off to the agent.
    const refreshed = (await generateRepairContext(project.path, task.id)) ?? task;
    const prompt = buildRepairPrompt(refreshed, quoteDraft?.text ?? "", noteText);
    if (auto) {
      setAutoLoop((current) =>
        current?.loopId === auto.loop.loopId
          ? { ...current, repairAttempts: auto.attempt, status: "repairing" }
          : current,
      );
    }
    const invocation = await prepareAgentInvocation({
      projectPath: project.path,
      taskId: task.id,
      agentId: selectedAgent.id,
      stage: "debugging",
      prompt,
    });
    if (!invocation) {
      setCommandError("The backend rejected the selected Agent invocation.");
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
      loopId: auto?.loop.loopId,
      iteration: auto?.attempt,
      attempt: auto?.attempt,
    });
    if (run) {
      setFixRunIds((prev) => [...prev, run.id]);
      if (auto) {
        setAutoNotice(`Auto repair attempt ${auto.attempt}/${MAX_AUTO_REPAIR_ATTEMPTS} started.`);
      }
    } else if (auto) {
      setAutoLoop((current) =>
        current?.loopId === auto.loop.loopId ? { ...current, status: "escalated" } : current,
      );
      setAutoNotice("Auto repair failed to start.");
    }
  }

  async function handleAskAgent() {
    if (
      !note.trim() &&
      !reproductionSteps.trim() &&
      !expectedBehavior.trim() &&
      !quote?.text.trim() &&
      attachmentPaths.length === 0
    ) {
      return;
    }
    const quotedLog = quote?.text.trim()
      ? formatQuotedFeedback("", quote.text, quote.command)
      : undefined;
    const persistedFeedback = await appendFeedback(
      project.path,
      task.id,
      quote?.failedRunId ?? latestBlockingFailure?.id,
      {
        content: note.trim(),
        reproductionSteps: reproductionSteps.trim() || undefined,
        expectedBehavior: expectedBehavior.trim() || undefined,
        quotedLog,
        attachmentPaths,
      },
    );
    if (!persistedFeedback) {
      setCommandError("Feedback or attachment validation failed; no repair Agent was started.");
      return;
    }
    const structuredNote = [
      note.trim(),
      reproductionSteps.trim() ? `Reproduction steps:\n${reproductionSteps.trim()}` : "",
      expectedBehavior.trim() ? `Expected behavior:\n${expectedBehavior.trim()}` : "",
    ].filter(Boolean).join("\n\n");
    await startFixRun(structuredNote, quote);
    setNote("");
    setReproductionSteps("");
    setExpectedBehavior("");
    setAttachmentPaths([]);
    setQuote(null);
  }

  async function selectFeedbackAttachments() {
    if (!hasTauriRuntime()) {
      return;
    }
    const selected = await open({
      multiple: true,
      directory: false,
      filters: [{
        name: "Debug evidence",
        extensions: ["png", "jpg", "jpeg", "webp", "gif", "txt", "log", "md", "json", "pdf"],
      }],
    });
    const paths = Array.isArray(selected) ? selected : selected ? [selected] : [];
    setAttachmentPaths((current) => [...new Set([...current, ...paths])].slice(0, 8));
  }

  async function startAutoLoopFromFailure(failureRun: CommandRun) {
    const validationSlot = validationSlotForRun(failureRun);
    if (!validationSlot) {
      setAutoNotice("Auto repair stopped because no validation command is configured.");
      return;
    }

    const startedAtMs = Date.now();
    const loop: AutoTestingLoop = {
      loopId: `loop-${task.id}-testing-${failureRun.id}-${startedAtMs}`,
      sourceFailureRunId: failureRun.id,
      validationSlotId: validationSlot.id,
      validationCommand: validationSlot.command.trim(),
      validationCwd: slotCwd(validationSlot),
      repairAttempts: 0,
      repeatedFailureCount: 0,
      startedAtMs,
      status: "repairing",
    };
    const decision = decideAfterValidation(
      loop,
      failureRun,
      DEFAULT_LOOP_BUDGET,
      startedAtMs,
    );

    handledAutoLoopRunsRef.current = new Set();
    if (decision.kind === "pass") {
      setAutoLoop({ ...loop, status: "passed" });
      setAutoNotice("Auto validation passed. The task has passing evidence.");
      return;
    }
    if (decision.kind === "escalate") {
      setAutoLoop({
        ...loop,
        lastFailureFingerprint: decision.fingerprint,
        repeatedFailureCount: decision.repeatedFailureCount ?? loop.repeatedFailureCount,
        status: "escalated",
      });
      setAutoNotice(escalationNotice(decision.reason, DEFAULT_LOOP_BUDGET));
      return;
    }

    const nextLoop: AutoTestingLoop = {
      ...loop,
      lastFailureFingerprint: decision.fingerprint,
      repeatedFailureCount: decision.repeatedFailureCount,
      repairAttempts: decision.attempt,
      status: "repairing",
    };
    setAutoLoop(nextLoop);
    await startFixRun(
      `Auto repair attempt ${decision.attempt}/${MAX_AUTO_REPAIR_ATTEMPTS}: validation failed in \`${failureRun.command}\`.`,
      null,
      { loop: nextLoop, failedRun: failureRun, attempt: decision.attempt },
    );
  }

  // Auto mode: a fresh blocking failure enters the shared bounded loop policy.
  useEffect(() => {
    if (!autoMode || readOnly || !latestBlockingFailure) {
      return;
    }
    if (autoLoop?.sourceFailureRunId === latestBlockingFailure.id) {
      return;
    }
    if (autoLoop && autoLoop.status !== "passed" && autoLoop.status !== "escalated") {
      return;
    }
    void startAutoLoopFromFailure(latestBlockingFailure);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [autoMode, readOnly, latestBlockingFailure?.id, autoLoop?.sourceFailureRunId, autoLoop?.status]);

  useEffect(() => {
    if (
      !autoMode ||
      readOnly ||
      !autoLoop ||
      autoLoop.status === "passed" ||
      autoLoop.status === "escalated"
    ) {
      return;
    }

    const tick = () => {
      const timedOut = isTimedOut(autoLoop, DEFAULT_LOOP_BUDGET, Date.now());
      if (!timedOut) {
        return;
      }

      setAutoLoop((current) =>
        current?.loopId === autoLoop.loopId ? { ...current, status: "escalated" } : current,
      );
      setAutoNotice(escalationNotice("timeout", DEFAULT_LOOP_BUDGET));
      if (runningAutoLoopRun) {
        void stopCommandRun(runningAutoLoopRun.id, "timeout");
      }
    };

    tick();
    const timer = window.setInterval(tick, 1_000);
    return () => window.clearInterval(timer);
  }, [autoMode, autoLoop, readOnly, runningAutoLoopRun, stopCommandRun]);

  useEffect(() => {
    if (
      !autoMode ||
      readOnly ||
      !autoLoop ||
      autoLoop.status === "passed" ||
      autoLoop.status === "escalated" ||
      !latestAutoCompletedRun
    ) {
      return;
    }
    if (handledAutoLoopRunsRef.current.has(latestAutoCompletedRun.id)) {
      return;
    }
    handledAutoLoopRunsRef.current.add(latestAutoCompletedRun.id);

    if (latestAutoCompletedRun.intent === "validation") {
      const decision = decideAfterValidation(
        autoLoop,
        latestAutoCompletedRun,
        DEFAULT_LOOP_BUDGET,
        Date.now(),
      );
      if (decision.kind === "pass") {
        setAutoLoop({ ...autoLoop, sourceFailureRunId: latestAutoCompletedRun.id, status: "passed" });
        setAutoNotice("Auto validation passed. The task has passing evidence.");
        return;
      }
      if (decision.kind === "escalate") {
        setAutoLoop({
          ...autoLoop,
          sourceFailureRunId: latestAutoCompletedRun.id,
          lastFailureFingerprint: decision.fingerprint ?? autoLoop.lastFailureFingerprint,
          repeatedFailureCount: decision.repeatedFailureCount ?? autoLoop.repeatedFailureCount,
          status: "escalated",
        });
        setAutoNotice(escalationNotice(decision.reason, DEFAULT_LOOP_BUDGET));
        return;
      }

      const nextLoop: AutoTestingLoop = {
        ...autoLoop,
        sourceFailureRunId: latestAutoCompletedRun.id,
        lastFailureFingerprint: decision.fingerprint,
        repeatedFailureCount: decision.repeatedFailureCount,
        repairAttempts: decision.attempt,
        status: "repairing",
      };
      setAutoLoop(nextLoop);
      void startFixRun(
        `Auto repair attempt ${decision.attempt}/${MAX_AUTO_REPAIR_ATTEMPTS}: validation failed in \`${latestAutoCompletedRun.command}\`.`,
        null,
        { loop: nextLoop, failedRun: latestAutoCompletedRun, attempt: decision.attempt },
      );
      return;
    }

    if (latestAutoCompletedRun.intent === "agent_action") {
      const decision = decideAfterRepair(autoLoop, latestAutoCompletedRun, DEFAULT_LOOP_BUDGET, Date.now());
      if (decision.kind === "escalate") {
        setAutoLoop({ ...autoLoop, status: "escalated" });
        setAutoNotice(escalationNotice(decision.reason, DEFAULT_LOOP_BUDGET));
        return;
      }

      const nextLoop: AutoTestingLoop = { ...autoLoop, status: "validating" };
      setAutoLoop(nextLoop);
      void startAutoValidationRun(nextLoop);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [autoMode, readOnly, autoLoop, latestAutoCompletedRun?.id]);

  const conversation = useMemo<ConversationTurn[]>(() => {
    const humanTurns: ConversationTurn[] = task.feedback.map((entry) => ({
      id: entry.id,
      role: "human",
      timestampMs: entry.timestampMs,
      content: feedbackConversationContent(entry),
    }));
    const agentTurns: ConversationTurn[] = fixRunIds
      .map((id) => state.commandRuns.find((run) => run.id === id))
      .filter((run): run is CommandRun => Boolean(run))
      .map((run) => ({
        id: run.id,
        role: "agent",
        timestampMs: run.startedAtMs,
        content: run.command,
        run,
      }));
    return [...humanTurns, ...agentTurns].sort((left, right) => left.timestampMs - right.timestampMs);
  }, [task.feedback, fixRunIds, state.commandRuns]);

  async function handleAccept() {
    await completeTask(project.path, task.id);
  }

  return (
    <div className="testing-pane">
      <div className="testing-header">
        <div className="testing-title-block">
          <div className="testing-kicker">调试验收</div>
          <div className="testing-title-row">
            <span className={`testing-status-pill testing-status-${gate.tone}`}>
              <span />
              {gate.title}
            </span>
            <span className="testing-header-meta">
              最后活动：
              {latestTaskRun
                ? `${latestTaskRun.command} · ${shortTime(latestTaskRun.startedAtMs)}`
                : "暂无"}
            </span>
          </div>
        </div>
        <div className="testing-header-actions">
          {!readOnly && (
            <Button
              type="button"
              variant="ghost"
              iconLeft={<RefreshCw size={14} />}
              onClick={runAllValidations}
            >
              运行检查
            </Button>
          )}
          <button
            type="button"
            className="testing-cockpit-toggle"
            title={cockpitOpen ? "Hide testing cockpit" : "Show testing cockpit"}
            onClick={() => setCockpitOpen((open) => !open)}
          >
            {cockpitOpen ? <PanelRightClose size={16} /> : <PanelRightOpen size={16} />}
          </button>
        </div>
      </div>

      <div className={`testing-grid${cockpitOpen ? "" : " cockpit-collapsed"}`}>
        <TerminalGrid
          slots={slots}
          readOnly={readOnly}
          commandError={commandError ?? state.app.commandError}
          autoNotice={autoNotice}
          slotRun={slotRun}
          logsForRun={(runId) => state.commandLogs[runId] ?? []}
          onRun={(slot) => void runSlot(slot)}
          onStop={(slot, run) => void stopSlot(slot, run)}
          onEdit={(slot) => setDraft(draftFromTerminalSlot(slot))}
          onRemove={removeSlot}
          onQuote={handleQuote}
          onAdd={() => setDraft(blankTerminalSlotDraft())}
          onReset={() => void resetToDetected()}
        />

        {cockpitOpen && (
        <aside className="debug-agent-panel">
          <div className="debug-agent-header">
            <div className="debug-agent-avatar">AI</div>
            <span>调试验收 Cockpit</span>
            <span className="testing-panel-state">{task.status}</span>
          </div>

          <div className="debug-agent-body">
            <ValidationGate
              gate={gate}
              validationCommandLabel={validationCommandLabel}
              successfulRun={successfulValidationRun}
              failedRun={failedRun}
              blockingFailure={latestBlockingFailure}
              canAccept={canAccept}
              readOnly={readOnly}
              onAccept={() => void handleAccept()}
            />

            <section className={`debug-card ${latestBlockingFailure ? "debug-card-error" : ""}`}>
              <div className="debug-card-label">
                <AlertTriangle size={14} />
                失败信号
              </div>
              <strong>{latestBlockingFailure ? latestBlockingFailure.command : "无活动失败"}</strong>
              <p>
                {latestBlockingFailure
                  ? errorFinding(latestBlockingFailure)
                  : "没有比最近通过验证更新的失败命令。"}
              </p>
            </section>

            <section className="debug-card fix-cycle-card">
              <div className="debug-card-label">
                <RefreshCw size={14} />
                自动修复循环
              </div>
              <div className="fix-cycle">
                <div className={`fix-step ${latestBlockingFailure || taskRuns.length > 0 ? "is-done" : ""}`}>
                  <span className="fix-step-no">1</span>
                  <div className="fix-step-body">
                    <span className="fix-step-name">发现问题</span>
                    <span className="fix-step-detail loom-mono">
                      {latestBlockingFailure ? errorFinding(latestBlockingFailure) : "等待失败日志"}
                    </span>
                  </div>
                </div>
                <span className="fix-arrow" aria-hidden="true">›</span>
                <div className={`fix-step ${fixRunIds.length > 0 || autoLoop?.repairAttempts ? "is-done" : ""}`}>
                  <span className="fix-step-no">2</span>
                  <div className="fix-step-body">
                    <span className="fix-step-name">修复</span>
                    <span className="fix-step-detail loom-mono">
                      {autoLoop ? `${autoLoop.repairAttempts}/${MAX_AUTO_REPAIR_ATTEMPTS} 轮` : "手动或自动投喂 Agent"}
                    </span>
                  </div>
                </div>
                <span className="fix-arrow" aria-hidden="true">›</span>
                <div className={`fix-step is-final ${hasPassingEvidence ? "is-done" : ""}`}>
                  <span className="fix-step-no">3</span>
                  <div className="fix-step-body">
                    <span className="fix-step-name">再验证</span>
                    <span className="fix-step-detail loom-mono">
                      {successfulValidationRun ? statusLabel(successfulValidationRun) : "等待通过证据"}
                    </span>
                  </div>
                </div>
              </div>
            </section>

            <FeedbackComposer
              readOnly={readOnly}
              agents={implementationAgents}
              selectedAgent={selectedAgent}
              autoMode={autoMode}
              conversation={conversation}
              logs={state.commandLogs}
              quote={quote}
              note={note}
              reproductionSteps={reproductionSteps}
              expectedBehavior={expectedBehavior}
              attachmentPaths={attachmentPaths}
              onAgentChange={setSelectedAgentId}
              onModeChange={(enabled) => {
                setAutoMode(enabled);
                if (!enabled) setAutoLoop(null);
                setAutoNotice(null);
              }}
              onQuoteClear={() => setQuote(null)}
              onNoteChange={setNote}
              onReproductionStepsChange={setReproductionSteps}
              onExpectedBehaviorChange={setExpectedBehavior}
              onSelectAttachments={() => void selectFeedbackAttachments()}
              onRemoveAttachment={(path) =>
                setAttachmentPaths((current) => current.filter((candidate) => candidate !== path))
              }
              onSubmit={() => void handleAskAgent()}
            />

            <RepairCycleHistory
              runs={taskRuns}
              logs={state.commandLogs}
              open={cyclesOpen}
              expandedRunId={expandedCycle}
              onToggleOpen={() => setCyclesOpen((open) => !open)}
              onToggleRun={(runId) => setExpandedCycle((current) => current === runId ? null : runId)}
            />
          </div>
        </aside>
        )}
      </div>

      {draft && (
        <TerminalSlotEditor
          draft={draft}
          setDraft={setDraft}
          onSave={saveDraft}
          onClose={() => setDraft(null)}
        />
      )}
    </div>
  );
}
