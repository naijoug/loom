import { useEffect, useMemo, useState, type FormEvent } from "react";
import {
  AlertTriangle,
  Bot,
  CheckCircle2,
  ChevronRight,
  Circle,
  Copy,
  ExternalLink,
  FileText,
  GitMerge,
  Loader2,
  MessageSquare,
  RefreshCw,
  Scale,
  XCircle,
} from "lucide-react";
import type {
  AgentInvocation,
  PlanReview,
  PlanningAgentLogEvent,
  PlanningAgentStatusEvent,
  PlanningRun,
  Task,
} from "../../domain";
import { useAgentBridge } from "../../hooks/useAgentBridge";
import { useTaskBridge } from "../../hooks/useTaskBridge";
import { useAppState } from "../../state/AppStateContext";
import { PLAN_PREVIEW_SANDBOX, wireIframeHashNavigation } from "../../utils/iframeNavigation";
import { Button } from "../common/Button";
import { describeRoundDraftStatus } from "./roundSummary";

const SYNTHESIS_PROMPT_SUMMARY = "Synthesize final plan";

interface PlanningTimelineProps {
  projectPath: string;
  task: Task | null;
  // Read-only review of a past stage: keep view/copy/open actions, hide every
  // control that re-runs agents or mutates task state.
  readOnly?: boolean;
}

interface DraftRow {
  agentId: string;
  agentName: string;
  status: string;
  attempt: number;
  failureKind?: string;
  invocation?: AgentInvocation;
  elapsedMs?: number;
}

interface RoundData {
  run: PlanningRun;
  index: number;
  drafts: DraftRow[];
  reviews: PlanReview[];
  synthesis?: AgentInvocation;
  liveReviews: PlanningAgentStatusEvent[];
  liveSynthesis?: PlanningAgentStatusEvent;
}

function elapsedLabel(ms?: number) {
  if (ms === undefined) {
    return null;
  }
  if (ms < 1000) {
    return `${ms} ms`;
  }
  const seconds = Math.round(ms / 1000);
  if (seconds < 90) {
    return `${seconds} s`;
  }
  return `${Math.floor(seconds / 60)}m ${seconds % 60}s`;
}

