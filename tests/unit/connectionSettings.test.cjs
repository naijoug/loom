const test=require('node:test');const assert=require('node:assert/strict');const{JSDOM}=require('jsdom');
const dom=new JSDOM('<div id="root"></div>',{url:'http://localhost'});global.window=dom.window;global.document=dom.window.document;global.navigator=dom.window.navigator;global.IS_REACT_ACT_ENVIRONMENT=true;
dom.window.HTMLDialogElement.prototype.showModal=function(){this.open=true;};dom.window.HTMLDialogElement.prototype.close=function(){this.open=false;};
const React=require('react');const{act}=React;const{createRoot}=require('react-dom/client');
const{workbenchClient:client}=require('../../.tmp/test-build/src/api/workbenchClient.js');const{ConnectionSettings}=require('../../.tmp/test-build/src/workbench/ConnectionSettings.js');
function input(label,value){const node=document.querySelector(`[aria-label="${label}"]`);const proto=node.tagName==='TEXTAREA'?dom.window.HTMLTextAreaElement.prototype:dom.window.HTMLInputElement.prototype;Object.getOwnPropertyDescriptor(proto,'value').set.call(node,value);node.dispatchEvent(new dom.window.Event('input',{bubbles:true}));}
function submit(){document.querySelector('form').dispatchEvent(new dom.window.Event('submit',{bubbles:true,cancelable:true}));}
test('connection form retries the same operation, clears secret on success and edits without echoing it',async()=>{
 const original=client.saveConnection;const requests=[];let saved;let refreshes=0;
 client.saveConnection=async request=>{requests.push(structuredClone(request));if(requests.length===1)throw new Error('temporary transport failure');saved={id:request.id,operationId:request.operationId,revision:(request.expectedRevision??0)+1,createdAtMs:1,updatedAtMs:2,config:request.config,credentialRef:'opaque-slot'};return saved;};
 const root=createRoot(document.getElementById('root'));
 const render=connections=>root.render(React.createElement(ConnectionSettings,{connections,credentialStore:'macos_keychain',onClose:()=>{},onSaved:()=>{refreshes++;}}));
 try{
  await act(async()=>render([]));await act(async()=>{input('连接名称','Fixture');input('连接凭据','synthetic-private-fixture');input('模型 ID 列表','model-a\nmodel-b');});
  await act(async()=>submit());assert.match(document.body.textContent,/temporary transport failure/);await act(async()=>submit());
  assert.equal(requests[0].operationId,requests[1].operationId);assert.deepEqual(requests[1].credential,{kind:'replace',value:'synthetic-private-fixture'});assert.equal(document.querySelector('[aria-label="连接凭据"]').value,'');assert.equal(refreshes,1);assert.match(document.body.textContent,/尚未验证模型连接/);assert.doesNotMatch(document.body.textContent,/synthetic-private-fixture/);
  await act(async()=>render([saved]));await act(async()=>input('连接名称','Renamed fixture'));await act(async()=>submit());assert.equal(requests[2].expectedRevision,1);assert.deepEqual(requests[2].credential,{kind:'keep'});assert.notEqual(requests[2].operationId,requests[1].operationId);
 }finally{await act(async()=>root.unmount());client.saveConnection=original;dom.window.close();}
});
