import type { AgentConfig, AgentDiagnostic, AppSettings, ChatSession, ChatEvent, ChatSendInput, ChatSendReceipt, ChatResumeHandle, ChatTurn, ChatLogPage, ChatExportInput, ChatExportResult, ChatMessagePart } from "../domain";

export const TAURI_COMMANDS = {
  getUpdateState: "get_update_state",
  checkForUpdates: "check_for_updates",
  downloadUpdate: "download_update",
  installUpdate: "install_update",
  loadUpdateDrafts: "load_update_drafts",
  saveUpdateDrafts: "save_update_drafts",
  healthCheck: "health_check",
  createAgent: "create_agent",
  deleteAgent: "delete_agent",
  diagnoseAgents: "diagnose_agents",
  listAgents: "list_agents",
  chatListSessions: "chat_list_sessions",
  chatListProjectGroups: "chat_list_project_groups",
  chatCreate: "chat_create",
  chatGet: "chat_get",
  chatReadEvents: "chat_read_events",
  chatReadRunLogs: "chat_read_run_logs",
  chatExport: "chat_export",
  chatSetAgent: "chat_set_agent",
  chatUpdateMeta: "chat_update_meta",
  chatClearResume: "chat_clear_resume",
  chatSend: "chat_send",
  chatAbort: "chat_abort",
  setAgentEnabled: "set_agent_enabled",
  updateAgent: "update_agent",
  exportDiagnosticBundle: "export_diagnostic_bundle",
  loadAppSettings: "load_app_settings",
  saveAppSettings: "save_app_settings",
  listRecentProjects: "list_recent_projects",
  removeRecentProject: "remove_recent_project",
  registerProject: "register_project",
} as const;
export const TAURI_EVENTS = {
  updateState: "loom-update-state",
  chatEvent: "loom://chat-event",
  chatError: "loom://chat-error",
} as const;
export const STORE_SCHEMA_VERSIONS = {"agents": 1, "settings": 1, "chat": 2} as const;
export type TauriCommand = (typeof TAURI_COMMANDS)[keyof typeof TAURI_COMMANDS];
export type TauriEvent = (typeof TAURI_EVENTS)[keyof typeof TAURI_EVENTS];
export const MODEL_CONTRACT_SAMPLES = {
  appSettings: {
  "themeMode": "system",
  "confirmBeforeCommands": true,
  "commandTimeoutSeconds": 600,
  "locale": "en"
} satisfies AppSettings,
  agentConfig: {
  "id": "agent-contract",
  "name": "Contract Agent",
  "command": "agent",
  "args": [
    "run"
  ],
  "workingDirectoryPolicy": "project_root",
  "capabilities": [
    "planning"
  ],
  "adapterType": "cli",
  "canWriteFiles": false,
  "canRunCommands": false,
  "enabled": true,
  "available": true
} satisfies AgentConfig,
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
  chatToolPart: {
  "type": "tool",
  "name": "Edit",
  "callId": "call-contract",
  "kind": "edit",
  "target": "file.txt",
  "exitCode": 0,
  "diffStats": {
    "added": 1,
    "removed": 1
  },
  "diffPreview": "-old\n+new\n",
  "inputSummary": "file.txt",
  "outputSummary": "done",
  "status": "done"
} satisfies ChatMessagePart,
  agentDiagnostic: {
  "agentId": "agent-contract",
  "status": "ready",
  "resolvedPath": "/usr/bin/agent",
  "version": "1.0",
  "authStatus": "unknown",
  "detail": "Version probe succeeded.",
  "checkedAtMs": 1,
  "capabilities": {
    "structuredStream": true,
    "toolEvents": true,
    "nativeResume": true,
    "modes": [
      {
        "mode": "explore",
        "policy": "read-only"
      }
    ]
  }
} satisfies AgentDiagnostic,
} as const;
