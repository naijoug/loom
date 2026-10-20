require.extensions['.css']=()=>undefined;
const test=require('node:test');const assert=require('node:assert/strict');const {JSDOM}=require('jsdom');
const dom=new JSDOM('<div id="root"></div>',{url:'http://localhost'});
global.window=dom.window;global.document=dom.window.document;global.navigator=dom.window.navigator;global.IS_REACT_ACT_ENVIRONMENT=true;
const React=require('react');const{act}=React;const{createRoot}=require('react-dom/client');
const{workbenchClient:client}=require('../../.tmp/test-build/src/api/workbenchClient.js');
const{WorkbenchApp}=require('../../.tmp/test-build/src/workbench/WorkbenchApp.js');
const deferred=()=>{let resolve;const promise=new Promise(r=>{resolve=r;});return{promise,resolve};};
function view(id){return{metadata:{key:{workspaceId:'w',sessionId:id},title:id,projectId:null,statusId:'todo',archived:false,flagged:false,labels:[],permissionMode:'ask',connection:{connectionId:null,model:null,thinking:null},enabledSources:[],createdAtMs:1,updatedAtMs:1,lastReadMessageId:null},notes:'',lastSeq:1,metadataRevision:1,messages:[],turns:[],tools:{},activeTurnId:null};}
const views={A:view('A'),B:view('B')};const entries=Object.values(views).map(v=>({sessionId:v.metadata.key.sessionId,summary:{metadata:v.metadata,lastSeq:1,messageCount:0,preview:'',activeTurnId:null,queuedTurnCount:0,hasUnread:false},error:null}));
function input(node,value){Object.getOwnPropertyDescriptor(dom.window.HTMLTextAreaElement.prototype,'value').set.call(node,value);node.dispatchEvent(new dom.window.Event('input',{bubbles:true}));}
function row(id){return [...document.querySelectorAll('.wb-session-row')].find(node=>node.textContent.startsWith(id));}

test('REA source toggle patches the active session only',async()=>{
 const original={...client};
 client.capabilities=async()=>({protocolVersion:1,runtime:'fixture'});
 client.watchWorkspace=async()=>({close:async()=>{}});
 client.watch=async key=>({snapshot:views[key.sessionId],close:async()=>{}});
 client.request=async command=>{
  if(command.method==='list_workspaces')return[{id:'w',name:'Workspace',createdAtMs:1}];
  if(command.method==='list_sessions')return entries;
  if(command.method==='list_projects'||command.method==='list_connections')return[];
  if(command.method==='read_events')return{events:[],lastSeq:1,hasMore:false};
  if(command.method==='get_session')return views[command.params.key.sessionId];
  if(command.method==='patch_session'){
   const view=views[command.params.key.sessionId];
   view.metadata.enabledSources=command.params.patch.enabledSources;
   view.lastSeq++;
   return view;
  }
  throw new Error(`unexpected ${command.method}`);
 };
 const root=createRoot(document.getElementById('root'));
 try{
  await act(async()=>root.render(React.createElement(React.StrictMode,null,React.createElement(WorkbenchApp))));
  assert.equal(document.querySelector('[aria-label="REA 调研源"]'),null);
  await act(async()=>row('A').click());
  const toggle=document.querySelector('[aria-label="REA 调研源"]');
  assert.equal(toggle.getAttribute('aria-pressed'),'false');
  await act(async()=>toggle.click());
  assert.equal(document.querySelector('[aria-label="REA 调研源"]').getAttribute('aria-pressed'),'true');
  await act(async()=>document.querySelector('[aria-label="REA 调研源"]').click());
  assert.equal(document.querySelector('[aria-label="REA 调研源"]').getAttribute('aria-pressed'),'false');
 }finally{await act(async()=>root.unmount());Object.assign(client,original);}
});

test('late send acknowledgement cannot clear another session draft and IME Enter does not send',async()=>{
 const original={...client};const slow=deferred();let sends=0,watches=0,closes=0;
 client.capabilities=async()=>({protocolVersion:1,runtime:'fixture'});
 client.watchWorkspace=async()=>({close:async()=>{}});
 client.watch=async key=>{watches++;return{snapshot:views[key.sessionId],close:async()=>{closes++;}};};
 client.request=async command=>{
  if(command.method==='list_workspaces')return[{id:'w',name:'Workspace',createdAtMs:1}];
  if(command.method==='list_sessions')return entries;
  if(command.method==='list_projects'||command.method==='list_connections')return[];
  if(command.method==='read_events')return{events:[],lastSeq:1,hasMore:false};
  if(command.method==='get_session')return views[command.params.key.sessionId];
  if(command.method==='send_message'){sends++;return slow.promise;}
  throw new Error(`unexpected ${command.method}`);
 };
 const root=createRoot(document.getElementById('root'));
 try{
  await act(async()=>root.render(React.createElement(React.StrictMode,null,React.createElement(WorkbenchApp))));
  await act(async()=>row('A').click());let composer=document.querySelector('[aria-label="消息"]');assert.ok(composer);
  await act(async()=>input(composer,'A draft'));
  await act(async()=>composer.dispatchEvent(new dom.window.KeyboardEvent('keydown',{bubbles:true,key:'Enter',isComposing:true,keyCode:229})));assert.equal(sends,0);
  await act(async()=>document.querySelector('[aria-label="发送消息"]').click());assert.equal(sends,1);
  await act(async()=>row('B').click());composer=document.querySelector('[aria-label="消息"]');await act(async()=>input(composer,'B unsent draft'));
  await act(async()=>slow.resolve({receipt:{},alreadyAccepted:false,lastSeq:2}));assert.equal(document.querySelector('[aria-label="消息"]').value,'B unsent draft');
  await act(async()=>row('A').click());assert.equal(document.querySelector('[aria-label="消息"]').value,'');
 }finally{await act(async()=>root.unmount());Object.assign(client,original);dom.window.close();}
 assert.equal(closes,watches,'every generation watch is closed');
});
