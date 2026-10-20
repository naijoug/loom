import type { ChatPermissionMode } from "../../domain";
import { useT } from "../../i18n";
import { ModeSwitch } from "./ModeSwitch";
export function EmptySession({ mode, onChange }: { mode: ChatPermissionMode; onChange: (mode: ChatPermissionMode) => void }) {
  const t = useT();
  return <div className="chat-empty-session"><span className="chat-empty-mark" aria-hidden="true">✳</span>
    <h1>{t("newSession")}</h1><p>{t("emptyHint")}</p>
    <ModeSwitch value={mode} onChange={onChange} cards />
  </div>;
}
