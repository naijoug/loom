import { UpdateButton, useUpdateDrafts } from "../features/app-update/UpdateProvider";
import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore, type FormEvent } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { ArrowUp, Archive, ArchiveRestore, Check, ChevronDown, Circle, Flag, Folder, FolderPlus, Inbox, MessageSquare, MoreHorizontal, PanelLeft, Plus, Search, Square, Star, X, Wrench } from "lucide-react";
import type { HostCapabilities, ModelConnection, Project, SessionListEntry, SessionPatch, SessionSummary, SessionView, Workspace } from "../../packages/protocol/src/workbench";
import { workbenchClient as client, type SessionKey } from "../api/workbenchClient";
import { SessionController } from "./sessionController";
import { LegacyPreviewDialog } from "./LegacyPreviewDialog";
import { ImportedHistory } from "./ImportedHistory";
import { ToolApprovals } from "./ToolApprovals";
import { PermissionSettings } from "./PermissionSettings";
import { ConnectionSettings } from "./ConnectionSettings";
import { latestReadable } from "./projection";
import "./workbench.css";

const emptyConnection={connectionId:null,model:null,thinking:null};
export const patch=(value:Partial<SessionPatch>):SessionPatch=>({title:{op:"keep"},statusId:null,archived:null,flagged:null,labels:null,permissionMode:null,connection:null,enabledSources:null,notes:null,...value});
const statusLabels:Record<string,string>={backlog:"待整理",todo:"待办",in_progress:"进行中",needs_review:"待审阅",done:"已完成",cancelled:"已取消"};
const turnLabels:Record<string,string>={queued:"等待执行",running:"正在回复",cancelling:"正在停止",completed:"已完成",failed:"失败",cancelled:"已停止",interrupted:"已中断"};
const keyString=(key:SessionKey)=>`${key.workspaceId}/${key.sessionId}`;
const titleOf=(summary:SessionSummary)=>summary.metadata.title||summary.preview||"新会话";
function useSession(key:SessionKey|null){
  const token=key?keyString(key):"";
  const controller=useMemo(()=>key?new SessionController(key,client):null,[token]);
  const empty=useMemo(()=>({view:null,error:null,loading:false}),[]);
  const state=useSyncExternalStore(controller?.subscribe??(()=>()=>{}),controller?.getSnapshot??(()=>empty));
  useEffect(()=>{
    if(!controller)return;void controller.start();
    const sync=()=>{void controller.sync();};window.addEventListener("focus",sync);
    const timer=window.setInterval(sync,2500);
    return()=>{window.removeEventListener("focus",sync);clearInterval(timer);void controller.dispose().catch(()=>{});};
  },[controller]);
  return{...state,controller};
}
function NameDialog({title,initial,onSave,onClose}:{title:string;initial:string;onSave(value:string):Promise<void>;onClose():void}){
  const dialog=useRef<HTMLDialogElement>(null);const input=useRef<HTMLInputElement>(null);const [value,setValue]=useState(initial);const [busy,setBusy]=useState(false);const [error,setError]=useState("");
  useEffect(()=>{const node=dialog.current;node?.showModal();input.current?.focus();input.current?.select();return()=>node?.close();},[]);
  async function submit(event:FormEvent){event.preventDefault();if(!value.trim()||busy)return;setBusy(true);try{await onSave(value.trim());onClose();}catch(e){setError(String(e));}finally{setBusy(false);}}
  return <dialog ref={dialog} className="wb-dialog" aria-label={title} onCancel={onClose}><form onSubmit={event=>void submit(event)}><header><h2>{title}</h2><button type="button" aria-label="关闭" onClick={onClose}><X size={18}/></button></header><label>名称<input ref={input} autoFocus value={value} onChange={event=>setValue(event.target.value)} maxLength={200}/></label>{error&&<p role="alert" className="wb-error">{error}</p>}<footer><button type="button" onClick={onClose}>取消</button><button className="wb-primary" disabled={busy||!value.trim()}>{busy?"保存中…":"保存"}</button></footer></form></dialog>;
}
function Transcript({view}:{view:SessionView}){
  const scroll=useRef<HTMLDivElement>(null);const pinned=useRef(true);
  useEffect(()=>{if(pinned.current&&scroll.current)scroll.current.scrollTop=scroll.current.scrollHeight;},[view.lastSeq]);
  useEffect(()=>{pinned.current=true;if(scroll.current)scroll.current.scrollTop=scroll.current.scrollHeight;},[view.metadata.key.sessionId]);
  return <div className="wb-transcript" ref={scroll} onScroll={()=>{const node=scroll.current;if(node)pinned.current=node.scrollHeight-node.scrollTop-node.clientHeight<100;}} aria-label="会话记录">
    {view.importedHistory&&<ImportedHistory key={keyString(view.metadata.key)} sessionKey={view.metadata.key}/>}
    {view.messages.length===0&&!view.importedHistory?<div className="wb-chat-empty"><div className="wb-mark">L</div><h2>今天想做些什么？</h2><p>围绕一个想法开始，让工作留在同一段对话里。</p></div>:view.messages.map(message=>{
      const turn=view.turns.find(item=>item.id===message.turnId);const pending=message.role==="assistant"&&message.parts.length===0;
      return <article key={message.id} className={`wb-message wb-message-${message.role}`} aria-label={message.role==="user"?"你的消息":"助手消息"}>
        {message.role==="assistant"&&<div className="wb-assistant-name"><span className="wb-mini-mark">L</span>Loom</div>}
        {message.parts.map((part,index)=>part.type==="text"?(part.kind==="thinking"?<details key={index} className="wb-thinking"><summary>思考过程</summary><div>{part.text}</div></details>:<div key={index} className="wb-message-text">{part.text}</div>):part.type==="tool"?(()=>{const tool=Object.prototype.hasOwnProperty.call(view.tools,part.toolCallId)?view.tools[part.toolCallId]:null;return tool?<details key={index} className="wb-tool"><summary><Wrench size={14}/>{tool.name}<span>{tool.status}</span></summary><pre>{JSON.stringify({input:tool.input,output:tool.output},null,2)}</pre></details>:null;})():<span key={index}>附件 {part.attachmentId}</span>)}
        {pending&&<div className="wb-pending">{turnLabels[turn?.status??"queued"]}</div>}
        {message.role==="assistant"&&turn&&<div className={`wb-turn-status ${turn.status}`}>{turnLabels[turn.status]}{turn.error&&<span role="alert"> · {turn.error}</span>}</div>}
      </article>;
    })}
  </div>;
}
export function WorkbenchApp(){
  const [workspaces,setWorkspaces]=useState<Workspace[]>([]);const [workspace,setWorkspace]=useState("");const workspaceRef=useRef(workspace);workspaceRef.current=workspace;
  const [entries,setEntries]=useState<SessionListEntry[]>([]);const [projects,setProjects]=useState<Project[]>([]);const [project,setProject]=useState("");
  const selections=useRef(new Map<string,string>());const [,selectionChanged]=useState(0);const selected=selections.current.get(workspace)??"";const setSelected=(id:string)=>{selections.current.set(workspaceRef.current,id);selectionChanged(n=>n+1);};const [filter,setFilter]=useState("all");const [search,setSearch]=useState("");
  const [caps,setCaps]=useState<HostCapabilities|null>(null);const [error,setError]=useState("");const [booting,setBooting]=useState(true);
  const [connections,setConnections]=useState<ModelConnection[]>([]);const [connectionSettings,setConnectionSettings]=useState(false);
  const [permissionSettings,setPermissionSettings]=useState(false);
  const [sidebar,setSidebar]=useState(true);const [info,setInfo]=useState(false);
  const [legacyProject,setLegacyProject]=useState<Project|null>(null);
  const [dialog,setDialog]=useState<{kind:"workspace"|"rename";id:string;key?:SessionKey;initial:string}|null>(null);
  const drafts=useRef(new Map<string,string>());const [,draftChanged]=useState(0);const notes=useRef(new Map<string,string>());const [,rerender]=useState(0);
  useUpdateDrafts("workbench",Object.fromEntries(drafts.current),saved=>{for(const [key,value] of Object.entries(saved))if(!drafts.current.has(key))drafts.current.set(key,value);draftChanged(n=>n+1);});
  useUpdateDrafts("workbench-notes",Object.fromEntries(notes.current),saved=>{for(const [key,value] of Object.entries(saved))if(!notes.current.has(key))notes.current.set(key,value);rerender(n=>n+1);});
  const actionErrors=useRef(new Map<string,string>());
  const sending=useRef(new Map<string,{id:string;text:string;error?:string}>());const pendingSends=useRef(new Set<string>());const creating=useRef(false);
  const selectedKey=useMemo(()=>workspace&&selected?{workspaceId:workspace,sessionId:selected}:null,[workspace,selected]);
  const session=useSession(selectedKey);const view=session.view;
  const refreshing=useRef(new Map<string,{again:boolean;promise:Promise<void>}>());
  const refresh=useCallback((id:string):Promise<void>=>{
    const current=refreshing.current.get(id);if(current){current.again=true;return current.promise;}
    const entry={again:false,promise:Promise.resolve()};
    entry.promise=(async()=>{try{do{entry.again=false;const [items,folders,modelConnections]=await Promise.all([client.request({method:"list_sessions",params:{workspaceId:id}},"sessions"),client.request({method:"list_projects",params:{workspaceId:id}},"projects"),client.request({method:"list_connections"},"connections")]);if(workspaceRef.current===id){setEntries(items);setProjects(folders);setConnections(modelConnections);}}while(entry.again&&workspaceRef.current===id);}catch(e){if(workspaceRef.current===id)setError(String(e));}finally{if(refreshing.current.get(id)===entry)refreshing.current.delete(id);}})();
    refreshing.current.set(id,entry);return entry.promise;
  },[]);
  useEffect(()=>{let live=true;void(async()=>{try{
    const capabilities=await client.capabilities();let items=await client.request({method:"list_workspaces"},"workspaces");
    if(items.length===0){const initial=await client.request({method:"create_workspace",params:{id:"default-workspace",name:"我的工作区"}},"workspace");items=[initial];}
    if(live){setCaps(capabilities);setWorkspaces(items);setWorkspace(items[0].id);}
  }catch(e){if(live)setError(String(e));}finally{if(live)setBooting(false);}})();return()=>{live=false;};},[]);
  useEffect(()=>{
    setEntries([]);setProjects([]);setProject("");setFilter("all");setSearch("");setLegacyProject(null);
    if(!workspace)return;
    let live=true;let watch:{close():Promise<void>}|null=null;
    const update=()=>{if(live)void refresh(workspace);};
    void client.watchWorkspace(workspace,update).then(handle=>{if(!live){void handle.close().catch(()=>{});return;}watch=handle;update();}).catch(error=>{if(live){setError(String(error));update();}});
    const timer=window.setInterval(update,5000);
    window.addEventListener("focus",update);
    return()=>{live=false;clearInterval(timer);window.removeEventListener("focus",update);if(watch)void watch.close().catch(()=>{});};
  },[workspace,refresh]);
  const selectionToken=selectedKey?keyString(selectedKey):"";
  const selectionRef=useRef(selectionToken);selectionRef.current=selectionToken;
  const draft=drafts.current.get(selectionToken)??"";const busy=pendingSends.current.has(selectionToken);
  useEffect(()=>{setError(sending.current.get(selectionToken)?.error??actionErrors.current.get(selectionToken)??"");},[selectionToken]);
  const listRevision=view?`${view.metadataRevision}/${view.activeTurnId}/${view.turns.length}/${view.turns[view.turns.length-1]?.status}/${view.metadata.lastReadMessageId}`:"";
  useEffect(()=>{if(workspace&&view)void refresh(workspace);},[workspace,listRevision,refresh]);
  const readable=view?latestReadable(view):null;const marking=useRef("");
  useEffect(()=>{if(!view||!readable||view.metadata.lastReadMessageId===readable||marking.current===readable)return;marking.current=readable;void client.request({method:"mark_read",params:{key:view.metadata.key,messageId:readable}},"session").catch(()=>{marking.current="";});},[readable,view?.metadata.lastReadMessageId]);
  const rows=entries.filter((entry):entry is SessionListEntry&{summary:SessionSummary}=>!!entry.summary&&entry.summary.metadata.key.workspaceId===workspace);
  const visible=rows.filter(({summary:s})=>filter==="archived"?s.metadata.archived:!s.metadata.archived&&(filter==="all"||filter==="flagged"&&s.metadata.flagged||s.metadata.statusId===filter)).filter(({summary:s})=>titleOf(s).toLowerCase().includes(search.toLowerCase()));
  const selectedSummary=rows.find(row=>row.sessionId===selected)?.summary;
  async function createSession(){
    if(!workspace||creating.current)return;creating.current=true;setError("");const currentWorkspace=workspace;
    try{const created=await client.request({method:"create_session",params:{key:{workspaceId:workspace,sessionId:crypto.randomUUID()},title:null,projectId:projects.find(item=>item.id===project&&item.workspaceId===workspace)?.id??null,permissionMode:"ask",connection:emptyConnection}},"session");if(workspaceRef.current===currentWorkspace){setFilter("all");setSearch("");setSelected(created.metadata.key.sessionId);}await refresh(currentWorkspace);}catch(e){setError(String(e));}finally{creating.current=false;}
  }
  useEffect(()=>{const onKey=(event:KeyboardEvent)=>{if((event.metaKey||event.ctrlKey)&&event.key.toLowerCase()==="n"&&!dialog&&!legacyProject&&!connectionSettings){event.preventDefault();void createSession();}};window.addEventListener("keydown",onKey);return()=>window.removeEventListener("keydown",onKey);},[workspace,project,dialog,legacyProject,connectionSettings]);
  function sessionError(key:SessionKey,error:unknown){const token=keyString(key);actionErrors.current.set(token,String(error));if(selectionRef.current===token)setError(String(error));}
  async function change(value:Partial<SessionPatch>){if(!view)return;const current=session.controller?.getSnapshot().view??view;actionErrors.current.delete(keyString(current.metadata.key));setError("");try{await client.request({method:"patch_session",params:{key:current.metadata.key,expectedSeq:current.lastSeq,patch:patch(value)}},"session");await session.controller?.sync();}catch(e){sessionError(current.metadata.key,e);await session.controller?.sync();throw e;}}
  const reaEnabled=!!view?.metadata.enabledSources.includes("rea");
  async function toggleRea(){
    if(!view||view.activeTurnId)return;
    const current=session.controller?.getSnapshot().view??view;
    const sources=current.metadata.enabledSources.includes("rea")
      ?current.metadata.enabledSources.filter(source=>source!=="rea")
      :[...current.metadata.enabledSources,"rea"];
    await change({enabledSources:sources});
  }
  async function send(){
    if(!selectedKey||!draft.trim()||caps?.runtime==="unavailable"||(caps?.runtime==="sdk"&&!selectedConnection)||view?.metadata.archived)return;
    const token=keyString(selectedKey);if(pendingSends.current.has(token))return;const text=draft;let pending=sending.current.get(token);if(!pending||pending.text!==text){pending={id:crypto.randomUUID(),text};sending.current.set(token,pending);}
    pendingSends.current.add(token);draftChanged(n=>n+1);setError("");
    try{await client.request({method:"send_message",params:{key:selectedKey,clientRequestId:pending.id,text,attachmentIds:[]}},"send");sending.current.delete(token);if(drafts.current.get(token)===text){drafts.current.delete(token);draftChanged(n=>n+1);}await session.controller?.sync();}catch(e){pending.error=String(e);if(selectionRef.current===token)setError(String(e));await session.controller?.sync();}finally{pendingSends.current.delete(token);draftChanged(n=>n+1);}
  }
  async function registerProject(){if(!workspace)return;try{const selected=await open({directory:true,multiple:false});if(typeof selected!=="string")return;const name=selected.split(/[\\/]/).filter(Boolean).pop()??"项目";const registered=await client.request({method:"register_project",params:{workspaceId:workspace,id:crypto.randomUUID(),name,directory:selected}},"project");await refresh(workspace);if(workspaceRef.current===workspace)setProject(registered.id);}catch(e){setError(String(e));}}
  const activeTurnId=view?.activeTurnId;
  const selectedConnection=view?.turns.find(turn=>turn.id===activeTurnId)?.configuration.resolvedConnection??connections.find(connection=>connection.id===view?.metadata.connection.connectionId);
  const displayedConnections=connections.map(connection=>selectedConnection&&connection.id===selectedConnection.id?selectedConnection:connection);
  const currentWorkspace=workspaces.find(item=>item.id===workspace);
  return <div className={`wb-app ${sidebar?"":"wb-sidebar-collapsed"}`}>
    <header className="wb-titlebar" data-tauri-drag-region><div className="wb-window-space"/><button aria-label="切换侧栏" onClick={()=>setSidebar(!sidebar)}><PanelLeft size={17}/></button><UpdateButton /><span className="wb-logo">L</span><span className="wb-divider"/><div className="wb-workspace-picker"><span className="wb-avatar">{currentWorkspace?.name.charAt(0)||"L"}</span><select aria-label="工作区" value={workspace} onChange={event=>setWorkspace(event.target.value)}>{workspaces.map(item=><option key={item.id} value={item.id}>{item.name}</option>)}</select><ChevronDown size={14}/></div><button aria-label="创建工作区" onClick={()=>setDialog({kind:"workspace",id:crypto.randomUUID(),initial:""})}><Plus size={16}/></button><div className="wb-title-spacer"/>{caps?.runtime==="fixture"&&<span className="wb-fixture">本地验证模式 · 测试回复</span>}<span className="wb-app-name">Loom</span></header>
    {sidebar&&<aside className="wb-sidebar"><button className="wb-new" onClick={()=>void createSession()} disabled={!workspace}><Plus size={18}/>新会话<span>⌘N</span></button><nav aria-label="会话分类">
      <button className={filter==="all"?"selected":""} onClick={()=>setFilter("all")}><Inbox size={17}/>所有会话<span>{rows.filter(row=>!row.summary.metadata.archived).length}</span></button>
      <div className="wb-status-tree">{Object.entries(statusLabels).map(([id,label])=><button key={id} className={filter===id?"selected":""} onClick={()=>setFilter(id)}><Circle size={13} className={`wb-status-dot ${id}`}/>{label}<span>{rows.filter(row=>!row.summary.metadata.archived&&row.summary.metadata.statusId===id).length||""}</span></button>)}</div>
      <button className={filter==="flagged"?"selected":""} onClick={()=>setFilter("flagged")}><Flag size={16}/>已标记</button><button className={filter==="archived"?"selected":""} onClick={()=>setFilter("archived")}><Archive size={16}/>已归档</button>
    </nav><div className="wb-nav-separator"/><div className="wb-project-heading"><span>项目</span><button aria-label="登记项目文件夹" onClick={()=>void registerProject()}><FolderPlus size={15}/></button></div><button className={!project?"wb-project selected":"wb-project"} onClick={()=>setProject("")}><MessageSquare size={15}/>无项目会话</button>{projects.map(item=><button className={`wb-project ${project===item.id?"selected":""}`} key={item.id} title={item.directory} onClick={()=>setProject(item.id)}><Folder size={15}/>{item.name}</button>)}{project&&<button className="wb-project" onClick={()=>{const selected=projects.find(item=>item.id===project);if(selected)setLegacyProject(selected);}}>检查项目旧数据</button>}<button className="wb-project" onClick={()=>setConnectionSettings(true)}><Wrench size={15}/>模型连接</button><button className="wb-project" disabled={!workspace} onClick={()=>setPermissionSettings(true)}><Wrench size={15}/>权限设置</button><UpdateButton manual /><div className="wb-sidebar-footer"><span className="wb-avatar small">L</span><div><strong>本地工作区</strong><span>会话保存在这台设备</span></div></div></aside>}
    <section className="wb-session-list" aria-label="会话列表"><header><h1>{filter==="all"?"所有会话":filter==="flagged"?"已标记":filter==="archived"?"已归档":statusLabels[filter]}</h1><button aria-label="新会话" onClick={()=>void createSession()}><Plus size={17}/></button></header><div className="wb-search"><Search size={15}/><input aria-label="搜索会话" placeholder="搜索会话" value={search} onChange={event=>setSearch(event.target.value)}/></div>
      <div className="wb-session-scroll">{visible.length>0?<><div className="wb-list-label">会话</div>{visible.map(({sessionId,summary:s})=><button className={`wb-session-row ${selected===sessionId?"selected":""}`} key={sessionId} onClick={()=>setSelected(sessionId)}><span className={`wb-row-state ${s.activeTurnId?"running":""}`}>{s.metadata.flagged?<Flag size={13}/>:<Circle size={12}/>}</span><div><strong>{titleOf(s)}</strong><span>{s.activeTurnId?"正在回复":s.queuedTurnCount?`${s.queuedTurnCount} 条等待执行`:statusLabels[s.metadata.statusId]??s.metadata.statusId}</span></div>{s.hasUnread&&<span className="wb-unread" aria-label="未读"/>}</button>)}</>:<div className="wb-list-empty"><Inbox size={30}/><strong>{booting?"正在加载…":"这里还没有会话"}</strong><p>新的对话会出现在这里。</p><button onClick={()=>void createSession()} disabled={!workspace}>新会话</button></div>}{entries.filter(entry=>entry.error).map(entry=><div className="wb-storage-error" key={entry.sessionId} role="alert">会话 {entry.sessionId} 无法读取：{entry.error}</div>)}</div>
    </section>
    <main className="wb-chat-panel">{error&&<div role="alert" className="wb-error-banner">{error}<button aria-label="关闭错误" onClick={()=>setError("")}><X size={14}/></button></div>}
      {!selectedKey?<div className="wb-no-selection"><MessageSquare size={28}/><p>{booting?"正在打开工作区…":"选择会话，或开始新的对话"}</p></div>:<><header className="wb-chat-header"><button className="wb-title-edit" disabled={!view} onClick={()=>view&&setDialog({kind:"rename",id:crypto.randomUUID(),key:view.metadata.key,initial:view.metadata.title??selectedSummary?.preview??"新会话"})}>{view?.metadata.title??(selectedSummary?titleOf(selectedSummary):"新会话")}<ChevronDown size={13}/></button><div className="wb-chat-actions"><button aria-label={view?.metadata.flagged?"取消标记":"标记会话"} disabled={!view} onClick={()=>void change({flagged:!view?.metadata.flagged}).catch(()=>{})}><Star size={16} fill={view?.metadata.flagged?"currentColor":"none"}/></button><button aria-label={view?.metadata.archived?"恢复会话":"归档会话"} disabled={!view||!!view.activeTurnId} onClick={()=>void change({archived:!view?.metadata.archived}).catch(()=>{})}>{view?.metadata.archived?<ArchiveRestore size={16}/>:<Archive size={16}/>}</button><button aria-label="会话信息" onClick={()=>setInfo(!info)}><MoreHorizontal size={19}/></button></div></header>
      {session.error&&<div role="alert" className="wb-error-banner">{session.error}<button onClick={()=>void session.controller?.sync()}>重试</button></div>}
      {view?<><div className="wb-conversation-body"><Transcript view={view}/>{info&&<aside className="wb-info"><h3>会话信息</h3><label>状态<select aria-label="会话状态" value={view.metadata.statusId} onChange={event=>void change({statusId:event.target.value}).catch(()=>{})}>{Object.entries(statusLabels).map(([id,label])=><option key={id} value={id}>{label}</option>)}</select></label><label>笔记<textarea aria-label="会话笔记" value={notes.current.get(selectionToken)??view.notes} onChange={event=>{notes.current.set(selectionToken,event.target.value);rerender(n=>n+1);}}/></label><button onClick={()=>void change({notes:notes.current.get(selectionToken)??view.notes}).then(()=>{notes.current.delete(selectionToken);rerender(n=>n+1);}).catch(()=>{})}><Check size={14}/>保存笔记</button></aside>}</div>
      <ToolApprovals key={keyString(view.metadata.key)} view={view} onChange={()=>void session.controller?.sync()}/>
      {view.metadata.archived?<div className="wb-archived"><Archive size={16}/>此会话已归档<button onClick={()=>void change({archived:false}).catch(()=>{})}>恢复会话</button></div>:<div className="wb-composer-area"><div className="wb-composer-options"><select aria-label="权限模式" disabled={!!view.activeTurnId} value={view.metadata.permissionMode} onChange={event=>void change({permissionMode:event.target.value as SessionView["metadata"]["permissionMode"]}).catch(()=>{})}><option value="explore">Explore · 只读</option><option value="ask">Ask · 询问</option><option value="execute">Execute · 自动</option></select><button type="button" className={reaEnabled?"wb-source-toggle selected":""} aria-label="REA 调研源" aria-pressed={reaEnabled} disabled={!!view.activeTurnId} onClick={()=>void toggleRea().catch(()=>{})}>{reaEnabled?"REA · 已启用":"REA · 已关闭"}</button><span>{statusLabels[view.metadata.statusId]??view.metadata.statusId}</span><div className="wb-title-spacer"/>{view.turns.some(turn=>turn.status==="queued")&&!view.activeTurnId&&caps?.runtime!=="unavailable"&&<button onClick={()=>void client.resume(view.metadata.key).catch(e=>sessionError(view.metadata.key,e))}>继续等待中的消息</button>}</div><div className="wb-composer"><textarea aria-label="消息" placeholder={caps?.runtime==="unavailable"?(selectedConnection?"此连接的运行时暂不可用":"先选择模型连接"):"想一起完成什么？"} value={draft} onChange={event=>{drafts.current.set(selectionToken,event.target.value);draftChanged(n=>n+1);}} onKeyDown={event=>{if(event.key==="Enter"&&!event.shiftKey&&!event.nativeEvent.isComposing&&event.nativeEvent.keyCode!==229){event.preventDefault();void send();}}}/><footer><span><Folder size={14}/>{view.metadata.projectId?projects.find(item=>item.id===view.metadata.projectId)?.name??"项目":"无项目"}</span><div className="wb-title-spacer"/><select aria-label="会话模型连接" disabled={!!view.activeTurnId||view.turns.length>0} value={view.metadata.connection.connectionId??""} onChange={event=>{const connection=connections.find(item=>item.id===event.target.value);if(connection)void change({connection:{connectionId:connection.id,model:connection.config.defaultModel,thinking:null}}).catch(()=>{});}}><option value="">选择模型连接</option>{displayedConnections.map(connection=><option key={connection.id} value={connection.id}>{connection.config.name}</option>)}</select>{selectedConnection&&<select aria-label="会话模型" disabled={!!view.activeTurnId} value={view.metadata.connection.model??selectedConnection.config.defaultModel} onChange={event=>void change({connection:{...view.metadata.connection,model:event.target.value}}).catch(()=>{})}>{selectedConnection.config.models.map(model=><option key={model} value={model}>{model}</option>)}</select>}<span className="wb-model-label">{caps?.runtime==="fixture"?"本地测试流":caps?.runtime==="sdk"?(selectedConnection?"SDK 已就绪":"未配置连接"):selectedConnection?"运行时不可用":"未配置连接"}</span>{view.activeTurnId?<button className="wb-send stop" aria-label="停止回复" disabled={view.turns.find(turn=>turn.id===view.activeTurnId)?.status==="cancelling"} onClick={()=>void client.request({method:"cancel_turn",params:{key:view.metadata.key,turnId:view.activeTurnId!}},"session").catch(e=>sessionError(view.metadata.key,e))}><Square size={14} fill="currentColor"/></button>:<button className="wb-send" aria-label="发送消息" disabled={!draft.trim()||busy||caps?.runtime==="unavailable"||(caps?.runtime==="sdk"&&!selectedConnection)} onClick={()=>void send()}><ArrowUp size={17}/></button>}</footer></div><div className="wb-composer-hint">Enter 发送 · Shift+Enter 换行{view.activeTurnId&&" · 新消息将排队"}</div></div>}</>:<div className="wb-no-selection">正在读取会话…</div>}</>}
    </main>
    {permissionSettings&&<PermissionSettings key={workspace} workspaceId={workspace} onClose={()=>setPermissionSettings(false)}/>}
    {connectionSettings&&<ConnectionSettings connections={connections} credentialStore={caps?.credentialStore??"unavailable"} onClose={()=>setConnectionSettings(false)} onSaved={()=>void refresh(workspace)}/> }
    {legacyProject&&<LegacyPreviewDialog key={`${legacyProject.workspaceId}/${legacyProject.id}`} project={legacyProject} onClose={()=>setLegacyProject(null)} onImported={()=>void refresh(legacyProject.workspaceId)} onOpen={result=>{
      if(!result.sessionKey)return;
      void client.request({method:"get_session",params:{key:result.sessionKey}},"session").then(imported=>{
        if(workspaceRef.current!==imported.metadata.key.workspaceId)return;
        setFilter(imported.metadata.archived?"archived":"all");setSearch("");setSelected(imported.metadata.key.sessionId);setLegacyProject(null);
      }).catch(reason=>setError(String(reason)));
    }}/>}

    {dialog&&<NameDialog title={dialog.kind==="workspace"?"创建工作区":"重命名会话"} initial={dialog.initial} onClose={()=>setDialog(null)} onSave={async name=>{if(dialog.kind==="workspace"){const created=await client.request({method:"create_workspace",params:{id:dialog.id,name}},"workspace");setWorkspaces(await client.request({method:"list_workspaces"},"workspaces"));setWorkspace(created.id);}else if(dialog.key){const current=await client.request({method:"get_session",params:{key:dialog.key}},"session");await client.request({method:"patch_session",params:{key:dialog.key,expectedSeq:current.lastSeq,patch:patch({title:{op:"set",value:name}})}},"session");await session.controller?.sync();await refresh(dialog.key.workspaceId);}}}/>}</div>;
}
