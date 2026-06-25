import { useEffect, useMemo, useRef, useState } from "react";
import {
  AlertTriangle,
  Bot,
  CheckCircle2,
  ChevronDown,
  Clock3,
  PanelRightClose,
  PanelRightOpen,
  Quote,
  RefreshCw,
  RotateCcw,
  Send,
  ShieldCheck,
  Plus,
  User,
  X,
} from "lucide-react";
import type { CommandRun, ProjectSummary, Task, TerminalSlot, TerminalSlotKind } from "../../domain";
import { useAgentBridge } from "../../hooks/useAgentBridge";
import { useCommandBridge } from "../../hooks/useCommandBridge";
import { usePtyBridge } from "../../hooks/usePtyBridge";
import { useTaskBridge } from "../../hooks/useTaskBridge";
import { useTerminalBridge } from "../../hooks/useTerminalBridge";
import { useAppState } from "../../state/AppStateContext";
import {
  buildAgentCommandArgs,
  buildRepairPrompt,
  formatQuotedFeedback,
  hasImplementationCapability,
} from "../../utils/agentRun";
import { parseCommandLine } from "../../utils/commandLine";
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


interface TestingPaneProps {
  project: ProjectSummary;
  task: Task;
  readOnly?: boolean;
}

function preferredPreviewCommand(project: ProjectSummary) {
  return (
    project.suggestedCommands.find((command) =>
      /\b(pnpm|npm|yarn)\s+(dev|run\s+dev)\b/.test(command),
    ) ?? "pnpm dev"
  );
}

function commandPriority(command: string) {
  if (/\b(test|smoke)\b/.test(command)) {
    return 0;
  }
  if (/\b(typecheck|check)\b/.test(command)) {
    return 1;
  }
  if (/\bbuild\b/.test(command)) {
    return 2;
  }
  if (/\blint\b/.test(command)) {
    return 3;
  }
  return 4;
}

function isValidationCommand(command: string) {
  return /\b(test|smoke|typecheck|check|build|lint)\b/.test(command);
}

function preferredValidationCommand(project: ProjectSummary) {
  const validationCommands = project.suggestedCommands
    .filter(isValidationCommand)
    .sort((left, right) => commandPriority(left) - commandPriority(right));

  if (validationCommands[0]) {
    return validationCommands[0];
  }
  if (project.detectedStacks.includes("Tauri")) {
    return "cargo check --manifest-path src-tauri/Cargo.toml";
  }
  if (project.detectedStacks.includes("Rust")) {
    return "cargo test";
  }
  if (project.detectedStacks.includes("Go")) {
    return "go test ./...";
  }
  return "pnpm test";
}

function newSlotId() {
  return `slot-${crypto.randomUUID()}`;
}

function buildDefaultSlots(project: ProjectSummary): TerminalSlot[] {
  return [
    { id: newSlotId(), name: "Preview", command: preferredPreviewCommand(project), kind: "preview" },
    {
      id: newSlotId(),
      name: "Validation",
      command: preferredValidationCommand(project),
      kind: "validation",
    },
  ];
}

function slotEndpoint(slot: TerminalSlot) {
  return slot.kind === "preview" ? "localhost:1420" : "exit code required";
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
        run.taskId === taskId && run.status === "succeeded" && validationCommands.has(run.command),
    )
    .sort((left, right) => right.startedAtMs - left.startedAtMs)[0];
}

function latestFailedRun(runs: CommandRun[], taskId: string) {
  return runs
    .filter((run) => run.taskId === taskId && (run.status === "failed" || run.errorSummary?.failed))
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
      title: "Repair required",
      copy: "A newer failing run blocks acceptance until a validation command succeeds after it.",
    };
  }
  if (hasPassingEvidence) {
    return {
      tone: "ok",
      title: "Passing evidence captured",
      copy: "A validation command succeeded and no newer failing run blocks acceptance.",
    };
  }
  if (hasRunningValidationRun) {
    return {
      tone: "running",
      title: "Checks running",
      copy: "Wait for the validation command to finish before accepting the task.",
    };
  }
  if (hasValidationEvidence) {
    return {
      tone: "idle",
      title: "Validation did not pass",
      copy: "Run a validation command again and capture a successful exit before accepting.",
    };
  }
  return {
    tone: "idle",
    title: "No validation evidence",
    copy: "Run a validation command to capture stdout, stderr, exit code, and log references.",
  };
}

interface SlotDraft {
  id: string | null;
  name: string;
  command: string;
  kind: TerminalSlotKind;
  cwd: string;
}

