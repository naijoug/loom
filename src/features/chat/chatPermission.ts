import type { ChatPermissionMode } from "../../domain/chat";
import { normalizeChatPermissionMode } from "../../domain/chat";

/** Cycle order for Shift+Tab / permission control. */
export const CHAT_PERMISSION_CYCLE: readonly ChatPermissionMode[] = [
  "explore",
  "ask",
  "auto",
] as const;

const LABELS: Record<ChatPermissionMode, string> = {
  explore: "探索",
  ask: "询问编辑",
  auto: "自动",
};

export function permissionModeLabel(mode: ChatPermissionMode): string {
  return LABELS[normalizeChatPermissionMode(mode)];
}

/** explore → ask → auto → explore */
export function cyclePermissionMode(mode: ChatPermissionMode): ChatPermissionMode {
  const current = normalizeChatPermissionMode(mode);
  const index = CHAT_PERMISSION_CYCLE.indexOf(current);
  const next = CHAT_PERMISSION_CYCLE[(index + 1) % CHAT_PERMISSION_CYCLE.length];
  return next ?? "explore";
}
