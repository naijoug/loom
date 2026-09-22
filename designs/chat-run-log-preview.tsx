// Local visual fixture for the production component; never calls a CLI or IPC.
import React from "react";
import { createRoot } from "react-dom/client";
import { ChatRunDetails } from "../src/features/chat/ChatRunDetails";
import type { ChatTurn } from "../src/domain/chat";
import fixture from "../contracts/tauri-contract.json";
import "../src/styles/theme.css";
import "../src/features/chat/ChatPage.css";

const turn: ChatTurn = { ...fixture.modelSamples.chatTurn, status: "failed", invocation: { ...fixture.modelSamples.chatTurn.invocation, permissionMode: "explore" },
  acceptedAtMs: Date.now()-4600, startedAtMs: Date.now()-4200, finishedAtMs: Date.now(), exitCode:7,
  errorSummary:"Agent 退出码为 7，已保留部分输出。", terminationReason:"exit status: 7" };
const readLogs = async (_project:string,_session:string,_turn:string,stream:"stdout"|"stderr",offset=0) => {
  const first=stream==="stdout" ? "开始分析项目…\n读取 src/main.rs 完成\napi_key=[REDACTED]\n" : "诊断：测试命令未通过\nAuthorization: [REDACTED]\n";
  const second="后续日志：部分输出在异常退出后仍可查看。\n";
  return offset===0 ? {text:first,nextOffset:new TextEncoder().encode(first).length,hasMore:true} : {text:second,nextOffset:offset+new TextEncoder().encode(second).length,hasMore:false};
};
createRoot(document.getElementById("root")!).render(<main style={{maxWidth:window.location.search.includes("narrow") ? 360 : 760,margin:"40px auto",padding:24,fontFamily:"sans-serif",color:"var(--text)",background:"var(--bg)"}}>
  <h1 style={{fontSize:22}}>Loom · 执行记录</h1><p>本地视觉样例，使用生产组件和合成日志，不调用 Agent。</p>
  <article className="chat-turn"><header className="chat-turn-header">Agent</header><p>已完成项目检查，但测试命令失败。展开执行记录查看配置、退出状态和日志。</p>
    <ChatRunDetails projectPath="/project" sessionId="chat-contract" turn={turn} readLogs={readLogs}/>
  </article>
</main>);
