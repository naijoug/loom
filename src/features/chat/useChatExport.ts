import { useCallback, useEffect, useRef, useState } from "react";
import { chatExport } from "../../api/chatClient";
import type { ChatExportResult } from "../../domain/chat";

export function useChatExport(projectPath: string | null, sessionId: string | null, api = chatExport) {
  const owner = JSON.stringify([projectPath, sessionId]);
  const current = useRef(owner); current.current = owner;
  const generation = useRef(0);
  const pending = useRef<{owner:string; request:number}|null>(null);
  const [state, setState] = useState<{ owner: string; busy: boolean; result: ChatExportResult | null; error: string | null }>({owner,busy:false,result:null,error:null});
  useEffect(() => {
    setState({owner,busy:false,result:null,error:null});
    return () => { generation.current++; };
  }, [owner]);
  const exportSession = useCallback(async () => {
    if (!projectPath || !sessionId || (pending.current?.owner === owner && pending.current.request === generation.current)) return;
    const request = ++generation.current;
    pending.current = {owner,request};
    setState({owner,busy:true,result:null,error:null});
    try {
      const result = await api({projectPath,sessionId});
      if (current.current === owner && generation.current === request) setState({owner,busy:false,result,error:null});
    } catch (error) {
      if (current.current === owner && generation.current === request) setState({owner,busy:false,result:null,error:String(error)});
    } finally { if (pending.current?.request === request) pending.current = null; }
  }, [projectPath,sessionId,owner,api]);
  return { ...(state.owner === owner ? state : {busy:false,result:null,error:null}), exportSession };
}
