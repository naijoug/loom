import { useState } from "react";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import type { ChatExportResult } from "../../domain/chat";

export function ChatExportNotice({busy,error,result,reveal = revealItemInDir}: {
  busy:boolean; error:string|null; result:ChatExportResult|null; reveal?:(path:string)=>Promise<void>;
}) {
  const [revealError,setRevealError]=useState<string|null>(null);
  if (!busy && !error && !result) return null;
  return <section className="chat-export-notice" aria-label="会话导出结果">
    {busy && <p role="status">正在导出会话与日志…</p>}
    {error && <p role="alert">导出失败：{error}</p>}
    {result && <>
      <p role="status">已导出 JSON、Markdown 和可用日志 · 快照序号 {result.snapshotSeq}</p>
      <code>{result.directory}</code>
      <p>请备份整个目录；JSON 包含完整会话。{result.inProgress ? "这是运行中快照，不是最终结果。" : ""}</p>
      {result.warnings.length > 0 && <ul>{result.warnings.map((warning,index)=><li key={index}>{warning}</li>)}</ul>}
      <button type="button" className="chat-link-btn" onClick={() => {
        setRevealError(null); void reveal(result.directory).catch(error=>setRevealError(String(error)));
      }}>在文件夹中显示</button>
      {revealError && <p role="alert">无法打开文件夹：{revealError}</p>}
    </>}
  </section>;
}
