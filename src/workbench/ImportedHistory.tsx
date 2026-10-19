import {useEffect,useRef,useState} from "react";
import type {HistoricalMessage,ImportHistoryPage,ImportedResource,SessionKey} from "../../packages/protocol/src/workbench";
import {workbenchClient as client} from "../api/workbenchClient";

function ResourcePreview({sessionKey,resource,onClose}:{sessionKey:SessionKey;resource:ImportedResource;onClose():void}){
  const dialog=useRef<HTMLDialogElement>(null);const [bytes,setBytes]=useState<number[]>([]);const [more,setMore]=useState(true);const [busy,setBusy]=useState(false);const [error,setError]=useState("");
  const live=useRef(false);const pending=useRef(false);const offset=useRef(0);
  async function load(){
    if(pending.current)return;pending.current=true;setBusy(true);setError("");
    try{const value=await client.request({method:"read_import_resource",params:{key:sessionKey,resourceId:resource.id,offset:offset.current}},"import_resource");
      if(live.current){offset.current+=value.bytes.length;setBytes(previous=>[...previous,...value.bytes]);setMore(value.hasMore);}
    }catch(reason){if(live.current)setError(String(reason));}finally{pending.current=false;if(live.current)setBusy(false);}
  }
  useEffect(()=>{live.current=true;dialog.current?.showModal();void load();return()=>{live.current=false;dialog.current?.close();};},[]);
  let text="";let binary=false;
  try{text=new TextDecoder("utf-8",{fatal:true}).decode(new Uint8Array(bytes),{stream:more});binary=bytes.includes(0);}catch{binary=true;}
  return <dialog ref={dialog} className="wb-dialog wb-import-dialog" aria-label="历史资源" onCancel={onClose}>
    <header><h2>历史资源</h2><button autoFocus onClick={onClose}>关闭</button></header><code className="wb-import-path">{resource.sourcePath}</code><p>{resource.bytes.toLocaleString()} 字节 · 校验后读取</p>
    {error&&<p role="alert" className="wb-error">{error}</p>}
    <div className="wb-import-report">{binary?<p>二进制附件已完整保留。当前历史阅读器提供文本预览。</p>:<pre className="wb-history-resource">{text}</pre>}</div>
    <footer>{more&&!binary&&<button disabled={busy||bytes.length>=1024*1024} onClick={()=>void load()}>{busy?"读取中…":bytes.length>=1024*1024?"已预览前 1 MiB":"继续读取"}</button>}{error&&<button disabled={busy} onClick={()=>void load()}>重试读取</button>}<button onClick={onClose}>关闭</button></footer>
  </dialog>;
}

function HistoricalMessageView({message}:{message:HistoricalMessage}){
  const textParts=message.parts.filter(part=>part.type==="text").map(part=>part.text).join("");
  return <article className={`wb-message wb-message-${message.role}`}>
    <div className="wb-assistant-name">{message.role==="user"?"你":"助手"} · 历史记录</div>
    <div className="wb-message-text">{message.content||textParts}</div>
    {message.parts.map((part,index)=>part.type==="tool"?<details key={index} className="wb-tool"><summary>{part.name}<span>{part.status||"历史工具记录"}</span></summary><pre>{part.inputSummary}</pre><pre>{part.outputSummary}</pre></details>:part.type==="error"?<p key={index} className="wb-error">{part.message}</p>:null)}
    {message.errorSummary&&<p className="wb-error">{message.errorSummary}</p>}
    {message.status==="interrupted"&&<p className="wb-turn-status interrupted">导入时已中断；不会自动继续旧执行</p>}
  </article>;
}

export function ImportedHistory({sessionKey}:{sessionKey:SessionKey}){
  const [page,setPage]=useState<ImportHistoryPage|null>(null);const [messages,setMessages]=useState<HistoricalMessage[]>([]);const [error,setError]=useState("");const [busy,setBusy]=useState(false);const [resource,setResource]=useState<ImportedResource|null>(null);
  const generation=useRef(0);const pending=useRef(false);const offset=useRef(0);
  async function load(){
    if(pending.current)return;pending.current=true;setBusy(true);setError("");const token=generation.current;
    try{const value=await client.request({method:"read_import_history",params:{key:sessionKey,offset:offset.current,limit:100}},"import_history");if(generation.current===token){offset.current+=value.messages.length;setMessages(previous=>[...previous,...value.messages]);setPage(value);}}
    catch(reason){if(generation.current===token)setError(String(reason));}
    finally{if(generation.current===token){pending.current=false;setBusy(false);}}
  }
  useEffect(()=>{generation.current++;pending.current=false;offset.current=0;setPage(null);setMessages([]);setResource(null);void load();return()=>{generation.current++;};},[sessionKey.workspaceId,sessionKey.sessionId]);
  return <section aria-label="导入的历史记录" className="wb-history">
    <header><strong>{page?.history.origin.kind==="task"?"导入的历史任务":"导入的历史会话"}</strong><p>旧执行和授权未延续；本地副本可独立于原项目读取。</p></header>
    {error&&<p role="alert" className="wb-error">{error}<button disabled={busy} onClick={()=>void load()}>重新读取</button></p>}
    {!page&&busy&&<p role="status">正在读取历史记录…</p>}
    {messages.map(message=><HistoricalMessageView key={message.id} message={message}/>)}
    {page?.hasMore&&<button disabled={busy} onClick={()=>void load()}>{busy?"读取中…":"加载更多历史消息"}</button>}
    {page&&<details className="wb-history-resources" open={page.history.origin.kind==="task"}><summary>历史文件与附件（{page.resources.length}）</summary>{page.resources.map(item=><button key={item.id} onClick={()=>setResource(item)}>{item.id==="original-record"?(page.history.origin.kind==="task"?"任务完整记录":"会话完整记录"):item.sourcePath}<span>{item.bytes.toLocaleString()} 字节</span></button>)}</details>}
    {resource&&<ResourcePreview key={resource.id} sessionKey={sessionKey} resource={resource} onClose={()=>setResource(null)}/>}
  </section>;
}
