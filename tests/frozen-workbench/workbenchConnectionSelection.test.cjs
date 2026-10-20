require.extensions['.css']=()=>undefined;
const test=require('node:test');const assert=require('node:assert/strict');const{JSDOM}=require('jsdom');
const dom=new JSDOM('<div id="root"></div>',{url:'http://localhost'});global.window=dom.window;global.document=dom.window.document;global.navigator=dom.window.navigator;global.IS_REACT_ACT_ENVIRONMENT=true;
const React=require('react');const{act}=React;const{createRoot}=require('react-dom/client');const{workbenchClient:client}=require('../../.tmp/test-build/src/api/workbenchClient.js');const{WorkbenchApp}=require('../../.tmp/test-build/src/workbench/WorkbenchApp.js');
test('running session shows its accepted connection and model even after the global connection changes',async()=>{
 const original={...client};const key={workspaceId:'w',sessionId:'s'};const config={name:'Accepted account',models:['model-a'],defaultModel:'model-a'};const old={id:'c',revision:1,config};const latest={...old,revision:2,config:{name:'Replacement account',models:['model-b'],defaultModel:'model-b'}};
 const view={metadata:{key,title:'Session',projectId:null,statusId:'todo',archived:false,flagged:false,labels:[],permissionMode:'ask',connection:{connectionId:'c',model:null,thinking:null},enabledSources:[],createdAtMs:1,updatedAtMs:1,lastReadMessageId:null},notes:'',lastSeq:3,metadataRevision:1,messages:[],turns:[{id:'t',status:'running',configuration:{connection:{connectionId:'c',model:null,thinking:null},permissionMode:'ask',resolvedConnection:old}}],tools:{},approvals:{},activeTurnId:'t'};
 client.capabilities=async()=>({protocolVersion:1,runtime:'fixture',credentialStore:'macos_keychain'});client.watchWorkspace=async()=>({close:async()=>{}});client.watch=async()=>({snapshot:view,close:async()=>{}});
 client.request=async command=>{
  if(command.method==='list_workspaces')return[{id:'w',name:'Workspace',createdAtMs:1}];if(command.method==='list_projects')return[];if(command.method==='list_connections')return[latest];if(command.method==='get_session')return view;if(command.method==='read_events')return{events:[],lastSeq:3,hasMore:false};if(command.method==='list_sessions')return[{sessionId:'s',summary:{metadata:view.metadata,lastSeq:3,messageCount:0,preview:'',activeTurnId:'t',queuedTurnCount:0,hasUnread:false},error:null}];throw new Error(command.method);
 };
 const root=createRoot(document.getElementById('root'));
 try{await act(async()=>root.render(React.createElement(WorkbenchApp)));await act(async()=>document.querySelector('.wb-session-row').click());const connection=document.querySelector('[aria-label="会话模型连接"]');const model=document.querySelector('[aria-label="会话模型"]');assert.equal(connection.selectedOptions[0].textContent,'Accepted account');assert.equal(model.value,'model-a');assert.ok(connection.disabled&&model.disabled);}
 finally{await act(async()=>root.unmount());Object.assign(client,original);dom.window.close();}
});
