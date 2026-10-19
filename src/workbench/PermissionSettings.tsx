import {useEffect,useRef,useState,type FormEvent} from "react";
import type {PermissionDocument,PermissionScope,PermissionSettings as Settings,SavePermissionRequest,JsonValue} from "../../packages/protocol/src/workbench";
import {permissionScopeKey,workbenchClient as client} from "../api/workbenchClient";
export function PermissionSettings({workspaceId,onClose}:{workspaceId:string;onClose():void}){
 const dialog=useRef<HTMLDialogElement>(null);const live=useRef(false);const pending=useRef(false);const retry=useRef<SavePermissionRequest|null>(null);
 const [settings,setSettings]=useState<Settings|null>(null);const[scope,setScope]=useState<PermissionScope>({kind:"workspace",workspaceId});const[text,setText]=useState("{}");
 const[editing,setEditing]=useState<PermissionDocument|null>(null);const[busy,setBusy]=useState(false);const[error,setError]=useState("");const[message,setMessage]=useState("");
 function choose(next:PermissionScope,data:Settings){const document=data.documents.find(d=>permissionScopeKey(d.scope)===permissionScopeKey(next))??null;setScope(next);setEditing(document);setText(JSON.stringify(document?.config??(next.kind==="application"?data.applicationDefaults:{}),null,2));retry.current=null;setMessage("");setError("");}
 useEffect(()=>{live.current=true;dialog.current?.showModal();void client.permissionSettings(workspaceId).then(data=>{if(live.current){setSettings(data);choose({kind:"workspace",workspaceId},data);}}).catch(e=>{if(live.current)setError(String(e));});return()=>{live.current=false;dialog.current?.close();};},[workspaceId]);
 async function save(reset=false){
  if(pending.current||!settings)return;setError("");setMessage("");let config:JsonValue|null=null;
  if(!reset){try{config=JSON.parse(text);if(!config||typeof config!=="object"||Array.isArray(config))throw new Error("权限规则必须是 JSON 对象");}catch(e){setError(String(e));return;}}
  const old=retry.current;const same=old&&permissionScopeKey(old.scope)===permissionScopeKey(scope)&&old.expectedRevision===(editing?.revision??null)&&JSON.stringify(old.config)===JSON.stringify(config);
  const request:SavePermissionRequest=same?old:{scope,operationId:crypto.randomUUID(),expectedRevision:editing?.revision??null,config};retry.current=request;pending.current=true;setBusy(true);
  try{const saved=await client.savePermissions(request);if(live.current){const next={...settings,documents:[...settings.documents.filter(d=>permissionScopeKey(d.scope)!==permissionScopeKey(scope)),saved]};setSettings(next);setEditing(saved);retry.current=null;if(reset)setText(JSON.stringify(scope.kind==="application"?settings.applicationDefaults:{},null,2));setMessage("已保存。新回合使用更新后的规则；正在运行的回合保持当前规则。");}}
  catch(e){if(live.current)setError(String(e));}finally{pending.current=false;if(live.current)setBusy(false);}
 }
 async function reload(){if(pending.current)return;pending.current=true;setBusy(true);try{const data=await client.permissionSettings(workspaceId);if(live.current){setSettings(data);choose(scope,data);}}catch(e){if(live.current)setError(String(e));}finally{pending.current=false;if(live.current)setBusy(false);}}
 function submit(event:FormEvent){event.preventDefault();void save();}
 return <dialog ref={dialog} className="wb-dialog wb-permission-dialog" aria-label="权限设置" onCancel={onClose}>
  <form onSubmit={submit}><header><h2>权限设置</h2><button type="button" aria-label="关闭权限设置" onClick={onClose}>关闭</button></header>
  <p>工作区规则扩展应用默认规则。Source 规则仅在对应 Source 启用时参与判断。</p>
  <label>配置范围<select aria-label="权限配置范围" disabled={busy||!settings} value={permissionScopeKey(scope)} onChange={e=>{if(!settings)return;const next=e.target.value==="application"?{kind:"application" as const}:e.target.value===`workspace/${workspaceId}`?{kind:"workspace" as const,workspaceId}:settings.documents.find(d=>permissionScopeKey(d.scope)===e.target.value)!.scope;choose(next,settings);}}><option value={`workspace/${workspaceId}`}>当前工作区</option><option value="application">应用默认</option>{settings?.documents.filter(d=>d.scope.kind==="source").map(d=><option key={permissionScopeKey(d.scope)} value={permissionScopeKey(d.scope)}>Source：{d.scope.kind==="source"?d.scope.sourceId:""}</option>)}</select></label>
  <label>权限规则（JSON）<textarea aria-label="权限规则 JSON" spellCheck={false} rows={18} disabled={busy||!settings} value={text} onChange={e=>setText(e.target.value)}/></label>
  <small>可配置 allowedBashPatterns、allowedMcpPatterns、allowedApiEndpoints 和 allowedWritePaths；规则由宿主校验后保存。</small>
  {error&&<p role="alert" className="wb-error">{error}</p>}{message&&<p role="status">{message}</p>}
  <footer><span>{editing?`版本 ${editing.revision}`:"未添加自定义规则"}</span><button type="button" disabled={busy} onClick={()=>void reload()}>重新加载</button><button type="button" disabled={busy||!settings} onClick={()=>void save(true)}>恢复此范围默认规则</button><button className="wb-primary" disabled={busy||!settings}>{busy?"保存中…":"保存规则"}</button></footer></form>
 </dialog>;
}
