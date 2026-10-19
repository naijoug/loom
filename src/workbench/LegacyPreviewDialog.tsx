import { useEffect, useRef, useState } from "react";
import { X } from "lucide-react";
import type { LegacyIssue, LegacyPreview, LegacyImportPlan, LegacyImportReport, LegacyImportResult, Project } from "../../packages/protocol/src/workbench";
import { workbenchClient } from "../api/workbenchClient";

function Issues({ issues }: { issues: LegacyIssue[] }) {
  return <ul className="wb-import-issues">{issues.slice(0, 100).map((issue, index) =>
    <li key={index} className={issue.severity === "error" ? "wb-error" : ""}>
      <strong>{issue.severity === "error" ? "无法读取" : "注意"}：</strong>{issue.message}
      <code>{issue.path}</code>
    </li>)}{issues.length > 100 && <li>另外 {issues.length - 100} 项记录未展开。</li>}</ul>;
}

export function LegacyPreviewDialog({ project, onClose, onImported, onOpen }: { project: Project; onClose(): void; onImported?(): void; onOpen?(result:LegacyImportResult):void }) {
  const dialog = useRef<HTMLDialogElement>(null);
  const close = useRef<HTMLButtonElement>(null);
  const [report, setReport] = useState<LegacyPreview | null>(null);
  const [error, setError] = useState("");
  const [attempt, setAttempt] = useState(0);
  const [plan,setPlan]=useState<LegacyImportPlan|null>(null);
  const [result,setResult]=useState<LegacyImportReport|null>(null);
  const [busy,setBusy]=useState(false);
  const pending=useRef(false);const generation=useRef(0);
  useEffect(() => {
    const node = dialog.current;
    node?.showModal();
    close.current?.focus();
    return () => node?.close();
  }, []);
  useEffect(() => {
    let live = true;
    generation.current++;setReport(null); setError("");setPlan(null);setResult(null);setBusy(false);pending.current=false;
    void workbenchClient.request({ method: "preview_legacy_import", params: { workspaceId: project.workspaceId, projectId: project.id } }, "legacy_preview")
      .then(value => { if (live) setReport(value); })
      .catch(reason => { if (live) setError(String(reason)); });
    return () => { live = false;generation.current++; };
  }, [project.workspaceId, project.id, attempt]);
  const readable = report?.items.filter(item => item.contentHash !== null).length ?? 0;
  async function importRecords(){
    if(!report||pending.current)return;pending.current=true;setBusy(true);setError("");const token=generation.current;
    try{
      const prepared=plan??await workbenchClient.request({method:"prepare_legacy_import",params:{workspaceId:project.workspaceId,projectId:project.id,expectedFingerprint:report.fingerprint}},"legacy_import_plan");
      if(generation.current!==token)return;setPlan(prepared);
      const imported=await workbenchClient.request({method:"execute_legacy_import",params:{workspaceId:project.workspaceId,planId:prepared.id}},"legacy_import_report");
      if(generation.current===token){setResult(imported);onImported?.();}
    }catch(reason){if(generation.current===token)setError(String(reason));}
    finally{if(generation.current===token){pending.current=false;setBusy(false);}}
  }
  return <dialog ref={dialog} className="wb-dialog wb-import-dialog" aria-label="旧数据预检" onCancel={onClose}>
    <header><h2>旧数据预检</h2><button ref={close} type="button" aria-label="关闭预检" onClick={onClose}><X size={18} /></button></header>
    <p><strong>{project.name}</strong><code className="wb-import-path">{project.directory}</code></p>
    <p className="wb-import-hint">检查旧会话、任务与引用资源。原文件保持不变；预检不会执行导入。选择导入后，将复制可读取记录与资源，缺失资源和损坏记录保留在报告中。</p>
    {!report && !error && <p role="status">正在检查旧数据…</p>}
    {error && <p role="alert" className="wb-error">{error}</p>}
    {report && <div className="wb-import-report">
      {result&&<section aria-label="导入结果"><h3>导入结果</h3><p role="status">新增 {result.results.filter(item=>item.status==="imported").length} 项 · 已有 {result.results.filter(item=>item.status==="already_imported").length} 项 · 失败 {result.results.filter(item=>item.status==="failed").length} 项 · 跳过 {result.results.filter(item=>item.status==="skipped").length} 项</p>{result.results.slice(0,100).map(item=><div className="wb-import-result" key={`${item.kind}/${item.sourceId}`}><span>{item.sourceId} · {({imported:"已导入",already_imported:"已导入过",failed:"失败",skipped:"已跳过",prepared:"等待发布"} as Record<string,string>)[item.status]}</span>{item.error&&<p className="wb-error">{item.error}</p>}{item.sessionKey&&["imported","already_imported"].includes(item.status)&&onOpen&&<button onClick={()=>onOpen(item)}>打开历史记录</button>}</div>)}</section>}
      <p role="status">可读取 {readable} 项 · 无法读取 {report.items.length - readable} 项 · 已检查 {report.files.length} 个文件路径</p>
      <Issues issues={report.issues} />
      {report.items.length === 0 && <p>{report.issues.some(issue => issue.severity === "error") ? "部分目录无法读取，尚不能确认数据是否完整。" : "此项目没有发现旧 Chat 或 Task 记录。"}</p>}
      {report.items.slice(0, 100).map(item => <details key={`${item.kind}/${item.sourceId}`} className="wb-import-item" open={item.contentHash === null}>
        <summary>{item.kind === "chat" ? "会话" : "历史任务"} · {item.title || item.sourceId}<span>{item.contentHash === null ? "无法读取" : item.kind === "chat" ? `${item.messageCount} 条消息` : "可读取"}</span></summary>
        <code className="wb-import-path">{item.sourcePath}</code><Issues issues={item.issues} />
      </details>)}
      {report.items.length > 100 && <p>另外 {report.items.length - 100} 项未展开。</p>}
      <details><summary>来源校验标识</summary><code className="wb-import-path">{report.fingerprint}</code></details>
    </div>}
    {busy&&<p role="status">正在核对并导入…关闭窗口不会取消已开始的导入。</p>}
    <footer><button type="button" disabled={busy||(!report && !error)} onClick={() => setAttempt(value => value + 1)}>重新检查</button>{readable>0&&<button type="button" disabled={busy} className="wb-primary" onClick={()=>void importRecords()}>{busy?"导入中…":plan?"重试导入":"导入可读取的记录"}</button>}<button type="button" onClick={onClose}>关闭</button></footer>
  </dialog>;
}
