const test=require('node:test');const assert=require('node:assert/strict');const {JSDOM}=require('jsdom');
const dom=new JSDOM('<div id="root"></div>',{url:'http://localhost'});global.window=dom.window;global.document=dom.window.document;global.navigator=dom.window.navigator;global.IS_REACT_ACT_ENVIRONMENT=true;
dom.window.HTMLDialogElement.prototype.showModal=function(){this.open=true;};dom.window.HTMLDialogElement.prototype.close=function(){this.open=false;};
const React=require('react');const{act}=React;const{createRoot}=require('react-dom/client');
const{workbenchClient:client}=require('../../.tmp/test-build/src/api/workbenchClient.js');const{ImportedHistory}=require('../../.tmp/test-build/src/workbench/ImportedHistory.js');
test('historical pagination ignores a departed session and resource text remains inert',async()=>{
 const original=client.request;let resolveA;const lateA=new Promise(resolve=>{resolveA=resolve;});const calls=[];
 const resource={id:'source',sourcePath:'original.json',bytes:24,sha256:'hash'};
 const page=(id,offset)=>({history:{origin:{kind:'chat'},messageCount:2},offset,messages:[{id:`${id}-${offset}`,role:'assistant',content:offset?'第二条历史消息':'第一条历史消息',status:'interrupted',createdAtMs:1,errorSummary:null,parts:[]}],hasMore:offset===0,resources:[resource]});
 client.request=async command=>{calls.push(command);if(command.method==='read_import_history')return command.params.key.sessionId==='A'?lateA:page('B',command.params.offset);if(command.method==='read_import_resource')return{resource,offset:0,bytes:[...Buffer.from('<script>window.x=1</script>')],hasMore:false};throw new Error(command.method);};
 const root=createRoot(document.getElementById('root'));const render=id=>root.render(React.createElement(ImportedHistory,{sessionKey:{workspaceId:'w',sessionId:id}}));
 try{
  await act(async()=>render('A'));await act(async()=>render('B'));await act(async()=>resolveA({...page('A',0),messages:[{...page('A',0).messages[0],content:'OLD-SESSION-CONTENT'}]}));
  assert.doesNotMatch(document.body.textContent,/OLD-SESSION-CONTENT/);assert.match(document.body.textContent,/第一条历史消息/);
  await act(async()=>[...document.querySelectorAll('button')].find(node=>node.textContent==='加载更多历史消息').click());assert.match(document.body.textContent,/第二条历史消息/);
  const list=document.querySelector('.wb-history-resources');list.open=true;await act(async()=>list.querySelector('button').click());
  assert.equal(document.querySelector('script'),null);assert.match(document.querySelector('.wb-history-resource').textContent,/<script>/);
  assert.equal(calls.find(c=>c.method==='read_import_resource').params.key.sessionId,'B');
 }finally{await act(async()=>root.unmount());client.request=original;dom.window.close();}
});
