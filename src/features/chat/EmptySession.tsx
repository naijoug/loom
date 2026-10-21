import { useT } from "../../i18n";
export function EmptySession({ projectName, projectPath }: { projectName: string; projectPath: string }) {
  const t = useT();
  return <div className="chat-empty-session">
    <div className="chat-empty-inner">
      <h1>{t("newSessionIn", { project: projectName })}</h1>
      <code className="chat-empty-path" title={projectPath}>{projectPath}</code>
      <p>{t("emptyHint")}</p>
    </div>
  </div>;
}
