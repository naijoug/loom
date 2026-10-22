require.extensions['.css']=()=>undefined;
const test=require('node:test'), assert=require('node:assert/strict');
const React=require('react'),{act}=React,{createRoot}=require('react-dom/client'),{JSDOM}=require('jsdom');
const {AppStateProvider,useAppState}=require('../../.tmp/test-build/src/state/AppStateContext.js');
const {initialAppState}=require('../../.tmp/test-build/src/state/reducer.js');
const {ThemeProvider}=require('../../.tmp/test-build/src/contexts/ThemeContext.js');
const {AppContent}=require('../../.tmp/test-build/src/App.js');
global.IS_REACT_ACT_ENVIRONMENT=true;
test('default Chat and Settings never load Task data or subscribe to Planning events',async t=>{
 const dom=new JSDOM('<div id="root"></div>',{url:'http://localhost'});
 global.window=dom.window;global.document=dom.window.document;global.navigator=dom.window.navigator;global.localStorage=dom.window.localStorage;
 window.matchMedia=()=>({matches:false,addEventListener(){},removeEventListener(){}});
 const project={id:'p',path:'/p',name:'Project',detectedStacks:[],suggestedCommands:[]};
 const calls=[];let dispatch;
 window.__TAURI_INTERNALS__={transformCallback(){return 1},unregisterCallback(){},async invoke(command,args){
  calls.push({command,args});
  if(command==='list_recent_projects')return [project];
  if(command==='chat_list_project_groups')return [{project,sessionCount:0,sessions:[]}];
  if(['list_agents','diagnose_agents'].includes(command))return [];
  if(command==='load_app_settings')return {};
  if(command==='skills_list_library')return {sources:[],skills:[],issues:[]};
  if(command==='skills_list_operations')return [];
  if(command==='health_check')return {version:'test'};
  if(command.startsWith('plugin:event|'))return 1;
  throw Error(command);
 }};
 window.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener(){}};
 const root=createRoot(document.getElementById('root'));t.after(async()=>{await act(async()=>root.unmount());dom.window.close()});
 function Harness(){dispatch=useAppState().dispatch;return React.createElement(AppContent)}
 await act(async()=>root.render(React.createElement(ThemeProvider,null,React.createElement(AppStateProvider,{initialStateOverride:{...initialAppState,projects:{current:project,recent:[project]},app:{...initialAppState.app,activeProjectId:'p'}}},React.createElement(Harness)))));
 assert.ok(document.querySelector('[data-testid="chat-page"]'));
 assert.equal(document.querySelectorAll('button[aria-haspopup="menu"][aria-label^="Permissions"]').length,1);
 const mountedChat=document.querySelector('[data-testid="chat-page"]');
 await act(async()=>dispatch({type:'app/viewSelected',view:'settings'}));
 const skillsTab=[...document.querySelectorAll('.chat-settings > aside nav button')].find(node=>node.textContent==='Skills');
 assert.ok(skillsTab);
 await act(async()=>skillsTab.dispatchEvent(new window.MouseEvent('click',{bubbles:true})));
 assert.equal(document.querySelector('.settings-content').dataset.tab,'skills');
 assert.equal([...document.querySelectorAll('.chat-project-tree button')].some(node=>node.textContent==='Skills'),false);
 assert.ok(document.querySelector('[data-testid="skills-panel"]'));
 assert.equal(document.querySelector('[data-testid="chat-page"]'),mountedChat);
 assert.equal(mountedChat.closest('.app-chat-container').hidden,true);
 await act(async()=>dispatch({type:'app/viewSelected',view:'chat'}));
 assert.equal(document.querySelector('[data-testid="chat-page"]'),mountedChat);
 assert.equal(mountedChat.closest('.app-chat-container').hidden,false);
 await act(async()=>dispatch({type:'app/viewSelected',view:'settings'}));
 assert.match(document.body.textContent,/Appearance/);
 assert.equal(calls.some(call=>/task|planning|command_run/.test(call.command)),false);
 assert.equal(calls.some(call=>/planning/.test(call.args?.event??'')),false);
 assert.doesNotMatch(document.body.textContent,/Task|Board/);
});

