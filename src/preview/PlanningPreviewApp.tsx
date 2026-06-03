import { Moon } from "lucide-react";
import { ThemeProvider } from "../contexts/ThemeContext";
import type { AppState } from "../state/reducer";
import { AppStateProvider } from "../state/AppStateContext";
import { PlanningWizard, type PlanningWizardStep } from "../components/Workspace/PlanningWizard";
import type { AgentConfig, Task } from "../domain";
import "../styles/theme.css";
import "../App.css";
import "../layouts/AppLayout.css";
import "../components/Sidebar/Sidebar.css";
import "../components/Workspace/Workspace.css";

const now = Date.now();

const previewAgents: AgentConfig[] = [
  {
    id: "agent-codex",
    name: "Codex",
    command: "codex",
    args: [],
    workingDirectoryPolicy: "project_root",
    capabilities: ["planning", "review", "testing"],
    adapterType: "codex_cli",
    canWriteFiles: true,
    canRunCommands: true,
    enabled: true,
    available: true,
  },
  {
    id: "agent-claude",
    name: "Claude Code",
    command: "claude",
    args: [],
    workingDirectoryPolicy: "project_root",
    capabilities: ["planning", "review", "testing"],
    adapterType: "claude_code_cli",
    canWriteFiles: true,
    canRunCommands: true,
    enabled: true,
    available: true,
  },
  {
    id: "agent-amp",
    name: "Amp",
    command: "amp",
    args: [],
    workingDirectoryPolicy: "project_root",
    capabilities: ["planning", "review", "testing"],
    adapterType: "amp_cli",
    canWriteFiles: false,
    canRunCommands: false,
    enabled: true,
    available: true,
  },
  {
    id: "agent-hermes",
    name: "Hermes",
    command: "hermes",
    args: [],
    workingDirectoryPolicy: "project_root",
    capabilities: ["planning", "review"],
    adapterType: "cli",
    canWriteFiles: false,
    canRunCommands: false,
    enabled: true,
    available: false,
  },
];

