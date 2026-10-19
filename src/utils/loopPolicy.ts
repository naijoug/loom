// Single source of truth for the auto repair-loop policy used by both the
// implement loop (SessionPane) and the testing repair loop (TestingPane).
// Pure functions only — no React/Tauri — so they're unit-testable under the
// node test runner and (if Loom ever goes L3/headless) portable to Rust.

export const MAX_AUTO_REPAIR_ATTEMPTS = 3;
export const NO_PROGRESS_LIMIT = 3;
export const LOOP_WALL_CLOCK_MS = 480_000;

export interface LoopBudget {
  /** Max repair attempts before escalating to a human. */
  maxAttempts: number;
  /** Consecutive identical-failure repeats before escalating (no progress). */
  noProgressLimit: number;
  /** Wall-clock budget for the whole loop before escalating. */
  wallClockMs: number;
}

export const DEFAULT_LOOP_BUDGET: LoopBudget = {
  maxAttempts: MAX_AUTO_REPAIR_ATTEMPTS,
  noProgressLimit: NO_PROGRESS_LIMIT,
  wallClockMs: LOOP_WALL_CLOCK_MS,
};

/** Progress carried across iterations of one auto loop. */
export interface LoopProgress {
  repairAttempts: number;
  repeatedFailureCount: number;
  lastFailureFingerprint?: string;
  startedAtMs: number;
}

/** Minimal shape of a finished run needed to judge a failure (CommandRun-compatible). */
export interface FailedRunLike {
  status: string;
  command: string;
  exitCode?: number;
  errorSummary?: { matchedLines: string[]; stderrTail: string[] };
}

export type EscalationReason = "timeout" | "no_progress" | "max_attempts" | "agent_failed";

export type ValidationDecision =
  | { kind: "pass" }
  | { kind: "repair"; attempt: number; fingerprint: string; repeatedFailureCount: number }
  | {
      kind: "escalate";
      reason: EscalationReason;
      fingerprint?: string;
      repeatedFailureCount?: number;
    };

export type RepairDecision = { kind: "validate" } | { kind: "escalate"; reason: EscalationReason };

/**
 * Stable, normalized signature of a failed run so repeated identical failures
 * can be detected (no-progress). Mirrors the previous SessionPane/Rust logic.
 */
export function failureFingerprint(run: FailedRunLike): string {
  const summary = run.errorSummary;
  const evidence =
    (summary && summary.matchedLines.join("\n")) ||
    (summary && summary.stderrTail.join("\n")) ||
    `${run.command}:${run.exitCode ?? "unknown"}`;
  return evidence.trim().toLowerCase();
}

export function isTimedOut(progress: LoopProgress, budget: LoopBudget, nowMs: number): boolean {
  return budget.wallClockMs > 0 && nowMs - progress.startedAtMs >= budget.wallClockMs;
}

/**
 * Decide what to do after a validation run finishes. Wall-clock timeout wins
 * over everything; then success → pass; otherwise classify the failure and
 * either repair (within budget) or escalate.
 */
export function decideAfterValidation(
  progress: LoopProgress,
  run: FailedRunLike,
  budget: LoopBudget,
  nowMs: number,
): ValidationDecision {
  if (isTimedOut(progress, budget, nowMs)) {
    return { kind: "escalate", reason: "timeout" };
  }

  if (run.status === "succeeded") {
    return { kind: "pass" };
  }

  const fingerprint = failureFingerprint(run);
  const repeatedFailureCount =
    fingerprint && fingerprint === progress.lastFailureFingerprint
      ? progress.repeatedFailureCount + 1
      : 1;
  const attempt = progress.repairAttempts + 1;

  if (budget.noProgressLimit > 0 && repeatedFailureCount >= budget.noProgressLimit) {
    return { kind: "escalate", reason: "no_progress", fingerprint, repeatedFailureCount };
  }
  if (attempt > budget.maxAttempts) {
    return { kind: "escalate", reason: "max_attempts", fingerprint, repeatedFailureCount };
  }

  return { kind: "repair", attempt, fingerprint, repeatedFailureCount };
}

/** Decide what to do after a repair agent run finishes. */
export function decideAfterRepair(
  progress: LoopProgress,
  run: { status: string },
  budget: LoopBudget,
  nowMs: number,
): RepairDecision {
  if (isTimedOut(progress, budget, nowMs)) {
    return { kind: "escalate", reason: "timeout" };
  }
  if (run.status !== "succeeded") {
    return { kind: "escalate", reason: "agent_failed" };
  }
  return { kind: "validate" };
}

/** Human-readable escalation copy, shared by both panes. */
export function escalationNotice(reason: EscalationReason, budget: LoopBudget): string {
  switch (reason) {
    case "timeout":
      return `Auto loop stopped: exceeded the ${Math.round(
        budget.wallClockMs / 1000,
      )}s time budget without passing.`;
    case "no_progress":
      return "Auto loop stopped: validation is failing with the same signal repeatedly.";
    case "max_attempts":
      return `Auto loop stopped after ${budget.maxAttempts} repair attempts.`;
    case "agent_failed":
      return "Auto loop stopped because the repair agent run did not complete successfully.";
  }
}
