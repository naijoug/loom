import { TAURI_COMMANDS, TAURI_EVENTS } from "./contract";
import { invokeCommand, listenToEvent } from "./tauriClient";

export interface SkillSource {
  id: string; name: string; kind: "local" | "git"; localPath: string | null;
  git: { sourceId: string; provider: string; namespace: string; url: string; cloneUrl: string; legacyId: string } | null;
  revision: string | null; updatedAtMs: number;
}
export interface SkillSummary {
  id: string; sourceId: string; relativePath: string; name: string;
  title: string; description: string; category: string; keywords: string;
}
export interface SkillDetail {
  summary: SkillSummary; content: string; manifest: Record<string, string>;
  relatedFiles: { relativePath: string; sizeBytes: number; content: string | null }[];
}
export interface SkillsLibrary {
  sources: SkillSource[]; skills: SkillSummary[];
  issues: { sourceId: string; relativePath: string; message: string }[];
}
export interface SkillsOperation {
  operationId: string; sourceId: string; action: string;
  phase: "running" | "committed" | "failed" | "cancelled" | "interrupted";
  stage: string; sequence: number; startedAtMs: number; updatedAtMs: number;
  error: { code: string; message: string } | null;
  executions: { program: string; args: string[]; cwd: string; startedAtMs: number; finishedAtMs: number | null; exitCode: number | null; error: string | null }[];
}
export interface SkillsApi {
  list(): Promise<SkillsLibrary>;
  detail(skillId: string): Promise<SkillDetail>;
  addLocal(path: string): Promise<SkillsLibrary>;
  remove(sourceId: string): Promise<SkillsLibrary>;
  importRepository(url: string): Promise<SkillsOperation>;
  refresh(sourceId: string): Promise<SkillsOperation>;
  operations(): Promise<SkillsOperation[]>;
  cancel(operationId: string): Promise<void>;
  subscribe(handler: (operation: SkillsOperation) => void): Promise<() => void>;
}
export const skillsApi: SkillsApi = {
  list: () => invokeCommand(TAURI_COMMANDS.skillsListLibrary),
  detail: skillId => invokeCommand(TAURI_COMMANDS.skillsGetDetail, { skillId }),
  addLocal: path => invokeCommand(TAURI_COMMANDS.skillsAddLocalSource, { path }),
  remove: sourceId => invokeCommand(TAURI_COMMANDS.skillsRemoveSource, { sourceId }),
  importRepository: url => invokeCommand(TAURI_COMMANDS.skillsImportRepository, { url }),
  refresh: sourceId => invokeCommand(TAURI_COMMANDS.skillsRefreshSource, { sourceId }),
  operations: () => invokeCommand(TAURI_COMMANDS.skillsListOperations),
  cancel: operationId => invokeCommand(TAURI_COMMANDS.skillsCancelOperation, { operationId }),
  subscribe: handler => listenToEvent(TAURI_EVENTS.skillsOperation, handler),
};
