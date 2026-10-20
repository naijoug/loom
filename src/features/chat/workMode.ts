import type { ChatPermissionMode } from "../../domain";
export type WorkMode = "research" | "code";
export function toWorkMode(permission: unknown): WorkMode {
  return permission === "ask" || permission === "auto" || permission === "read_write" ? "code" : "research";
}
export function toPermissionMode(mode: unknown, auto = false): ChatPermissionMode {
  return mode === "code" ? auto ? "auto" : "ask" : "explore";
}