const previewTask: Task = {
  id: "task-preview-planning",
  projectPath: "/Users/guojian/Workspace/naijoug/loom",
  title: "多 Agent 共识计划 UI 重构",
  rawRequirement:
    "重构 Loom 的规划流程，让 Codex、Claude Code、Amp 先独立生成方案，再相互 Review，并允许人工介入形成最终共识计划。",
  status: "plan_review",
  selectedPlanningAgentIds: ["agent-codex", "agent-claude", "agent-amp"],
  primaryAgentId: undefined,
  reviewAgentIds: [],
  finalPlan:
    "# 多 Agent 共识计划 UI 重构 — Final Plan\n\n## 共识\n\n- 规划流程拆成需求与选择、生成方案、互评、人工决策、共识计划。\n- Agent 数量支持 1 到多个。\n- 2 个以上 Agent 时保存互评记录。\n\n## Human Decisions\n\n- **规划范围**: 先聚焦多 Agent 规划闭环。\n",
  finalPlanPath: "/Users/guojian/Workspace/naijoug/loom/docs/plans/preview-final-plan.md",
  discussionSummary:
    "Codex、Claude Code、Amp 均建议先完成规划闭环，再扩展实施、调试和验收。",
  planningRuns: [
    {
      id: "planning-preview",
      taskId: "task-preview-planning",
      requirement: "多 Agent 共识计划 UI 重构",
      selectedAgentIds: ["agent-codex", "agent-claude", "agent-amp"],
      status: "succeeded",
      summary: "三份方案已生成，进入互评和人工决策。",
      startedAtMs: now - 180000,
      endedAtMs: now - 120000,
    },
  ],
  agentInvocations: [
    {
      id: "invoke-codex",
      planningRunId: "planning-preview",
      taskId: "task-preview-planning",
      agentId: "agent-codex",
      agentName: "Codex",
      status: "succeeded",
      promptSummary: "规划分步 UI",
      rawOutput: "Goal\n- 拆分规划流程\n- 保留互评证据\nVerification\n- 前端构建与截图验证",
      outputSummary: "建议先做分步 Planning Wizard，并把互评作为独立记录。",
      evidenceRef: ".loom/tasks/task-preview-planning/planning/codex.stdout.md",
      stderrTail: [],
      timedOut: false,
      exitCode: 0,
      startedAtMs: now - 170000,
      endedAtMs: now - 132000,
    },
    {
      id: "invoke-claude",
      planningRunId: "planning-preview",
      taskId: "task-preview-planning",
      agentId: "agent-claude",
      agentName: "Claude Code",
      status: "succeeded",
      promptSummary: "规划分步 UI",
      rawOutput: "Goal\n- 强调人工决策\n- 保留最终计划文档\nRisk\n- 避免 dashboard 信息过载",
      outputSummary: "建议把人工决策从总览中独立出来，并同步写入最终计划。",
      evidenceRef: ".loom/tasks/task-preview-planning/planning/claude.stdout.md",
      stderrTail: [],
      timedOut: false,
      exitCode: 0,
      startedAtMs: now - 168000,
      endedAtMs: now - 129000,
    },
    {
      id: "invoke-amp",
      planningRunId: "planning-preview",
      taskId: "task-preview-planning",
      agentId: "agent-amp",
      agentName: "Amp",
      status: "succeeded",
      promptSummary: "规划分步 UI",
      rawOutput: "Goal\n- Agent 数量动态变化\n- 单 Agent 时进入人工 review\nRisk\n- 多 Agent 时互评成本增加",
      outputSummary: "建议明确 1、2、3+ Agent 的不同互评状态。",
      evidenceRef: ".loom/tasks/task-preview-planning/planning/amp.stdout.md",
      stderrTail: [],
      timedOut: false,
      exitCode: 0,
      startedAtMs: now - 166000,
      endedAtMs: now - 126000,
    },
  ],
  planReviews: [
    {
      id: "review-codex-claude",
      planningRunId: "planning-preview",
      taskId: "task-preview-planning",
      reviewerAgentId: "agent-codex",
      reviewerAgentName: "Codex",
      targetAgentId: "agent-claude",
      targetAgentName: "Claude Code",
      status: "succeeded",
      finding: "同意人工决策独立成步",
      severity: "info",
      accepted: false,
      rawOutput: "Agreement: 人工决策应该独立成步。\nSeverity: info",
      evidenceRef: ".loom/tasks/task-preview-planning/reviews/codex-claude.stdout.md",
      startedAtMs: now - 100000,
      endedAtMs: now - 92000,
    },
    {
      id: "review-codex-amp",
      planningRunId: "planning-preview",
      taskId: "task-preview-planning",
      reviewerAgentId: "agent-codex",
      reviewerAgentName: "Codex",
      targetAgentId: "agent-amp",
      targetAgentName: "Amp",
      status: "succeeded",
      finding: "建议先收窄到规划闭环",
      severity: "risk",
      accepted: false,
      rawOutput: "Concern: 调试入口暂缓。\nSeverity: risk",
      evidenceRef: ".loom/tasks/task-preview-planning/reviews/codex-amp.stdout.md",
      startedAtMs: now - 99000,
      endedAtMs: now - 91000,
    },
    {
      id: "review-claude-codex",
      planningRunId: "planning-preview",
      taskId: "task-preview-planning",
      reviewerAgentId: "agent-claude",
      reviewerAgentName: "Claude Code",
      targetAgentId: "agent-codex",
      targetAgentName: "Codex",
      status: "succeeded",
      finding: "发现最终计划确认缺口",
      severity: "risk",
      accepted: false,
      rawOutput: "Concern: 需要确认最终计划文档。\nSeverity: risk",
      evidenceRef: ".loom/tasks/task-preview-planning/reviews/claude-codex.stdout.md",
      startedAtMs: now - 98000,
      endedAtMs: now - 90000,
    },
    {
      id: "review-claude-amp",
      planningRunId: "planning-preview",
      taskId: "task-preview-planning",
      reviewerAgentId: "agent-claude",
      reviewerAgentName: "Claude Code",
      targetAgentId: "agent-amp",
      targetAgentName: "Amp",
      status: "succeeded",
      finding: "建议补充单 Agent 路径",
      severity: "info",
      accepted: false,
      rawOutput: "Agreement: 需要支持 1 个 Agent。\nSeverity: info",
      evidenceRef: ".loom/tasks/task-preview-planning/reviews/claude-amp.stdout.md",
      startedAtMs: now - 97000,
      endedAtMs: now - 89000,
    },
    {
      id: "review-amp-codex",
      planningRunId: "planning-preview",
      taskId: "task-preview-planning",
      reviewerAgentId: "agent-amp",
      reviewerAgentName: "Amp",
      targetAgentId: "agent-codex",
      targetAgentName: "Codex",
      status: "succeeded",
      finding: "同意保存互评证据",
      severity: "info",
      accepted: false,
      rawOutput: "Agreement: planReviews 模型合理。\nSeverity: info",
      evidenceRef: ".loom/tasks/task-preview-planning/reviews/amp-codex.stdout.md",
      startedAtMs: now - 96000,
      endedAtMs: now - 88000,
    },
    {
      id: "review-amp-claude",
      planningRunId: "planning-preview",
      taskId: "task-preview-planning",
      reviewerAgentId: "agent-amp",
      reviewerAgentName: "Amp",
      targetAgentId: "agent-claude",
      targetAgentName: "Claude Code",
      status: "succeeded",
      finding: "需要减少右侧面板复杂度",
      severity: "risk",
      accepted: false,
      rawOutput: "Concern: 人工决策面板不要过载。\nSeverity: risk",
      evidenceRef: ".loom/tasks/task-preview-planning/reviews/amp-claude.stdout.md",
      startedAtMs: now - 95000,
      endedAtMs: now - 87000,
    },
  ],
  planningDecisions: [
    {
      id: "decision-scope",
      taskId: "task-preview-planning",
      title: "规划范围",
      content: "先聚焦多 Agent 规划、互评、人工介入和共识计划文档。",
      status: "accepted",
      createdAtMs: now - 60000,
    },
  ],
  planTodos: [],
  events: [],
  commandRuns: [],
  feedback: [],
  repairContextPreview: undefined,
  createdAtMs: now - 220000,
  updatedAtMs: now - 50000,
};

