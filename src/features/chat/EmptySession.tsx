import { useT } from "../../i18n";
import { LoomMark } from "./icons";
/** Greeting above the composer; the project itself is chosen in the composer's project picker. */
export function EmptySession() {
  const t = useT();
  return <div className="chat-empty-session chat-new-greeting">
    <div className="chat-empty-inner">
      <span className="chat-empty-mark"><LoomMark size={22} /></span>
      <h1>{t("newChatTitle")}</h1>
      <p>{t("newChatHint")}</p>
    </div>
  </div>;
}