test('an active Chat keeps receiving stream events while Settings Skills is visible',async t=>{
 const dom=new JSDOM('<div id="root"></div>',{url:'http://localhost'});
 global.window=dom.window;global.document=dom.window.document;global.navigator=dom.window.navigator;global.localStorage=dom.window.localStorage;
 window.matchMedia=()=>({matches:false,addEventListener(){},removeEventListener(){}});
 const project={id:'p',path:'/p',name:'Project',detectedStacks:[],suggestedCommands:[]};
 const session=structuredClone(require('../../contracts/tauri-contract.json').modelSamples.chatSession);
 Object.assign(session,{id:'chat-live',projectPath:'/p',agentId:'codex',title:'Active fixture',turnStatus:'streaming',activeTurnId:'live-turn',lastSeq:1});
 Object.assign(session.messages[1],{id:'assistant',content:'Before settings.',status:'streaming',parts:[]});
 Object.assign(session.turns[0],{id:'live-turn',status:'running',finishedAtMs:null,exitCode:null});
 const callbacks=new Map(),listeners=new Map();let next=0,dispatch;
 window.__TAURI_INTERNALS__={transformCallback(callback){const id=++next;callbacks.set(id,callback);return id;},unregisterCallback(){},async invoke(command,args){
  if(command==='list_recent_projects')return [project];
  if(command==='chat_list_project_groups')return [{project,sessionCount:1,sessions:[session]}];
  if(command==='chat_get')return session;
  if(command==='chat_read_events')return {events:[],hasMore:false};
  if(['list_agents','diagnose_agents','skills_list_operations'].includes(command))return [];
  if(command==='skills_list_library')return {sources:[],skills:[],issues:[]};
  if(command==='load_app_settings')return {};
  if(command==='health_check')return {version:'test'};
  if(command==='plugin:event|listen'){listeners.set(args.event,callbacks.get(args.handler));return ++next;}
  if(command.startsWith('plugin:event|'))return;
  throw Error(command);
 }};
 window.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener(){}};
 const root=createRoot(document.getElementById('root'));t.after(async()=>{await act(async()=>root.unmount());dom.window.close();});
 function Harness(){dispatch=useAppState().dispatch;return React.createElement(AppContent);}
 await act(async()=>root.render(React.createElement(ThemeProvider,null,React.createElement(AppStateProvider,{initialStateOverride:{...initialAppState,projects:{current:project,recent:[project]},app:{...initialAppState.app,activeProjectId:'p'}}},React.createElement(Harness)))));
 await act(async()=>document.querySelector('.chat-tree-session').dispatchEvent(new window.MouseEvent('click',{bubbles:true})));
 const chat=document.querySelector('[data-testid="chat-page"]');assert.match(chat.textContent,/Before settings/);
 await act(async()=>dispatch({type:'app/viewSelected',view:'settings'}));
 await act(async()=>[...document.querySelectorAll('.chat-settings > aside nav button')].find(node=>node.textContent==='Skills').dispatchEvent(new window.MouseEvent('click',{bubbles:true})));
 assert.ok(chat.closest('.app-chat-container').hidden);
 const listener=listeners.get('loom://chat-event');assert.equal(typeof listener,'function');
 await act(async()=>listener({event:'loom://chat-event',id:1,payload:{schemaVersion:2,projectKey:'/p',sessionId:'chat-live',seq:2,timestampMs:2,kind:'stream',payload:{sessionId:'chat-live',turnId:'live-turn',messageId:'assistant',delta:' STREAM_WHILE_SKILLS_OPEN',done:false}}}));
 await act(async()=>dispatch({type:'app/viewSelected',view:'chat'}));
 assert.equal(document.querySelector('[data-testid="chat-page"]'),chat);assert.match(chat.textContent,/STREAM_WHILE_SKILLS_OPEN/);
});
