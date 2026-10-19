import { createChannel, invokeCommand } from "./tauriClient";
import { TAURI_COMMANDS, type TauriCommand } from "./contract";
import type { CoreCommand, CoreReply, CoreResponse, HostCapabilities, SessionKey, SessionView, WatchMessage, SaveConnectionRequest, ModelConnection, SavePermissionRequest, PermissionDocument, PermissionSettings, PermissionScope } from "../../packages/protocol/src/workbench";
export type { CoreCommand, SessionKey, SessionView } from "../../packages/protocol/src/workbench";

export function permissionScopeKey(scope:PermissionScope):string {return scope.kind==="application"?"application":scope.kind==="workspace"?`workspace/${scope.workspaceId}`:`source/${scope.workspaceId}/${scope.sourceId}`;}

type ResponseMap = { [Response in CoreResponse as Response["type"]]: Response["data"] };
type Invoke = (command: TauriCommand, args?: Record<string, unknown>) => Promise<unknown>;
type ChannelFactory = (handler: (message: WatchMessage) => void) => unknown;
export interface WatchHandle { snapshot: SessionView; close(): Promise<void> }
export class WorkbenchError extends Error { constructor(readonly code: string, message: string) { super(message); } }
export function createWorkbenchClient(invoke: Invoke, channel: ChannelFactory, id: () => string = () => crypto.randomUUID()) {
  return {
    async permissionSettings(workspaceId:string):Promise<PermissionSettings>{
      const value=await invoke(TAURI_COMMANDS.workbenchPermissions,{workspaceId}) as PermissionSettings;
      if(!value||!Array.isArray(value.documents)||!value.applicationDefaults||typeof value.applicationDefaults!=="object")throw new WorkbenchError("protocol","权限配置读取响应无效");
      return value;
    },
    async savePermissions(request:SavePermissionRequest):Promise<PermissionDocument>{
      const value=await invoke(TAURI_COMMANDS.workbenchSavePermissions,{request}) as PermissionDocument;
      if(!value||value.operationId!==request.operationId||permissionScopeKey(value.scope)!==permissionScopeKey(request.scope)||!Number.isSafeInteger(value.revision)||value.revision<1)throw new WorkbenchError("protocol","权限配置保存响应不匹配");
      return value;
    },
    async saveConnection(request:SaveConnectionRequest):Promise<ModelConnection>{
      const value=await invoke(TAURI_COMMANDS.workbenchSaveConnection,{request}) as ModelConnection;
      if(!value||value.id!==request.id||value.operationId!==request.operationId||!Number.isSafeInteger(value.revision)||value.revision<1)throw new WorkbenchError("protocol","模型连接保存响应不匹配");
      return value;
    },
    async request<T extends CoreResponse["type"]>(command: CoreCommand, expected: T): Promise<ResponseMap[T]> {
      const requestId = id();
      const raw = await invoke(TAURI_COMMANDS.workbenchInvoke, {request:{protocolVersion:1,requestId,command}});
      if (!raw || typeof raw !== "object") throw new WorkbenchError("protocol", "无效的工作区响应");
      const reply = raw as CoreReply;
      if (reply.protocolVersion !== 1 || reply.requestId !== requestId) throw new WorkbenchError("protocol", "工作区响应归属不匹配");
      if (reply.status === "failure") throw new WorkbenchError(reply.error.code, reply.error.message);
      if (reply.status !== "success" || reply.result.type !== expected) throw new WorkbenchError("protocol", "工作区响应类型不匹配");
      return reply.result.data as ResponseMap[T];
    },
    async capabilities(): Promise<HostCapabilities> {
      const result = await invoke(TAURI_COMMANDS.workbenchCapabilities) as HostCapabilities;
      if (result.protocolVersion !== 1) throw new WorkbenchError("protocol", "不支持的工作区版本"); return result;
    },
    async watch(key: SessionKey, onMessage: (message: WatchMessage) => void): Promise<WatchHandle> {
      const watchId = id();
      let closed = false;
      const close = async () => { if (closed) return; closed = true; await invoke(TAURI_COMMANDS.workbenchUnwatch,{watchId}); };
      try {
        const snapshot = await invoke(TAURI_COMMANDS.workbenchWatch,{key,watchId,onEvent:channel(message => { if (!closed) onMessage(message); })}) as SessionView;
        return { snapshot, close };
      } catch(error) { try { await close(); } catch { /* A destroyed window also drops its watches. */ } throw error; }
    },
    async watchWorkspace(workspaceId:string,onChange:()=>void):Promise<{close():Promise<void>}>{
      const watchId=id();let closed=false;
      const close=async()=>{if(closed)return;closed=true;await invoke(TAURI_COMMANDS.workbenchUnwatch,{watchId});};
      try{await invoke(TAURI_COMMANDS.workbenchWatchWorkspace,{workspaceId,watchId,onEvent:channel(message=>{if(!closed&&(message.kind==="resync_required"||message.kind==="workspace_changed"&&message.notice.workspaceId===workspaceId))onChange();})});return{close};}
      catch(error){try{await close();}catch{/* Window destruction also removes subscriptions. */}throw error;}
    },
    async resume(key: SessionKey) { await invoke(TAURI_COMMANDS.workbenchResumeQueue,{key}); },
  };
}
export type WorkbenchClient = ReturnType<typeof createWorkbenchClient>;
export const workbenchClient = createWorkbenchClient(invokeCommand, handler => createChannel(handler));
