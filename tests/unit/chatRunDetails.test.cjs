const test = require("node:test");
const assert = require("node:assert/strict");
const React = require("react");
const { act } = React;
const { createRoot } = require("react-dom/client");
const { JSDOM } = require("jsdom");
const { ChatRunDetails } = require("../../.tmp/test-build/src/features/chat/ChatRunDetails.js");
const sample = require("../../contracts/tauri-contract.json").modelSamples.chatTurn;
global.IS_REACT_ACT_ENVIRONMENT = true;
async function fixture(t) {
  const dom = new JSDOM("<div id='root'></div>");
  global.window=dom.window;global.document=dom.window.document;global.navigator=dom.window.navigator;
  const root=createRoot(document.getElementById("root"));
  t.after(async()=>{await act(async()=>root.unmount());dom.window.close();});
  return {render:async(props)=>act(async()=>root.render(React.createElement(ChatRunDetails,props)))};
}
const click = async(label) => act(async()=>[...document.querySelectorAll("button")].find(b=>b.textContent.includes(label)).click());

test("production run viewer shows immutable invocation, exit reason, separate streams and paginated offsets",async(t)=>{
  const f=await fixture(t);const calls=[];
  const readLogs=async(...args)=>{calls.push(args);return args[3]==="stderr" ? {text:"error stream",nextOffset:12,hasMore:false} : args[4]===0 ? {text:"first 中",nextOffset:9,hasMore:true} : {text:"<script>inert</script>",nextOffset:31,hasMore:false};};
  await f.render({projectPath:"/a",sessionId:"s",turn:{...sample,status:"failed",exitCode:7,terminationReason:"exit status: 7",errorSummary:"agent failed"},readLogs});
  assert.match(document.body.textContent,/失败/);assert.equal(calls.length,0);
  await click("执行记录");
  assert.match(document.body.textContent,/grok/);assert.match(document.body.textContent,/exit status: 7/);
  assert.equal(document.querySelector('[aria-label="stdout 日志"]').textContent,"first 中");
  await click("加载更多");assert.equal(calls[1][4],9);
  assert.match(document.querySelector('[aria-label="stdout 日志"]').textContent,/<script>inert<\/script>/);
  assert.equal(document.querySelector("script"),null);
  await click("stderr");
  assert.equal(document.querySelector('[aria-label="stderr 日志"]').textContent,"error stream");
  assert.equal(calls[2][3],"stderr");assert.equal(calls[2][4],0);
});

test("late log responses never replace a different stream/session and errors stay visible",async(t)=>{
  const f=await fixture(t);let resolveOld;
  const old=new Promise(r=>{resolveOld=r;});
  const readLogs=async(project,session,turn,stream)=> session==="a" && stream==="stdout" ? old : session==="b" ? Promise.reject(new Error("log unavailable")) : {text:"current stderr",nextOffset:14,hasMore:false};
  await f.render({projectPath:"/p",sessionId:"a",turn:sample,readLogs});
  await click("执行记录");await click("stderr");
  await act(async()=>resolveOld({text:"OLD STDOUT",nextOffset:10,hasMore:false}));
  assert.doesNotMatch(document.body.textContent,/OLD STDOUT/);
  await f.render({projectPath:"/p",sessionId:"b",turn:{...sample,id:"other-turn"},readLogs});
  assert.match(document.querySelector('[role="alert"]').textContent,/log unavailable/);
  assert.doesNotMatch(document.body.textContent,/current stderr/);
});
