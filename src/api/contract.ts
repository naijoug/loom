export const TAURI_COMMANDS = {
  healthCheck: "health_check",
  createAgent: "create_agent",
  deleteAgent: "delete_agent",
  diagnoseAgents: "diagnose_agents",
  listAgents: "list_agents",
  chatListSessions: "chat_list_sessions",
  chatCreate: "chat_create",
  chatGet: "chat_get",
  chatReadEvents: "chat_read_events",
  chatReadRunLogs: "chat_read_run_logs",
  chatExport: "chat_export",
  chatSetAgent: "chat_set_agent",
  chatUpdateMeta: "chat_update_meta",
  chatClearResume: "chat_clear_resume",
  chatPromoteToTask: "chat_promote_to_task",
  chatSend: "chat_send",
  chatAbort: "chat_abort",
  prepareAgentInvocation: "prepare_agent_invocation",
  retryPlanningAgent: "retry_planning_agent",
  runPlanningDiscussion: "run_planning_discussion",
  runPlanReviews: "run_plan_reviews",
  setAgentEnabled: "set_agent_enabled",
  updateAgent: "update_agent",
  commandRunnerReady: "command_runner_ready",
  readCommandRunLogs: "read_command_run_logs",
  assessExecution: "assess_execution",
  approveExecution: "approve_execution",
  decideImplementationReviewFinding: "decide_implementation_review_finding",
  runImplementationReviews: "run_implementation_reviews",
  exportDiagnosticBundle: "export_diagnostic_bundle",
  loadAppSettings: "load_app_settings",
  saveAppSettings: "save_app_settings",
  startCommandRun: "start_command_run",
  stopCommandRun: "stop_command_run",
  startPtyRun: "start_pty_run",
  stopPtyRun: "stop_pty_run",
  writePty: "write_pty",
  resizePty: "resize_pty",
  listTerminalSlots: "list_terminal_slots",
  saveTerminalSlots: "save_terminal_slots",
  suggestTerminalSlots: "suggest_terminal_slots",
  openPlanHtml: "open_plan_html",
  openPlanningEvidence: "open_planning_evidence",
  openPlanViewer: "open_plan_viewer",
  readPlanHtml: "read_plan_html",
  loadProjectAgentPreferences: "load_project_agent_preferences",
  saveProjectAgentPreferences: "save_project_agent_preferences",
  listRecentProjects: "list_recent_projects",
  removeRecentProject: "remove_recent_project",
  registerProject: "register_project",
  appendFeedback: "append_feedback",
  blockTask: "block_task",
  buildImplementationContext: "build_implementation_context",
  completeTask: "complete_task",
  completeTodo: "complete_todo",
  confirmPlan: "confirm_plan",
  createTask: "create_task",
  deleteTask: "delete_task",
  generateRepairContext: "generate_repair_context",
  listTasks: "list_tasks",
  markReadyForTesting: "mark_ready_for_testing",
  pauseTask: "pause_task",
  recordPlanningDecision: "record_planning_decision",
  resumeTask: "resume_task",
  cancelTask: "cancel_task",
  startTodo: "start_todo",
  switchPrimaryAgent: "switch_primary_agent",
  regenerateTaskSummary: "regenerate_task_summary",
  exportTaskSummary: "export_task_summary",
} as const;

export const TAURI_EVENTS = {
  commandFinished: "loom://command-finished",
  commandLog: "loom://command-log",
  planningAgentLog: "loom://planning-agent-log",
  planningAgentStatus: "loom://planning-agent-status",
  ptyOutput: "loom://pty-output",
  chatEvent: "loom://chat-event",
  chatError: "loom://chat-error",
} as const;

export const TASK_STATUSES = [
  "drafting_requirements",
  "planning",
  "plan_review",
  "ready_to_implement",
  "implementing",
  "reviewing",
  "debugging",
  "fixing",
  "verifying",
  "completed",
  "blocked",
  "cancelled",
] as const;

export const COMMAND_RUN_STATUSES = [
  "running",
  "succeeded",
  "failed",
  "cancelled",
  "interrupted",
] as const;

export const PLAN_TODO_STATUSES = ["pending", "implementing", "done", "blocked"] as const;

export const STORE_SCHEMA_VERSIONS = {
  task: 1,
  agents: 1,
  settings: 1,
  terminalSlots: 1,
  projectAgentPreferences: 1,
  chat: 2,
} as const;

