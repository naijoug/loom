import {useEffect,useRef,useState} from "react";
import type {SessionView,ToolApproval} from "../../packages/protocol/src/workbench";
import {workbenchClient as client} from "../api/workbenchClient";

const statuses={pending:"等待决定",allowed:"已允许，等待工具启动",denied:"已拒绝",expired:"已过期",cancelled:"已撤销",consumed:"授权已使用"};
export function ToolApprovals({view,onChange}:{view:SessionView;onChange():void}){
  const [error,setError]=useState("");const [,render]=useState(0);const pending=useRef(new Set<string>());const live=useRef(false);
  useEffect(()=>{live.current=true;return()=>{live.current=false;};},[]);
  const approvals=Object.values(view.approvals??{});const grants=Object.values(view.rememberedGrants??{}).filter(g=>g.revokedAtMs===null);if(approvals.length===0&&grants.length===0)return null;
  const active=approvals.filter(a=>a.status==="pending"||a.status==="allowed");const history=approvals.filter(a=>a.status!=="pending"&&a.status!=="allowed");
  async function decide(approval:ToolApproval,allowed:boolean,remember=false){
    if(pending.current.has(approval.id))return;pending.current.add(approval.id);render(n=>n+1);setError("");
    try{await client.request(remember?{method:"remember_tool_approval",params:{key:view.metadata.key,requestId:approval.id,expectedDigest:approval.bindingDigest}}:{method:"respond_tool_approval",params:{key:view.metadata.key,requestId:approval.id,expectedDigest:approval.bindingDigest,allowed}},"tool_approval");}
    catch(reason){if(live.current)setError(String(reason));}
    finally{pending.current.delete(approval.id);if(live.current){render(n=>n+1);onChange();}}
  }
  async function clear(){if(pending.current.has("clear"))return;pending.current.add("clear");render(n=>n+1);setError("");try{await client.request({method:"clear_remembered_grants",params:{key:view.metadata.key}},"session");}catch(reason){if(live.current)setError(String(reason));}finally{pending.current.delete("clear");if(live.current){render(n=>n+1);onChange();}}}
  return <section className="wb-approvals" aria-label="工具授权">
    {error&&<p className="wb-error" role="alert">{error}</p>}
    {active.map(approval=><div className="wb-approval" key={approval.id}>
      <header><strong>{approval.toolName} 请求授权</strong><span>{statuses[approval.status]}</span></header>
      {approval.workingDirectory&&<p>执行目录：<code>{approval.workingDirectory}</code></p>}
      <details><summary>查看工具参数（敏感内容已隐藏）</summary><pre>{JSON.stringify(approval.preview,null,2)}</pre></details>
      {approval.remember&&<p className="wb-remember-scope">可记住范围：{approval.remember.keys.map(k=>k.label).join("；")}。仅限此会话，应用重启后失效。</p>}
      {approval.status==="pending"&&<footer><span>{approval.remember?"选择授权范围":"仅限本次调用"}</span><button disabled={pending.current.has(approval.id)} onClick={()=>void decide(approval,false)}>拒绝</button><button className="wb-approval-allow" disabled={pending.current.has(approval.id)} onClick={()=>void decide(approval,true)}>允许一次</button>{approval.remember&&<button disabled={pending.current.has(approval.id)} onClick={()=>void decide(approval,true,true)}>此会话中记住</button>}</footer>}
    </div>)}
    {history.length>0&&<details className="wb-approval-history"><summary>工具授权记录（{history.length}）</summary>{history.map(approval=>{const grant=view.rememberedGrants&&Object.prototype.hasOwnProperty.call(view.rememberedGrants,approval.id)?view.rememberedGrants[approval.id]:null;return <p key={approval.id}>{approval.toolName} · {statuses[approval.status]}{grant&&(grant.revokedAtMs===null?" · 会话授权有效":" · 会话授权已撤销")}</p>;})}</details>}
    {grants.length>0&&<details className="wb-approval-history"><summary>已记住的会话授权（{grants.length}）</summary>{grants.map(grant=><p key={grant.id}>{grant.offer.keys.map(k=>k.label).join("；")}</p>)}<button disabled={pending.current.has("clear")} onClick={()=>void clear()}>清除会话授权</button></details>}
  </section>;
}
