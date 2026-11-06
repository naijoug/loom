import { TAURI_COMMANDS, TAURI_EVENTS } from "./contract";
import { invokeCommand, listenToEvent } from "./tauriClient";

export type ResearchTarget = { kind: "app"; path: string } | { kind: "github"; url: string; gitRef: string | null } | { kind: "website"; url: string; browser?: string | null };
export interface TargetBudget { maxRounds: number; maxCalls: number; maxCollections: number; maxActiveMs: number; maxDiskBytes: number }
export interface TargetInput {
  target: ResearchTarget; question: string; agentId: string; model: string | null;
  language: "en" | "zh-CN"; mode: "single" | "iterative"; budget: TargetBudget | null;
  tools: { node: string; entry: string } | null;
}
export interface TargetPreflight {
  preflightId: string; researchId: string | null; input: TargetInput; budget: TargetBudget;
  digest: string; expiresAtMs: number; operations: string[]; limitations: string[];
}
export interface TargetRunSpec {
  schemaVersion: number; researchId: string; runId: string; kind: "initial" | "refresh" | "evidence_followup";
  input: TargetInput; budget: TargetBudget; configDigest: string; parentReportId: string | null;
  agentConfigDigest: string; targetIdentity: string;
  sourceSnapshots: string[]; sourceEvidence: string[]; createdAtMs: number;
}
export type TargetStatus = "queued" | "preflight" | "collecting" | "analyzing" | "reporting" | "cancelling" | "completed" | "cancelled" | "failed" | "interrupted" | "awaiting_user";
export interface TargetRunState {
  runId: string; status: TargetStatus; attempt: number; sequence: number;
  usage: { calls: number; collections: number; rounds: number; activeMs: number; diskBytes: number; diskReservedBytes: number; reservationMs: number };
  snapshotIds: string[]; evidenceIds: string[]; reportIds: string[]; stopReason: string | null; error: string | null; updatedAtMs: number;
  exchange: { calls: number; reads: number; corrections: number };
  materialContext?: { allocationKey: string; sha256: string };
}
export interface TargetRun { spec: TargetRunSpec; state: TargetRunState }
export interface TargetEvent {
  researchId: string; runId: string; stepId: string | null; status: TargetStatus; phase: TargetStatus;
  progress: TargetRunState["usage"]; stopReason: string | null; error: string | null;
}
export interface TargetReport {
  reportId: string; runId: string; revision: number; snapshotIds: string[]; evidenceIds: string[];
  defaultSourceSnapshotId: string | null; markdownSha256: string; createdAtMs: number;
}
export interface TargetReportContent { report: TargetReport; markdown: string }
export interface EvidenceRecord {
  evidenceId: string; runId: string; sourceId: string | null; tool: string; toolVersion: string;
  rawSha256: string; modelSha256: string; bytes: number; coverage: string; limitations: string[]; createdAtMs: number;
}
export interface Material { text: string; nextCursor: number | null; truncated: boolean }
export interface TargetExport { directory: string; files: string[]; includesRaw: boolean; warning: string | null }
export interface TargetCapabilities { singleShot: boolean; iteration: boolean; experiments: boolean }

/** Lowercase request id accepted by `valid_id`. A new id is required for every start, follow-up, retry, and resume. */
export function researchRequestId() {
  const bytes = new Uint8Array(8);
  globalThis.crypto.getRandomValues(bytes);
  return `req-${[...bytes].map(byte => byte.toString(16).padStart(2, "0")).join("")}`;
}

export const targetResearchApi = {
  capabilities: () => invokeCommand<TargetCapabilities>(TAURI_COMMANDS.researchTargetCapabilities),
  subscribe: (onEvent: (event: TargetEvent) => void) => listenToEvent<TargetEvent>(TAURI_EVENTS.researchEvent, event => { if (event.runId) onEvent(event); }),
  export: (researchId: string, reportId: string, evidenceIds: string[] = [], includeRaw = false) => invokeCommand<TargetExport>(TAURI_COMMANDS.researchExport, { researchId, input: { reportId, evidenceIds, includeRaw } }),
  preflight: (spec: TargetInput, researchId?: string) => invokeCommand<TargetPreflight>(TAURI_COMMANDS.researchPreflight, { spec, researchId }),
  start: (preflightId: string, requestId: string) => invokeCommand<TargetRun>(TAURI_COMMANDS.researchStartRun, { preflightId, requestId }),
  getRun: (researchId: string, runId: string) => invokeCommand<TargetRun>(TAURI_COMMANDS.researchGetRun, { researchId, runId }),
  listRuns: (researchId: string) => invokeCommand<TargetRun[]>(TAURI_COMMANDS.researchListRuns, { researchId }),
  cancel: (researchId: string, runId: string) => invokeCommand<boolean>(TAURI_COMMANDS.researchCancelRun, { researchId, runId }),
  ask: (researchId: string, parentReportId: string, question: string, requestId: string) => invokeCommand<TargetRun>(TAURI_COMMANDS.researchAskRun, { researchId, parentReportId, question, requestId }),
  retry: (researchId: string, runId: string, requestId: string) => invokeCommand<TargetRun>(TAURI_COMMANDS.researchRetryRun, { researchId, runId, requestId }),
  resume: (researchId: string, runId: string, requestId: string) => invokeCommand<TargetRun>(TAURI_COMMANDS.researchResumeRun, { researchId, runId, requestId }),
  report: (researchId: string, reportId: string) => invokeCommand<TargetReportContent>(TAURI_COMMANDS.researchReadReport, { researchId, reportId }),
  tree: (researchId: string, reportId: string, offset = 0, limit = 100) => invokeCommand<Material>(TAURI_COMMANDS.researchListReportTree, { researchId, reportId, offset, limit }),
  source: (researchId: string, reportId: string, path: string, startLine: number, endLine: number) => invokeCommand<Material>(TAURI_COMMANDS.researchReadReportSource, { researchId, reportId, path, startLine, endLine }),
  evidence: (researchId: string, reportId: string) => invokeCommand<EvidenceRecord[]>(TAURI_COMMANDS.researchListEvidence, { researchId, reportId }),
  readEvidence: (researchId: string, reportId: string, evidenceId: string, offset = 0, limit = 12000) => invokeCommand<Material>(TAURI_COMMANDS.researchReadEvidence, { researchId, reportId, evidenceId, offset, limit }),
};