const previewState: AppState = {
  app: {
    currentView: "workspace",
    activeProjectId: "project-loom",
    selectedTaskId: previewTask.id,
    selectedTodoId: null,
    activeCommandRunId: null,
    isLoadingProjects: false,
    isLoadingAgents: false,
    isLoadingTasks: false,
    projectError: null,
    agentError: null,
    taskError: null,
    commandError: null,
  },
  projects: {
    current: {
      id: "project-loom",
      path: "/Users/guojian/Workspace/naijoug/loom",
      name: "loom",
      detectedStacks: ["Tauri", "React", "Rust"],
      suggestedCommands: ["pnpm build", "cargo test"],
      isGitRepository: true,
      gitBranch: "codex/planning-ui",
      hasUncommittedChanges: true,
      loomDirReady: true,
      schemaVersion: 1,
    },
    recent: [],
  },
  agents: previewAgents,
  tasks: [previewTask],
  commandRuns: [],
  commandLogs: [],
};

function PreviewSidebar() {
  return (
    <>
      <div className="sidebar-brand">
        <span className="brand-icon">L</span>
        <span className="brand-title">LOOM</span>
        <button type="button" className="settings-icon-button" title="Dark mode">
          <Moon size={15} />
        </button>
      </div>
      <div className="sidebar-nav-container">
        <nav className="sidebar-nav-section">
          <div className="nav-heading-row">
            <h3 className="nav-heading">PROJECTS</h3>
          </div>
          <ul className="nav-list">
            <li>
              <a className="nav-item active" href="#loom">
                <span>loom</span>
              </a>
            </li>
          </ul>
        </nav>
        <nav className="sidebar-nav-section">
          <div className="nav-heading-row">
            <h3 className="nav-heading">PLANNING</h3>
          </div>
          <ul className="nav-list">
            <li>
              <a className="nav-item active" href="#task">
                <span>多 Agent 共识计划 UI 重构</span>
              </a>
            </li>
          </ul>
        </nav>
      </div>
    </>
  );
}

function previewStep(value: string): PlanningWizardStep {
  return ["setup", "generate", "review", "decision", "final"].includes(value)
    ? (value as PlanningWizardStep)
    : "review";
}

export function PlanningPreviewApp() {
  const params = new URLSearchParams(window.location.search);
  const step = previewStep(params.get("step") ?? "review");

  return (
    <ThemeProvider forcedTheme="dark">
      <AppStateProvider initialStateOverride={previewState}>
        <div className="app-layout">
          <aside className="app-sidebar">
            <PreviewSidebar />
          </aside>
          <main className="app-main">
            <div className="app-content">
              <PlanningWizard previewMode initialStep={step} />
            </div>
          </main>
        </div>
      </AppStateProvider>
    </ThemeProvider>
  );
}
