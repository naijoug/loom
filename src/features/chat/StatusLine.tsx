import type { AgentDiagnostic } from "../../domain";
import { useT } from "../../i18n";
export function StatusLine({ running, resume, diagnostic, detail }: {
  running: boolean; resume: boolean; diagnostic?: AgentDiagnostic; detail?: string;
}) {
  const t = useT();
  return <div className="chat-status-line" role="status" title={[detail, diagnostic?.version, diagnostic?.detail].filter(Boolean).join(" · ")}>
    <span className="chat-status-run" data-running={running}><span className="chat-status-dot" aria-hidden="true" />{t(running ? "running" : "ready")}</span>
    <span>{t(resume && diagnostic?.capabilities?.nativeResume !== false ? "nativeResume" : "historyReplay")}</span>
    {!running && <span className="chat-status-tip">{t("slashHint")}</span>}
  </div>;
}