export function TestingPane({ project, task, readOnly = false }: TestingPaneProps) {
  const { state } = useAppState();
  const { loadAgents } = useAgentBridge();
  const { startCommandRun, stopCommandRun } = useCommandBridge();
  const { startPtyRun, stopPtyRun } = usePtyBridge();
  const { listTerminalSlots, saveTerminalSlots, suggestTerminalSlots } = useTerminalBridge();
  const { appendFeedback, completeTask, generateRepairContext } = useTaskBridge();

  const [slots, setSlots] = useState<TerminalSlot[]>([]);
  const [runIds, setRunIds] = useState<Record<string, string>>({});
  const [draft, setDraft] = useState<SlotDraft | null>(null);
  const [note, setNote] = useState("");
  const [quote, setQuote] = useState<QuoteDraft | null>(null);
  const [autoMode, setAutoMode] = useState(false);
  const [selectedAgentId, setSelectedAgentId] = useState("");
  const [fixRunIds, setFixRunIds] = useState<string[]>([]);
  const [expandedCycle, setExpandedCycle] = useState<string | null>(null);
  const [cyclesOpen, setCyclesOpen] = useState(false);
  const [cockpitOpen, setCockpitOpen] = useState(true);
  const [commandError, setCommandError] = useState<string | null>(null);
  const autoHandledRef = useRef<Set<string>>(new Set());

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
  const failedRun = latestFailedRun(state.commandRuns, task.id);
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
    (run) => run.taskId === task.id && validationCommands.has(run.command),
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

  function slotCwd(slot: TerminalSlot) {
    // cwd may be a relative subdir (monorepo app) or absent (project root).
    return slot.cwd ? `${project.path}/${slot.cwd}` : project.path;
  }

  async function runSlot(slot: TerminalSlot) {
    if (!slot.command.trim()) {
      // Unconfigured slot — open the editor instead of running an empty command.
      setDraft({ id: slot.id, name: slot.name, command: slot.command, kind: slot.kind, cwd: slot.cwd ?? "" });
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
              taskId: task.id,
              rows: 24,
              cols: 80,
            })
          : await startCommandRun({
              program: parsed.program,
              args: parsed.args,
              cwd,
              taskId: task.id,
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
    if (!draft || !draft.name.trim() || !draft.command.trim()) {
      return;
    }
    const cwd = draft.cwd.trim() || undefined;
    const next = draft.id
      ? slots.map((slot) =>
          slot.id === draft.id
            ? { ...slot, name: draft.name.trim(), command: draft.command.trim(), kind: draft.kind, cwd }
            : slot,
        )
      : [
          ...slots,
          {
            id: newSlotId(),
            name: draft.name.trim(),
            command: draft.command.trim(),
            kind: draft.kind,
            cwd,
          },
        ];
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

  async function startFixRun(noteText: string, quoteDraft: QuoteDraft | null) {
    if (!selectedAgent) {
      setCommandError("No implementation agent available to run a fix.");
      return;
    }
    setCommandError(null);
    // Refresh the repair context (packages logs / todo / feedback) so the
    // prompt carries the latest evidence, then hand off to the agent.
    const refreshed =
      (latestBlockingFailure ? await generateRepairContext(project.path, task.id) : null) ?? task;
    const prompt = buildRepairPrompt(refreshed, quoteDraft?.text ?? "", noteText);
    const run = await startCommandRun({
      program: selectedAgent.command,
      args: buildAgentCommandArgs(selectedAgent, project.path, prompt),
      cwd: project.path,
      taskId: task.id,
    });
    if (run) {
      setFixRunIds((prev) => [...prev, run.id]);
    }
  }

  async function handleAskAgent() {
    if (!note.trim() && !quote?.text.trim()) {
      return;
    }
    const content = formatQuotedFeedback(note, quote?.text ?? "", quote?.command ?? "");
    await appendFeedback(project.path, task.id, quote?.failedRunId ?? latestBlockingFailure?.id, content);
    await startFixRun(note, quote);
    setNote("");
    setQuote(null);
  }

  // Auto mode: on a fresh blocking failure, kick off one fix run (deduped).
  useEffect(() => {
    if (!autoMode || readOnly || !latestBlockingFailure) {
      return;
    }
    if (autoHandledRef.current.has(latestBlockingFailure.id)) {
      return;
    }
    autoHandledRef.current.add(latestBlockingFailure.id);
    void startFixRun("Auto repair: a validation command failed.", null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [autoMode, readOnly, latestBlockingFailure?.id]);

  const conversation = useMemo<ConversationTurn[]>(() => {
    const humanTurns: ConversationTurn[] = task.feedback.map((entry) => ({
      id: entry.id,
      role: "human",
      timestampMs: entry.timestampMs,
      content: entry.content,
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
          <div className="testing-kicker">Debug validation</div>
          <div className="testing-title-row">
            <span className={`testing-status-pill testing-status-${gate.tone}`}>
              <span />
              {gate.title}
            </span>
            <span className="testing-header-meta">
              Last activity:{" "}
              {latestTaskRun
                ? `${latestTaskRun.command} · ${shortTime(latestTaskRun.startedAtMs)}`
                : "none"}
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
              Run checks
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
                endpoint={slotEndpoint(slot)}
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
                    : () =>
                        setDraft({
                          id: slot.id,
                          name: slot.name,
                          command: slot.command,
                          kind: slot.kind,
                          cwd: slot.cwd ?? "",
                        })
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
                onClick={() => setDraft({ id: null, name: "", command: "", kind: "validation", cwd: "" })}
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
        </div>

        {cockpitOpen && (
        <aside className="debug-agent-panel">
          <div className="debug-agent-header">
            <div className="debug-agent-avatar">AI</div>
            <span>Testing cockpit</span>
            <span className="testing-panel-state">{task.status}</span>
          </div>

          <div className="debug-agent-body">
            <section className={`debug-card testing-gate-card gate-${gate.tone}`}>
              <div className="debug-card-label">
                <ShieldCheck size={14} />
                Acceptance gate
              </div>
              <strong>{gate.title}</strong>
              <p>{gate.copy}</p>
              <div className="testing-evidence-grid">
                <div>
                  <span>Validation command(s)</span>
                  <strong>{validationCommandLabel}</strong>
                </div>
                <div>
                  <span>Latest validation</span>
                  <strong>{statusLabel(successfulValidationRun ?? failedRun)}</strong>
                </div>
                <div>
                  <span>Passing evidence</span>
                  <strong>
                    {successfulValidationRun ? shortTime(successfulValidationRun.endedAtMs) : "Missing"}
                  </strong>
                </div>
                <div>
                  <span>Newer failure</span>
                  <strong>
                    {latestBlockingFailure ? shortTime(latestBlockingFailure.startedAtMs) : "None"}
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
                    Accept / Done
                  </Button>
                  {!canAccept && (
                    <div className="testing-accept-note">
                      Acceptance unlocks after a validation command succeeds with no newer failure.
                    </div>
                  )}
                </>
              )}
            </section>

            <section className={`debug-card ${latestBlockingFailure ? "debug-card-error" : ""}`}>
              <div className="debug-card-label">
                <AlertTriangle size={14} />
                Failure signal
              </div>
              <strong>{latestBlockingFailure ? latestBlockingFailure.command : "No active failure"}</strong>
              <p>
                {latestBlockingFailure
                  ? errorFinding(latestBlockingFailure)
                  : "No failing command is newer than the latest passing validation run."}
              </p>
            </section>

            <section className="debug-card testing-chat-card">
              <div className="testing-chat-head">
                <div className="debug-card-label">
                  <Bot size={14} />
                  Debug agent
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
                      onClick={() => setAutoMode(false)}
                    >
                      Manual
                    </button>
                    <button
                      type="button"
                      className={autoMode ? "active" : ""}
                      disabled={readOnly}
                      onClick={() => setAutoMode(true)}
                    >
                      Auto
                    </button>
                  </div>
                </div>
              </div>

              <div className="testing-chat-stream">
                {conversation.length === 0 ? (
                  <p className="testing-chat-empty">
                    Select log lines in a terminal and quote them here, or write a note, then ask the
                    agent to fix it.
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
                    placeholder="Tell the agent what's wrong (quote log lines from a terminal to point at the error)…"
                  />
                  <Button
                    type="button"
                    variant="primary"
                    iconRight={<Send size={14} />}
                    disabled={(!note.trim() && !quote?.text.trim()) || !selectedAgent}
                    onClick={handleAskAgent}
                  >
                    Ask agent to fix
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
                Test cycles
                <span className="testing-cycles-count">{taskRuns.length}</span>
                <ChevronDown
                  size={14}
                  className={`testing-cycles-chevron${cyclesOpen ? " open" : ""}`}
                />
              </button>
              {cyclesOpen && (
              <div className="testing-cycle-list">
                {taskRuns.length === 0 ? (
                  <span>No command cycles yet.</span>
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
                  setDraft({ ...draft, kind: event.target.value as TerminalSlotKind })
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
