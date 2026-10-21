import { TriangleAlert } from "lucide-react";
import { useT } from "../../i18n";

export interface Confirmation { title: string; body: string; confirmLabel: string; onConfirm: () => void; onCancel: () => void }

/** In-app confirmation shown above the composer input; replaces window.confirm, which WKWebView may not show. */
export function ConfirmBanner({ title, body, confirmLabel, onConfirm, onCancel, className = "" }: Confirmation & { className?: string }) {
  const t = useT();
  return <div className={`chat-confirm ${className}`} role="dialog" aria-label={title}>
    <TriangleAlert size={16} aria-hidden="true" className="chat-confirm-icon" />
    <div className="chat-confirm-text"><strong>{title}</strong><p>{body}</p></div>
    <div className="chat-confirm-actions">
      <button type="button" onClick={onCancel}>{t("cancel")}</button>
      <button type="button" className="chat-confirm-primary" onClick={onConfirm}>{confirmLabel}</button>
    </div>
  </div>;
}
