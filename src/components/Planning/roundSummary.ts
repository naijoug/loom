function countLabel(count: number, singular: string, plural = `${singular}s`) {
  return `${count} ${count === 1 ? singular : plural}`;
}

export function describeRoundDraftStatus(drafts: Array<{ status: string }>) {
  const total = drafts.length;
  if (total === 0) {
    return "0 agents";
  }

  const counts = {
    succeeded: 0,
    failed: 0,
    retrying: 0,
    running: 0,
    pending: 0,
  };

  for (const draft of drafts) {
    if (draft.status === "succeeded") {
      counts.succeeded += 1;
    } else if (draft.status === "failed") {
      counts.failed += 1;
    } else if (draft.status === "retrying") {
      counts.retrying += 1;
    } else if (draft.status === "running") {
      counts.running += 1;
    } else {
      counts.pending += 1;
    }
  }

  const parts = [countLabel(total, "agent")];
  if (counts.succeeded > 0) {
    parts.push(countLabel(counts.succeeded, "succeeded", "succeeded"));
  }
  if (counts.failed > 0) {
    parts.push(countLabel(counts.failed, "failed", "failed"));
  }
  if (counts.retrying > 0) {
    parts.push(countLabel(counts.retrying, "retrying", "retrying"));
  }
  if (counts.running > 0) {
    parts.push(countLabel(counts.running, "running", "running"));
  }
  if (counts.pending > 0) {
    parts.push(countLabel(counts.pending, "pending", "pending"));
  }

  return parts.join(" · ");
}
