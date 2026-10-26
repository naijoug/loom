import { openUrl } from "@tauri-apps/plugin-opener";
import { hasTauriRuntime } from "../../hooks/runtime";

/** Only web and mail links leave Loom; anything else (file paths, custom schemes) stays plain text. */
export function isExternalUrl(href: string | null | undefined): href is string {
  if (!href || !/^(?:https?:\/\/|mailto:)/i.test(href)) return false;
  try { new URL(href); return true; } catch { return false; }
}

/** Opens in the system browser; the webview itself never navigates away from the app. */
export function openExternalLink(href: string) {
  if (!isExternalUrl(href)) return;
  if (!hasTauriRuntime()) {
    window.open(href, "_blank", "noopener,noreferrer");
    return;
  }
  openUrl(href).catch(error => console.warn("failed to open external link", error));
}
