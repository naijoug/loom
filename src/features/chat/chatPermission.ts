import type { AgentAdapterType } from "../../domain/agent";
import type { ChatPermissionMode } from "../../domain/chat";
import {
  chatPermissionAllowsWrite,
  normalizeChatPermissionMode,
} from "../../domain/chat";

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

/** Copy for the Ask per-turn write gate (P2-M1). */
export const ASK_TURN_CONFIRM_LABEL = "允许本回合写文件/跑可写工具";

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

/**
 * Phase 2 Ask gate: ask requires an explicit in-UI confirm before each send.
 * explore / auto do not.
 */
export function askTurnRequiresConfirm(mode: ChatPermissionMode | string): boolean {
  return normalizeChatPermissionMode(mode) === "ask";
}

/** M0/M2/P2 mapping — Chat UI CLI hint when switching agents. */
export function permissionCliHint(
  adapterType: AgentAdapterType | string | undefined,
  mode: ChatPermissionMode,
): string {
  const write = chatPermissionAllowsWrite(mode);
  const askGate = askTurnRequiresConfirm(mode);
  const gateSuffix = askGate ? " · 发送前确认" : "";
  switch (adapterType) {
    case "grok_cli":
      return (write ? "CLI · --permission-mode acceptEdits" : "CLI · --permission-mode plan") + gateSuffix;
    case "codex_cli":
      return (write ? "CLI · --sandbox workspace-write" : "CLI · --sandbox read-only") + gateSuffix;
    case "claude_code_cli":
      return (
        (write ? "CLI · --permission-mode acceptEdits" : "CLI · 默认只读（不传 permission-mode）") +
        gateSuffix
      );
    default:
      return (write ? "CLI · 可写 stage" : "CLI · 只读 stage") + gateSuffix;
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
