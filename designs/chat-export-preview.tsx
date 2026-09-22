// Local-only visual fixture: production controls, synthetic response, no IPC or model.
import React from "react";
import {createRoot} from "react-dom/client";
import {ChatSessionHeader} from "../src/features/chat/ChatSessionHeader";
import {ChatExportNotice} from "../src/features/chat/ChatExportNotice";
import {useChatExport} from "../src/features/chat/useChatExport";
import type {ChatSession} from "../src/domain/chat";
import fixtures from "../contracts/tauri-contract.json";
import "../src/styles/theme.css";
import "../src/features/chat/ChatPage.css";
const session=fixtures.modelSamples.chatSession as ChatSession;
const api=async()=>({...fixtures.modelSamples.chatExportResult,inProgress:true,warnings:["导出发生在回合运行中，日志仅包含已捕获的完整行。","turn-contract stderr：源日志不可用，未伪造空日志。"]});
function Preview(){
  const state=useChatExport("/project","chat-contract",api);
  return <main style={{maxWidth:600,margin:"40px auto",padding:20,color:"var(--text)",fontFamily:"sans-serif"}}>
    <h1 style={{fontSize:22}}>Loom · 会话导出</h1><p>本地视觉样例，不写文件、不调用 Agent。</p>
    <ChatSessionHeader session={session} useBackend canPromote={false} turnRunning onClearResume={()=>{}} onPromote={()=>{}} onRename={()=>{}} onTitleFromFirstMessage={()=>{}} onToggleFlag={()=>{}} onArchive={()=>{}} onExport={()=>void state.exportSession()} exporting={state.busy}/>
    <ChatExportNotice busy={state.busy} error={state.error} result={state.result} reveal={async()=>{throw new Error("视觉样例不打开真实目录");}}/>
    <p>通过会话菜单测试导出入口。</p>
  </main>;
}
createRoot(document.getElementById("root")!).render(<Preview/>);
