import { useEffect, useMemo, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  Bot,
  CheckCircle2,
  ChevronDown,
  Clock3,
  PanelRightClose,
  PanelRightOpen,
  Paperclip,
  Quote,
  RefreshCw,
  RotateCcw,
  Send,
  ShieldCheck,
  Plus,
  User,
  X,
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
import {
  buildRepairPrompt,
  formatQuotedFeedback,
  hasImplementationCapability,
} from "../../utils/agentRun";
import { parseCommandLine } from "../../utils/commandLine";
import {
  isFailedValidationRun,
  isSuccessfulValidationRun,
  isValidationRun,
} from "../../utils/commandRun";
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
import { TerminalCard } from "./TerminalCard";
import "./TaskDetail.css";

interface QuoteDraft {
  text: string;
  command: string;
  failedRunId?: string;
}

interface ConversationTurn {
  id: string;
  role: "human" | "agent";
  timestampMs: number;
  content: string;
  run?: CommandRun;
}

interface AutoTestingLoop {
  loopId: string;
  sourceFailureRunId: string;
  validationSlotId?: string;
  validationCommand: string;
  validationCwd: string;
  repairAttempts: number;
  lastFailureFingerprint?: string;
  repeatedFailureCount: number;
  startedAtMs: number;
  status: "repairing" | "validating" | "passed" | "escalated";
}

function feedbackConversationContent(feedback: Task["feedback"][number]) {
  return [
    feedback.content,
    feedback.reproductionSteps ? `复现步骤:\n${feedback.reproductionSteps}` : "",
    feedback.expectedBehavior ? `期望行为:\n${feedback.expectedBehavior}` : "",
    feedback.quotedLog ? `引用日志:\n${feedback.quotedLog}` : "",
    feedback.attachments?.length
      ? `附件:\n${feedback.attachments.map((attachment) => `- ${attachment.name}`).join("\n")}`
      : "",
  ].filter(Boolean).join("\n\n");
}

interface TestingPaneProps {
  project: ProjectSummary;
  task: Task;
  readOnly?: boolean;
}

function slotEndpoint(slot: TerminalSlot, run?: CommandRun) {
  return run?.errorSummary?.urls?.[0]
    ?? run?.errorSummary?.ports?.[0]?.toString()
    ?? (slot.kind === "preview" ? "localhost:1420" : "exit code required");
}

function slotEmptyMessage(slot: TerminalSlot) {
  return slot.kind === "preview"
    ? "Start the preview command to stream runtime logs and keep a live surface for manual checks."
    : "Run this check to create acceptance evidence: command, cwd, stdout/stderr logs, and exit status.";
}

function latestRunForCommand(runs: CommandRun[], taskId: string, command: string) {
  return runs
    .filter((run) => run.taskId === taskId && run.command === command)
    .sort((left, right) => right.startedAtMs - left.startedAtMs)[0];
}

function latestSuccessfulValidationRun(
  runs: CommandRun[],
  taskId: string,
  validationCommands: Set<string>,
) {
  return runs
    .filter(
      (run) =>
        run.taskId === taskId && isSuccessfulValidationRun(run, validationCommands),
    )
    .sort((left, right) => right.startedAtMs - left.startedAtMs)[0];
}

function latestFailedRun(runs: CommandRun[], taskId: string, validationCommands: Set<string>) {
  return runs
    .filter((run) => run.taskId === taskId && isFailedValidationRun(run, validationCommands))
    .sort((left, right) => right.startedAtMs - left.startedAtMs)[0];
}

function latestRun(runs: CommandRun[], taskId: string) {
  return runs
    .filter((run) => run.taskId === taskId)
    .sort((left, right) => right.startedAtMs - left.startedAtMs)[0];
}

function cycleLabel(run: CommandRun, index: number) {
  const started = new Date(run.startedAtMs).toLocaleTimeString("en-US", {
    hour: "2-digit",
    minute: "2-digit",
  });
  return `Cycle ${index + 1} · ${run.status} · ${started}`;
}

function shortTime(timestampMs?: number) {
  if (!timestampMs) {
    return "not captured";
  }
  return new Date(timestampMs).toLocaleTimeString("en-US", {
    hour: "2-digit",
    minute: "2-digit",
  });
}

function statusLabel(run?: CommandRun) {
  if (!run) {
    return "Not run";
  }
  if (typeof run.exitCode === "number") {
    return `${run.status} · exit ${run.exitCode}`;
  }
  return run.status;
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

function gateStatus({
  hasPassingEvidence,
  hasRunningValidationRun,
  latestBlockingFailure,
  hasValidationEvidence,
}: {
  hasPassingEvidence: boolean;
  hasRunningValidationRun: boolean;
  latestBlockingFailure?: CommandRun;
  hasValidationEvidence: boolean;
}) {
  if (latestBlockingFailure) {
    return {
      tone: "err",
      title: "需要修复",
      copy: "有新的失败验证阻塞验收，需要在其后重新跑通验证命令。",
    };
  }
  if (hasPassingEvidence) {
    return {
      tone: "ok",
      title: "验证已通过",
      copy: "已有成功验证命令，且没有更新的失败运行阻塞验收。",
    };
  }
  if (hasRunningValidationRun) {
    return {
      tone: "running",
      title: "检查运行中",
      copy: "等待验证命令结束后再验收任务。",
    };
  }
  if (hasValidationEvidence) {
    return {
      tone: "idle",
      title: "验证未通过",
      copy: "重新运行验证命令，并捕获成功退出后再验收。",
    };
  }
  return {
    tone: "idle",
    title: "暂无验证证据",
    copy: "运行验证命令以捕获 stdout、stderr、退出码和日志引用。",
  };
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
  const failedRun = latestFailedRun(state.commandRuns, task.id, validationCommands);
  const latestTaskRun = latestRun(state.commandRuns, task.id);
  const successfulValidationRun = latestSuccessfulValidationRun(
    state.commandRuns,
    task.id,
    validationCommands,
  );
  const latestBlockingFailure =
    failedRun &&
    (!successfulValidationRun || failedRun.startedAtMs > successfulValidationRun.startedAtMs)
      ? failedRun
      : undefined;
  const hasValidationEvidence = state.commandRuns.some(
    (run) => run.taskId === task.id && isValidationRun(run, validationCommands),
  );
  const hasRunningValidationRun = slots
    .filter((slot) => slot.kind === "validation")
    .some((slot) => slotRun(slot)?.status === "running");
  const hasPassingEvidence = Boolean(successfulValidationRun && !latestBlockingFailure);
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
        <div className="testing-terminals">
          {slots.map((slot) => {
            const run = slotRun(slot);
            return (
              <TerminalCard
                key={slot.id}
                title={slot.name}
                command={slot.command}
                endpoint={slotEndpoint(slot, run)}
                tone={slot.kind === "preview" ? "frontend" : "validation"}
                mode={slot.kind === "preview" ? "pty" : "logs"}
                emptyMessage={slotEmptyMessage(slot)}
                run={run}
                logs={slot.kind === "validation" && run ? state.commandLogs[run.id] ?? [] : []}
                onRun={() => runSlot(slot)}
                onStop={() => stopSlot(slot, run)}
                      onEdit={
                  readOnly
                    ? undefined
                    : () => setDraft(draftFromTerminalSlot(slot))
                }
                onRemove={readOnly ? undefined : () => removeSlot(slot)}
                onQuote={readOnly ? undefined : handleQuote}
                disabled={readOnly}
              />
            );
          })}

          {!readOnly && (
            <div className="testing-terminal-actions">
              <button
                type="button"
                className="testing-add-terminal"
                onClick={() => setDraft(blankTerminalSlotDraft())}
              >
                <Plus size={14} />
                Add terminal
              </button>
              <button
                type="button"
                className="testing-add-terminal"
                title="Re-scan the project and replace terminals with detected commands"
                onClick={() => void resetToDetected()}
              >
                <RotateCcw size={13} />
                Reset to detected
              </button>
            </div>
          )}

          {(commandError || state.app.commandError) && (
            <div className="testing-inline-error">{commandError ?? state.app.commandError}</div>
          )}
          {autoNotice && <div className="testing-inline-note">{autoNotice}</div>}
        </div>

        {cockpitOpen && (
        <aside className="debug-agent-panel">
          <div className="debug-agent-header">
            <div className="debug-agent-avatar">AI</div>
            <span>调试验收 Cockpit</span>
            <span className="testing-panel-state">{task.status}</span>
          </div>

          <div className="debug-agent-body">
            <section className={`debug-card testing-gate-card gate-${gate.tone}`}>
              <div className="debug-card-label">
                <ShieldCheck size={14} />
                验收门禁
              </div>
              <strong>{gate.title}</strong>
              <p>{gate.copy}</p>
              <div className="testing-evidence-grid">
                <div>
                  <span>验证命令</span>
                  <strong>{validationCommandLabel}</strong>
                </div>
                <div>
                  <span>最近验证</span>
                  <strong>{statusLabel(successfulValidationRun ?? failedRun)}</strong>
                </div>
                <div>
                  <span>通过证据</span>
                  <strong>
                    {successfulValidationRun ? shortTime(successfulValidationRun.endedAtMs) : "缺失"}
                  </strong>
                </div>
                <div>
                  <span>更新失败</span>
                  <strong>
                    {latestBlockingFailure ? shortTime(latestBlockingFailure.startedAtMs) : "无"}
                  </strong>
                </div>
              </div>
              {!readOnly && (
                <>
                  <Button
                    type="button"
                    variant="primary"
                    iconLeft={<CheckCircle2 size={14} />}
                    disabled={!canAccept}
                    onClick={handleAccept}
                  >
                    全部通过
                  </Button>
                  {!canAccept && (
                    <div className="testing-accept-note">
                      需要验证命令成功，且没有更新失败后才能验收。
                    </div>
                  )}
                </>
              )}
            </section>

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

            <section className="debug-card testing-chat-card">
              <div className="testing-chat-head">
                <div className="debug-card-label">
                  <Bot size={14} />
                  调试 Agent
                </div>
                <div className="testing-chat-controls">
                  <select
                    className="testing-agent-select"
                    value={selectedAgent?.id ?? ""}
                    disabled={implementationAgents.length === 0 || readOnly}
                    onChange={(event) => setSelectedAgentId(event.target.value)}
                  >
                    {implementationAgents.length === 0 && <option value="">No agent</option>}
                    {implementationAgents.map((agent) => (
                      <option value={agent.id} key={agent.id}>
                        {agent.name}
                      </option>
                    ))}
                  </select>
                  <div className="testing-mode-toggle" role="group" aria-label="Repair mode">
                    <button
                      type="button"
                      className={autoMode ? "" : "active"}
                      disabled={readOnly}
                      onClick={() => {
                        setAutoMode(false);
                        setAutoLoop(null);
                        setAutoNotice(null);
                      }}
                    >
                      手动
                    </button>
                    <button
                      type="button"
                      className={autoMode ? "active" : ""}
                      disabled={readOnly}
                      onClick={() => {
                        setAutoMode(true);
                        setAutoNotice(null);
                      }}
                    >
                      自动
                    </button>
                  </div>
                </div>
              </div>

              <div className="testing-chat-stream">
                {conversation.length === 0 ? (
                  <p className="testing-chat-empty">
                    选中终端日志并引用到这里，或直接写反馈给 Agent 修复。
                  </p>
                ) : (
                  conversation.map((turn) => (
                    <div className={`testing-chat-turn turn-${turn.role}`} key={`${turn.role}-${turn.id}`}>
                      <div className="testing-chat-avatar">
                        {turn.role === "human" ? <User size={13} /> : <Bot size={13} />}
                      </div>
                      <div className="testing-chat-bubble">
                        {turn.role === "agent" && turn.run ? (
                          <>
                            <div className="testing-chat-runline">
                              <span className={`testing-status-pill testing-status-${
                                turn.run.status === "running"
                                  ? "running"
                                  : turn.run.status === "succeeded"
                                    ? "ok"
                                    : turn.run.status === "failed"
                                      ? "err"
                                      : "idle"
                              }`}>
                                <span />
                                {turn.run.status}
                              </span>
                              <span className="testing-chat-runcmd">fix run</span>
                            </div>
                            <pre className="testing-chat-runlog">
                              {(state.commandLogs[turn.run.id] ?? [])
                                .slice(-12)
                                .map((entry) => entry.line)
                                .join("\n") || "Agent started. Waiting for output…"}
                            </pre>
                          </>
                        ) : (
                          <pre className="testing-chat-text">{turn.content}</pre>
                        )}
                      </div>
                    </div>
                  ))
                )}
              </div>

              {quote && (
                <div className="testing-chat-quote">
                  <Quote size={12} />
                  <div className="testing-chat-quote-body">
                    <span className="testing-chat-quote-cmd">{quote.command}</span>
                    <pre>{quote.text.trim() || "(empty selection)"}</pre>
                  </div>
                  <button type="button" title="Remove quote" onClick={() => setQuote(null)}>
                    <X size={12} />
                  </button>
                </div>
              )}

              {!readOnly && (
                <div className="testing-chat-composer">
                  <textarea
                    value={note}
                    onChange={(event) => setNote(event.target.value)}
                    placeholder="告诉 Agent 哪里不对，也可以从终端引用日志定位问题…"
                  />
                  <textarea
                    className="testing-chat-structured-field"
                    value={reproductionSteps}
                    onChange={(event) => setReproductionSteps(event.target.value)}
                    placeholder="复现步骤（可选）"
                  />
                  <textarea
                    className="testing-chat-structured-field"
                    value={expectedBehavior}
                    onChange={(event) => setExpectedBehavior(event.target.value)}
                    placeholder="期望行为（可选）"
                  />
                  <div className="testing-chat-attachments">
                    <button type="button" onClick={() => void selectFeedbackAttachments()}>
                      <Paperclip size={13} /> 添加截图或文件
                    </button>
                    {attachmentPaths.map((path) => (
                      <span key={path} title={path}>
                        {path.split(/[\\/]/).pop()}
                        <button
                          type="button"
                          aria-label="Remove attachment"
                          onClick={() => setAttachmentPaths((current) => current.filter((candidate) => candidate !== path))}
                        ><X size={10} /></button>
                      </span>
                    ))}
                  </div>
                  <Button
                    type="button"
                    variant="primary"
                    iconRight={<Send size={14} />}
                    disabled={
                      (!note.trim() &&
                        !reproductionSteps.trim() &&
                        !expectedBehavior.trim() &&
                        !quote?.text.trim() &&
                        attachmentPaths.length === 0) ||
                      !selectedAgent
                    }
                    onClick={handleAskAgent}
                  >
                    打回修复
                  </Button>
                </div>
              )}
            </section>

            <section className="debug-card">
              <button
                type="button"
                className="debug-card-label testing-cycles-toggle"
                aria-expanded={cyclesOpen}
                onClick={() => setCyclesOpen((open) => !open)}
              >
                <Clock3 size={14} />
                测试循环
                <span className="testing-cycles-count">{taskRuns.length}</span>
                <ChevronDown
                  size={14}
                  className={`testing-cycles-chevron${cyclesOpen ? " open" : ""}`}
                />
              </button>
              {cyclesOpen && (
              <div className="testing-cycle-list">
                {taskRuns.length === 0 ? (
                  <span>暂无命令循环。</span>
                ) : (
                  taskRuns.map((run, index) => {
                    const expanded = expandedCycle === run.id;
                    const cycleLogs = state.commandLogs[run.id] ?? [];
                    return (
                      <div className={`testing-cycle cycle-${run.status}`} key={run.id}>
                        <span />
                        <div className="testing-cycle-body">
                          <button
                            type="button"
                            className="testing-cycle-toggle"
                            aria-expanded={expanded}
                            onClick={() => setExpandedCycle(expanded ? null : run.id)}
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
          </div>
        </aside>
        )}
      </div>

      {draft && (
        <div className="confirm-backdrop" role="presentation" onClick={() => setDraft(null)}>
          <div
            className="confirm-modal testing-slot-modal"
            role="dialog"
            aria-modal="true"
            onClick={(event) => event.stopPropagation()}
          >
            <h2>{draft.id ? "Edit terminal" : "Add terminal"}</h2>
            <label className="testing-slot-field">
              <span>Name</span>
              <input
                value={draft.name}
                autoFocus
                placeholder="Preview"
                onChange={(event) => setDraft({ ...draft, name: event.target.value })}
              />
            </label>
            <label className="testing-slot-field">
              <span>Command</span>
              <input
                value={draft.command}
                placeholder="pnpm dev"
                onChange={(event) => setDraft({ ...draft, command: event.target.value })}
              />
            </label>
            <label className="testing-slot-field">
              <span>Working directory (optional, relative to project)</span>
              <input
                value={draft.cwd}
                placeholder="(project root) e.g. todo-react"
                onChange={(event) => setDraft({ ...draft, cwd: event.target.value })}
              />
            </label>
            <label className="testing-slot-field">
              <span>Kind</span>
              <select
                value={draft.kind}
                onChange={(event) =>
                  setDraft({ ...draft, kind: event.target.value as TerminalSlot["kind"] })
                }
              >
                <option value="preview">Preview (live dev server)</option>
                <option value="validation">Validation (one-shot check)</option>
              </select>
            </label>
            <p className="testing-slot-hint">
              {draft.kind === "preview"
                ? "Runs in a real terminal (PTY) with colors. Long-running; not used for the acceptance gate."
                : "Piped one-shot command. Its exit code and logs drive the acceptance gate."}
            </p>
            <div className="confirm-modal-actions">
              <Button variant="ghost" onClick={() => setDraft(null)}>
                Cancel
              </Button>
              <Button
                variant="primary"
                onClick={saveDraft}
                disabled={!draft.name.trim() || !draft.command.trim()}
              >
                Save
              </Button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
