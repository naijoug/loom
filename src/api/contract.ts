import type { SkillSource, SkillsHttpRequest, TranslationProviders, TranslationResult, TranslationSummary, MigrationPreview } from "./skills";
import type { InstructionCheck, ResourcePreview } from "./skills";
import type { SkillsLibrary, SkillsOperation, InstallTarget, Installation, InstallPlan, InstallReceipt } from "./skills";
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
  skillsListLibrary: "skills_list_library",
  skillsGetDetail: "skills_get_detail",
  skillsAddLocalSource: "skills_add_local_source",
  skillsRemoveSource: "skills_remove_source",
  skillsImportRepository: "skills_import_repository",
  skillsRefreshSource: "skills_refresh_source",
  skillsListOperations: "skills_list_operations",
  skillsCancelOperation: "skills_cancel_operation",
  skillsListInstallTargets: "skills_list_install_targets",
  skillsGetInstallStatus: "skills_get_install_status",
  skillsListInstallations: "skills_list_installations",
  skillsPlanInstall: "skills_plan_install",
  skillsPlanUninstall: "skills_plan_uninstall",
  skillsApplyInstall: "skills_apply_install",
  skillsListPending: "skills_list_pending",
  skillsRecoverInstallation: "skills_recover_installation",
  skillsListTranslationProviders: "skills_list_translation_providers",
  skillsSaveTranslationConfig: "skills_save_translation_config",
  skillsPlanMigration: "skills_plan_migration",
  skillsTranslate: "skills_translate",
  skillsTranslationHistory: "skills_translation_history",
  skillsTranslationResult: "skills_translation_result",
} as const;
export const TAURI_EVENTS = {
  updateState: "loom-update-state",
  chatEvent: "loom://chat-event",
  chatError: "loom://chat-error",
  skillsOperation: "loom://skills-operation",
} as const;
export const STORE_SCHEMA_VERSIONS = {"agents": 1, "settings": 1, "chat": 2} as const;
export type TauriCommand = (typeof TAURI_COMMANDS)[keyof typeof TAURI_COMMANDS];
export type TauriEvent = (typeof TAURI_EVENTS)[keyof typeof TAURI_EVENTS];
export const MODEL_CONTRACT_SAMPLES = {
  appSettings: {
  "themeMode": "system",
  "confirmBeforeCommands": true,
  "commandTimeoutSeconds": 600,
  "experimentalInlineCards": false,
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
    "inlineCards": false,
    "modes": [
      {
        "mode": "explore",
        "policy": "read-only"
      }
    ]
  }
} satisfies AgentDiagnostic,
  skillsLibrary: {
  "sources": [
    {
      "id": "src-fixture",
      "name": "Fixture skills",
      "kind": "local",
      "localPath": "/fixture/skills",
      "git": null,
      "revision": null,
      "updatedAtMs": 1
    }
  ],
  "skills": [
    {
      "id": "src-fixture::manual%2Fdemo%2FSKILL.md",
      "sourceId": "src-fixture",
      "relativePath": "manual/demo/SKILL.md",
      "name": "demo",
      "title": "Demo skill",
      "description": "A synthetic contract fixture.",
      "category": "manual",
      "keywords": "fixture"
    }
  ],
  "issues": []
} satisfies SkillsLibrary,
  skillsOperation: {
  "operationId": "skills-fixture",
  "sourceId": "src-fixture",
  "action": "refresh",
  "phase": "committed",
  "stage": "complete",
  "sequence": 2,
  "startedAtMs": 1,
  "updatedAtMs": 2,
  "error": null,
  "executions": []
} satisfies SkillsOperation,
  skillsInstallTarget: {
  "id": "codex-project",
  "toolId": "codex",
  "label": "Codex / ChatGPT",
  "scope": "project",
  "skillsDir": "/fixture/project/.codex/skills",
  "exists": false,
  "blockedReason": null
} satisfies InstallTarget,
  skillsInstallation: {
  "installationId": "installed-f29f41cc641f79c6ff7319ece84f20b9",
  "skillId": "src-fixture::manual%2Fdemo%2FSKILL.md",
  "sourceId": "src-fixture",
  "relativePath": "manual/demo/SKILL.md",
  "installName": "fixture-demo",
  "sourceDir": "/fixture/source/manual/demo",
  "mode": "copy",
  "revision": null,
  "sourceDigest": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "installedState": {
    "kind": "directory",
    "digest": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
  },
  "installedAtMs": 1,
  "updatedAtMs": 2,
  "managedResources": []
} satisfies Installation,
  skillsInstallPlan: {
  "schemaVersion": 1,
  "planId": "install-fixture",
  "action": "install",
  "projectPath": "/fixture/project",
  "mode": "copy",
  "createdAtMs": 1,
  "source": {
    "skillId": "src-fixture::manual%2Fdemo%2FSKILL.md",
    "sourceId": "src-fixture",
    "relativePath": "manual/demo/SKILL.md",
    "name": "fixture-demo",
    "directory": "/fixture/source/manual/demo",
    "revision": null,
    "digest": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
  },
  "items": [
    {
      "target": {
        "id": "codex-project",
        "toolId": "codex",
        "label": "Codex / ChatGPT",
        "scope": "project",
        "skillsDir": "/fixture/project/.codex/skills",
        "exists": false,
        "blockedReason": null
      },
      "installName": "fixture-demo",
      "destination": "/fixture/project/.codex/skills/fixture-demo",
      "before": {
        "kind": "missing"
      },
      "manifestBefore": {
        "kind": "missing"
      },
      "previous": null,
      "decision": "install",
      "message": null
    }
  ]
} satisfies InstallPlan,
  skillsInstallReceipt: {
  "planId": "install-fixture",
  "operationId": "skills-fixture",
  "items": [
    {
      "targetId": "codex-project",
      "destination": "/fixture/project/.codex/skills/fixture-demo",
      "status": "installed",
      "message": null
    }
  ]
} satisfies InstallReceipt,
skillsInstructionCheck: {
  "path": "/fixture/project/AGENTS.md",
  "state": "current",
  "message": null
} satisfies InstructionCheck,
skillsResourcePreview: {
  "path": "/fixture/project/AGENTS.md",
  "before": {
    "kind": "file",
    "digest": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "mode": 420
  }
} satisfies ResourcePreview,
skillsCoordinatedInstallPlan: {
  "schemaVersion": 1,
  "planId": "install-group-fixture",
  "action": "install",
  "projectPath": "/fixture/project",
  "mode": "copy",
  "createdAtMs": 1,
  "source": {
    "skillId": "src-fixture::manual%2Fdemo%2FSKILL.md",
    "sourceId": "src-fixture",
    "relativePath": "manual/demo/SKILL.md",
    "name": "fixture-demo",
    "directory": "/fixture/source/manual/demo",
    "revision": null,
    "digest": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
  },
  "items": [
    {
      "target": {
        "id": "codex-project",
        "toolId": "codex",
        "label": "Codex / ChatGPT",
        "scope": "project",
        "skillsDir": "/fixture/project/.codex/skills",
        "exists": false,
        "blockedReason": null
      },
      "installName": "fixture-demo",
      "destination": "/fixture/project/.codex/skills/fixture-demo",
      "before": {
        "kind": "missing"
      },
      "manifestBefore": {
        "kind": "missing"
      },
      "previous": null,
      "decision": "install",
      "message": null
    },
    {
      "target": {
        "id": "amp-project",
        "toolId": "amp",
        "label": "Amp / Codex",
        "scope": "project",
        "skillsDir": "/fixture/project/.agents/skills",
        "exists": false,
        "blockedReason": null
      },
      "installName": "fixture-demo",
      "destination": "/fixture/project/.agents/skills/fixture-demo",
      "before": {
        "kind": "missing"
      },
      "manifestBefore": {
        "kind": "missing"
      },
      "previous": null,
      "decision": "install",
      "message": null
    }
  ],
  "coordinated": true
} satisfies InstallPlan,
skillsGithubApiSource: {
  "id": "src-6394f5f031b445239b1374a0469f29d9",
  "name": "fixture/skills",
  "kind": "github_api",
  "localPath": null,
  "git": {
    "sourceId": "src-f1da8c300522c1ed4540ca8381f35a9d",
    "provider": "github",
    "namespace": "fixture/skills",
    "url": "https://github.com/fixture/skills",
    "cloneUrl": "https://github.com/fixture/skills.git",
    "legacyId": "github:fixture/skills"
  },
  "revision": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "updatedAtMs": 1,
  "defaultBranch": "trunk"
} satisfies SkillSource,
skillsMigrationPreview: {
  "schemaVersion": 1,
  "previewId": "preview-fixture",
  "readOnly": true,
  "canApply": false,
  "dataDirectories": [
    "/fixture/legacy"
  ],
  "catalogDirectory": null,
  "sources": [],
  "installations": [],
  "configs": [],
  "files": [
    {
      "path": "/fixture/legacy/library.json",
      "state": "missing",
      "digest": null
    }
  ],
  "issues": [],
  "complete": true
} satisfies MigrationPreview,
skillsTranslationResult: {
  "context": {
    "schemaVersion": 1,
    "operationId": "skills-translation-fixture",
    "skillId": "src-fixture::SKILL.md",
    "sourceId": "src-fixture",
    "sourceDigest": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "inputDigest": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    "providerId": "openai",
    "model": "gpt-5",
    "targetLanguage": "zh-CN",
    "sourceMode": "markdown",
    "createdAtMs": 1
  },
  "markdown": "# 翻译"
} satisfies TranslationResult,
skillsTranslationSummary: {
  "context": {
    "schemaVersion": 1,
    "operationId": "skills-translation-fixture",
    "skillId": "src-fixture::SKILL.md",
    "sourceId": "src-fixture",
    "sourceDigest": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "inputDigest": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    "providerId": "openai",
    "model": "gpt-5",
    "targetLanguage": "zh-CN",
    "sourceMode": "markdown",
    "createdAtMs": 1
  },
  "operation": {
    "operationId": "skills-translation-fixture",
    "sourceId": "src-fixture",
    "action": "translate",
    "phase": "committed",
    "stage": "complete",
    "sequence": 2,
    "startedAtMs": 1,
    "updatedAtMs": 2,
    "error": null,
    "executions": []
  }
} satisfies TranslationSummary,
skillsTranslationProviders: {
  revision: 0, providers: [{ id: "openai", label: "OpenAI", configured: false, supportsApiKey: true, storedKey: false, keySource: "none", model: "gpt-5", storedModel: null, modelFromEnvironment: false }],
} satisfies TranslationProviders,
skillsHttpRequest: {
  "method": "GET",
  "url": "https://api.github.com/repos/fixture/skills",
  "startedAtMs": 1,
  "finishedAtMs": 2,
  "status": 200,
  "error": null
} satisfies SkillsHttpRequest,
} as const;
