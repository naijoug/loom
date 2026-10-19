// Development-only visual harness. Uses the production component; no native IPC.
import React, {useState} from "react";
import {createRoot} from "react-dom/client";
import {UpdateDialog} from "../../src/features/app-update/UpdateProvider";
import {initialUpdate, type UpdateSnapshot} from "../../src/features/app-update/model";
function Preview(){
 const [state,setState]=useState<UpdateSnapshot>({...initialUpdate,currentVersion:"0.1.0",version:"0.2.0",phase:"available",notes:"新增应用内更新\n\n• 后台发现新版本，侧栏显示更新提示\n• 下载后校验签名，任务结束后再安装\n• 更新前保存对话草稿"});
 const [open,setOpen]=useState(true);
 return <><div style={{padding:28}}>Loom · 更新组件验收（模拟数据，不安装软件）<button onClick={()=>setOpen(true)}>打开更新</button></div>{open&&<UpdateDialog state={state} error="" busy={false} frozen={false} onClose={()=>setOpen(false)} onAction={async()=>setState({...state,phase:"ready"})}/>}</>;
}
createRoot(document.getElementById("root")!).render(<Preview/>);
