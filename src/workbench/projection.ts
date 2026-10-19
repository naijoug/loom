import type { SessionEnvelope, SessionKey, SessionView, Turn } from "../../packages/protocol/src/workbench";
export const sessionStream = (key: SessionKey) => `session/${key.workspaceId}/${key.sessionId}`;
export const sameSession = (a: SessionKey, b: SessionKey) => a.workspaceId===b.workspaceId && a.sessionId===b.sessionId;
const terminal = (turn: Turn) => !["queued","running","cancelling"].includes(turn.status);

/** Read projection only; authorization and state transitions remain in Rust. */
export function project(current: SessionView, event: SessionEnvelope): SessionView {
  if (event.schemaVersion!==1 || event.stream!==sessionStream(current.metadata.key) || event.seq!==current.lastSeq+1) throw new Error("事件序号或会话归属不匹配");
  const next: SessionView={...current,metadata:{...current.metadata,updatedAtMs:Math.max(current.metadata.updatedAtMs,event.timestampMs)},messages:[...current.messages],turns:[...current.turns],lastSeq:event.seq};
  const body=event.payload;
  function mutableTurn(turnId:string) { const index=next.turns.findIndex(turn=>turn.id===turnId); if(index<0)throw new Error("事件引用了不存在的回合"); const value={...next.turns[index]};next.turns[index]=value;return value; }
  function assistant(turn:Turn) { const index=next.messages.findIndex(message=>message.id===turn.receipt.assistantMessageId);if(index<0)throw new Error("回合缺少助手消息");const message={...next.messages[index],parts:[...next.messages[index].parts]};next.messages[index]=message;return message; }
  switch(body.kind){
    case "created": throw new Error("已有会话不能重复创建");
    case "history_imported": next.importedHistory=body.history;break;
    case "tool_approval_requested":next.approvals={...next.approvals,[body.approval.id]:body.approval};break;
    case "tool_approval_resolved":{
      const approval=Object.prototype.hasOwnProperty.call(next.approvals,body.requestId)?next.approvals[body.requestId]:null;if(!approval)throw new Error("审批记录不存在");
      const status=({allow:"allowed",allow_remember:"allowed",deny:"denied",expire:"expired",cancel:"cancelled"} as const)[body.resolution];
      next.approvals={...next.approvals,[body.requestId]:{...approval,status,resolvedAtMs:event.timestampMs}};
      if(body.resolution==="allow_remember"){if(!body.rememberedGrant)throw new Error("记住授权缺少范围记录");next.rememberedGrants={...next.rememberedGrants,[body.requestId]:body.rememberedGrant};}break;
    }
    case "remembered_grants_revoked":{
      next.rememberedGrants={...next.rememberedGrants};for(const id of body.grantIds){const grant=Object.prototype.hasOwnProperty.call(next.rememberedGrants,id)?next.rememberedGrants[id]:null;if(!grant)throw new Error("会话授权不存在");next.rememberedGrants[id]={...grant,revokedAtMs:event.timestampMs};}break;
    }
    case "tool_approval_consumed":{
      const approval=Object.prototype.hasOwnProperty.call(next.approvals,body.requestId)?next.approvals[body.requestId]:null;if(!approval)throw new Error("审批记录不存在");
      next.approvals={...next.approvals,[body.requestId]:{...approval,status:"consumed",resolvedAtMs:event.timestampMs}};
      next.tools={...next.tools,[approval.toolCallId]:{id:approval.toolCallId,turnId:approval.turnId,name:approval.toolName,input:approval.preview,output:null,status:"running"}};
      assistant(mutableTurn(approval.turnId)).parts.push({type:"tool",toolCallId:approval.toolCallId});break;
    }
    case "metadata_changed": {
      const patch=body.patch;next.metadataRevision=event.seq;
      if(patch.title.op==="set")next.metadata.title=patch.title.value;else if(patch.title.op==="clear")next.metadata.title=null;
      if(patch.archived!==null)next.metadata.archived=patch.archived;
      if(patch.flagged!==null)next.metadata.flagged=patch.flagged;
      if(patch.statusId!==null)next.metadata.statusId=patch.statusId;
      if(patch.labels!==null)next.metadata.labels=patch.labels;
      if(patch.permissionMode!==null)next.metadata.permissionMode=patch.permissionMode;
      if(patch.connection!==null)next.metadata.connection=patch.connection;
      if(patch.enabledSources!==null)next.metadata.enabledSources=patch.enabledSources;
      if(patch.notes!==null)next.notes=patch.notes;
      break;
    }
    case "message_accepted": {
      const receipt=body.receipt;
      if(next.turns.some(turn=>turn.id===receipt.turnId))throw new Error("重复回合");
      next.messages.push({id:receipt.userMessageId,turnId:receipt.turnId,role:"user",createdAtMs:event.timestampMs,parts:[{type:"text",id:receipt.userMessageId,kind:"text",text:body.text},...body.attachmentIds.map(attachmentId=>({type:"attachment" as const,attachmentId}))]});
      next.messages.push({id:receipt.assistantMessageId,turnId:receipt.turnId,role:"assistant",createdAtMs:event.timestampMs,parts:[]});
      next.turns.push({id:receipt.turnId,receipt,status:"queued",configuration:body.configuration,acceptedAtMs:event.timestampMs,startedAtMs:null,endedAtMs:null,binding:null,error:null});
      break;
    }
    case "turn_started": {if(next.activeTurnId)throw new Error("会话已有活动回合");const turn=mutableTurn(body.turnId);turn.status="running";turn.startedAtMs=event.timestampMs;turn.binding=body.binding;next.activeTurnId=body.turnId;break;}
    case "turn_preparation_failed": {const turn=mutableTurn(body.turnId);turn.status="failed";turn.error=body.error;turn.endedAtMs=event.timestampMs;break;}
    case "runtime_applied": {
      if(next.activeTurnId!==body.turnId)throw new Error("运行事件不属于当前回合");
      const turn=mutableTurn(body.turnId);const message=assistant(turn);const update=body.update;
      if(update.kind==="usage"){turn.usage=update.usage;}
      else if(update.kind==="tool_policy"){
        next.tools={...next.tools,[update.toolCallId]:{id:update.toolCallId,turnId:body.turnId,name:update.name,input:update.input,output:update.allowed?null:{denied:true,reason:update.reason},status:update.allowed?"running":"denied"}};
        message.parts.push({type:"tool",toolCallId:update.toolCallId});
      }
      else if(update.kind==="text_delta"){
        const index=message.parts.findIndex(part=>part.type==="text" && part.id===update.partId);
        if(index<0)message.parts.push({type:"text",id:update.partId,kind:update.textKind,text:update.delta});
        else {const part=message.parts[index];if(part.type!=="text"||part.kind!==update.textKind)throw new Error("文本片段类型发生变化");message.parts[index]={...part,text:part.text+update.delta};}
      } else if(update.kind==="tool_started") {
        if(Object.prototype.hasOwnProperty.call(next.tools,update.toolCallId))throw new Error("重复工具调用");
        next.tools={...next.tools,[update.toolCallId]:{id:update.toolCallId,turnId:body.turnId,name:update.name,input:update.input,output:null,status:"running"}};
        message.parts.push({type:"tool",toolCallId:update.toolCallId});
      } else {
        if(!Object.prototype.hasOwnProperty.call(next.tools,update.toolCallId))throw new Error("未知工具调用");
        const tool=next.tools[update.toolCallId];if(tool.turnId!==body.turnId)throw new Error("工具结果归属不匹配");next.tools={...next.tools,[tool.id]:{...tool,status:update.status,output:update.output}};
      }
      break;
    }
    case "turn_cancel_requested": {const turn=mutableTurn(body.turnId);if(turn.status==="queued"){turn.status="cancelled";turn.endedAtMs=event.timestampMs;}else turn.status="cancelling";cancelApprovals(body.turnId);break;}
    case "turn_finished": {
      const turn=mutableTurn(body.turnId);turn.status=body.status;turn.error=body.error;turn.endedAtMs=event.timestampMs;next.activeTurnId=null;
      cancelApprovals(body.turnId);
      next.tools={...next.tools};for(const [id,tool]of Object.entries(next.tools))if(tool.turnId===body.turnId&&tool.status==="running")next.tools[id]={...tool,status:"interrupted"};
      break;
    }
    case "read_through":next.metadata.lastReadMessageId=body.messageId;break;
    default:{const exhaustive:never=body;throw new Error(`不支持的事件 ${String(exhaustive)}`);}
  }
  return next;
  function cancelApprovals(turnId:string){
    next.approvals={...next.approvals};for(const [id,approval]of Object.entries(next.approvals))if(approval.turnId===turnId&&["pending","allowed"].includes(approval.status))next.approvals[id]={...approval,status:"cancelled",resolvedAtMs:event.timestampMs};
  }
}
export function latestReadable(view:SessionView):string|null {
  return [...view.messages].reverse().find(message=>message.role==="assistant"&&message.parts.length>0&&view.turns.some(turn=>turn.id===message.turnId&&terminal(turn)))?.id??null;
}
export class SessionReplica {
  private current:SessionView|null=null;
  private pending=new Map<number,SessionEnvelope>();
  private bytes=0;
  needsSnapshot=false;
  constructor(readonly key:SessionKey){}
  get view(){return this.current;}
  get after(){return this.current?.lastSeq??0;}
  get hasGap(){return this.pending.size>0;}
  snapshot(value:SessionView){
    if(!sameSession(value.metadata.key,this.key))throw new Error("快照不属于当前会话");
    if(!Number.isSafeInteger(value.lastSeq)||value.lastSeq<1)throw new Error("无效快照序号");
    if(this.current&&value.lastSeq<this.current.lastSeq)return;
    this.current=value;this.needsSnapshot=false;
    for(const [seq,event]of this.pending)if(seq<=value.lastSeq){this.pending.delete(seq);this.bytes-=JSON.stringify(event).length;}
    this.drain();
  }
  receive(event:SessionEnvelope){
    if(event.stream!==sessionStream(this.key))return;
    if(event.schemaVersion!==1||!Number.isSafeInteger(event.seq)||event.seq<1)throw new Error("不支持的事件版本或序号");
    if(this.current&&event.seq<=this.current.lastSeq)return;
    if(this.pending.has(event.seq))return;
    const size=JSON.stringify(event).length;
    if(this.pending.size>=512||this.bytes+size>8*1024*1024){this.pending.clear();this.bytes=0;this.needsSnapshot=true;return;}
    this.pending.set(event.seq,event);this.bytes+=size;this.drain();
  }
  private drain(){
    if(!this.current||this.needsSnapshot)return;
    let event;while((event=this.pending.get(this.current.lastSeq+1))){const next=project(this.current,event);this.pending.delete(event.seq);this.bytes-=JSON.stringify(event).length;this.current=next;}
  }
}
