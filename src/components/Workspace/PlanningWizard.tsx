import { Fragment, useEffect, useMemo, useState, type FormEvent } from "react";
import {
  AlertTriangle,
  Bot,
  CheckCircle2,
  FileText,
  Plus,
  RefreshCw,
  Send,
  Users,
  X,
} from "lucide-react";
import type { AgentConfig, AgentInvocation, PlanReview, Task } from "../../domain";
import { useAgentBridge } from "../../hooks/useAgentBridge";
import { useTaskBridge } from "../../hooks/useTaskBridge";
import { useAppState } from "../../state/AppStateContext";
import { Button } from "../common/Button";
import "./Workspace.css";

export type PlanningWizardStep = "setup" | "generate" | "review" | "decision" | "final";

const WIZARD_STEPS: Array<{ id: PlanningWizardStep; label: string }> = [
  { id: "setup", label: "需求与选择" },
  { id: "generate", label: "生成方案" },
  { id: "review", label: "互评" },
  { id: "decision", label: "人工决策" },
  { id: "final", label: "共识计划" },
];

const REVIEW_FINDINGS = [
  "同意拆分 Todo",
  "发现测试缺口",
  "建议收窄范围",
  "补充回滚策略",
  "确认风险可控",
  "需要人工取舍",
];

function taskTitleFromRequirement(requirement: string) {
  const firstLine = requirement.trim().split(/\r?\n/)[0] ?? "Planning task";
  return firstLine.slice(0, 48) || "Planning task";
}

function planningCapable(agent: AgentConfig) {
  return agent.capabilities.includes("planning");
}

function selectablePlanningAgent(agent: AgentConfig) {
  return agent.enabled && agent.available && planningCapable(agent);
}

function adapterLabel(adapterType: string) {
  switch (adapterType) {
    case "codex_cli":
      return "Codex";
    case "claude_code_cli":
      return "Claude Code";
    case "amp_cli":
      return "Amp";
    case "dummy":
      return "Mock";
    default:
      return "CLI";
  }
}

function capabilityLabel(capability: string) {
  switch (capability) {
    case "planning":
      return "规划";
    case "review":
      return "Review";
    case "testing":
      return "测试建议";
    case "implementation":
      return "实施";
    case "debugging":
      return "调试";
    case "documentation":
      return "文档";
    default:
      return capability;
  }
}

function composeRequirement(goal: string, constraints: string, nonGoals: string, acceptance: string) {
  return [
    goal.trim(),
    constraints.trim() ? `\n约束:\n${constraints.trim()}` : "",
    nonGoals.trim() ? `\n非目标:\n${nonGoals.trim()}` : "",
    acceptance.trim() ? `\n验收标准:\n${acceptance.trim()}` : "",
  ]
    .filter(Boolean)
    .join("\n");
}

function latestPlanningInvocations(task: Task | null) {
  if (!task) {
    return [];
  }

  const latestPlanningRun = task.planningRuns[task.planningRuns.length - 1];
  if (!latestPlanningRun) {
    return task.agentInvocations;
  }

  return task.agentInvocations.filter(
    (invocation) => invocation.planningRunId === latestPlanningRun.id,
  );
}

function elapsedLabel(invocation: AgentInvocation) {
  const ended = invocation.endedAtMs ?? Date.now();
  const seconds = Math.max(1, Math.round((ended - invocation.startedAtMs) / 1000));
  return seconds < 60 ? `${seconds}s` : `${Math.round(seconds / 60)}m`;
}

function scoreFromInvocation(invocation: AgentInvocation | undefined, seed: number) {
  if (!invocation || invocation.status !== "succeeded") {
    return 0;
  }

  const outputWeight = Math.min(18, Math.floor(invocation.rawOutput.length / 480));
  return Math.min(96, 72 + outputWeight + seed);
}

function shortText(value: string | undefined, fallback: string) {
  const normalized = value?.replace(/\s+/g, " ").trim();
  if (!normalized) {
    return fallback;
  }

  return normalized.length > 128 ? `${normalized.slice(0, 128)}...` : normalized;
}

