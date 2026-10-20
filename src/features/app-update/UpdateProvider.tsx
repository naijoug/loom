import { useT } from "../../i18n";
import { createContext, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from "react";
import { Download, RefreshCw, X } from "lucide-react";
import { invokeCommand, listenToEvent } from "../../api/tauriClient";
import { hasTauriRuntime } from "../../hooks/runtime";
import { downloadPercent, initialUpdate, newerSnapshot, updateBusy, updateLabel, updateOffered, type UpdateDrafts, type UpdateSnapshot } from "./model";
import "./update.css";

interface UpdateContextValue {
  state: UpdateSnapshot;
  open(): void;
  initialDrafts: UpdateDrafts | null;
  setDraft(scope: string, data: Record<string,string>): void;
  getDraft(scope: string): Record<string,string> | undefined;
}
const Context = createContext<UpdateContextValue | null>(null);
export function UpdateProvider({ children }: { children: ReactNode }) {
  const t = useT();
  const [state,setState] = useState(initialUpdate);
  const [visible,setVisible] = useState(false);
  const [localError,setLocalError] = useState("");
  const [busy,setBusy] = useState(false);
  const [initialDrafts,setInitialDrafts] = useState<UpdateDrafts|null>(null);
  const drafts = useRef<UpdateDrafts>({});
  const timer = useRef<ReturnType<typeof setTimeout>>();
  const writes = useRef<Promise<unknown>>(Promise.resolve());
  const draftError = useRef("");
  const mounted = useRef(true);
  const lock = useRef(false);
  const apply = useCallback((incoming:UpdateSnapshot)=>setState(current=>newerSnapshot(current,incoming)),[]);
  const persist = useCallback(()=> {
    const value = structuredClone(drafts.current);
    writes.current = writes.current.catch(()=>{}).then(async()=>{
      if (!hasTauriRuntime()) return;
      try { await invokeCommand("save_update_drafts",{drafts:value}); draftError.current=""; }
      catch { draftError.current=t("updateDraftFailure"); }
    });
  },[]);
  const getDraft = useCallback((scope:string)=>drafts.current[scope],[]);
  const setDraft = useCallback((scope:string,data:Record<string,string>)=>{
    if (JSON.stringify(drafts.current[scope])===JSON.stringify(data)) return;
    drafts.current={...drafts.current,[scope]:data};
    clearTimeout(timer.current); timer.current=setTimeout(persist,500);
  },[persist]);
  useEffect(()=>{
    mounted.current=true;
    let disposed=false;
    let unlisten:(()=>void)|undefined;
    if (!hasTauriRuntime()) { setInitialDrafts({}); return; }
    void (async()=>{
      try {
        const stop=await listenToEvent<UpdateSnapshot>("loom-update-state",apply);
        if(disposed){stop();return;} unlisten=stop;
        const [snapshot,saved]=await Promise.all([
          invokeCommand<UpdateSnapshot>("get_update_state"), invokeCommand<UpdateDrafts>("load_update_drafts")
        ]);
        if(disposed)return;
        apply(snapshot); drafts.current=saved; setInitialDrafts(saved);
      } catch { if(!disposed)setLocalError(t("updateLoadFailure")); }
    })();
    return()=>{disposed=true;mounted.current=false;unlisten?.();clearTimeout(timer.current);};
  },[apply]);
  const action=async(kind:"check"|"download"|"install")=>{
    if(lock.current)return;
    lock.current=true;setBusy(true);setLocalError("");
    try {
      if(kind==="install"){
        if(initialDrafts===null)throw new Error(t("updateDraftPending"));
        clearTimeout(timer.current);persist();await writes.current;
        if(draftError.current)throw new Error(draftError.current);
        await invokeCommand("install_update",{drafts:drafts.current});
      } else apply(await invokeCommand<UpdateSnapshot>(kind==="check"?"check_for_updates":"download_update"));
    } catch(error) { if(mounted.current)setLocalError(String(error instanceof Error?error.message:error)); }
    finally{lock.current=false;if(mounted.current)setBusy(false);}
  };
  const frozen=state.phase==="installing"||state.phase==="restarting"||(busy&&state.phase==="ready");
  return <Context.Provider value={{state,open:()=>setVisible(true),initialDrafts,setDraft,getDraft}}>{children}{visible&&<UpdateDialog state={state} error={localError||state.error||""} busy={busy||updateBusy(state.phase)} frozen={frozen} onClose={()=>setVisible(false)} onAction={action}/>}</Context.Provider>;
}
export function UpdateButton({manual=false}:{manual?:boolean}){
  const t = useT();
  const context=useContext(Context);
  if(!context)return null;
  const offered=updateOffered(context.state.phase);
  if(!manual&&!offered)return null;
  return <button type="button" className={`loom-update-button ${offered?"has-update":""} ${manual?"manual":""}`} aria-label={updateLabel(context.state, t)} title={updateLabel(context.state, t)} onClick={context.open}>
    {offered?<Download size={15}/>:<RefreshCw size={15}/>} {manual&&<span>{offered?updateLabel(context.state, t):t("checkUpdates")}</span>}
  </button>;
}
export function useUpdateDrafts(scope:string,data:Record<string,string>,restore:(saved:Record<string,string>)=>void){
  const context=useContext(Context);
  const restored=useRef(false);
  const restoreRef=useRef(restore); restoreRef.current=restore;
  const serialized=JSON.stringify(data);
  useEffect(()=>{
    if(!context?.initialDrafts||restored.current)return;
    restored.current=true;
    const saved=context.getDraft(scope);
    if(saved)restoreRef.current(saved);
  },[context?.initialDrafts,scope]);
  useEffect(()=>{
    if(context?.initialDrafts&&restored.current)context.setDraft(scope,JSON.parse(serialized));
  },[context?.initialDrafts,context?.setDraft,scope,serialized]);
}
export function UpdateDialog({state,error,busy,frozen,onClose,onAction}:{state:UpdateSnapshot;error:string;busy:boolean;frozen:boolean;onClose():void;onAction(kind:"check"|"download"|"install"):Promise<void>}){
  const ref=useRef<HTMLDialogElement>(null);
  useEffect(()=>{const dialog=ref.current;dialog?.showModal();return()=>dialog?.close();},[]);
  const t = useT();
  const percent=downloadPercent(state);
  const disabled=state.phase==="disabled";
  return <dialog ref={ref} className="loom-update-dialog" aria-labelledby="loom-update-title" onCancel={event=>{event.preventDefault();if(!frozen)onClose();}}>
    <header><div className="loom-update-mark"><Download size={23}/></div><div><h2 id="loom-update-title">{t("softwareUpdate")}</h2><p>{state.currentVersion?t("currentVersion", { version: state.currentVersion }):t("inAppUpdate")}</p></div><button aria-label={t("closeUpdate")}  disabled={frozen} onClick={onClose}><X size={18}/></button></header>
    <div role="status" aria-live="polite" className="loom-update-status">{disabled?t("updateUnavailable"):updateLabel(state, t)}</div>
    {state.notes&&<section aria-label={t("releaseNotes")}  className="loom-update-notes">{state.notes}</section>}
    {state.phase==="downloading"&&<progress aria-label={t("downloadProgress")}  max={100} value={percent}/>}
    {state.phase==="ready"&&<p className="loom-update-hint">{t("updateRestartHint")}</p>}
    {disabled&&<p className="loom-update-hint">{t("updateBuildHint")}</p>}
    {error&&<p role="alert" className="loom-update-error">{error}</p>}
    <footer><button disabled={frozen} onClick={onClose}>{t("later")}</button>{state.phase==="available"?<button className="primary" disabled={busy} onClick={()=>void onAction("download")}>{t("downloadUpdate")}</button>:state.phase==="ready"?<button className="primary" disabled={busy} onClick={()=>void onAction("install")}>{t("installRestart")}</button>:<button className="primary" disabled={busy||disabled} onClick={()=>void onAction("check")}>{busy?updateLabel(state, t):t("checkUpdates")}</button>}</footer>
  </dialog>;
}
