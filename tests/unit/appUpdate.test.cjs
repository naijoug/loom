require.extensions['.css']=()=>undefined;
const test=require('node:test');const assert=require('node:assert/strict');const {JSDOM}=require('jsdom');
const dom=new JSDOM('<div id="root"></div>',{url:'http://localhost'});
global.window=dom.window;global.document=dom.window.document;global.navigator=dom.window.navigator;global.IS_REACT_ACT_ENVIRONMENT=true;
dom.window.HTMLDialogElement.prototype.showModal=function(){this.open=true};
dom.window.HTMLDialogElement.prototype.close=function(){this.open=false};
const React=require('react');const {act}=React;const {createRoot}=require('react-dom/client');
const api=require('../../.tmp/test-build/src/api/tauriClient.js');
const {UpdateProvider,UpdateButton,useUpdateDrafts}=require('../../.tmp/test-build/src/features/app-update/UpdateProvider.js');
const {initialUpdate,newerSnapshot,downloadPercent}=require('../../.tmp/test-build/src/features/app-update/model.js');
const button=text=>[...document.querySelectorAll(document.querySelector('dialog')?'dialog button':'button')].find(node=>node.textContent.trim()===text);
const flush=async()=>{await new Promise(resolve=>setImmediate(resolve));};

test('progress tolerates missing length and late snapshots never reverse state',()=>{
 const ready={...initialUpdate,revision:5,phase:'ready'};
 assert.equal(newerSnapshot(ready,{...ready,revision:4,phase:'downloading'}),ready);
 assert.equal(downloadPercent({...ready,downloaded:50,total:null}),undefined);
 assert.equal(downloadPercent({...ready,downloaded:150,total:100}),100);
});
test('update download retries; installation persists drafts and reports active work without duplicate operations',async()=>{
 window.__TAURI_INTERNALS__={};let state={...initialUpdate,phase:'available',currentVersion:'0.1.0',version:'0.2.0',notes:'改善更新体验',revision:1};
 const calls=[];let event;let attempt=0;
 const old={...api};
 api.listenToEvent=async(_,handler)=>{event=handler;return()=>{}};
 api.invokeCommand=async(command,args)=>{
   calls.push([command,args]);
   if(command==='get_update_state')return state;
   if(command==='load_update_drafts')return {workbench:{session:'未发送的草稿'}};
   if(command==='save_update_drafts')return;
   if(command==='download_update'){
     attempt++; if(attempt===1)throw new Error('下载或签名校验失败');
     state={...state,phase:'ready',revision:3};event(state);return state;
   }
   if(command==='install_update')throw new Error('仍有任务或终端正在运行');
   throw Error(command);
 };
 let restored;
 function Draft(){const[data,setData]=React.useState({});useUpdateDrafts('workbench',data,saved=>{restored=saved;setData(saved)});return null;}
 const root=createRoot(document.getElementById('root'));
 try{
  await act(async()=>{root.render(React.createElement(UpdateProvider,null,React.createElement(Draft),React.createElement(UpdateButton,{manual:true})));await flush()});
  assert.equal(restored.session,'未发送的草稿');assert.ok(document.querySelector('.has-update'));
  await act(async()=>document.querySelector('.loom-update-button').click());
  await act(async()=>{button('Download update').click();button('Download update').click();await flush()});
  assert.equal(attempt,1);assert.match(document.querySelector('[role="alert"]').textContent,/校验失败/);
  await act(async()=>{button('Download update').click();await flush()});
  await act(async()=>{button('Install and restart').click();button('Install and restart').click();await flush()});
  assert.equal(calls.filter(([c])=>c==='install_update').length,1,JSON.stringify({calls,error:document.querySelector('[role="alert"]')?.textContent}));
  const install=calls.find(([c])=>c==='install_update');assert.equal(install[1].drafts.workbench.session,'未发送的草稿');
  assert.ok(calls.findIndex(([c])=>c==='save_update_drafts')<calls.findIndex(([c])=>c==='install_update'));
  assert.match(document.querySelector('[role="alert"]').textContent,/仍有任务/);
  assert.equal(button('Install and restart').disabled,false);
 }finally{await act(async()=>root.unmount());Object.assign(api,old);delete window.__TAURI_INTERNALS__;}
});
test('web preview never invokes native update commands',async()=>{
 const old={...api};api.invokeCommand=async()=>{throw Error('native call in web preview')};
 const root=createRoot(document.getElementById('root'));
 try{
  await act(async()=>root.render(React.createElement(UpdateProvider,null,React.createElement(UpdateButton,{manual:true}))));
  assert.equal(document.querySelector('.has-update'),null);
  await act(async()=>button('Check for updates').click());
  assert.match(document.querySelector('dialog').textContent,/Web previews cannot update/);
  assert.equal(document.querySelector('dialog .primary').disabled,true);
 }finally{await act(async()=>root.unmount());Object.assign(api,old);}
});
test('remounting a composer restores its latest draft, never the stale startup copy',async()=>{
 window.__TAURI_INTERNALS__={};const old={...api};let edit,toggle;
 api.listenToEvent=async()=>()=>{};
 api.invokeCommand=async(command)=>command==='load_update_drafts'?{workbench:{session:'startup'}}:command==='get_update_state'?initialUpdate:undefined;
 function Draft(){const[data,setData]=React.useState({});edit=value=>setData({session:value});useUpdateDrafts('workbench',data,setData);return React.createElement('output',null,data.session??'');}
 function Harness(){const[visible,setVisible]=React.useState(true);toggle=setVisible;return visible?React.createElement(Draft):null;}
 const root=createRoot(document.getElementById('root'));
 try{
  await act(async()=>{root.render(React.createElement(UpdateProvider,null,React.createElement(Harness)));await flush()});
  assert.equal(document.querySelector('output').textContent,'startup');
  await act(async()=>edit('latest draft'));
  await act(async()=>toggle(false));await act(async()=>toggle(true));
  assert.equal(document.querySelector('output').textContent,'latest draft');
 }finally{await act(async()=>root.unmount());Object.assign(api,old);delete window.__TAURI_INTERNALS__;}
});
