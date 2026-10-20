const test=require('node:test');const assert=require('node:assert/strict');const{JSDOM}=require('jsdom');
const dom=new JSDOM('<div id="root"></div>',{url:'http://localhost'});global.window=dom.window;global.document=dom.window.document;global.navigator=dom.window.navigator;global.IS_REACT_ACT_ENVIRONMENT=true;
const React=require('react');const{act}=React;const{createRoot}=require('react-dom/client');
const{workbenchClient:client}=require('../../.tmp/test-build/src/api/workbenchClient.js');const{ToolApprovals}=require('../../.tmp/test-build/src/workbench/ToolApprovals.js');
test('tool decisions bind the displayed request, prevent duplicate clicks and cannot update another session',async()=>{
 const original=client.request;const calls=[];let resolve;const pending=new Promise(r=>{resolve=r;});let refreshes=0;
 client.request=async command=>{calls.push(command);return pending;};
 const approval={id:'approval',turnId:'turn',toolCallId:'tool',toolName:'Write',preview:{file_path:'file.txt',content:'[withheld]'},bindingDigest:'exact-binding',status:'pending',workingDirectory:'/fixture'};
 const view=id=>({metadata:{key:{workspaceId:'w',sessionId:id}},approvals:{approval}});
 const root=createRoot(document.getElementById('root'));
 try{
  await act(async()=>root.render(React.createElement(ToolApprovals,{key:'A',view:view('A'),onChange:()=>{refreshes++;}})));
  const allow=[...document.querySelectorAll('button')].find(button=>button.textContent==='允许一次');await act(async()=>{allow.click();allow.click();});assert.equal(calls.length,1);assert.deepEqual(calls[0],{method:'respond_tool_approval',params:{key:{workspaceId:'w',sessionId:'A'},requestId:'approval',expectedDigest:'exact-binding',allowed:true}});
  await act(async()=>root.render(React.createElement(ToolApprovals,{key:'B',view:view('B'),onChange:()=>{refreshes++;}})));
  await act(async()=>resolve({...approval,status:'allowed'}));assert.equal(refreshes,0);assert.ok([...document.querySelectorAll('button')].some(button=>button.textContent==='拒绝'&&!button.disabled));
  await act(async()=>root.render(React.createElement(ToolApprovals,{key:'B',view:{...view('B'),approvals:{approval:{...approval,status:'cancelled'}}},onChange:()=>{refreshes++;}})));assert.match(document.body.textContent,/已撤销/);assert.equal(document.querySelector('button'),null);
 }finally{await act(async()=>root.unmount());client.request=original;dom.window.close();}
});
