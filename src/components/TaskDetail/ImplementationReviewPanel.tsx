import { CheckCircle2, RefreshCw, ShieldAlert, XCircle } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import type { ImplementationReviewFinding, ProjectSummary, Task } from "../../domain";
import { useAgentBridge } from "../../hooks/useAgentBridge";
import { useCommandBridge } from "../../hooks/useCommandBridge";
import { useImplementationReviewBridge } from "../../hooks/useImplementationReviewBridge";
import { useTaskBridge } from "../../hooks/useTaskBridge";
import { useAppState } from "../../state/AppStateContext";
import { implementationReviewGate } from "../../utils/implementationReview";
import { Button } from "../common/Button";

interface ImplementationReviewPanelProps {
  project: ProjectSummary;
  task: Task;
  readOnly: boolean;
}

export function ImplementationReviewPanel({ project, task, readOnly }: ImplementationReviewPanelProps) {
  const { state } = useAppState();
  const { loadAgents, prepareAgentInvocation } = useAgentBridge();
  const { startCommandRun } = useCommandBridge();
  const { decideImplementationReviewFinding, runImplementationReviews } = useImplementationReviewBridge();
  const { appendFeedback, switchPrimaryAgent } = useTaskBridge();
  const reviewers = useMemo(
    () => state.agents.filter((agent) =>
      agent.id !== task.primaryAgentId &&
      agent.enabled &&
      agent.available &&
      agent.capabilities.includes("review"),
    ),
    [state.agents, task.primaryAgentId],
  );
  const [selectedReviewerIds, setSelectedReviewerIds] = useState<string[]>(task.reviewAgentIds ?? []);
  const [running, setRunning] = useState(false);
  const [decisionDrafts, setDecisionDrafts] = useState<Record<string, string>>({});
  const [repairingFindingId, setRepairingFindingId] = useState<string | null>(null);
  const [escalationAgentId, setEscalationAgentId] = useState("");
  const [escalationReason, setEscalationReason] = useState("");
  const gate = implementationReviewGate(task);
  const latestRun = [...(task.implementationReviewRuns ?? [])].reverse()[0];
  const latestReviews = latestRun
    ? (task.implementationReviews ?? []).filter((review) => review.runId === latestRun.id)
    : [];
  const escalationAgents = state.agents.filter((agent) =>
    agent.id !== task.primaryAgentId &&
    agent.enabled &&
    agent.available &&
    agent.canWriteFiles &&
    agent.canRunCommands &&
    agent.capabilities.includes("implementation"),
  );

  useEffect(() => {
    void loadAgents();
  }, [loadAgents]);

  useEffect(() => {
    if (reviewers.length === 0) {
      return;
    }
    const valid = selectedReviewerIds.filter((id) => reviewers.some((agent) => agent.id === id));
    if (valid.length !== selectedReviewerIds.length) {
      setSelectedReviewerIds(valid);
      return;
    }
    if (valid.length === 0) {
      const saved = (task.reviewAgentIds ?? []).filter((id) => reviewers.some((agent) => agent.id === id));
      setSelectedReviewerIds(saved.length > 0 ? saved : reviewers.slice(0, 2).map((agent) => agent.id));
    }
  }, [reviewers, selectedReviewerIds, task.reviewAgentIds]);

  function toggleReviewer(agentId: string) {
    setSelectedReviewerIds((current) =>
      current.includes(agentId)
        ? current.filter((id) => id !== agentId)
        : [...current, agentId],
    );
  }

  async function runReviews() {
    if (selectedReviewerIds.length === 0) {
      return;
    }
    setRunning(true);
    await runImplementationReviews(project.path, task.id, selectedReviewerIds);
    setRunning(false);
  }

  async function decideFinding(
    findingId: string,
    decision: "resolved" | "accepted_risk" | "dismissed",
  ) {
    const reason = decisionDrafts[findingId]?.trim();
    if (!reason) {
      return;
    }
    await decideImplementationReviewFinding(project.path, task.id, findingId, decision, reason);
  }

  async function repairFinding(finding: ImplementationReviewFinding) {
    const primaryAgent = state.agents.find((agent) => agent.id === task.primaryAgentId);
    if (!primaryAgent) {
      return;
    }
    setRepairingFindingId(finding.id);
    const repairNote = [
      `Implementation Review blocker: ${finding.title}`,
      finding.detail,
      finding.file ? `Location: ${finding.file}${finding.line ? `:${finding.line}` : ""}` : "",
      "Fix the root cause, run focused validation, and summarize the changed files and evidence. Do not mark the task complete.",
    ].filter(Boolean).join("\n\n");
    await appendFeedback(project.path, task.id, undefined, repairNote);
    const invocation = await prepareAgentInvocation({
      projectPath: project.path,
      taskId: task.id,
      agentId: primaryAgent.id,
      stage: "debugging",
      prompt: `${repairNote}\n\nConfirmed plan:\n${task.finalPlan ?? "No plan available."}`,
    });
    if (invocation) {
      await startCommandRun({
        program: invocation.program,
        args: invocation.args,
        cwd: invocation.cwd,
        projectPath: project.path,
        taskId: task.id,
        agentId: primaryAgent.id,
        intent: "agent_action",
      });
    }
    setRepairingFindingId(null);
  }

  async function escalatePrimaryAgent() {
    if (!escalationAgentId || escalationReason.trim().length < 5) {
      return;
    }
    const updated = await switchPrimaryAgent(
      project.path,
      task.id,
      escalationAgentId,
      escalationReason.trim(),
    );
    if (updated) {
      setEscalationAgentId("");
      setEscalationReason("");
    }
  }

  return (
    <section className="implementation-review-panel">
      <div className="implementation-review-heading">
        <div>
          <div className="debug-card-label">独立实施 Review</div>
          <p>{gate.detail}</p>
        </div>
        <span className={`settings-pill ${gate.ready ? "ok" : "err"}`}>
          {gate.ready ? <CheckCircle2 size={13} /> : <ShieldAlert size={13} />}
          {gate.ready ? "Passed" : "Required"}
        </span>
      </div>

      <div className="implementation-review-reviewers">
        {reviewers.map((agent) => (
          <label className="settings-check" key={agent.id}>
            <input
              type="checkbox"
              checked={selectedReviewerIds.includes(agent.id)}
              disabled={running || readOnly}
              onChange={() => toggleReviewer(agent.id)}
            />
            <span>{agent.name}</span>
          </label>
        ))}
        {reviewers.length === 0 && <span className="settings-dim">没有可用的独立 Review Agent。</span>}
      </div>

      {escalationAgents.length > 0 && latestRun && !gate.ready && !readOnly && (
        <div className="implementation-review-escalation">
          <select value={escalationAgentId} onChange={(event) => setEscalationAgentId(event.target.value)}>
            <option value="">升级给其他实施 Agent</option>
            {escalationAgents.map((agent) => (
              <option key={agent.id} value={agent.id}>{agent.name}</option>
            ))}
          </select>
          <input
            value={escalationReason}
            placeholder="记录切换原因"
            onChange={(event) => setEscalationReason(event.target.value)}
          />
          <button
            type="button"
            disabled={!escalationAgentId || escalationReason.trim().length < 5}
            onClick={() => void escalatePrimaryAgent()}
          >切换主 Agent</button>
        </div>
      )}

      <Button
        type="button"
        variant="ghost"
        iconLeft={<RefreshCw size={14} />}
        disabled={running || readOnly || selectedReviewerIds.length === 0 || task.status !== "reviewing"}
        onClick={() => void runReviews()}
      >
        {running ? "Review 运行中…" : latestRun ? "重新 Review" : "运行 Review"}
      </Button>

      {latestRun && (
        <div className="implementation-review-results">
          <div className="implementation-review-run-status">
            <b>{latestRun.status}</b>
            <span>{latestRun.reviewerAgentIds.length} reviewer(s)</span>
          </div>
          {latestReviews.map((review) => (
            <article className="implementation-review-result" key={review.id}>
              <header>
                <span>{review.status === "succeeded" ? <CheckCircle2 size={14} /> : <XCircle size={14} />}</span>
                <b>{review.reviewerAgentName}</b>
                <em>{review.status}</em>
              </header>
              <p>{review.summary}</p>
              {review.findings.map((finding) => (
                <div className={`implementation-review-finding severity-${finding.severity}`} key={finding.id}>
                  <div>
                    <strong>{finding.severity}: {finding.title}</strong>
                    <span>{finding.status}</span>
                  </div>
                  <p>{finding.detail}</p>
                  {(finding.file || finding.line) && (
                    <code>{finding.file ?? "unknown file"}{finding.line ? `:${finding.line}` : ""}</code>
                  )}
                  {["open", "pending_re_review"].includes(finding.status) && !readOnly && (
                    <div className="implementation-review-decision">
                      {finding.severity === "blocker" && (
                        <button
                          type="button"
                          disabled={!task.primaryAgentId || repairingFindingId === finding.id}
                          onClick={() => void repairFinding(finding)}
                        >
                          {repairingFindingId === finding.id ? "正在启动修复…" : "交给主 Agent 修复"}
                        </button>
                      )}
                      <input
                        value={decisionDrafts[finding.id] ?? ""}
                        placeholder="记录修复说明或接受风险理由"
                        onChange={(event) => setDecisionDrafts((current) => ({
                          ...current,
                          [finding.id]: event.target.value,
                        }))}
                      />
                      <button
                        type="button"
                        disabled={(decisionDrafts[finding.id]?.trim().length ?? 0) < 5}
                        onClick={() => void decideFinding(finding.id, "resolved")}
                      >已修复，待重审</button>
                      <button
                        type="button"
                        disabled={(decisionDrafts[finding.id]?.trim().length ?? 0) < 5}
                        onClick={() => void decideFinding(finding.id, "accepted_risk")}
                      >接受风险</button>
                      {finding.severity !== "blocker" && (
                        <button
                          type="button"
                          disabled={(decisionDrafts[finding.id]?.trim().length ?? 0) < 5}
                          onClick={() => void decideFinding(finding.id, "dismissed")}
                        >忽略</button>
                      )}
                    </div>
                  )}
                </div>
              ))}
              {review.status === "succeeded" && review.findings.length === 0 && (
                <div className="implementation-review-clean">未发现需要记录的问题。</div>
              )}
              <details>
                <summary>查看 Review 原始输出</summary>
                <pre>{review.rawOutput}</pre>
              </details>
            </article>
          ))}
        </div>
      )}
    </section>
  );
}