export type TauriCommand = (typeof TAURI_COMMANDS)[keyof typeof TAURI_COMMANDS];
export type TauriEvent = (typeof TAURI_EVENTS)[keyof typeof TAURI_EVENTS];
export type ContractTaskStatus = (typeof TASK_STATUSES)[number];
export type ContractCommandRunStatus = (typeof COMMAND_RUN_STATUSES)[number];
export type ContractPlanTodoStatus = (typeof PLAN_TODO_STATUSES)[number];

type JsonWire<T> = T extends readonly (infer Item)[]
  ? JsonWire<Item>[]
  : T extends object
    ? { [Key in keyof T]-?: undefined extends T[Key]
        ? JsonWire<Exclude<T[Key], undefined>> | null
        : JsonWire<T[Key]> }
    : T;

export const MODEL_CONTRACT_SAMPLES = {
  appSettings: {
    themeMode: "system",
    confirmBeforeCommands: true,
    commandTimeoutSeconds: 600,
  } satisfies JsonWire<AppSettings>,
  agentConfig: {
    id: "agent-contract",
    name: "Contract Agent",
    command: "agent",
    args: ["run"],
    workingDirectoryPolicy: "project_root",
    capabilities: ["planning"],
    adapterType: "cli",
    canWriteFiles: false,
    canRunCommands: false,
    enabled: true,
    available: true,
  } satisfies JsonWire<AgentConfig>,
  terminalSlot: {
    id: "validation",
    name: "Validation",
    command: "pnpm test",
    kind: "validation",
    cwd: null,
  } satisfies JsonWire<TerminalSlot>,
  projectAgentPreferences: {
    planningAgentIds: ["agent-contract"],
    implementationAgentId: null,
    reviewAgentIds: [],
    debuggingAgentId: null,
    testingAgentId: null,
    documentationAgentId: null,
    updatedAtMs: 1,
  } satisfies JsonWire<ProjectAgentPreferences>,
  commandRun: {
    id: "run-contract",
    taskId: "task-contract",
    command: "pnpm test",
    cwd: "/tmp/project",
    intent: "validation",
    loopId: null,
    iteration: null,
    attempt: null,
    terminationReason: null,
    sessionId: null,
    resumeCommand: null,
    startedAtMs: 1,
    endedAtMs: null,
    status: "running",
    exitCode: null,
    stdoutLogRef: null,
    stderrLogRef: null,
    errorSummary: null,
  } satisfies JsonWire<CommandRun>,
  task: {
    id: "task-contract",
    projectPath: "/tmp/project",
    title: "Contract task",
    rawRequirement: "Keep the wire model stable",
    status: "planning",
    lifecycle: {
      paused: false,
      resumeStatus: null,
      pauseReason: null,
      blockedReason: null,
      cancelledReason: null,
    },
    selectedPlanningAgentIds: ["agent-contract"],
    primaryAgentId: null,
    reviewAgentIds: [],
    implementationReviewRuns: [],
    implementationReviews: [],
    implementationReviewDecisions: [],
    finalPlan: null,
    finalPlanPath: null,
    finalPlanHtmlPath: null,
    discussionSummary: null,
    planningRuns: [],
    agentInvocations: [],
    planReviews: [],
    planningDecisions: [],
    planTodos: [],
    loopTrace: [],
    events: [],
    commandRuns: [],
    feedback: [],
    loopCompactSummary: null,
    repairContextPreview: null,
    gitBaseline: null,
    summary: null,
    createdAtMs: 1,
    updatedAtMs: 1,
  } satisfies JsonWire<Task>,
  chatSession: {
  "id": "chat-contract",
  "projectPath": "/project",
  "agentId": "agent-contract",
  "title": "Chat contract",
  "permissionMode": "explore",
  "messages": [
    {
      "id": "user-contract",
      "role": "user",
      "content": "hello",
      "status": "complete",
      "createdAtMs": 1
    },
    {
      "id": "assistant-contract",
      "role": "assistant",
      "content": "world",
      "status": "complete",
      "createdAtMs": 1
    }
  ],
  "createdAtMs": 1,
  "updatedAtMs": 1,
  "turnStatus": "idle",
  "status": "active",
  "flagged": false,
  "schemaVersion": 2,
  "lastSeq": 1,
  "revision": 1,
  "sendReceipts": [
    {
      "clientRequestId": "request-contract",
      "turnId": "turn-contract",
      "userMessageId": "user-contract",
      "assistantMessageId": "assistant-contract",
      "requestedPermissionMode": "explore"
    }
  ],
  "resumeHandle": {
    "version": 1,
    "adapterType": "grok_cli",
    "nativeSessionId": "native-contract",
    "configFingerprint": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
  },
  "turns": [
    {
      "id": "turn-contract",
      "clientRequestId": "request-contract",
      "userMessageId": "user-contract",
      "assistantMessageId": "assistant-contract",
      "invocation": {
        "agentId": "agent-contract",
        "adapterType": "grok_cli",
        "program": "grok",
        "args": [
          "-p",
          "[PROMPT]"
        ],
        "cwd": "/project",
        "permissionMode": "explore",
        "stdinPrompt": false,
        "outputMode": "streaming_json",
        "configFingerprint": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
      },
      "status": "completed",
      "acceptedAtMs": 1,
      "startedAtMs": 2,
      "finishedAtMs": 3,
      "processId": 123,
      "exitCode": 0,
      "terminationReason": null,
      "errorSummary": null,
      "stdoutLogRef": "runs/turn-contract/stdout.log",
      "stderrLogRef": "runs/turn-contract/stderr.log"
    }
  ]
} satisfies ChatSession,

  chatEvent: {
  "schemaVersion": 2,
  "projectKey": "/project",
  "sessionId": "chat-contract",
  "seq": 3,
  "timestampMs": 1,
  "kind": "stream",
  "payload": {
    "sessionId": "chat-contract",
    "turnId": "turn-contract",
    "messageId": "message-contract",
    "delta": "hello",
    "done": false
  }
} satisfies ChatEvent,

  chatSendInput: {
  "projectPath": "/project",
  "sessionId": "chat-contract",
  "clientRequestId": "request-contract",
  "text": "hello",
  "permissionMode": "explore"
} satisfies ChatSendInput,

  chatSendReceipt: {
  "clientRequestId": "request-contract",
  "turnId": "turn-contract",
  "userMessageId": "user-contract",
  "assistantMessageId": "assistant-contract",
  "requestedPermissionMode": "explore"
} satisfies ChatSendReceipt,

  chatResumeHandle: {
  "version": 1,
  "adapterType": "grok_cli",
  "nativeSessionId": "native-contract",
  "configFingerprint": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
} satisfies ChatResumeHandle,

  chatTurn: {
  "id": "turn-contract",
  "clientRequestId": "request-contract",
  "userMessageId": "user-contract",
  "assistantMessageId": "assistant-contract",
  "invocation": {
    "agentId": "agent-contract",
    "adapterType": "grok_cli",
    "program": "grok",
    "args": [
      "-p",
      "[PROMPT]"
    ],
    "cwd": "/project",
    "permissionMode": "explore",
    "stdinPrompt": false,
    "outputMode": "streaming_json",
    "configFingerprint": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
  },
  "status": "completed",
  "acceptedAtMs": 1,
  "startedAtMs": 2,
  "finishedAtMs": 3,
  "processId": 123,
  "exitCode": 0,
  "terminationReason": null,
  "errorSummary": null,
  "stdoutLogRef": "runs/turn-contract/stdout.log",
  "stderrLogRef": "runs/turn-contract/stderr.log"
} satisfies ChatTurn,

  chatLogPage: {
  "text": "hello\n",
  "nextOffset": 6,
  "hasMore": false
} satisfies ChatLogPage,

  chatExportInput: {
  "projectPath": "/project",
  "sessionId": "chat-contract"
} satisfies ChatExportInput,

  chatExportResult: {
  "directory": "/project/.loom/chat/exports/export-contract",
  "jsonPath": "/project/.loom/chat/exports/export-contract/session.json",
  "markdownPath": "/project/.loom/chat/exports/export-contract/transcript.md",
  "snapshotSeq": 3,
  "inProgress": false,
  "warnings": []
} satisfies ChatExportResult,


} as const;
import type { AgentConfig, ProjectAgentPreferences } from "../domain/agent";
import type { AppSettings } from "../domain/app";
import type { CommandRun, TerminalSlot } from "../domain/command";
import type { Task } from "../domain/task";

import type { ChatSession, ChatEvent, ChatSendInput, ChatSendReceipt, ChatResumeHandle, ChatTurn, ChatLogPage, ChatExportInput, ChatExportResult } from "../domain/chat";
