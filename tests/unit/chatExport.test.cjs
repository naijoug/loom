const test=require("node:test");
const assert=require("node:assert/strict");
const React=require("react");const {act}=React;const {createRoot}=require("react-dom/client");const {JSDOM}=require("jsdom");
const {useChatExport}=require("../../.tmp/test-build/src/features/chat/useChatExport.js");
const {ChatExportNotice}=require("../../.tmp/test-build/src/features/chat/ChatExportNotice.js");
const {ChatSessionHeader}=require("../../.tmp/test-build/src/features/chat/ChatSessionHeader.js");
const samples=require("../../contracts/tauri-contract.json").modelSamples;
global.IS_REACT_ACT_ENVIRONMENT=true;
function deferred(){let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b;});return{promise,resolve,reject};}
async function fixture(t){const dom=new JSDOM("<div id='root'></div>");global.window=dom.window;global.document=dom.window.document;global.navigator=dom.window.navigator;const root=createRoot(document.getElementById("root"));t.after(async()=>{await act(async()=>root.unmount());dom.window.close();});return{render:async(element)=>act(async()=>root.render(element))};}

test("export menu invokes production hook once and shows warnings plus an explicit folder action",async(t)=>{
  const f=await fixture(t);const response=deferred();const calls=[];const reveals=[];let latest;
  const api=async(input)=>{calls.push(input);return response.promise;};
  function Harness(){latest=useChatExport("/project","chat-contract",api);return React.createElement(React.Fragment,null,
    React.createElement(ChatSessionHeader,{session:samples.chatSession,useBackend:true,canPromote:false,onClearResume(){},onPromote(){},onRename(){},onTitleFromFirstMessage(){},onToggleFlag(){},onArchive(){},onExport:()=>void latest.exportSession(),exporting:latest.busy}),
    React.createElement(ChatExportNotice,{...latest,reveal:async(path)=>{reveals.push(path);}}));}
  await f.render(React.createElement(Harness));
  await act(async()=>document.querySelector('[aria-haspopup="menu"]').click());
  await act(async()=>[...document.querySelectorAll('[role="menuitem"]')].find(b=>b.textContent.includes("导出会话")).click());
  await act(async()=>{void latest.exportSession();});
  assert.equal(calls.length,1);assert.deepEqual(calls[0],{projectPath:"/project",sessionId:"chat-contract"});
  assert.match(document.body.textContent,/正在导出/);assert.equal(reveals.length,0);
  await act(async()=>response.resolve({...samples.chatExportResult,inProgress:true,warnings:["stderr 缺失"]}));
  assert.match(document.body.textContent,/运行中快照/);assert.match(document.body.textContent,/stderr 缺失/);
  await act(async()=>[...document.querySelectorAll("button")].find(b=>b.textContent==="在文件夹中显示").click());
  assert.deepEqual(reveals,[samples.chatExportResult.directory]);
});

test("late export responses cannot overwrite a new session, and returning to the old session is not stuck busy",async(t)=>{
  const f=await fixture(t);const a=deferred();let latest;let count=0;
  const api=async(input)=>{count++;return input.sessionId==="a"&&count===1 ? a.promise : {...samples.chatExportResult,directory:input.sessionId};};
  function Harness({id}){latest=useChatExport("/project",id,api);return React.createElement(ChatExportNotice,{...latest});}
  await f.render(React.createElement(Harness,{id:"a"}));await act(async()=>{void latest.exportSession();});
  await f.render(React.createElement(Harness,{id:"b"}));await act(async()=>latest.exportSession());
  await act(async()=>a.resolve({...samples.chatExportResult,directory:"OLD A"}));
  assert.equal(latest.result.directory,"b");assert.doesNotMatch(document.body.textContent,/OLD A/);
  await f.render(React.createElement(Harness,{id:"a"}));assert.equal(latest.busy,false);assert.equal(latest.result,null);
  await act(async()=>latest.exportSession());assert.equal(latest.result.directory,"a");
});

test("export and folder errors stay visible and export can be retried",async(t)=>{
  const f=await fixture(t);let latest;let fail=true;
  const api=async()=>{if(fail)throw new Error("disk full");return samples.chatExportResult;};
  function Harness(){latest=useChatExport("/p","s",api);return React.createElement(ChatExportNotice,{...latest,reveal:async()=>{throw new Error("folder denied");}});}
  await f.render(React.createElement(Harness));await act(async()=>latest.exportSession());
  assert.match(document.querySelector('[role="alert"]').textContent,/disk full/);assert.equal(latest.busy,false);
  fail=false;await act(async()=>latest.exportSession());assert.equal(latest.error,null);
  await act(async()=>document.querySelector("button").click());assert.match(document.querySelector('[role="alert"]').textContent,/folder denied/);
});
