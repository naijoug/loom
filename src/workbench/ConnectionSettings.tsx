import {useEffect,useRef,useState,type FormEvent} from "react";
import type {ConnectionAuth,ConnectionConfig,ConnectionProvider,EndpointApi,ModelConnection,SaveConnectionRequest} from "../../packages/protocol/src/workbench";
import {workbenchClient as client} from "../api/workbenchClient";

const initialConfig=():ConnectionConfig=>({name:"",provider:"claude",auth:"api_key",endpoint:null,piProvider:null,customApi:null,models:[],defaultModel:"",supportsImages:false,midStreamBehavior:"queue"});
export function ConnectionSettings({connections,credentialStore,onClose,onSaved}:{connections:ModelConnection[];credentialStore:string;onClose():void;onSaved():void}){
  const dialog=useRef<HTMLDialogElement>(null);const nameInput=useRef<HTMLInputElement>(null);
  const [editing,setEditing]=useState<ModelConnection|null>(null);const [config,setConfig]=useState<ConnectionConfig>(initialConfig);const [modelText,setModelText]=useState("");const [secret,setSecret]=useState("");
  const [message,setMessage]=useState("");const [error,setError]=useState("");const [busy,setBusy]=useState(false);const pending=useRef(false);const live=useRef(false);const draftId=useRef<string>(crypto.randomUUID());
  const retry=useRef<SaveConnectionRequest|null>(null);
  useEffect(()=>{live.current=true;dialog.current?.showModal();nameInput.current?.focus();return()=>{live.current=false;retry.current=null;dialog.current?.close();};},[]);
  function choose(connection:ModelConnection|null){if(pending.current)return;setEditing(connection);setConfig(connection?.config??initialConfig());setModelText(connection?.config.models.join("\n")??"");setSecret("");setError("");setMessage("");retry.current=null;draftId.current=connection?.id??crypto.randomUUID();nameInput.current?.focus();}
  function provider(value:ConnectionProvider){setConfig(c=>({...c,provider:value,auth:value==="claude"&&c.auth==="none"?"api_key":c.auth,endpoint:null,piProvider:value==="pi"?"":null,customApi:value==="pi_compat"?"openai-completions":null}));setSecret("");}
  async function save(event:FormEvent){
    event.preventDefault();if(pending.current)return;setError("");setMessage("");
    const models=modelText.split(/[\n,]/).map(s=>s.trim()).filter(Boolean);const normalized={...config,name:config.name.trim(),endpoint:config.endpoint?.trim()||null,piProvider:config.provider==="pi"?config.piProvider?.trim()||null:null,models,defaultModel:config.defaultModel.trim()||models[0]||""};
    const credential=normalized.auth==="none"?{kind:"clear" as const}:secret?{kind:"replace" as const,value:secret}:{kind:"keep" as const};
    const old=retry.current;const same=old&&old.id===draftId.current&&old.expectedRevision===(editing?.revision??null)&&JSON.stringify(old.config)===JSON.stringify(normalized)&&old.credential.kind===credential.kind&&(old.credential.kind!=="replace"||credential.kind==="replace"&&old.credential.value===credential.value);
    const request:SaveConnectionRequest=same?old:{id:draftId.current,operationId:crypto.randomUUID(),expectedRevision:editing?.revision??null,config:normalized,credential};retry.current=request;
    pending.current=true;setBusy(true);
    try{const saved=await client.saveConnection(request);if(live.current){setEditing(saved);setConfig(saved.config);setSecret("");retry.current=null;setMessage(saved.credentialRef?"凭据已保存到系统凭据库；尚未验证模型连接。":"连接配置已保存；尚未验证模型连接。");onSaved();}}
    catch(reason){if(live.current)setError(String(reason));}
    finally{pending.current=false;if(live.current)setBusy(false);}
  }
  const needsSecret=config.auth!=="none";
  return <dialog ref={dialog} className="wb-dialog wb-connection-dialog" aria-label="模型连接" onCancel={onClose}>
    <header><h2>模型连接</h2><button type="button" aria-label="关闭模型连接" onClick={onClose}>关闭</button></header>
    <div className="wb-connection-layout"><nav aria-label="已保存模型连接"><button disabled={busy} onClick={()=>choose(null)}>＋ 新建连接</button>{connections.map(connection=><button disabled={busy} key={connection.id} className={editing?.id===connection.id?"selected":""} onClick={()=>choose(connection)}>{connection.config.name}<small>{connection.credentialRef?"凭据已保存":"无认证"} · 未验证</small></button>)}</nav>
      <form onSubmit={event=>void save(event)} className="wb-connection-form">
        <label>名称<input ref={nameInput} aria-label="连接名称" value={config.name} disabled={busy} maxLength={200} onChange={e=>setConfig({...config,name:e.target.value})} required/></label>
        <label>运行后端<select aria-label="连接后端" value={config.provider} disabled={busy} onChange={e=>provider(e.target.value as ConnectionProvider)}><option value="claude">Claude Agent SDK</option><option value="pi">Pi · 内置提供方</option><option value="pi_compat">Pi · 自定义兼容端点</option></select></label>
        {config.provider==="pi"&&<label>Pi 提供方 ID<input aria-label="提供方 ID" value={config.piProvider??""} disabled={busy} onChange={e=>setConfig({...config,piProvider:e.target.value})} required/></label>}
        {config.provider==="pi_compat"&&<label>端点协议<select aria-label="端点协议" value={config.customApi??"openai-completions"} disabled={busy} onChange={e=>setConfig({...config,customApi:e.target.value as EndpointApi})}><option value="openai-completions">OpenAI Chat Completions 兼容</option><option value="anthropic-messages">Anthropic Messages 兼容</option></select></label>}
        <label>API 地址{config.provider!=="pi_compat"&&"（可选覆盖）"}<input aria-label="API 地址" value={config.endpoint??""} disabled={busy} placeholder="https://… 或本机 HTTP 地址" onChange={e=>setConfig({...config,endpoint:e.target.value||null})} required={config.provider==="pi_compat"}/></label>
        <label>认证方式<select aria-label="认证方式" value={config.auth} disabled={busy} onChange={e=>{setConfig({...config,auth:e.target.value as ConnectionAuth});setSecret("");}}><option value="api_key">API Key</option><option value="bearer_token">Bearer Token</option>{config.provider!=="claude"&&<option value="none">无认证</option>}</select></label>
        {needsSecret&&<label>{editing?.credentialRef?"替换凭据（留空保留）":"凭据"}<input aria-label="连接凭据" type="password" autoComplete="off" spellCheck={false} value={secret} disabled={busy} onChange={e=>setSecret(e.target.value)} required={!editing?.credentialRef}/><small>凭据由系统凭据库保存，不会回显。更改提供方或端点时请重新输入。</small></label>}
        <label>模型 ID（每行一个）<textarea aria-label="模型 ID 列表" value={modelText} disabled={busy} rows={3} onChange={e=>setModelText(e.target.value)} required/></label>
        <label>默认模型<input aria-label="默认模型" value={config.defaultModel} disabled={busy} placeholder="留空使用列表中的第一个" onChange={e=>setConfig({...config,defaultModel:e.target.value})}/></label>
        <label className="wb-inline-label"><input type="checkbox" checked={config.supportsImages} disabled={busy} onChange={e=>setConfig({...config,supportsImages:e.target.checked})}/>此端点支持图像输入</label>
        <label>回复中发送新消息<select aria-label="回复中消息行为" value={config.midStreamBehavior} disabled={busy} onChange={e=>setConfig({...config,midStreamBehavior:e.target.value as ConnectionConfig["midStreamBehavior"]})}><option value="queue">排队</option><option value="steer">尝试引导当前回合</option></select></label>
        {needsSecret&&credentialStore!=="macos_keychain"&&<p className="wb-error">此平台当前没有可用的系统凭据库，暂不能保存需要认证的连接。</p>}
        {error&&<p role="alert" className="wb-error">{error}</p>}{message&&<p role="status">{message}</p>}
        <footer><span>{credentialStore==="macos_keychain"?"凭据存储：macOS 钥匙串":"凭据存储：不可用"}</span><button className="wb-primary" disabled={busy||(needsSecret&&credentialStore!=="macos_keychain")}>{busy?"保存中…":"保存连接"}</button></footer>
      </form></div>
  </dialog>;
}
