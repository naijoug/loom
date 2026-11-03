require.extensions['.css']=()=>undefined;
const test=require('node:test'), assert=require('node:assert/strict');
const {JSDOM}=require('jsdom');
// react-dom detects input-event support when it loads, so a DOM must exist first.
const initial=new JSDOM('<div id="root"></div>',{url:'http://localhost'});
global.window=initial.window;global.document=initial.window.document;global.navigator=initial.window.navigator;
const React=require('react'),{act}=React,{createRoot}=require('react-dom/client');
const {AppStateProvider}=require('../../.tmp/test-build/src/state/AppStateContext.js');
const {initialAppState}=require('../../.tmp/test-build/src/state/reducer.js');
const {ThemeProvider}=require('../../.tmp/test-build/src/contexts/ThemeContext.js');
const {AppContent}=require('../../.tmp/test-build/src/App.js');
const {FEATURES}=require('../../.tmp/test-build/src/shell/features.js');
const {shortcutView}=require('../../.tmp/test-build/src/shell/AppShell.js');
global.IS_REACT_ACT_ENVIRONMENT=true;

const key=(k,mods={})=>({key:k,metaKey:false,ctrlKey:false,altKey:false,shiftKey:false,isComposing:false,...mods});

test('the registry lists only delivered features, Chat first and mounted from startup',()=>{
 assert.deepEqual(FEATURES.map(feature=>feature.id),['chat','learn','research','discuss']);
 assert.deepEqual(FEATURES.map(feature=>feature.keepMounted),[true,false,false,false]);
});

test('Cmd+1…4 follow rail order and Cmd+, opens Settings; other chords are ignored',()=>{
 assert.equal(shortcutView(key('1',{metaKey:true}),true),'chat');
 assert.equal(shortcutView(key('2',{metaKey:true}),true),'learn');
 assert.equal(shortcutView(key('3',{metaKey:true}),true),'research');
 assert.equal(shortcutView(key('4',{metaKey:true}),true),'discuss');
 assert.equal(shortcutView(key('5',{metaKey:true}),true),null);
 assert.equal(shortcutView(key(',',{metaKey:true}),true),'settings');
 assert.equal(shortcutView(key('2',{ctrlKey:true}),true),null);
 assert.equal(shortcutView(key('2',{ctrlKey:true}),false),'learn');
 assert.equal(shortcutView(key('2',{metaKey:true}),false),null);
 assert.equal(shortcutView(key('2',{metaKey:true,shiftKey:true}),true),null);
 assert.equal(shortcutView(key('2',{metaKey:true,altKey:true}),true),null);
 assert.equal(shortcutView(key('2',{metaKey:true,isComposing:true}),true),null);
 assert.equal(shortcutView(key('2'),true),null);
});

test('the rail switches features without remounting Chat or losing its draft',async t=>{
 const dom=new JSDOM('<div id="root"></div>',{url:'http://localhost'});
 global.window=dom.window;global.document=dom.window.document;global.navigator=dom.window.navigator;global.localStorage=dom.window.localStorage;
 window.matchMedia=()=>({matches:false,addEventListener(){},removeEventListener(){}});
 window.HTMLElement.prototype.scrollIntoView=()=>{};
 const project={id:'p',path:'/p',name:'Project',detectedStacks:[],suggestedCommands:[]};
 const agent={id:'codex',name:'Codex',command:'codex',args:[],workingDirectoryPolicy:'project_root',capabilities:[],adapterType:'codex_cli',canWriteFiles:false,canRunCommands:false,enabled:true,available:true};
 const calls=[];
 window.__TAURI_INTERNALS__={transformCallback(){return 1},unregisterCallback(){},async invoke(command,args){
  calls.push({command,args});
  if(command==='list_recent_projects')return [project];
  if(command==='chat_list_project_groups')return [{project,sessionCount:0,sessions:[]}];
  if(command==='list_agents')return [agent];
  if(command==='diagnose_agents')return [];
  if(command==='load_app_settings')return {};
  if(command==='health_check')return {version:'test'};
  if(command==='learn_list_studies')return {studies:[],storageErrors:[]};
  if(command.startsWith('plugin:event|'))return 1;
  throw Error(command);
 }};
 window.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener(){}};
 const root=createRoot(document.getElementById('root'));t.after(async()=>{await act(async()=>root.unmount());dom.window.close()});
 const settle=()=>act(async()=>{await new Promise(r=>setTimeout(r,20));});
 await act(async()=>root.render(React.createElement(ThemeProvider,null,React.createElement(AppStateProvider,{initialStateOverride:{...initialAppState,projects:{current:project,recent:[project]},app:{...initialAppState.app,activeProjectId:'p'}}},React.createElement(AppContent)))));
 await settle();
 const rail=document.querySelector('[data-testid="activity-rail"]');
 assert.deepEqual([...rail.querySelectorAll('.activity-rail-item')].map(item=>item.getAttribute('aria-label')),['Chat (Ctrl+1)','Learn (Ctrl+2)','Research (Ctrl+3)','Discuss (Ctrl+4)','Settings (Ctrl+,)']);
 const current=()=>rail.querySelector('[aria-current="page"]').getAttribute('aria-label');
 assert.equal(current(),'Chat (Ctrl+1)');
 // The Chat sidebar keeps only the project and session tree.
 assert.deepEqual([...document.querySelectorAll('.chat-project-tree footer button')].map(item=>item.textContent),['Archived']);
 const chat=document.querySelector('[data-testid="chat-page"]');
 const input=document.querySelector('.chat-composer textarea');
 const setValue=(el,value)=>{Object.getOwnPropertyDescriptor(Object.getPrototypeOf(el),'value').set.call(el,value);el.dispatchEvent(new window.Event('input',{bubbles:true}));el.dispatchEvent(new window.Event('change',{bubbles:true}));};
 await act(async()=>setValue(input,'draft survives switching'));
 // Learn mounts on first visit; Chat stays mounted but hidden.
 assert.equal(document.querySelector('[data-testid="learn-page"]'),null);
 await act(async()=>rail.querySelectorAll('.activity-rail-item')[1].dispatchEvent(new window.MouseEvent('click',{bubbles:true})));
 await settle();
 const learn=document.querySelector('[data-testid="learn-page"]');
 assert.ok(learn);
 assert.equal(current(),'Learn (Ctrl+2)');
 assert.equal(chat.closest('.app-shell-pane').hidden,true);
 // Ctrl+, opens Settings, Ctrl+1 returns to the same Chat with its draft; Learn stays mounted, Settings does not.
 await act(async()=>window.dispatchEvent(new window.KeyboardEvent('keydown',{key:',',ctrlKey:true})));
 await settle();
 assert.ok(document.querySelector('.chat-settings'));
 assert.equal(current(),'Settings (Ctrl+,)');
 await act(async()=>window.dispatchEvent(new window.KeyboardEvent('keydown',{key:'1',ctrlKey:true})));
 assert.equal(document.querySelector('.chat-settings'),null);
 assert.equal(document.querySelector('[data-testid="chat-page"]'),chat);
 assert.equal(chat.closest('.app-shell-pane').hidden,false);
 assert.equal(document.querySelector('.chat-composer textarea').value,'draft survives switching');
 assert.equal(document.querySelector('[data-testid="learn-page"]'),learn);
 assert.equal(learn.closest('.app-shell-pane').hidden,true);
 assert.equal(calls.some(call=>/^chat_send/.test(call.command)),false);
});
