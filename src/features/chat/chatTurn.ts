/** Mirrors `CHAT_TURN_TIMEOUT_MS` in `src-tauri/src/chat.rs` (10 minutes). */
export const CHAT_TURN_TIMEOUT_MS = 10 * 60 * 1000;

export function isChatTurnRunning(input: {
  sending: boolean;
  turnStatus?: string | null;
}): boolean {
  return input.sending || input.turnStatus === "streaming";
}

/** Format elapsed ms as `m:ss` (or `h:mm:ss` past one hour). */
export function formatTurnElapsed(elapsedMs: number): string {
  const totalSeconds = Math.max(0, Math.floor(elapsedMs / 1000));
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  const mm = hours > 0 ? String(minutes).padStart(2, "0") : String(minutes);
  const ss = String(seconds).padStart(2, "0");
  if (hours > 0) {
    return `${hours}:${mm}:${ss}`;
  }
  return `${mm}:${ss}`;
}

export function turnRunningHint(elapsedMs: number | null | undefined): string {
  if (elapsedMs == null || elapsedMs < 0) {
    return "生成中…";
  }
  return `生成中 · ${formatTurnElapsed(elapsedMs)}`;
}
