const test=require('node:test');const assert=require('node:assert/strict');const{JSDOM}=require('jsdom');
const dom=new JSDOM('<div id="root"></div>',{url:'http://localhost'});global.window=dom.window;global.document=dom.window.document;global.navigator=dom.window.navigator;global.IS_REACT_ACT_ENVIRONMENT=true;
const React=require('react');const{act}=React;const{createRoot}=require('react-dom/client');const{workbenchClient:client}=require('../../.tmp/test-build/src/api/workbenchClient.js');const{ToolApprovals}=require('../../.tmp/test-build/src/workbench/ToolApprovals.js');
test('remembered approval uses the server-offered scope and exposes explicit clearing',async()=>{
 const original=client.request;const calls=[];let complete;client.request=async command=>{calls.push(command);if(calls.length===1)return new Promise(resolve=>{complete=resolve;});return{};};
 const key={workspaceId:'w',sessionId:'s'},offer={keys:[{kind:'directory',digest:'opaque-selector',label:'同一测试目录内的写入'}]};
 const approval={id:'a',toolName:'Write',preview:{path:'/fixture/file'},bindingDigest:'bound-scope',status:'pending',remember:offer};const base={metadata:{key},approvals:{a:approval}};let changed=0;
 const root=createRoot(document.getElementById('root'));
 try{
  await act(async()=>root.render(React.createElement(ToolApprovals,{view:base,onChange:()=>{changed++;}})));
  assert.match(document.body.textContent,/应用重启后失效/);const remember=[...document.querySelectorAll('button')].find(b=>b.textContent==='此会话中记住');await act(async()=>{remember.click();remember.click();});assert.equal(calls.length,1);assert.deepEqual(calls[0],{method:'remember_tool_approval',params:{key,requestId:'a',expectedDigest:'bound-scope'}});assert.ok(!JSON.stringify(calls[0]).includes('opaque-selector'));
  await act(async()=>complete({...approval,status:'allowed'}));assert.equal(changed,1);
  const granted={...base,approvals:{a:{...approval,status:'consumed'}},rememberedGrants:{a:{id:'a',offer,revokedAtMs:null}}};await act(async()=>root.render(React.createElement(ToolApprovals,{view:granted,onChange:()=>{changed++;}})));
  const clear=[...document.querySelectorAll('button')].find(b=>b.textContent==='清除会话授权');assert.ok(clear);await act(async()=>clear.click());assert.deepEqual(calls[1],{method:'clear_remembered_grants',params:{key}});
  await act(async()=>root.render(React.createElement(ToolApprovals,{view:{...base,approvals:{a:{...approval,remember:undefined}}},onChange(){}})));
  assert.ok(![...document.querySelectorAll('button')].some(b=>b.textContent==='此会话中记住'));
 }finally{await act(async()=>root.unmount());client.request=original;dom.window.close();}
});
