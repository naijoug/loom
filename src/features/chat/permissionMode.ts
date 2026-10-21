import type { ChatPermissionMode } from "../../domain";
/** Mirrors the Rust normalizer: legacy `read_only` → `explore`, `read_write` → `ask`, unknown → `explore`. */
export function normalizePermissionMode(value: unknown): ChatPermissionMode {
  return value === "ask" || value === "auto" ? value : value === "read_write" ? "ask" : "explore";
}
export const PERMISSION_LABELS = { explore: "readOnly", ask: "ask", auto: "auto" } as const;