function timeLabel(ms: number) {
  return new Date(ms).toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function statusIcon(status: string) {
  if (status === "succeeded") {
    return <CheckCircle2 size={14} className="timeline-status-ok" />;
  }
  if (status === "failed") {
    return <XCircle size={14} className="timeline-status-bad" />;
  }
  if (status === "running" || status === "retrying") {
    return <Loader2 size={14} className="timeline-spin" />;
  }
  return <Circle size={14} className="timeline-status-idle" />;
}

function failureLabel(kind?: string) {
  switch (kind) {
    case "timeout":
      return "Timed out";
    case "empty_output":
      return "Produced no output";
    case "not_retryable":
      return "Configuration error — fix the agent setup, then retry";
    default:
      return "Failed";
  }
}

function planSource(summary: string) {
  const match = summary.match(/Final plan source: (.+?)\.?$/);
  return match ? match[1] : null;
}

function planningLogKey(runId: string, phase: string, agentId: string) {
  return `${runId}:${phase}:${agentId}`;
}

function flattenLogLines(events: PlanningAgentLogEvent[] | undefined) {
  return (events ?? []).flatMap((event) =>
    event.lines.map((line) => ({
      stream: event.stream,
      line,
      timestampMs: event.timestampMs,
    })),
  );
}

async function copyToClipboard(value: string) {
  await navigator.clipboard.writeText(value);
}

/** Latest drafting invocation per agent for one run (retries supersede). */
function draftInvocations(task: Task, runId: string) {
  const byAgent = new Map<string, AgentInvocation>();
  for (const invocation of task.agentInvocations) {
    if (
      invocation.planningRunId === runId &&
      invocation.promptSummary !== SYNTHESIS_PROMPT_SUMMARY
    ) {
      byAgent.set(invocation.agentId, invocation);
    }
  }
  return byAgent;
}

function buildRounds(
  task: Task,
  progress: PlanningAgentStatusEvent[],
): RoundData[] {
  return task.planningRuns.map((run, index) => {
    const invocations = draftInvocations(task, run.id);
    const runEvents = progress.filter((event) => event.planningRunId === run.id);

    const drafts = new Map<string, DraftRow>();
    for (const [agentId, invocation] of invocations) {
      drafts.set(agentId, {
        agentId,
        agentName: invocation.agentName,
        status: invocation.status,
        attempt: invocation.attempt,
        failureKind: invocation.failureKind,
        invocation,
        elapsedMs:
          invocation.endedAtMs !== undefined
            ? invocation.endedAtMs - invocation.startedAtMs
            : undefined,
      });
    }
    // Live planning events override persisted rows while a retry is in flight
    // (and cover agents whose invocation has not been persisted yet).
    for (const event of runEvents) {
      if (event.phase !== "planning") {
        continue;
      }
      const existing = drafts.get(event.agentId);
      const isLive = event.status === "running" || event.status === "retrying";
      if (!existing) {
        drafts.set(event.agentId, {
          agentId: event.agentId,
          agentName: event.agentName,
          status: event.status,
          attempt: event.attempt,
          elapsedMs: event.elapsedMs,
        });
      } else if (isLive && event.startedAtMs >= (existing.invocation?.endedAtMs ?? 0)) {
        existing.status = event.status;
        existing.attempt = event.attempt;
      }
    }

    const synthesisInvocations = task.agentInvocations.filter(
      (invocation) =>
        invocation.planningRunId === run.id &&
        invocation.promptSummary === SYNTHESIS_PROMPT_SUMMARY,
    );

    return {
      run,
      index,
      drafts: Array.from(drafts.values()),
      reviews: task.planReviews.filter((review) => review.planningRunId === run.id),
      synthesis: synthesisInvocations[synthesisInvocations.length - 1],
      liveReviews: runEvents.filter(
        (event) => event.phase === "review" && event.status === "running",
      ),
      liveSynthesis: runEvents.find(
        (event) =>
          event.phase === "synthesis" &&
          (event.status === "running" || event.status === "retrying"),
      ),
    };
  });
}

/** Progress events that do not belong to any persisted run yet (in-flight round). */
function liveRoundEvents(task: Task | null, progress: PlanningAgentStatusEvent[]) {
  const knownRuns = new Set(task?.planningRuns.map((run) => run.id) ?? []);
  return progress.filter((event) => !knownRuns.has(event.planningRunId));
}

function StageMarker({ icon, label }: { icon: React.ReactNode; label: string }) {
  return (
    <div className="timeline-stage-marker">
      <span className="timeline-stage-icon">{icon}</span>
      <span className="timeline-stage-label">{label}</span>
    </div>
  );
}

function AgentLogPanel({
  events,
  live,
}: {
  events: PlanningAgentLogEvent[] | undefined;
  live: boolean;
}) {
  const lines = flattenLogLines(events);
  if (lines.length === 0) {
    return null;
  }
  const tail = lines.slice(-8);

  return (
    <div className="timeline-agent-log">
      {live && (
        <pre className="timeline-live-tail">
          {tail.map((line) => `[${line.stream}] ${line.line}`).join("\n")}
        </pre>
      )}
      <details className="timeline-execution-log">
        <summary>Execution log</summary>
        <pre>{lines.map((line) => `[${line.stream}] ${line.line}`).join("\n")}</pre>
      </details>
    </div>
  );
}

function ResumeCommand({ command }: { command?: string }) {
  if (!command) {
    return null;
  }

  return (
    <Button
      type="button"
      variant="ghost"
      iconLeft={<Copy size={13} />}
      onClick={() => void copyToClipboard(command)}
    >
      Copy resume
    </Button>
  );
}

function DraftStage({
  rows,
  runId,
  logsByKey,
  busy,
  readOnly,
  onRetry,
  onOpenCandidate,
  onOpenEvidence,
  onOpenPartial,
}: {
  rows: DraftRow[];
  runId: string;
  logsByKey: Record<string, PlanningAgentLogEvent[]>;
  busy: boolean;
  readOnly: boolean;
  onRetry: (agentId: string) => void;
  onOpenCandidate: (mdPath: string) => void;
  onOpenEvidence: (path: string) => void;
  onOpenPartial: (mdPath: string) => void;
}) {
  if (rows.length === 0) {
    return null;
  }

  return (
    <div className="timeline-stage">
      <StageMarker icon={<Bot size={13} />} label="Drafting" />
      <div className="timeline-stage-body">
        {rows.map((row) => {
          const logs = logsByKey[planningLogKey(runId, "planning", row.agentId)];
          const isLive = row.status === "running" || row.status === "retrying";
          const errorLines = row.invocation?.errorLines ?? [];
          const partialPath =
            row.status === "failed" && row.invocation?.rawOutput.trim()
              ? row.invocation.evidenceRef
              : undefined;

          return (
            <div className={`timeline-agent-row status-${row.status}`} key={row.agentId}>
              <div className="timeline-agent-head">
                {statusIcon(row.status)}
                <span className="timeline-agent-name">{row.agentName}</span>
                {row.attempt > 1 && (
                  <span className="timeline-attempt-badge">attempt {row.attempt}</span>
                )}
                <span className="timeline-agent-meta">
                  {row.status === "retrying" ? "retrying…" : row.status}
                  {elapsedLabel(row.elapsedMs) ? ` · ${elapsedLabel(row.elapsedMs)}` : ""}
                  {row.invocation?.sessionId ? ` · ${row.invocation.sessionId.slice(0, 8)}` : ""}
                </span>
                <span className="timeline-agent-actions">
                  <ResumeCommand command={row.invocation?.resumeCommand} />
                  {row.status === "succeeded" && row.invocation?.planPath && (
                    <Button
                      type="button"
                      variant="ghost"
                      onClick={() => onOpenCandidate(row.invocation!.planPath!)}
                    >
                      View plan
                    </Button>
                  )}
                  {row.status === "failed" && row.invocation?.stderrRef && (
                    <Button
                      type="button"
                      variant="ghost"
                      onClick={() => onOpenEvidence(row.invocation!.stderrRef!)}
                    >
                      Open log file
                    </Button>
                  )}
                  {partialPath && (
                    <Button
                      type="button"
                      variant="ghost"
                      onClick={() => onOpenPartial(partialPath)}
                    >
                      View partial output
                    </Button>
                  )}
                  {row.status === "failed" && !readOnly && (
                    <Button
                      type="button"
                      variant="ghost"
                      iconLeft={<RefreshCw size={13} />}
                      disabled={busy}
                      onClick={() => onRetry(row.agentId)}
                    >
                      Retry
                    </Button>
                  )}
                </span>
              </div>
              {row.status === "succeeded" && row.invocation?.outputSummary && (
                <p className="timeline-agent-summary">{row.invocation.outputSummary}</p>
              )}
              {row.status === "failed" && row.invocation && (
                <div className="timeline-agent-failure">
                  <span className="timeline-failure-kind">
                    {row.invocation.failureDetail ?? failureLabel(row.failureKind)}
                  </span>
                  {errorLines.length > 0 && (
                    <pre className="timeline-error-lines">{errorLines.join("\n")}</pre>
                  )}
                  {row.invocation.stderrTail.length > 0 && (
                    <details>
                      <summary>Error details</summary>
                      <pre>{row.invocation.stderrTail.join("\n")}</pre>
                    </details>
                  )}
                </div>
              )}
              <AgentLogPanel events={logs} live={isLive} />
            </div>
          );
        })}
      </div>
    </div>
  );
}

function severityClass(severity: string) {
  if (severity === "blocker") {
    return "severity-blocker";
  }
  if (severity === "risk") {
    return "severity-risk";
  }
  return "severity-info";
}

function ReviewStage({
  reviews,
  liveReviews,
  logsByKey,
}: {
  reviews: PlanReview[];
  liveReviews: PlanningAgentStatusEvent[];
  logsByKey: Record<string, PlanningAgentLogEvent[]>;
}) {
  if (reviews.length === 0 && liveReviews.length === 0) {
    return null;
  }

  return (
    <div className="timeline-stage">
      <StageMarker icon={<Scale size={13} />} label="Cross-review" />
      <div className="timeline-stage-body">
        {liveReviews.map((event) => (
          <div className="timeline-review-block" key={`live-${event.agentId}`}>
            <div className="timeline-review-row">
              {statusIcon(event.status)}
              <span className="timeline-review-pair">{event.agentName}</span>
              <span className="timeline-agent-meta">reviewing…</span>
            </div>
            <AgentLogPanel
              events={logsByKey[planningLogKey(event.planningRunId, "review", event.agentId)]}
              live
            />
          </div>
        ))}
        {reviews.map((review) => (
          <div className="timeline-review-block" key={review.id}>
            <div className="timeline-review-row">
              {statusIcon(review.status)}
              <span className="timeline-review-pair">
                {review.reviewerAgentName} → {review.targetAgentName}
              </span>
              <span className={`timeline-severity ${severityClass(review.severity)}`}>
                {review.severity}
              </span>
              <span className="timeline-review-finding">{review.finding}</span>
              <ResumeCommand command={review.resumeCommand} />
            </div>
            <AgentLogPanel
              events={
                logsByKey[
                  planningLogKey(
                    review.planningRunId,
                    "review",
                    `${review.reviewerAgentId}->${review.targetAgentId}`,
                  )
                ]
              }
              live={false}
            />
          </div>
        ))}
      </div>
    </div>
  );
}

function SynthesisStage({
  synthesis,
  liveSynthesis,
  source,
  logsByKey,
}: {
  synthesis?: AgentInvocation;
  liveSynthesis?: PlanningAgentStatusEvent;
  source: string | null;
  logsByKey: Record<string, PlanningAgentLogEvent[]>;
}) {
  if (!synthesis && !liveSynthesis && !source) {
    return null;
  }

  const status = liveSynthesis?.status ?? synthesis?.status ?? "succeeded";
  const logRunId = liveSynthesis?.planningRunId ?? synthesis?.planningRunId;
  const logAgentId = liveSynthesis?.agentId ?? synthesis?.agentId;
  const logs =
    logRunId && logAgentId ? logsByKey[planningLogKey(logRunId, "synthesis", logAgentId)] : undefined;

  return (
    <div className="timeline-stage">
      <StageMarker icon={<GitMerge size={13} />} label="Synthesis" />
      <div className="timeline-stage-body">
        <div className="timeline-review-row">
          {statusIcon(status)}
          <span className="timeline-review-finding">
            {liveSynthesis
              ? `${liveSynthesis.agentName} is synthesizing the final plan…`
              : source ?? synthesis?.outputSummary ?? ""}
          </span>
          <ResumeCommand command={synthesis?.resumeCommand} />
        </div>
        <AgentLogPanel
          events={logs}
          live={status === "running" || status === "retrying"}
        />
      </div>
    </div>
  );
}

function FinalPlanStage({
  projectPath,
  task,
  busy,
  canRerunReviews,
  readOnly,
}: {
  projectPath: string;
  task: Task;
  busy: boolean;
  canRerunReviews: boolean;
  readOnly: boolean;
}) {
  const { dispatch } = useAppState();
  const { runPlanReviews } = useAgentBridge();
  const {
    confirmPlan,
    openPlanHtml,
    openPlanViewer,
    readPlanHtml,
    recordPlanningDecision,
  } = useTaskBridge();
  const [html, setHtml] = useState<string | null>(null);
  const [decision, setDecision] = useState("");

  useEffect(() => {
    if (!task.finalPlanPath) {
      setHtml(null);
      return;
    }

    let cancelled = false;
    void readPlanHtml(projectPath, task.finalPlanPath).then((value) => {
      if (!cancelled) {
        setHtml(value);
      }
    });
    return () => {
      cancelled = true;
    };
  }, [projectPath, readPlanHtml, task.finalPlanPath, task.updatedAtMs]);

  if (!task.finalPlan) {
    return null;
  }

  // Self-contained colors: the srcdoc document defaults to a white body and
  // black text, which clashes with (or disappears on) the app surface.
  const fallback = `<body style="margin:0;background:#1b1e24;"><pre style="white-space:pre-wrap;font:13px ui-monospace,monospace;line-height:1.5;margin:0;padding:16px;color:#e6e9ef;">${task.finalPlan
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")}</pre></body>`;

  async function handleRecordDecision(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!decision.trim()) {
      return;
    }
    const updated = await recordPlanningDecision({
      projectPath,
      taskId: task.id,
      title: "Planning decision",
      content: decision.trim(),
    });
    if (updated) {
      setDecision("");
    }
  }

  async function handleCreateTasks() {
    const updated = await confirmPlan(projectPath, task.id);
    if (updated) {
      dispatch({ type: "app/viewSelected", view: "board" });
    }
  }

  return (
    <div className="timeline-stage timeline-final-stage">
      <StageMarker icon={<FileText size={13} />} label="Final plan" />
      <div className="timeline-stage-body">
        <div className="timeline-final-card">
          <div className="timeline-final-toolbar">
            <span className="timeline-final-path" title={task.finalPlanPath}>
              {task.finalPlanPath?.split("/").slice(-2).join("/")}
            </span>
            <span className="timeline-final-actions">
              <Button
                type="button"
                variant="ghost"
                disabled={!task.finalPlanPath}
                onClick={() => {
                  if (task.finalPlanPath) {
                    void openPlanViewer(projectPath, task.finalPlanPath);
                  }
                }}
              >
                Open in Loom
              </Button>
              <Button
                type="button"
                variant="ghost"
                iconRight={<ExternalLink size={13} />}
                disabled={!task.finalPlanHtmlPath}
                onClick={() => {
                  if (task.finalPlanHtmlPath) {
                    void openPlanHtml(projectPath, task.finalPlanHtmlPath);
                  }
                }}
              >
                Open in browser
              </Button>
              {canRerunReviews && !readOnly && (
                <Button
                  type="button"
                  variant="ghost"
                  disabled={busy}
                  onClick={() => void runPlanReviews(projectPath, task.id)}
                >
                  Re-run reviews
                </Button>
              )}
            </span>
          </div>
          <details className="timeline-final-preview" open>
            <summary>Preview</summary>
            <iframe
              title="Final plan preview"
              sandbox={PLAN_PREVIEW_SANDBOX}
              srcDoc={html ?? fallback}
              onLoad={(event) => wireIframeHashNavigation(event.currentTarget)}
            />
          </details>

          {task.planningDecisions.length > 0 && (
            <ul className="timeline-decision-list">
              {task.planningDecisions.map((item) => (
                <li key={item.id}>
                  <strong>{item.title}</strong>
                  <span>{item.content}</span>
                </li>
              ))}
            </ul>
          )}

          {!readOnly && (
            <details className="timeline-decision-form">
              <summary>Record a decision</summary>
              <form onSubmit={handleRecordDecision}>
                <textarea
                  value={decision}
                  onChange={(event) => setDecision(event.target.value)}
                  placeholder="Record a scope, tradeoff, or risk decision."
                />
                <Button type="submit" variant="ghost" disabled={!decision.trim()}>
                  Record
                </Button>
              </form>
            </details>
          )}

          {!readOnly && (
            <div className="timeline-final-cta">
              <span>Confirm the final plan to generate implementation todos.</span>
              <Button
                type="button"
                variant="primary"
                disabled={busy}
                onClick={() => void handleCreateTasks()}
              >
                Create tasks from plan
              </Button>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function RoundSection({
  projectPath,
  task,
  round,
  isLatest,
  logsByKey,
  busy,
  readOnly,
  onRetry,
  onOpenCandidate,
  onOpenEvidence,
  onOpenPartial,
}: {
  projectPath: string;
  task: Task;
  round: RoundData;
  isLatest: boolean;
  logsByKey: Record<string, PlanningAgentLogEvent[]>;
  busy: boolean;
  readOnly: boolean;
  onRetry: (agentId: string) => void;
  onOpenCandidate: (mdPath: string) => void;
  onOpenEvidence: (path: string) => void;
  onOpenPartial: (mdPath: string) => void;
}) {
  const [expanded, setExpanded] = useState(isLatest);

  useEffect(() => {
    setExpanded(isLatest);
  }, [isLatest]);

  const source = planSource(round.run.summary);
  const successfulCandidates = round.drafts.filter(
    (row) => row.status === "succeeded",
  ).length;
  const draftSummary = describeRoundDraftStatus(round.drafts);

  return (
    <section className={`timeline-round${expanded ? " expanded" : ""}`}>
      <button
        type="button"
        className="timeline-round-header"
        onClick={() => setExpanded((value) => !value)}
        aria-expanded={expanded}
      >
        <ChevronRight size={14} className="timeline-round-chevron" />
        <span className="timeline-round-title">Round {round.index + 1}</span>
        <span className="timeline-round-meta">
          {timeLabel(round.run.startedAtMs)} · {draftSummary}
          {!expanded && source ? ` · ${source}` : ""}
        </span>
      </button>

      {expanded && (
        <div className="timeline-round-body">
          <div className="timeline-stage">
            <StageMarker icon={<MessageSquare size={13} />} label="Requirement" />
            <div className="timeline-stage-body">
              <p className="timeline-requirement">{round.run.requirement}</p>
            </div>
          </div>

          <DraftStage
            rows={round.drafts}
            runId={round.run.id}
            logsByKey={logsByKey}
            busy={busy}
            readOnly={readOnly}
            onRetry={onRetry}
            onOpenCandidate={onOpenCandidate}
            onOpenEvidence={onOpenEvidence}
            onOpenPartial={onOpenPartial}
          />
          <ReviewStage
            reviews={round.reviews}
            liveReviews={round.liveReviews}
            logsByKey={logsByKey}
          />
          <SynthesisStage
            synthesis={round.synthesis}
            liveSynthesis={round.liveSynthesis}
            source={source}
            logsByKey={logsByKey}
          />
          {isLatest && (
            <FinalPlanStage
              projectPath={projectPath}
              task={task}
              busy={busy}
              canRerunReviews={successfulCandidates >= 2}
              readOnly={readOnly}
            />
          )}
        </div>
      )}
    </section>
  );
}

export function PlanningTimeline({ projectPath, task, readOnly = false }: PlanningTimelineProps) {
  const { state } = useAppState();
  const { retryPlanningAgent } = useAgentBridge();
  const { openPlanViewer, openPlanningEvidence } = useTaskBridge();

  const progress = useMemo(
    () =>
      Object.values(state.planningProgress).filter(
        (event) => task && event.taskId === task.id,
      ),
    [state.planningProgress, task],
  );
  const logsByKey = useMemo(
    () =>
      Object.fromEntries(
        Object.entries(state.planningLogs).filter(
          ([, events]) => events[0] && task && events[0].taskId === task.id,
        ),
      ),
    [state.planningLogs, task],
  );
  const rounds = useMemo(() => (task ? buildRounds(task, progress) : []), [task, progress]);
  const liveEvents = useMemo(() => liveRoundEvents(task, progress), [task, progress]);
  const busy = state.app.isLoadingTasks;

  if (!task && liveEvents.length === 0) {
    return (
      <div className="timeline-empty">
        <p>
          Describe what you want to build and send it to the local agents. Each round
          shows drafting, cross-review, and the synthesized final plan here.
        </p>
      </div>
    );
  }

  const hasLiveRound = liveEvents.length > 0;

  return (
    <div className="planning-timeline">
      {hasLiveRound && (
        <section className="timeline-round expanded timeline-live-round">
          <div className="timeline-round-header static">
            <Loader2 size={14} className="timeline-spin" />
            <span className="timeline-round-title">
              Round {rounds.length + 1} · in progress
            </span>
          </div>
          <div className="timeline-round-body">
            <div className="timeline-stage">
              <StageMarker icon={<Bot size={13} />} label="Drafting" />
              <div className="timeline-stage-body">
                {liveEvents
                  .filter((event) => event.phase === "planning")
                  .map((event) => (
                    <div
                      className={`timeline-agent-row status-${event.status}`}
                      key={`${event.planningRunId}:${event.agentId}`}
                    >
                      <div className="timeline-agent-head">
                        {statusIcon(event.status)}
                        <span className="timeline-agent-name">{event.agentName}</span>
                        {event.attempt > 1 && (
                          <span className="timeline-attempt-badge">
                            attempt {event.attempt}
                          </span>
                        )}
	                        <span className="timeline-agent-meta">
	                          {event.status === "retrying" ? "retrying…" : event.status}
	                        </span>
	                      </div>
	                      <AgentLogPanel
	                        events={
	                          logsByKey[
	                            planningLogKey(event.planningRunId, "planning", event.agentId)
	                          ]
	                        }
	                        live
	                      />
	                    </div>
	                  ))}
              </div>
            </div>
            {liveEvents.some((event) => event.phase === "review") && (
              <ReviewStage
                reviews={[]}
	                liveReviews={liveEvents.filter(
	                  (event) => event.phase === "review" && event.status === "running",
	                )}
	                logsByKey={logsByKey}
	              />
            )}
            {liveEvents.some((event) => event.phase === "synthesis") && (
              <SynthesisStage
                liveSynthesis={liveEvents.find(
                  (event) => event.phase === "synthesis" && event.status === "running",
	                )}
	                source={null}
	                logsByKey={logsByKey}
	              />
            )}
          </div>
        </section>
      )}

      {task &&
        [...rounds].reverse().map((round) => (
          <RoundSection
            key={round.run.id}
            projectPath={projectPath}
	            task={task}
	            round={round}
	            isLatest={round.index === rounds.length - 1 && !hasLiveRound}
	            logsByKey={logsByKey}
	            busy={busy}
	            readOnly={readOnly}
	            onRetry={(agentId) => void retryPlanningAgent(projectPath, task.id, agentId)}
	            onOpenCandidate={(mdPath) => void openPlanViewer(projectPath, mdPath)}
	            onOpenEvidence={(path) => void openPlanningEvidence(projectPath, path)}
	            onOpenPartial={(mdPath) => void openPlanViewer(projectPath, mdPath)}
	          />
        ))}

      {task && rounds.length === 0 && !hasLiveRound && (
        <div className="timeline-empty">
          {task.rawRequirement ? (
            <div className="timeline-stage">
              <StageMarker icon={<MessageSquare size={13} />} label="Requirement" />
              <div className="timeline-stage-body">
                <p className="timeline-requirement">{task.rawRequirement}</p>
              </div>
            </div>
          ) : (
            <p>No planning round yet. Send the requirement below to start one.</p>
          )}
          {task.events.length > 0 && state.app.taskError && (
            <p className="timeline-empty-hint">
              <AlertTriangle size={13} /> {state.app.taskError}
            </p>
          )}
        </div>
      )}
    </div>
  );
}
