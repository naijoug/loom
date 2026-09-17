import type { AgentAdapterType } from "../../domain/agent";
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

/** M0/M2 mapping table — keep Chat UI aligned when switching agents. */
export function permissionCliHint(
  adapterType: AgentAdapterType | string | undefined,
  mode: ChatPermissionMode,
): string {
  const write = normalizeChatPermissionMode(mode) === "auto";
  switch (adapterType) {
    case "grok_cli":
      return write ? "CLI · --permission-mode acceptEdits" : "CLI · --permission-mode plan";
    case "codex_cli":
      return write ? "CLI · --sandbox workspace-write" : "CLI · --sandbox read-only";
    case "claude_code_cli":
      return write ? "CLI · --permission-mode acceptEdits" : "CLI · 默认只读（不传 permission-mode）";
    default:
      return write ? "CLI · 可写 stage" : "CLI · 只读 stage";
  }
}

export function diagnosticStatusLabel(status: string | undefined): string {
  switch (status) {
    case "ready":
      return "就绪";
    case "disabled":
      return "已停用";
    case "missing":
      return "缺失";
    default:
      return "未知";
  }
}
