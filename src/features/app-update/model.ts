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
export function updateLabel(state: UpdateSnapshot): string {
  if (state.phase === "downloading") { const percent = downloadPercent(state); return percent === undefined ? "正在下载更新…" : `正在下载更新 ${percent}%`; }
  return ({ disabled: "检查更新", idle: "检查更新", checking: "正在检查更新…", current: "已是最新版本", available: `发现新版本 ${state.version ?? ""}`, ready: "安装并重启", installing: "正在安装更新…", restarting: "正在重启…" } as Partial<Record<UpdatePhase,string>>)[state.phase] ?? "检查更新";
}
