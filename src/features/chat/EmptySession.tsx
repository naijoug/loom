import type { ChatPermissionMode } from "../../domain";
import { useT } from "../../i18n";
import { ModeSwitch } from "./ModeSwitch";
export function EmptySession({ mode, onChange, projectName, projectPath }: {
  mode: ChatPermissionMode; onChange: (mode: ChatPermissionMode) => void; projectName: string; projectPath: string;
}) {
  const t = useT();
  return <div className="chat-empty-session">
    <div className="chat-empty-inner">
      <code className="chat-empty-path" title={projectPath}>{projectPath}</code>
      <h1>{t("newSessionIn", { project: projectName })}</h1>
      <p>{t("emptyHint")}</p>
      <ModeSwitch value={mode} onChange={onChange} cards />
    </div>
  </div>;
}
