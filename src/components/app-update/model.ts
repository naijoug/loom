import { translate } from "../../i18n";
import type { MessageKey } from "../../i18n/en";
export type UpdatePhase = "disabled" | "idle" | "checking" | "current" | "available" | "downloading" | "ready" | "installing" | "restarting";
export interface UpdateSnapshot {
  revision: number;
  phase: UpdatePhase;
  currentVersion: string;
  version: string | null;
  notes: string | null;
  downloaded: number;
  total: number | null;
  error: string | null;
  checkedAt: number | null;
}
export type UpdateDrafts = Record<string, Record<string, string>>;
export const initialUpdate: UpdateSnapshot = { revision: 0, phase: "disabled", currentVersion: "", version: null, notes: null, downloaded: 0, total: null, error: null, checkedAt: null };
export function newerSnapshot(current: UpdateSnapshot, incoming: UpdateSnapshot): UpdateSnapshot {
  return incoming.revision >= current.revision ? incoming : current;
}
export const updateBusy = (phase: UpdatePhase) => ["checking", "downloading", "installing", "restarting"].includes(phase);
export const updateOffered = (phase: UpdatePhase) => ["available", "downloading", "ready", "installing", "restarting"].includes(phase);
export function downloadPercent(state: UpdateSnapshot): number | undefined {
  return state.total && state.total > 0 ? Math.min(100, Math.floor(state.downloaded / state.total * 100)) : undefined;
}
export function updateLabel(state: UpdateSnapshot, t: (key: MessageKey, values?: Record<string, string | number>) => string = (key, values) => translate("en", key, values)): string {
  if (state.phase === "downloading") { const percent = downloadPercent(state); return percent === undefined ? t("updateDownloading") : t("updatePercent", { percent }); }
  const keys: Record<UpdatePhase, MessageKey> = { disabled: "checkUpdates", idle: "checkUpdates", checking: "updateChecking", current: "updateCurrent", available: "updateAvailable", downloading: "updateDownloading", ready: "installRestart", installing: "updateInstalling", restarting: "updateRestarting" };
  return t(keys[state.phase], { version: state.version ?? "" });
}
