import type { SessionKey, SessionView, WatchMessage } from "../../packages/protocol/src/workbench";
import type { WorkbenchClient, WatchHandle } from "../api/workbenchClient";
import { SessionReplica } from "./projection";
export interface SessionState { view:SessionView|null; error:string|null; loading:boolean }
export class SessionController {
  readonly replica:SessionReplica;
  private state:SessionState={view:null,error:null,loading:true};
  private listeners=new Set<()=>void>();
  private handle:WatchHandle|null=null;
  private active=false;
  private generation=0;
  private syncing:Promise<void>|null=null;
  private watching=false;
  constructor(readonly key:SessionKey,private client:WorkbenchClient){this.replica=new SessionReplica(key);}
  getSnapshot=()=>this.state;
  subscribe=(listener:()=>void)=>{this.listeners.add(listener);return()=>{this.listeners.delete(listener);};};
  private current(generation:number){return this.active&&generation===this.generation;}
  private publish(error:string|null=null){if(!this.active)return;this.state={view:this.replica.view,error,loading:!this.replica.view};for(const listener of this.listeners)listener();}
  async start(){if(this.active)return;this.active=true;const generation=++this.generation;await this.attach(generation);}
  private async attach(generation:number){
    if(!this.current(generation)||this.watching||this.handle)return;this.watching=true;
    try{
      let invalidated=false;
      const handle=await this.client.watch(this.key,message=>{if(message.kind==="resync_required")invalidated=true;this.receive(message,generation);});
      if(!this.current(generation)){await handle.close();return;}
      if(invalidated){await handle.close();this.watching=false;void this.sync();return;}
      this.handle=handle;this.replica.snapshot(handle.snapshot);this.publish();
      if(this.replica.hasGap||this.replica.needsSnapshot)void this.sync();
    }catch(error){if(this.current(generation))this.publish(String(error));}
    finally{if(this.current(generation))this.watching=false;}
  }
  private receive(message:WatchMessage,generation:number){
    if(!this.current(generation))return;
    try{
      if(message.kind==="resync_required"){
        const previous=this.handle;this.handle=null;if(previous)void previous.close().catch(()=>{});
        void this.sync();return;
      }
      if(message.kind!=="event")return;
      this.replica.receive(message.event);this.publish();if(this.replica.hasGap||this.replica.needsSnapshot)void this.sync();
    }catch(error){this.replica.needsSnapshot=true;this.publish(String(error));void this.sync();}
  }
  sync():Promise<void>{
    if(!this.active)return Promise.resolve();if(this.syncing)return this.syncing;
    const generation=this.generation;
    const operation=this.reconcile(generation).finally(()=>{if(this.syncing===operation)this.syncing=null;});
    this.syncing=operation;return operation;
  }
  private async reconcile(generation:number){
    try{
      if(!this.replica.view||this.replica.needsSnapshot){const view=await this.client.request({method:"get_session",params:{key:this.key}},"session");if(!this.current(generation))return;this.replica.snapshot(view);}
      for(let pageCount=0;pageCount<20&&this.current(generation);pageCount++){
        const page=await this.client.request({method:"read_events",params:{key:this.key,after:this.replica.after,limit:500}},"events");if(!this.current(generation))return;
        const before=this.replica.after;for(const event of page.events)this.replica.receive(event);
        if(this.replica.needsSnapshot||this.replica.hasGap&&this.replica.after===before){const view=await this.client.request({method:"get_session",params:{key:this.key}},"session");if(!this.current(generation))return;this.replica.snapshot(view);break;}
        if(!page.hasMore)break;
      }
      if(this.current(generation)){this.publish();void this.attach(generation);}
    }catch(error){if(this.current(generation))this.publish(String(error));}
  }
  async dispose(){this.active=false;++this.generation;this.watching=false;this.syncing=null;const handle=this.handle;this.handle=null;if(handle)await handle.close();}
}