function matrixFinding(rowIndex: number, columnIndex: number) {
  return REVIEW_FINDINGS[(rowIndex * 3 + columnIndex) % REVIEW_FINDINGS.length];
}

function stepAvailability(step: PlanningWizardStep, task: Task | null, hasInvocations: boolean) {
  if (step === "setup") {
    return true;
  }

  if (step === "generate") {
    return Boolean(task);
  }

  if (step === "review" || step === "decision") {
    return hasInvocations;
  }

  return Boolean(task?.finalPlan);
}

interface PlanningWizardProps {
  previewMode?: boolean;
  initialStep?: PlanningWizardStep;
}

export function PlanningWizard({ previewMode = false, initialStep = "setup" }: PlanningWizardProps) {
  const { state, dispatch } = useAppState();
  const { loadAgents, runPlanningDiscussion, runPlanReviews } = useAgentBridge();
  const { appendFeedback, confirmPlan, createTask, loadTasks, recordPlanningDecision } =
    useTaskBridge();
  const project = state.projects.current;
  const task = state.tasks.find((candidate) => candidate.id === state.app.selectedTaskId) ?? null;
  const invocations = useMemo(() => latestPlanningInvocations(task), [task]);
  const planReviews = useMemo(() => {
    if (!task) {
      return [];
    }

    const latestPlanningRun = task.planningRuns[task.planningRuns.length - 1];
    if (!latestPlanningRun) {
      return task.planReviews;
    }

    return task.planReviews.filter((review) => review.planningRunId === latestPlanningRun.id);
  }, [task]);
  const [activeStep, setActiveStep] = useState<PlanningWizardStep>(initialStep);
  const [goal, setGoal] = useState("");
  const [constraints, setConstraints] = useState("");
  const [nonGoals, setNonGoals] = useState("");
  const [acceptance, setAcceptance] = useState("");
  const [selectedAgentIds, setSelectedAgentIds] = useState<string[]>([]);
  const [humanComment, setHumanComment] = useState("");

  useEffect(() => {
    if (previewMode) {
      return;
    }

    void loadAgents();
  }, [loadAgents, previewMode]);

  useEffect(() => {
    if (previewMode) {
      return;
    }

    if (project) {
      void loadTasks(project.path);
    }
  }, [loadTasks, previewMode, project]);

  const planningAgents = useMemo(
    () => state.agents.filter((agent) => planningCapable(agent)),
    [state.agents],
  );
  const selectedAgents = useMemo(
    () =>
      selectedAgentIds
        .map((id) => planningAgents.find((agent) => agent.id === id))
        .filter((agent): agent is AgentConfig => Boolean(agent)),
    [planningAgents, selectedAgentIds],
  );
  const visibleAgents = useMemo(() => {
    if (selectedAgents.length > 0) {
      return selectedAgents;
    }

    return planningAgents.filter(selectablePlanningAgent);
  }, [planningAgents, selectedAgents]);

  useEffect(() => {
    if (goal || !task?.rawRequirement) {
      return;
    }

    setGoal(task.rawRequirement);
  }, [goal, task?.rawRequirement]);

  useEffect(() => {
    if (selectedAgentIds.length > 0 || planningAgents.length === 0) {
      return;
    }

    const taskAgentIds = task?.selectedPlanningAgentIds.filter((id) =>
      planningAgents.some((agent) => agent.id === id),
    );
    const defaultIds =
      taskAgentIds && taskAgentIds.length > 0
        ? taskAgentIds
        : planningAgents
            .filter((agent) => selectablePlanningAgent(agent) && agent.adapterType !== "dummy")
            .map((agent) => agent.id);

    setSelectedAgentIds(defaultIds);
  }, [planningAgents, selectedAgentIds.length, task?.selectedPlanningAgentIds]);

  useEffect(() => {
    if (state.app.isLoadingTasks && activeStep !== "generate") {
      setActiveStep("generate");
      return;
    }

    if (task?.status === "plan_review" && activeStep === "generate") {
      setActiveStep("review");
    }
  }, [activeStep, state.app.isLoadingTasks, task?.status]);

  const selectedCount = selectedAgentIds.length;
  const runShape =
    selectedCount <= 1
      ? "1 个 Agent 将生成方案，随后进入人工 Review"
      : `${selectedCount} 个 Agent 将各自生成方案，随后进入互评`;
  const hasInvocations = invocations.length > 0;
  const hasPlanReviews = planReviews.length > 0;
  const isGenerating = state.app.isLoadingTasks && activeStep === "generate";
  const currentStep = isGenerating ? "generate" : activeStep;

  function toggleAgent(agent: AgentConfig) {
    if (!selectablePlanningAgent(agent)) {
      return;
    }

    setSelectedAgentIds((current) =>
      current.includes(agent.id)
        ? current.filter((id) => id !== agent.id)
        : [...current, agent.id],
    );
  }

  async function handleStartPlanning(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();

    if (!project || !goal.trim() || selectedAgentIds.length === 0) {
      return;
    }

    if (previewMode) {
      setActiveStep("generate");
      return;
    }

    const requirement = composeRequirement(goal, constraints, nonGoals, acceptance);
    setActiveStep("generate");
    const activeTask =
      task ??
      (await createTask({
        projectPath: project.path,
        title: taskTitleFromRequirement(goal),
        rawRequirement: requirement,
      }));

    if (!activeTask) {
      return;
    }

    const updatedTask = await runPlanningDiscussion({
      projectPath: project.path,
      taskId: activeTask.id,
      requirement,
      agentIds: selectedAgentIds,
    });

    if (updatedTask) {
      setActiveStep("review");
    }
  }

  async function handleHumanComment(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();

    if (!project || !task || !humanComment.trim()) {
      return;
    }

    if (previewMode) {
      setHumanComment("");
      return;
    }

    const updated = await appendFeedback(project.path, task.id, undefined, humanComment.trim());
    if (updated) {
      setHumanComment("");
    }
  }

  async function handleEnterReview() {
    if (!project || !task) {
      setActiveStep("review");
      return;
    }

    if (previewMode) {
      setActiveStep("review");
      return;
    }

    const successfulInvocations = invocations.filter(
      (invocation) => invocation.status === "succeeded",
    );
    if (successfulInvocations.length > 1 && !hasPlanReviews) {
      const updatedTask = await runPlanReviews(project.path, task.id);
      if (!updatedTask) {
        return;
      }
    }

    setActiveStep("review");
  }

  async function handlePlanningDecision(title: string, content: string) {
    if (!project || !task) {
      return;
    }

    if (previewMode) {
      return;
    }

    await recordPlanningDecision({
      projectPath: project.path,
      taskId: task.id,
      title,
      content,
    });
  }

  async function handleConfirmPlan() {
    if (!project || !task?.finalPlan) {
      return;
    }

    if (previewMode) {
      return;
    }

    await confirmPlan(project.path, task.id);
  }

  function renderStepper() {
    const activeIndex = WIZARD_STEPS.findIndex((step) => step.id === currentStep);

    return (
      <div className="planning-wizard-stepper">
        {WIZARD_STEPS.map((step, index) => {
          const available = stepAvailability(step.id, task, hasInvocations);
          const status = index < activeIndex ? "done" : index === activeIndex ? "active" : "pending";

          return (
            <button
              type="button"
              className={`planning-step planning-step-${status}`}
              disabled={!available || isGenerating}
              key={step.id}
              onClick={() => setActiveStep(step.id)}
            >
              <span className="planning-step-index">
                {status === "done" ? <CheckCircle2 size={13} /> : index + 1}
              </span>
              <span>{step.label}</span>
            </button>
          );
        })}
      </div>
    );
  }

  function renderAgentSelector() {
    return (
      <section className="wizard-panel agent-picker-panel">
        <div className="wizard-section-heading">
          <div>
            <span className="wizard-kicker">参与 Agent</span>
            <h3>选择 1 到多个规划 Agent</h3>
          </div>
          <span className="wizard-count-pill">已选 {selectedCount} 个</span>
        </div>

        <div className="agent-select-list">
          {planningAgents.map((agent) => {
            const selected = selectedAgentIds.includes(agent.id);
            const selectable = selectablePlanningAgent(agent);

            return (
              <button
                type="button"
                className={`agent-select-row${selected ? " selected" : ""}`}
                disabled={!selectable}
                key={agent.id}
                onClick={() => toggleAgent(agent)}
              >
                <span className={`agent-status-dot ${selectable ? "available" : "missing"}`} />
                <span className="agent-select-main">
                  <span className="agent-select-name">{agent.name}</span>
                  <span>{adapterLabel(agent.adapterType)}</span>
                </span>
                <span className="agent-select-capabilities">
                  {agent.capabilities.slice(0, 3).map((capability) => (
                    <span key={capability}>{capabilityLabel(capability)}</span>
                  ))}
                </span>
                {selected ? <X size={14} /> : <Plus size={14} />}
              </button>
            );
          })}

          {planningAgents.length === 0 && (
            <div className="wizard-empty-copy">
              还没有可用 Agent。请先在 Settings 中配置 Codex、Claude Code、Amp 或自定义 CLI。
            </div>
          )}
        </div>

        <button
          type="button"
          className="wizard-add-agent"
          onClick={() => dispatch({ type: "app/viewSelected", view: "settings" })}
        >
          <Plus size={14} />
          添加 Agent
        </button>
      </section>
    );
  }

  function renderSetupStep() {
    return (
      <form className="planning-step-workspace setup-step" onSubmit={handleStartPlanning}>
        <section className="wizard-panel requirement-editor">
          <div className="wizard-section-heading">
            <div>
              <span className="wizard-kicker">Step 1</span>
              <h2>开发目标</h2>
            </div>
            <span className="wizard-mode-pill">{runShape}</span>
          </div>

          <label className="wizard-field wizard-field-large">
            <span>需求</span>
            <textarea
              value={goal}
              onChange={(event) => setGoal(event.target.value)}
              placeholder="例如：重构 Loom 的规划流程，让多个 Agent 先独立生成方案，再互评，并允许人工介入形成最终计划。"
            />
          </label>

          <div className="wizard-field-grid">
            <label className="wizard-field">
              <span>约束</span>
              <textarea
                value={constraints}
                onChange={(event) => setConstraints(event.target.value)}
                placeholder="不引入新依赖；先做 MVP；保留现有任务记录。"
              />
            </label>
            <label className="wizard-field">
              <span>非目标</span>
              <textarea
                value={nonGoals}
                onChange={(event) => setNonGoals(event.target.value)}
                placeholder="暂不做实施、调试、部署自动化。"
              />
            </label>
            <label className="wizard-field wizard-field-wide">
              <span>验收标准</span>
              <textarea
                value={acceptance}
                onChange={(event) => setAcceptance(event.target.value)}
                placeholder="能看到各 Agent 方案、互评结果、人工决策和最终计划文档。"
              />
            </label>
          </div>
        </section>

        <div className="wizard-side-stack">
          {renderAgentSelector()}
          <section className="wizard-panel planning-controls-panel">
            <div className="wizard-section-heading">
              <div>
                <span className="wizard-kicker">运行方式</span>
                <h3>计划策略</h3>
              </div>
            </div>
            <label className="wizard-check-row">
              <input type="checkbox" checked readOnly />
              <span>生成独立方案</span>
            </label>
            <label className="wizard-check-row">
              <input type="checkbox" checked={selectedCount > 1} readOnly />
              <span>{selectedCount > 1 ? "方案后自动互评" : "单 Agent 时使用人工 Review"}</span>
            </label>
            <label className="wizard-check-row">
              <input type="checkbox" checked readOnly />
              <span>最终计划需要人工确认</span>
            </label>
          </section>
        </div>

        <div className="wizard-action-bar">
          <span>{runShape}</span>
          <div className="wizard-action-buttons">
            <Button type="button" variant="ghost" disabled={!goal.trim()}>
              保存草稿
            </Button>
            <Button
              type="submit"
              variant="primary"
              iconRight={<Send size={14} />}
              disabled={!project || !goal.trim() || selectedAgentIds.length === 0 || state.app.isLoadingTasks}
            >
              开始生成方案
            </Button>
          </div>
        </div>
      </form>
    );
  }

  function renderGenerateStep() {
    const generationAgents = visibleAgents;

    return (
      <div className="planning-step-workspace generation-step">
        <section className="wizard-panel requirement-summary-strip">
          <div>
            <span className="wizard-kicker">Step 2</span>
            <h2>生成独立方案</h2>
            <p>{shortText(task?.rawRequirement ?? goal, "等待需求输入。")}</p>
          </div>
          <div className="generation-run-controls">
            <Button type="button" variant="ghost" iconLeft={<RefreshCw size={14} />} disabled={isGenerating}>
              重新运行失败项
            </Button>
            <Button
              type="button"
              variant="primary"
              disabled={!hasInvocations || isGenerating || state.app.isLoadingTasks}
              onClick={handleEnterReview}
            >
              进入互评
            </Button>
          </div>
        </section>

        <section className="proposal-lane-grid">
          {generationAgents.map((agent, index) => {
            const invocation = invocations.find((candidate) => candidate.agentId === agent.id);
            const status = isGenerating
              ? "running"
              : invocation?.status ?? (task ? "pending" : "idle");
            const scoreSeed = index * 3;

            return (
              <article className={`proposal-lane proposal-${status}`} key={agent.id}>
                <div className="proposal-lane-header">
                  <div>
                    <span className="wizard-kicker">{adapterLabel(agent.adapterType)}</span>
                    <h3>{agent.name}</h3>
                  </div>
                  <span className={`proposal-status proposal-status-${status}`}>{status}</span>
                </div>

                <div className="proposal-progress">
                  <span style={{ width: `${invocation ? scoreFromInvocation(invocation, scoreSeed) : isGenerating ? 54 + index * 12 : 12}%` }} />
                </div>

                <dl className="proposal-score-grid">
                  <div>
                    <dt>完整度</dt>
                    <dd>{scoreFromInvocation(invocation, scoreSeed) || (isGenerating ? 64 : 0)}%</dd>
                  </div>
                  <div>
                    <dt>风险</dt>
                    <dd>{invocation?.status === "failed" ? "高" : "中"}</dd>
                  </div>
                  <div>
                    <dt>耗时</dt>
                    <dd>{invocation ? elapsedLabel(invocation) : isGenerating ? "运行中" : "-"}</dd>
                  </div>
                </dl>

                <div className="proposal-outline">
                  <div>目标：{shortText(invocation?.outputSummary, "生成任务目标与边界。")}</div>
                  <div>影响范围：识别相关模块、文件和状态流。</div>
                  <div>测试策略：列出最小验证命令与人工验收点。</div>
                  <div>风险：标记需要互评或人工决策的问题。</div>
                </div>

                {invocation?.status === "failed" && invocation.stderrTail.length > 0 && (
                  <pre className="proposal-error">{invocation.stderrTail.slice(-4).join("\n")}</pre>
                )}

                <div className="proposal-actions">
                  <button type="button" disabled={!invocation?.evidenceRef}>查看原文</button>
                  <button type="button" disabled={isGenerating}>重新生成</button>
                  <button type="button">标记需澄清</button>
                </div>
              </article>
            );
          })}
        </section>

        <aside className="wizard-panel human-notes-panel">
          <div className="wizard-section-heading">
            <div>
              <span className="wizard-kicker">人工旁注</span>
              <h3>生成过程中补充</h3>
            </div>
          </div>
          {renderHumanCommentForm("例如：请所有 Agent 特别关注计划文档如何落盘。")}
        </aside>
      </div>
    );
  }

  function reviewForPair(reviewerId: string, targetId: string): PlanReview | undefined {
    return planReviews.find(
      (review) => review.reviewerAgentId === reviewerId && review.targetAgentId === targetId,
    );
  }

  function renderReviewMatrix() {
    const matrixAgents = visibleAgents.length > 0 ? visibleAgents : planningAgents;
    const singleAgent = matrixAgents.length <= 1;

    if (singleAgent) {
      const agent = matrixAgents[0];
      const invocation = agent ? invocations.find((candidate) => candidate.agentId === agent.id) : undefined;

      return (
        <section className="wizard-panel single-review-panel">
          <div className="wizard-section-heading">
            <div>
              <span className="wizard-kicker">人工 Review</span>
              <h2>单 Agent 计划审查</h2>
            </div>
          </div>
          <div className="single-review-grid">
            <div>
              <span>方案来源</span>
              <strong>{agent?.name ?? "未选择 Agent"}</strong>
            </div>
            <div>
              <span>方案状态</span>
              <strong>{invocation?.status ?? "pending"}</strong>
            </div>
            <div>
              <span>审查方式</span>
              <strong>人工确认</strong>
            </div>
          </div>
          <p className="wizard-muted-copy">
            选择 2 个或更多 Agent 后，这里会切换为 Agent 互评矩阵。
          </p>
        </section>
      );
    }

    return (
      <section className="wizard-panel review-matrix-panel">
        <div className="wizard-section-heading">
          <div>
            <span className="wizard-kicker">Step 3</span>
            <h2>互评矩阵</h2>
          </div>
          <span className="wizard-count-pill">{matrixAgents.length} 个 Agent</span>
        </div>

        <div
          className="review-matrix"
          style={{ gridTemplateColumns: `150px repeat(${matrixAgents.length}, minmax(160px, 1fr))` }}
        >
          <div className="review-matrix-cell review-matrix-corner">评审者 \\ 方案</div>
          {matrixAgents.map((agent) => (
            <div className="review-matrix-cell review-matrix-head" key={`head-${agent.id}`}>
              {agent.name} 方案
            </div>
          ))}
          {matrixAgents.map((reviewer, rowIndex) => (
            <Fragment key={reviewer.id}>
              <div className="review-matrix-cell review-matrix-row-head">
                <Bot size={14} />
                {reviewer.name}
              </div>
              {matrixAgents.map((target, columnIndex) => {
                const reviewerInvocation = invocations.find((candidate) => candidate.agentId === reviewer.id);
                const targetInvocation = invocations.find((candidate) => candidate.agentId === target.id);
                const self = reviewer.id === target.id;
                const blocked =
                  reviewerInvocation?.status === "failed" || targetInvocation?.status === "failed";
                const review = reviewForPair(reviewer.id, target.id);
                const statusClass = self
                  ? "self"
                  : blocked || review?.severity === "blocker"
                    ? "blocked"
                    : review
                      ? "ready"
                      : "pending";

                return (
                  <div
                    className={`review-matrix-cell review-matrix-result ${statusClass}`}
                    key={`${reviewer.id}-${target.id}`}
                  >
                    <strong>
                      {self
                        ? "自身方案"
                        : blocked
                          ? "无法互评"
                          : review?.finding ?? matrixFinding(rowIndex, columnIndex)}
                    </strong>
                    <span>
                      {self
                        ? targetInvocation?.status ?? "pending"
                        : blocked
                          ? "等待成功方案"
                          : review
                            ? `${review.status} / ${review.severity}`
                            : "待互评"}
                    </span>
                  </div>
                );
              })}
            </Fragment>
          ))}
        </div>
      </section>
    );
  }

  function renderPlanScoreCards() {
    return (
      <div className="plan-score-row">
        {visibleAgents.map((agent, index) => {
          const invocation = invocations.find((candidate) => candidate.agentId === agent.id);

          return (
            <article className="plan-score-card" key={agent.id}>
              <div>
                <span className="wizard-kicker">{adapterLabel(agent.adapterType)}</span>
                <h3>{agent.name} 方案</h3>
              </div>
              <dl>
                <div>
                  <dt>完整度</dt>
                  <dd>{scoreFromInvocation(invocation, index * 4)}%</dd>
                </div>
                <div>
                  <dt>风险</dt>
                  <dd>{invocation?.status === "failed" ? "高" : "中"}</dd>
                </div>
                <div>
                  <dt>可测试性</dt>
                  <dd>{scoreFromInvocation(invocation, index * 2 + 2)}%</dd>
                </div>
              </dl>
            </article>
          );
        })}
      </div>
    );
  }

  function renderHumanCommentForm(placeholder: string) {
    return (
      <form className="human-comment-form" onSubmit={handleHumanComment}>
        <div className="agent-mention-row">
          {visibleAgents.slice(0, 4).map((agent) => (
            <span key={agent.id}>@{agent.name}</span>
          ))}
        </div>
        <textarea
          value={humanComment}
          onChange={(event) => setHumanComment(event.target.value)}
          placeholder={placeholder}
          disabled={!task}
        />
        <Button
          type="submit"
          variant="primary"
          iconRight={<Send size={14} />}
          disabled={!task || !humanComment.trim()}
        >
          发送评论
        </Button>
      </form>
    );
  }

  function renderReviewStep() {
    return (
      <div className="planning-step-workspace review-step">
        {renderPlanScoreCards()}
        <div className="review-main-column">
          {renderReviewMatrix()}
        </div>
        <aside className="wizard-panel decision-side-panel">
          <div className="wizard-section-heading">
            <div>
              <span className="wizard-kicker">人工入口</span>
              <h3>Review 中介入</h3>
            </div>
          </div>
          <div className="decision-list">
            <button type="button">追问选中 Agent</button>
            <button type="button">要求修订方案</button>
            <button type="button" onClick={() => setActiveStep("decision")}>
              进入人工决策
            </button>
          </div>
          {renderHumanCommentForm("例如：@Codex 请对 Claude Code 的测试策略再给一次反证。")}
        </aside>
        {renderConsensusStrip()}
      </div>
    );
  }

  function renderDecisionStep() {
    return (
      <div className="planning-step-workspace decision-step">
        <section className="wizard-panel conflict-panel">
          <div className="wizard-section-heading">
            <div>
              <span className="wizard-kicker">Step 4</span>
              <h2>人工决策</h2>
            </div>
            <span className="wizard-count-pill">2 个冲突待选</span>
          </div>

          <div className="conflict-list">
            <article className="conflict-item">
              <div>
                <AlertTriangle size={15} />
                <strong>方案范围是否先收窄到规划闭环？</strong>
              </div>
              <p>Codex 建议先做规划闭环；Amp 建议同时保留调试入口。</p>
              <div className="conflict-actions">
                <button
                  type="button"
                  onClick={() =>
                    void handlePlanningDecision(
                      "规划范围",
                      "先收窄到多 Agent 规划、互评、人工决策和共识计划文档。",
                    )
                  }
                >
                  采纳收窄范围
                </button>
                <button
                  type="button"
                  onClick={() =>
                    void handlePlanningDecision("规划范围", "保留调试入口，但不作为当前 MVP 主流程。")
                  }
                >
                  保留调试入口
                </button>
              </div>
            </article>
            <article className="conflict-item">
              <div>
                <AlertTriangle size={15} />
                <strong>互评是否作为独立后端模型？</strong>
              </div>
              <p>Claude Code 建议先 UI 承载；Codex 建议补充持久化 review 结构。</p>
              <div className="conflict-actions">
                <button
                  type="button"
                  onClick={() =>
                    void handlePlanningDecision(
                      "互评记录",
                      "先由 UI 承载互评流程，再逐步补充后端模型。",
                    )
                  }
                >
                  先 UI 承载
                </button>
                <button
                  type="button"
                  onClick={() =>
                    void handlePlanningDecision(
                      "互评记录",
                      "加入后端 planReviews 模型，保存 Agent 互评证据。",
                    )
                  }
                >
                  加入模型变更
                </button>
              </div>
            </article>
          </div>
        </section>

        <aside className="wizard-panel human-decision-panel">
          <div className="wizard-section-heading">
            <div>
              <span className="wizard-kicker">人工评论</span>
              <h3>补充决策依据</h3>
            </div>
          </div>
          {renderHumanCommentForm("说明你选择某个方案的原因，或 @Agent 要求修订。")}
          <div className="decision-list">
            <button type="button">追问选中 Agent</button>
            <button type="button">要求修订</button>
            <button type="button" onClick={() => setActiveStep("final")}>
              合并最终计划
            </button>
          </div>
        </aside>

        {renderConsensusStrip()}
      </div>
    );
  }

  function renderConsensusStrip() {
    return (
      <section className="consensus-strip">
        <div>
          <span className="consensus-label success">共识</span>
          <p>先完成多 Agent 规划、互评、人工确认和最终计划文档。</p>
        </div>
        <div>
          <span className="consensus-label warning">冲突</span>
          <p>
            {task?.planningDecisions.length
              ? `已记录 ${task.planningDecisions.length} 条人工决策。`
              : "互评是否进入后端模型；调试入口是否暂缓。"}
          </p>
        </div>
        <div>
          <span className="consensus-label risk">风险</span>
          <p>当前后端仍是一次性 planning run，逐步状态需后续补齐。</p>
        </div>
        <div>
          <span className="consensus-label">下一步</span>
          <p>人工决策后合并共识计划。</p>
        </div>
      </section>
    );
  }

  function renderFinalStep() {
    return (
      <div className="planning-step-workspace final-step">
        <section className="wizard-panel final-plan-panel">
          <div className="wizard-section-heading">
            <div>
              <span className="wizard-kicker">Step 5</span>
              <h2>共识计划文档</h2>
            </div>
            {task?.finalPlanPath && (
              <span className="plan-path">
                <FileText size={14} />
                {task.finalPlanPath}
              </span>
            )}
          </div>

          <pre className="final-plan-document">
            {task?.finalPlan ?? "等待 Agent 方案和人工决策生成最终计划。"}
          </pre>
        </section>

        <aside className="wizard-panel final-approval-panel">
          <div className="wizard-section-heading">
            <div>
              <span className="wizard-kicker">确认</span>
              <h3>进入实施前检查</h3>
            </div>
          </div>
          <label className="wizard-check-row">
            <input type="checkbox" checked readOnly />
            <span>已包含目标、非目标和验收标准</span>
          </label>
          <label className="wizard-check-row">
            <input type="checkbox" checked={hasInvocations} readOnly />
            <span>已记录 Agent 方案证据</span>
          </label>
          <label className="wizard-check-row">
            <input type="checkbox" checked readOnly />
            <span>人工可接受剩余风险</span>
          </label>
          <Button
            type="button"
            variant="primary"
            iconRight={<CheckCircle2 size={14} />}
            disabled={!task?.finalPlan}
            onClick={handleConfirmPlan}
          >
            确认计划并进入实施
          </Button>
        </aside>
      </div>
    );
  }

  function renderCurrentStep() {
    switch (currentStep) {
      case "generate":
        return renderGenerateStep();
      case "review":
        return renderReviewStep();
      case "decision":
        return renderDecisionStep();
      case "final":
        return renderFinalStep();
      case "setup":
      default:
        return renderSetupStep();
    }
  }

  if (!project) {
    return (
      <div className="workspace-empty-state redesigned-empty">
        <Users size={28} />
        <div className="workspace-empty-title">先打开一个项目</div>
        <div className="workspace-empty-copy">规划室需要项目路径、Agent 配置和计划文档落盘位置。</div>
      </div>
    );
  }

  return (
    <div className="planning-wizard-shell">
      <div className="planning-wizard-topbar">
        <div className="planning-wizard-title">
          <span className="wizard-kicker">Planning Wizard</span>
          <h1>{task?.title ?? "创建共识计划"}</h1>
        </div>
        {renderStepper()}
        <div className="planning-wizard-status">
          <span>{project.isGitRepository ? project.gitBranch ?? "git repo" : "no git"}</span>
          <span className={project.hasUncommittedChanges ? "status-attention" : "status-ok"}>
            {project.hasUncommittedChanges ? "有未提交变更" : "工作区干净"}
          </span>
        </div>
      </div>

      <div className="planning-wizard-content">
        {state.app.taskError && <div className="wizard-error">{state.app.taskError}</div>}
        {state.app.agentError && <div className="wizard-error">{state.app.agentError}</div>}
        {renderCurrentStep()}
      </div>
    </div>
  );
}
