import { useT } from "../../i18n";
import { useState } from "react";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import type { ChatExportResult } from "../../domain/chat";

export function ChatExportNotice({busy,error,result,reveal = revealItemInDir}: {
  busy:boolean; error:string|null; result:ChatExportResult|null; reveal?:(path:string)=>Promise<void>;
}) {
  const t = useT();
  const [revealError,setRevealError]=useState<string|null>(null);
  if (!busy && !error && !result) return null;
  return <section className="chat-export-notice" aria-label={t("exportResult")} >
    {busy && <p role="status">{t("exportLogsBusy")}</p>}
    {error && <p role="alert">{t("exportFailure", { error })}</p>}
    {result && <>
      <p role="status">{t("exportSuccess", { seq: result.snapshotSeq })}</p>
      <code>{result.directory}</code>
      <p>{t("exportBackup")}{result.inProgress ? t("exportLive") : ""}</p>
      {result.warnings.length > 0 && <div role="status" aria-label={t("incompleteEvidence")} >
        <p>{t("warnings")}</p>
        <ul>{result.warnings.map((warning,index)=><li key={index}>{warning}</li>)}</ul>
      </div>}
      <button type="button" className="chat-link-btn" onClick={() => {
        setRevealError(null); void reveal(result.directory).catch(error=>setRevealError(String(error)));
      }}>{t("revealFolder")}</button>
      {revealError && <p role="alert">{t("revealError", { error: revealError })}</p>}
    </>}
  </section>;
}
