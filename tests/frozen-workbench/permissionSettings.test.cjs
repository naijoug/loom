require.extensions['.css']=()=>undefined;
const test=require('node:test');const assert=require('node:assert/strict');const{JSDOM}=require('jsdom');
const dom=new JSDOM('<div id="root"></div>',{url:'http://localhost'});global.window=dom.window;global.document=dom.window.document;global.navigator=dom.window.navigator;global.IS_REACT_ACT_ENVIRONMENT=true;
dom.window.HTMLDialogElement.prototype.showModal=function(){this.open=true;};dom.window.HTMLDialogElement.prototype.close=function(){this.open=false;};
const React=require('react');const{act}=React;const{createRoot}=require('react-dom/client');const{workbenchClient:client}=require('../../.tmp/test-build/src/api/workbenchClient.js');const{PermissionSettings}=require('../../.tmp/test-build/src/workbench/PermissionSettings.js');
function input(value){const node=document.querySelector('[aria-label="权限规则 JSON"]');Object.getOwnPropertyDescriptor(dom.window.HTMLTextAreaElement.prototype,'value').set.call(node,value);node.dispatchEvent(new dom.window.Event('input',{bubbles:true}));}
async function submit(){await act(async()=>document.querySelector('form').dispatchEvent(new dom.window.Event('submit',{bubbles:true,cancelable:true})));}
test('permission editor preserves retry identity, revision conflicts and scoped reset',async()=>{
 const original={...client};const scope={kind:'workspace',workspaceId:'w'};let revision=3;const calls=[];let fail=true;
 client.permissionSettings=async()=>({applicationDefaults:{allowedBashPatterns:['^ls']},documents:[{scope,revision,operationId:'old',config:{allowedWritePaths:['/old/**']},updatedAtMs:1}]});
 client.savePermissions=async request=>{calls.push(request);if(fail){fail=false;throw new Error('uncertain save');}return{...request,revision:++revision,updatedAtMs:2};};
 const root=createRoot(document.getElementById('root'));
 try{
  await act(async()=>root.render(React.createElement(PermissionSettings,{workspaceId:'w',onClose(){}})));
  await act(async()=>input('{invalid'));await submit();assert.equal(calls.length,0);assert.ok(document.querySelector('[role="alert"]'));
  await act(async()=>input('{"allowedWritePaths":["/new/**"]}'));await submit();assert.equal(calls.length,1);assert.equal(calls[0].expectedRevision,3);await submit();assert.equal(calls[1].operationId,calls[0].operationId);assert.ok(document.querySelector('[role="status"]').textContent.includes('新回合'));
  const reset=[...document.querySelectorAll('button')].find(b=>b.textContent==='恢复此范围默认规则');await act(async()=>reset.click());assert.equal(calls[2].config,null);assert.equal(calls[2].expectedRevision,4);assert.notEqual(calls[2].operationId,calls[1].operationId);
  const select=document.querySelector('[aria-label="权限配置范围"]');await act(async()=>{select.value='application';select.dispatchEvent(new dom.window.Event('change',{bubbles:true}));});assert.deepEqual(JSON.parse(document.querySelector('textarea').value),{allowedBashPatterns:['^ls']});await submit();assert.equal(calls[3].scope.kind,'application');assert.equal(calls[3].expectedRevision,null);
 }finally{await act(async()=>root.unmount());Object.assign(client,original);dom.window.close();}
});
