import { Eye, Hand, Zap } from "lucide-react";
import type { ChatPermissionMode } from "../../domain";

/** Warp and weft threads: the Loom brand mark. */
export function LoomMark({ size = 18 }: { size?: number }) {
  return <svg className="chat-loom-mark" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" aria-hidden="true">
    <path d="M4 6h16M4 12h16M4 18h16" /><path d="M8 3v18M16 3v18" />
  </svg>;
}
export function PermissionIcon({ mode, size = 14 }: { mode: ChatPermissionMode; size?: number }) {
  const Icon = mode === "auto" ? Zap : mode === "ask" ? Hand : Eye;
  return <Icon size={size} aria-hidden="true" />;
}
