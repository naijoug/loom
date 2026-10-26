import { TAURI_COMMANDS, TAURI_EVENTS } from "./contract";
import { invokeCommand, listenToEvent } from "./tauriClient";

export interface SkillSource {
  id: string; name: string; kind: "local" | "git" | "github_api"; localPath: string | null;
  git: { sourceId: string; provider: string; namespace: string; url: string; cloneUrl: string; legacyId: string } | null;
  revision: string | null; updatedAtMs: number; defaultBranch?: string;
}
export interface SkillSummary {
  id: string; sourceId: string; relativePath: string; name: string;
  title: string; description: string; category: string; keywords: string;
}
export interface SkillDetail {
  summary: SkillSummary; content: string; contentDigest: string; manifest: Record<string, string>;
  relatedFiles: { relativePath: string; sizeBytes: number; content: string | null }[];
}
export interface SkillsLibrary {
  sources: SkillSource[]; skills: SkillSummary[];
  issues: { sourceId: string; relativePath: string; message: string }[];
}
export interface SkillsHttpRequest { method: string; url: string; startedAtMs: number; finishedAtMs: number | null; status: number | null; error: string | null; }
export interface SkillsOperation {
  operationId: string; sourceId: string; action: string;
  phase: "running" | "committed" | "failed" | "cancelled" | "interrupted";
  stage: string; sequence: number; startedAtMs: number; updatedAtMs: number;
  error: { code: string; message: string } | null;
  requests?: SkillsHttpRequest[];
  executions: { program: string; args: string[]; cwd: string; startedAtMs: number; finishedAtMs: number | null; exitCode: number | null; error: string | null }[];
}
export type InstallMode = "copy" | "symlink";
export type EntryState = { kind: "missing" } | { kind: "directory"; digest: string } | { kind: "file"; digest: string; mode: number } | { kind: "symlink"; target: string };
export interface InstallTarget { id: string; toolId: string; label: string; scope: "global" | "project"; skillsDir: string; exists: boolean; blockedReason: string | null; }
export interface Installation {
  installationId: string; skillId: string; sourceId: string; relativePath: string; installName: string;
  sourceDir: string; mode: InstallMode; revision: string | null; sourceDigest: string;
  installedState: EntryState; installedAtMs: number; updatedAtMs: number;
  managedResources: { path: string; marker: string; contentDigest: string }[];
}
export interface InstructionCheck { path: string; state: "current" | "missing" | "stale" | "malformed"; message: string | null; }
export interface ResourcePreview { path: string; before: EntryState; }
export interface InstallStatus { target: InstallTarget; skillId: string; destination: string; state: string; installation: Installation | null; updateAvailable: boolean; instructionChecks?: InstructionCheck[]; message: string | null; }
export interface InstallPlan {
  schemaVersion: number; planId: string; action: "install" | "uninstall"; coordinated?: boolean; projectPath: string | null;
  mode: InstallMode; createdAtMs: number;
  source: { skillId: string; sourceId: string; relativePath: string; name: string; directory: string; revision: string | null; digest: string } | null;
  items: { target: InstallTarget; installName: string; destination: string; before: EntryState; manifestBefore: EntryState; previous: Installation | null; resourceChanges?: ResourcePreview[]; withSlashCommands?: boolean; decision: string; message: string | null }[];
}
export interface InstallReceipt { planId: string; operationId: string; items: { targetId: string; destination: string; status: string; message: string | null; backupDirectory?: string }[]; }
export interface RecoveryItem { operationId: string; resources: string[]; canRecover: boolean; message: string | null; }
export interface InstallRequest { skillId: string; targetIds: string[]; mode: InstallMode; conflictPolicy: "fail" | "skip" | "overwrite"; projectPath: string | null; withSlashCommands?: boolean; }
export type TranslationProviderId = "openai" | "openrouter" | "codex" | "claude-code";
export interface TranslationProvider { id: TranslationProviderId; label: string; configured: boolean; supportsApiKey: boolean; storedKey: boolean; keySource: "environment" | "stored" | "none" | "local_cli"; model: string | null; storedModel: string | null; modelFromEnvironment: boolean; }
export interface TranslationProviders { revision: number; providers: TranslationProvider[]; }
export interface TranslationConfigInput { providerId: TranslationProviderId; expectedRevision: number; key: { action: "keep" | "clear" } | { action: "set"; value: string }; model: string | null; }
export type TranslationSourceMode = "markdown" | "summary";
export interface TranslationInput { skillId: string; expectedContentDigest: string; providerId: TranslationProviderId; targetLanguage: string; sourceMode: TranslationSourceMode; }
export interface TranslationContext { schemaVersion: number; operationId: string; skillId: string; sourceId: string; sourceDigest: string; inputDigest: string; providerId: TranslationProviderId; model: string | null; targetLanguage: string; sourceMode: TranslationSourceMode; createdAtMs: number; }
export interface TranslationSummary { context: TranslationContext; operation: SkillsOperation; }
export interface TranslationResult { context: TranslationContext; markdown: string; }
export interface SkillsApi {
  translate(input: TranslationInput): Promise<TranslationSummary>;
  translationHistory(skillId: string): Promise<TranslationSummary[]>;
  translationResult(operationId: string): Promise<TranslationResult>;
  translationProviders(): Promise<TranslationProviders>;
  saveTranslationConfig(input: TranslationConfigInput): Promise<TranslationProviders>;
  list(): Promise<SkillsLibrary>;
  detail(skillId: string): Promise<SkillDetail>;
  addLocal(path: string): Promise<SkillsLibrary>;
  remove(sourceId: string): Promise<SkillsLibrary>;
  importRepository(url: string, sourceMode?: "git" | "github-api"): Promise<SkillsOperation>;
  refresh(sourceId: string): Promise<SkillsOperation>;
  operations(): Promise<SkillsOperation[]>;
  cancel(operationId: string): Promise<void>;
  subscribe(handler: (operation: SkillsOperation) => void): Promise<() => void>;
  targets(projectPath: string | null): Promise<InstallTarget[]>;
  installStatus(skillId: string, projectPath: string | null): Promise<InstallStatus[]>;
  installations(projectPath: string | null): Promise<InstallStatus[]>;
  planInstall(input: InstallRequest): Promise<InstallPlan>;
  planUninstall(installationIds: string[], projectPath: string | null): Promise<InstallPlan>;
  applyInstall(planId: string, projectPath: string | null): Promise<InstallReceipt>;
  pending(projectPath: string | null): Promise<RecoveryItem[]>;
  recover(operationId: string, projectPath: string | null): Promise<string>;
}
export const skillsApi: SkillsApi = {
  translate: input => invokeCommand(TAURI_COMMANDS.skillsTranslate, { input }),
  translationHistory: skillId => invokeCommand(TAURI_COMMANDS.skillsTranslationHistory, { skillId }),
  translationResult: operationId => invokeCommand(TAURI_COMMANDS.skillsTranslationResult, { operationId }),
  translationProviders: () => invokeCommand(TAURI_COMMANDS.skillsListTranslationProviders),
  saveTranslationConfig: input => invokeCommand(TAURI_COMMANDS.skillsSaveTranslationConfig, { input }),
  list: () => invokeCommand(TAURI_COMMANDS.skillsListLibrary),
  detail: skillId => invokeCommand(TAURI_COMMANDS.skillsGetDetail, { skillId }),
  addLocal: path => invokeCommand(TAURI_COMMANDS.skillsAddLocalSource, { path }),
  remove: sourceId => invokeCommand(TAURI_COMMANDS.skillsRemoveSource, { sourceId }),
  importRepository: (url, sourceMode = "git") => invokeCommand(TAURI_COMMANDS.skillsImportRepository, { url, sourceMode }),
  refresh: sourceId => invokeCommand(TAURI_COMMANDS.skillsRefreshSource, { sourceId }),
  operations: () => invokeCommand(TAURI_COMMANDS.skillsListOperations),
  cancel: operationId => invokeCommand(TAURI_COMMANDS.skillsCancelOperation, { operationId }),
  subscribe: handler => listenToEvent(TAURI_EVENTS.skillsOperation, handler),
  targets: projectPath => invokeCommand(TAURI_COMMANDS.skillsListInstallTargets, { projectPath }),
  installStatus: (skillId, projectPath) => invokeCommand(TAURI_COMMANDS.skillsGetInstallStatus, { skillId, projectPath }),
  installations: projectPath => invokeCommand(TAURI_COMMANDS.skillsListInstallations, { projectPath }),
  planInstall: input => invokeCommand(TAURI_COMMANDS.skillsPlanInstall, { input }),
  planUninstall: (installationIds, projectPath) => invokeCommand(TAURI_COMMANDS.skillsPlanUninstall, { input: { installationIds, projectPath } }),
  applyInstall: (planId, projectPath) => invokeCommand(TAURI_COMMANDS.skillsApplyInstall, { planId, projectPath }),
  pending: projectPath => invokeCommand(TAURI_COMMANDS.skillsListPending, { projectPath }),
  recover: (operationId, projectPath) => invokeCommand(TAURI_COMMANDS.skillsRecoverInstallation, { operationId, projectPath }),
};
