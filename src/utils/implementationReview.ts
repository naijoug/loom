import type { Task } from "../domain";

export function implementationReviewGate(task: Task) {
  const latestRun = [...(task.implementationReviewRuns ?? [])]
    .reverse()
    .find((run) => run.status !== "running");
  if (!latestRun || latestRun.status !== "succeeded") {
    return { ready: false, detail: "Run an independent implementation Review before Testing." };
  }
  const reviews = (task.implementationReviews ?? []).filter(
    (review) => review.runId === latestRun.id && review.status === "succeeded",
  );
  if (!reviews.some((review) => review.reviewerAgentId !== task.primaryAgentId)) {
    return { ready: false, detail: "A different Agent must successfully review the implementation." };
  }
  const blockers = reviews.flatMap((review) => review.findings).filter(
    (finding) => finding.severity === "blocker" && ["open", "pending_re_review"].includes(finding.status),
  );
  if (blockers.length > 0) {
    return { ready: false, detail: `${blockers.length} Review blocker(s) must be fixed, re-reviewed, or explicitly accepted.` };
  }
  return { ready: true, detail: "Independent Review passed with no unresolved blockers." };
}
