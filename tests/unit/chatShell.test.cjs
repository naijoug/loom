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
 await act(async()=>dispatch({type:'app/viewSelected',view:'settings'}));
 assert.match(document.body.textContent,/Appearance/);
 assert.equal(calls.some(call=>/task|planning|command_run/.test(call.command)),false);
 assert.equal(calls.some(call=>/planning/.test(call.args?.event??'')),false);
 assert.doesNotMatch(document.body.textContent,/Task|Board/);
});
