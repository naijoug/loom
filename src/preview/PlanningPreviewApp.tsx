import { ThemeProvider } from "../contexts/ThemeContext";
import { AppLayout } from "../layouts/AppLayout";
import { Board } from "../components/Board";
import { NewTaskModal, NewTaskModalHost } from "../components/Board";
import { Header } from "../components/Header";
import { Sidebar } from "../components/Sidebar";
import { AddProjectModal } from "../components/Sidebar/AddProjectModal";
import { SettingsPage } from "../components/Settings";
import { PlanningChat } from "../components/Planning";
import { WorkspaceSplit } from "../components/Workspace";
import type { AppState, AppView } from "../state/reducer";
import { AppStateProvider, useAppState } from "../state/AppStateContext";
import type { AgentConfig, ProjectSummary, Task } from "../domain";
import "../styles/theme.css";
import "../App.css";
import "../layouts/AppLayout.css";
import "../components/Sidebar/Sidebar.css";
import "../components/Workspace/Workspace.css";
import "../components/Board/Board.css";
import "../components/Planning/Planning.css";
import "../components/TaskDetail/TaskDetail.css";
import "../components/Settings/SettingsPage.css";

const now = Date.now();

const previewAgents: AgentConfig[] = [
  {
    id: "agent-codex",
    name: "Codex",
    command: "codex",
    args: [],
    workingDirectoryPolicy: "project_root",
    capabilities: ["planning", "implementation", "review", "testing"],
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
    capabilities: ["planning", "implementation", "review", "debugging", "testing"],
    adapterType: "claude_code_cli",
    canWriteFiles: true,
    canRunCommands: true,
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
  title: "Add regex toggle to search bar",
  rawRequirement:
    "Redesign Loom's planning flow so Codex and Claude Code draft proposals independently, review each other, and let a human guide the final consensus plan.",
  status: "plan_review",
  selectedPlanningAgentIds: ["agent-codex", "agent-claude", "agent-hermes"],
  primaryAgentId: undefined,
  reviewAgentIds: [],
  finalPlan:
    "# Multi-Agent Consensus Planning UI Refactor — Final Plan\n\n## Consensus\n\n- Split planning into requirement intake, proposal drafting, cross-review, human decisions, and the consensus plan.\n- Support one or more agents.\n- Save cross-review records when two or more agents participate.\n\n## Human Decisions\n\n- **Planning scope**: Focus on the multi-agent planning loop first.\n",
  finalPlanPath: "/Users/guojian/Workspace/naijoug/loom/docs/plans/preview-final-plan.md",
  discussionSummary:
    "Codex and Claude Code both recommended finishing the planning loop before expanding implementation, debugging, and acceptance.",
  planningRuns: [
    {
      id: "planning-preview",
      taskId: "task-preview-planning",
      requirement: "Multi-agent consensus planning UI refactor",
      selectedAgentIds: ["agent-codex", "agent-claude", "agent-hermes"],
      status: "succeeded",
      summary:
        "Planning agents requested: Codex, Claude Code, Hermes. Successful agents: Codex, Claude Code. Failed agents: Hermes. Final plan source: synthesized by Claude Code from 2 candidate plans and 2 cross-review findings.",
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
      promptSummary: "Step-based planning UI",
      rawOutput: "Goal\n- Split the planning flow\n- Preserve cross-review evidence\nVerification\n- Frontend build and screenshot validation",
      outputSummary: "Recommended a step-based planning wizard with cross-review as a separate record.",
      evidenceRef: ".loom/tasks/task-preview-planning/planning/codex.stdout.md",
      sessionId: "codex-preview-session",
      resumeCommand: "codex resume codex-preview-session",
      stderrTail: [],
      timedOut: false,
      attempt: 1,
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
      promptSummary: "Step-based planning UI",
      rawOutput: "Goal\n- Emphasize human decisions\n- Preserve the final plan document\nRisk\n- Avoid overloading the dashboard",
      outputSummary: "Recommended separating human decisions from the overview and writing them into the final plan.",
      evidenceRef: ".loom/tasks/task-preview-planning/planning/claude.stdout.md",
      sessionId: "claude-preview-session",
      resumeCommand: "claude --resume claude-preview-session",
      stderrTail: [],
      timedOut: false,
      attempt: 1,
      exitCode: 0,
      startedAtMs: now - 168000,
      endedAtMs: now - 129000,
    },
    {
      id: "invoke-hermes",
      planningRunId: "planning-preview",
      taskId: "task-preview-planning",
      agentId: "agent-hermes",
      agentName: "Hermes",
      status: "failed",
      promptSummary: "Step-based planning UI",
      rawOutput: "",
      outputSummary: "hermes timed out after 240000 ms.",
      evidenceRef: ".loom/tasks/task-preview-planning/planning/hermes.attempt-2.stdout.md",
      stderrRef: ".loom/tasks/task-preview-planning/planning/hermes.attempt-2.stderr.log",
      stderrTail: ["hermes timed out after 240000 ms."],
      timedOut: true,
      attempt: 2,
      failureKind: "timeout",
      failureDetail: "Timed out after 240s",
      errorLines: [],
      exitCode: undefined,
      startedAtMs: now - 166000,
      endedAtMs: now - 126000,
    },
    {
      id: "invoke-synthesis",
      planningRunId: "planning-preview",
      taskId: "task-preview-planning",
      agentId: "agent-claude",
      agentName: "Claude Code",
      status: "succeeded",
      promptSummary: "Synthesize final plan",
      rawOutput: "# Multi-Agent Consensus Planning UI Refactor\n\n## Goal\n\nMerge the two candidate plans.",
      outputSummary: "Synthesized final plan from candidate plans.",
      evidenceRef: ".loom/tasks/task-preview-planning/planning/synthesis.stdout.md",
      sessionId: "claude-synthesis-preview",
      resumeCommand: "claude --resume claude-synthesis-preview",
      stderrTail: [],
      timedOut: false,
      attempt: 1,
      exitCode: 0,
      startedAtMs: now - 110000,
      endedAtMs: now - 86000,
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
      finding: "Agrees that human decisions should be a separate step",
      severity: "info",
      accepted: false,
      rawOutput: "Agreement: human decisions should be a separate step.\nSeverity: info",
      evidenceRef: ".loom/tasks/task-preview-planning/reviews/codex-claude.stdout.md",
      startedAtMs: now - 100000,
      endedAtMs: now - 92000,
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
      finding: "Found a final-plan confirmation gap",
      severity: "risk",
      accepted: false,
      rawOutput: "Concern: the final plan document needs explicit confirmation.\nSeverity: risk",
      evidenceRef: ".loom/tasks/task-preview-planning/reviews/claude-codex.stdout.md",
      startedAtMs: now - 98000,
      endedAtMs: now - 90000,
    },
  ],
  planningDecisions: [
    {
      id: "decision-scope",
      taskId: "task-preview-planning",
      title: "Planning scope",
      content: "Focus first on multi-agent planning, cross-review, human intervention, and the consensus plan document.",
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

const implementationTask: Task = {
  ...previewTask,
  id: "task-preview-implementing",
  title: "Wire stream filter into reducer",
  rawRequirement: "Connect the data stream to the filter reducer and keep large log filtering off the main thread.",
  status: "implementing",
  primaryAgentId: "agent-claude",
  planTodos: [
    {
      id: "todo-read-filter",
      taskId: "task-preview-implementing",
      title: "Read current filter flow",
      description: "Trace reducer, TestingPane, and command log state boundaries.",
      status: "done",
      order: 0,
      planRef: "docs/plans/preview-final-plan.md#implementation",
    },
    {
      id: "todo-worker-filter",
      taskId: "task-preview-implementing",
      title: "Move filter into worker",
      description: "Move expensive regex filtering out of the render path.",
      status: "implementing",
      order: 1,
      planRef: "docs/plans/preview-final-plan.md#implementation",
    },
    {
      id: "todo-empty-state",
      taskId: "task-preview-implementing",
      title: "Fix empty-state guard",
      description: "Keep stale alerts out of filtered empty states.",
      status: "pending",
      order: 2,
      planRef: "docs/plans/preview-final-plan.md#implementation",
    },
  ],
  commandRuns: [
    {
      id: "run-impl-preview",
      taskId: "task-preview-implementing",
      command: "claude -p \"Implement stream-side filtering\"",
      cwd: "/Users/guojian/Workspace/naijoug/loom",
      startedAtMs: now - 50000,
      status: "running",
    },
  ],
  events: [
    {
      id: "event-impl-start",
      taskId: "task-preview-implementing",
      timestampMs: now - 50000,
      actor: "agent",
      status: "implementing",
      inputSummary: "Started Claude Code implementation loop",
    },
  ],
};

const testingTask: Task = {
  ...implementationTask,
  id: "task-preview-testing",
  title: "Match highlighting in log view",
  status: "debugging",
  planTodos: implementationTask.planTodos.map((todo) => ({
    ...todo,
    taskId: "task-preview-testing",
    status: "done",
  })),
  commandRuns: [
    {
      id: "run-frontend-preview",
      taskId: "task-preview-testing",
      command: "pnpm dev",
      cwd: "/Users/guojian/Workspace/naijoug/loom",
      startedAtMs: now - 45000,
      status: "running",
    },
    {
      id: "run-backend-preview",
      taskId: "task-preview-testing",
      command: "cargo tauri dev",
      cwd: "/Users/guojian/Workspace/naijoug/loom",
      startedAtMs: now - 42000,
      endedAtMs: now - 32000,
      status: "failed",
      exitCode: 1,
      errorSummary: {
        exitCode: 1,
        failed: true,
        matchedLines: ["ERROR thread 'main' panicked at src/handlers/logs.rs:88:24"],
        stderrTail: ["called `Result::unwrap()` on an `Err`: BufferOverflow { cap: 65536 }"],
      },
    },
  ],
  repairContextPreview:
    "Detected backend panic in log handler.\n\nSuggested fix:\n- Replace fixed 64k buffer allocation.\n+ Grow buffer from stream size and return recoverable errors.",
};

const doneTask: Task = {
  ...testingTask,
  id: "task-preview-done",
  title: "Scaffold search component",
  status: "completed",
  commandRuns: testingTask.commandRuns.map((run) => ({
    ...run,
    taskId: "task-preview-done",
    status: "succeeded",
    exitCode: 0,
    errorSummary: undefined,
    endedAtMs: run.endedAtMs ?? now - 12000,
  })),
  events: [
    ...testingTask.events,
    {
      id: "event-done",
      taskId: "task-preview-done",
      timestampMs: now - 8000,
      actor: "user",
      status: "completed",
      outputSummary: "Accepted after two repair cycles and passing checks.",
    },
  ],
};

const todoDebounceTask: Task = {
  ...previewTask,
  id: "loom-15",
  title: "Debounce input (300ms)",
  status: "ready_to_implement",
  primaryAgentId: undefined,
  planTodos: [
    {
      id: "loom-15-todo-1",
      taskId: "loom-15",
      title: "Add debounced search state",
      description: "Delay expensive filter updates while the user types.",
      status: "pending",
      order: 0,
    },
  ],
  commandRuns: [],
  events: [],
};

const todoPersistTask: Task = {
  ...previewTask,
  id: "loom-16",
  title: "Persist filter state across sessions",
  status: "ready_to_implement",
  primaryAgentId: undefined,
  planTodos: [
    {
      id: "loom-16-todo-1",
      taskId: "loom-16",
      title: "Persist search filter state",
      description: "Restore the last filter after reopening the project.",
      status: "pending",
      order: 0,
    },
  ],
  commandRuns: [],
  events: [],
};

const blockedTask: Task = {
  ...previewTask,
  id: "loom-19",
  title: "Web-worker regex - API 504 on fixtures",
  status: "blocked",
  primaryAgentId: "agent-codex",
  planTodos: [],
  commandRuns: [],
  events: [],
};

const doneStoreTask: Task = {
  ...doneTask,
  id: "loom-11",
  title: "Add filter state to store",
  primaryAgentId: "agent-codex",
  planTodos: [],
  commandRuns: [],
  events: [],
};

const speakerTasks: Task[] = [
  {
    ...previewTask,
    id: "spk-4",
    projectPath: "/Users/guojian/Workspace/naijoug/speaker",
    title: "TTS narration pipeline",
    rawRequirement: "Build the narration pipeline for children story audio.",
    status: "implementing",
    primaryAgentId: "agent-claude",
    planTodos: [],
    commandRuns: [],
    events: [],
  },
  {
    ...previewTask,
    id: "spk-5",
    projectPath: "/Users/guojian/Workspace/naijoug/speaker",
    title: "Chapter splitter from EPUB",
    rawRequirement: "Split EPUB chapters into narration-ready scenes.",
    status: "ready_to_implement",
    primaryAgentId: undefined,
    planTodos: [
      {
        id: "spk-5-todo-1",
        taskId: "spk-5",
        title: "Parse EPUB chapter markers",
        description: "Extract headings and reading order.",
        status: "pending",
        order: 0,
      },
    ],
    commandRuns: [],
    events: [],
  },
  {
    ...previewTask,
    id: "spk-3",
    projectPath: "/Users/guojian/Workspace/naijoug/speaker",
    title: "Pinyin disambiguation",
    rawRequirement: "Verify pinyin disambiguation before audio export.",
    status: "debugging",
    primaryAgentId: "agent-codex",
    planTodos: [],
    commandRuns: [],
    events: [],
  },
  {
    ...previewTask,
    id: "spk-2",
    projectPath: "/Users/guojian/Workspace/naijoug/speaker",
    title: "Voice preset: storyteller",
    rawRequirement: "Ship the storyteller voice preset.",
    status: "completed",
    primaryAgentId: "agent-codex",
    planTodos: [],
    commandRuns: [],
    events: [],
  },
];

const previewTasks = [
  implementationTask,
  previewTask,
  todoDebounceTask,
  todoPersistTask,
  testingTask,
  blockedTask,
  doneTask,
  doneStoreTask,
];

const loomProject: ProjectSummary = {
  id: "project-loom",
  path: "/Users/guojian/Workspace/naijoug/loom",
  name: "loom",
  detectedStacks: ["Tauri", "React", "Rust"],
  suggestedCommands: ["pnpm dev", "pnpm build", "cargo test --manifest-path src-tauri/Cargo.toml"],
  isGitRepository: true,
  gitBranch: "codex/planning-ui",
  hasUncommittedChanges: true,
  loomDirReady: true,
  schemaVersion: 1,
};

const speakerProject: ProjectSummary = {
  id: "project-speaker",
  path: "/Users/guojian/Workspace/naijoug/speaker",
  name: "speaker",
  detectedStacks: ["Node", "Audio", "TTS"],
  suggestedCommands: ["pnpm dev", "pnpm test"],
  isGitRepository: true,
  gitBranch: "main",
  hasUncommittedChanges: false,
  loomDirReady: true,
  schemaVersion: 1,
};

const previewState: AppState = {
  app: {
    currentView: "workspace",
    activeProjectId: "project-loom",
    selectedTaskId: previewTask.id,
    selectedTodoId: null,
    viewedStage: null,
    isCreatingTask: false,
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
    current: loomProject,
    recent: [loomProject, speakerProject],
  },
  agents: previewAgents,
  tasks: previewTasks,
  taskCache: {
    [loomProject.path]: previewTasks,
    [speakerProject.path]: speakerTasks,
  },
  commandRuns: [
    ...implementationTask.commandRuns,
    ...testingTask.commandRuns,
    ...doneTask.commandRuns,
  ],
  commandLogs: {
    "run-impl-preview": [
      {
        runId: "run-impl-preview",
        taskId: "task-preview-implementing",
        stream: "stdout",
        line: "Reading src/state/reducer.ts and TestingPane.tsx",
        timestampMs: now - 42000,
      },
      {
        runId: "run-impl-preview",
        taskId: "task-preview-implementing",
        stream: "stdout",
        line: "Editing worker-backed stream filter",
        timestampMs: now - 39000,
      },
    ],
    "run-frontend-preview": [
      {
        runId: "run-frontend-preview",
        taskId: "task-preview-testing",
        stream: "stdout",
        line: "VITE v7.3.3 ready in 92 ms",
        timestampMs: now - 41000,
      },
      {
        runId: "run-frontend-preview",
        taskId: "task-preview-testing",
        stream: "stdout",
        line: "Local: http://127.0.0.1:1420/",
        timestampMs: now - 40000,
      },
    ],
    "run-backend-preview": [
      {
        runId: "run-backend-preview",
        taskId: "task-preview-testing",
        stream: "stdout",
        line: "Running `target/debug/loom`",
        timestampMs: now - 38000,
      },
      {
        runId: "run-backend-preview",
        taskId: "task-preview-testing",
        stream: "stderr",
        line: "ERROR thread 'main' panicked at src/handlers/logs.rs:88:24",
        timestampMs: now - 36000,
      },
    ],
	  },
	  planningLogs: {
	    "planning-preview:planning:agent-codex": [
	      {
	        taskId: "task-preview-planning",
	        planningRunId: "planning-preview",
	        agentId: "agent-codex",
	        agentName: "Codex",
	        phase: "planning",
	        attempt: 1,
	        stream: "stdout",
	        lines: ["Session started: codex-preview-session", "Reading planning files", "Draft complete"],
	        timestampMs: now - 150000,
	      },
	    ],
	    "planning-preview:synthesis:agent-claude": [
	      {
	        taskId: "task-preview-planning",
	        planningRunId: "planning-preview",
	        agentId: "agent-claude",
	        agentName: "Claude Code",
	        phase: "synthesis",
	        attempt: 1,
	        stream: "stdout",
	        lines: ["Session started: claude-synthesis-preview", "Synthesizing final plan"],
	        timestampMs: now - 95000,
	      },
	    ],
	  },
	  planningProgress: {},
	};

type PreviewScreen = "planning" | "board" | "board-speaker" | "new-task" | "add-project" | "session" | "testing" | "done" | "settings";
type PreviewSettingsTab = "general" | "appearance" | "agents" | "safety" | "about";
type PreviewTheme = "dark" | "light";

function previewScreen(value: string | null): PreviewScreen {
  if (
    value === "board" ||
    value === "board-speaker" ||
    value === "new-task" ||
    value === "add-project" ||
    value === "session" ||
    value === "testing" ||
    value === "done" ||
    value === "settings"
  ) {
    return value;
  }

  return "planning";
}

function previewStateForScreen(screen: PreviewScreen): AppState {
  const selectedTaskId = {
    planning: previewTask.id,
    board: previewTask.id,
    "board-speaker": speakerTasks[0].id,
    "new-task": previewTask.id,
    "add-project": previewTask.id,
    session: implementationTask.id,
    testing: testingTask.id,
    done: doneTask.id,
    settings: previewTask.id,
  }[screen];
  const currentView: AppView =
    screen === "board" || screen === "board-speaker" || screen === "new-task" || screen === "add-project"
      ? "board"
      : screen === "settings"
        ? "settings"
        : "task-detail";

  return {
    ...previewState,
    app: {
      ...previewState.app,
      currentView,
      activeProjectId: screen === "board-speaker" ? speakerProject.id : previewState.app.activeProjectId,
      selectedTaskId,
      selectedTodoId: screen === "session" ? "todo-worker-filter" : null,
    },
    projects: {
      ...previewState.projects,
      current: screen === "board-speaker" ? speakerProject : previewState.projects.current,
    },
    tasks: screen === "board-speaker" ? speakerTasks : previewState.tasks,
  };
}

function previewSettingsTab(value: string | null): PreviewSettingsTab {
  if (
    value === "appearance" ||
    value === "agents" ||
    value === "safety" ||
    value === "about"
  ) {
    return value;
  }

  return "general";
}

function previewTheme(value: string | null): PreviewTheme {
  return value === "light" ? "light" : "dark";
}

function PreviewRoute({ screen, settingsTab }: { screen: PreviewScreen; settingsTab: PreviewSettingsTab }) {
  const { state, dispatch } = useAppState();

  if (state.app.currentView === "settings") {
    return (
      <SettingsPage
        initialTab={settingsTab}
        onBack={() => dispatch({ type: "app/viewSelected", view: "board" })}
      />
    );
  }

  const content =
    screen === "board" || screen === "board-speaker" || screen === "new-task" || screen === "add-project"
      ? <Board />
      : screen === "planning"
        ? <PlanningChat />
        : <WorkspaceSplit />;

  return (
    <AppLayout sidebar={<Sidebar />} header={<Header />}>
      {content}
      <NewTaskModalHost />
      {screen === "new-task" && previewState.projects.current && (
        <NewTaskModal project={previewState.projects.current} onClose={() => undefined} />
      )}
      {screen === "add-project" && <AddProjectModal onClose={() => undefined} />}
    </AppLayout>
  );
}

export function PlanningPreviewApp() {
  const params = new URLSearchParams(window.location.search);
  const screen = previewScreen(params.get("screen"));
  const settingsTab = previewSettingsTab(params.get("tab"));
  const theme = previewTheme(params.get("theme"));

  return (
    <ThemeProvider forcedTheme={theme}>
      <AppStateProvider initialStateOverride={previewStateForScreen(screen)}>
        <PreviewRoute screen={screen} settingsTab={settingsTab} />
      </AppStateProvider>
    </ThemeProvider>
  );
}
