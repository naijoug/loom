export function describeRoundDraftStatus(drafts: Array<{ status: string }>) {
  const total = drafts.length;
  if (total === 0) {
    return "0 个 Agent";
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

  const parts = [`${total} 个 Agent`];
  if (counts.succeeded > 0) {
    parts.push(`${counts.succeeded} 成功`);
  }
  if (counts.failed > 0) {
    parts.push(`${counts.failed} 失败`);
  }
  if (counts.retrying > 0) {
    parts.push(`${counts.retrying} 重试中`);
  }
  if (counts.running > 0) {
    parts.push(`${counts.running} 运行中`);
  }
  if (counts.pending > 0) {
    parts.push(`${counts.pending} 排队中`);
  }

  return parts.join(" · ");
}
