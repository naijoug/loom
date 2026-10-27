require.extensions['.css']=()=>undefined;
const test=require('node:test'),assert=require('node:assert/strict');
const {JSDOM}=require('jsdom');
// react-dom detects input-event support when it loads, so a DOM must exist first.
const initial=new JSDOM('<div id="root"></div>',{url:'http://localhost'});
global.window=initial.window;global.document=initial.window.document;global.navigator=initial.window.navigator;
const React=require('react'),{act}=React,{createRoot}=require('react-dom/client');
const {RESEARCH_ADAPTERS,lastTurn}=require('../../.tmp/test-build/src/api/research.js');
const {researchAgents}=require('../../.tmp/test-build/src/features/research/NewResearchForm.js');
const {AppStateProvider}=require('../../.tmp/test-build/src/state/AppStateContext.js');
const {initialAppState}=require('../../.tmp/test-build/src/state/reducer.js');
const {ThemeProvider}=require('../../.tmp/test-build/src/contexts/ThemeContext.js');
const {AppContent}=require('../../.tmp/test-build/src/App.js');
const samples=require('../../contracts/tauri-contract.json').modelSamples;
global.IS_REACT_ACT_ENVIRONMENT=true;

test('only enabled built-in agents are offered for Research',()=>{
 const agent=(id,adapterType,enabled=true)=>({id,adapterType,enabled,available:true});
 assert.deepEqual(researchAgents([agent('c','claude_code_cli'),agent('x','custom'),agent('o','opencode_cli',false),agent('k','codex_cli')]).map(a=>a.id),['c','k']);
 assert.deepEqual(RESEARCH_ADAPTERS,['claude_code_cli','codex_cli','opencode_cli','grok_cli']);
 assert.equal(lastTurn({turns:[]}),undefined);
 assert.equal(lastTurn({turns:[{index:1},{index:2}]}).index,2);
});

function setup(t,{research,turns}){
 const dom=new JSDOM('<div id="root"></div>',{url:'http://localhost'});
 global.window=dom.window;global.document=dom.window.document;global.navigator=dom.window.navigator;global.localStorage=dom.window.localStorage;
 window.matchMedia=()=>({matches:false,addEventListener(){},removeEventListener(){}});
 window.HTMLElement.prototype.scrollIntoView=()=>{};
 const project={id:'p',path:'/p',name:'Project',detectedStacks:[],suggestedCommands:[]};
 const agent={id:'agent-codex',name:'Codex',command:'codex',args:[],workingDirectoryPolicy:'project_root',capabilities:[],adapterType:'codex_cli',canWriteFiles:false,canRunCommands:false,enabled:true,available:true};
 const calls=[];const listeners=[];
 const state={research};
 window.__TAURI_INTERNALS__={transformCallback(callback){listeners.push(callback);return listeners.length},unregisterCallback(){},async invoke(command,args){
  calls.push({command,args});
  if(command==='list_recent_projects')return [project];
  if(command==='chat_list_project_groups')return [{project,sessionCount:0,sessions:[]}];
  if(command==='list_agents')return [agent];
  if(command==='diagnose_agents')return [{agentId:'agent-codex',status:'ready',authStatus:'unknown',detail:'',checkedAtMs:1,capabilities:{structuredStream:true,toolEvents:true,nativeResume:true,inlineCards:false,modelSelection:true,modes:[]}}];
  if(command==='load_app_settings')return {};
  if(command==='health_check')return {version:'test'};
  if(command==='research_list')return {items:state.research?[{researchId:state.research.researchId,title:state.research.title,agentName:'Codex',projectName:state.research.projectName,status:lastTurnStatus(state.research),turns:state.research.turns.length,updatedAtMs:2}]:[],storageErrors:[]};
  if(command==='research_get')return structuredClone(state.research);
  if(command==='research_read_turn')return turns[args.turn];
  if(command==='research_read_source')return {path:'src/main.rs',text:'fn a() {}\nfn main() {\n    run();\n}\n',firstLine:1,totalLines:4,truncated:false,language:'Rust'};
  if(command==='research_create'){state.research={...structuredClone(samples.research),researchId:'research-new',title:args.input.question,turns:[{index:1,question:args.input.question,status:'running',jobId:'job-n',startedAtMs:1,warnings:[]}]};return state.research;}
  if(command==='research_ask'){state.research.turns.push({index:state.research.turns.length+1,question:args.question,status:'running',jobId:'job-a',startedAtMs:3,warnings:[]});return structuredClone(state.research);}
  if(command==='research_cancel')return true;
  if(command==='research_retry'){const last=state.research.turns.at(-1);Object.assign(last,{status:'running',error:undefined});return structuredClone(state.research);}
  if(command.startsWith('plugin:event|'))return 1;
  throw Error(command);
 }};
 window.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener(){}};
 const root=createRoot(document.getElementById('root'));t.after(async()=>{await act(async()=>root.unmount());dom.window.close()});
 const settle=()=>act(async()=>{await new Promise(r=>setTimeout(r,20));});
 const setValue=(el,value)=>{Object.getOwnPropertyDescriptor(Object.getPrototypeOf(el),'value').set.call(el,value);el.dispatchEvent(new window.Event('input',{bubbles:true}));el.dispatchEvent(new window.Event('change',{bubbles:true}));};
 const click=el=>act(async()=>el.dispatchEvent(new window.MouseEvent('click',{bubbles:true})));
 const render=async()=>{
  await act(async()=>root.render(React.createElement(ThemeProvider,null,React.createElement(AppStateProvider,{initialStateOverride:{...initialAppState,projects:{current:project,recent:[project]},app:{...initialAppState.app,activeProjectId:'p'}}},React.createElement(AppContent)))));
  await settle();
  await click(document.querySelector('.activity-rail-item[aria-label="Research (Ctrl+3)"]'));
  await settle();
 };
 return {calls,state,settle,setValue,click,render};
}
const lastTurnStatus=research=>{const last=research.turns.at(-1);return last.status==='running'?'running':last.status==='done'?'ready':'failed';};

test('Research renders each question with its report, citations, context and follow-up',async t=>{
 const research={...structuredClone(samples.research),projectPath:'/p',projectName:'Project',turns:[
  {index:1,question:'How does it start?',status:'done',jobId:'job-1',startedAtMs:1,finishedAtMs:2,warnings:[],refs:{valid:1,invalid:1}},
  {index:2,question:'And stop?',status:'done',jobId:'job-2',startedAtMs:3,finishedAtMs:4,warnings:['The project\'s Git status or HEAD changed while the agent ran'],context:'replay',refs:{valid:0,invalid:0}},
 ]};
 const turns={1:{index:1,markdown:'# Start\n\nSee [main](loom-src:src/main.rs#L2-L3) and [gone](loom-src:gone.rs#L1-L2).',refs:[{target:'src/main.rs#L2-L3',path:'src/main.rs',startLine:2,endLine:3,status:'valid'},{target:'gone.rs#L1-L2',path:'gone.rs',startLine:1,endLine:2,status:'missing'}]},2:{index:2,markdown:'# Stop\n\nIt is killed.',refs:[]}};
 const {calls,settle,setValue,click,render}=setup(t,{research,turns});
 await render();
 assert.ok(document.querySelector('[data-testid="research-page"]'));
 assert.equal(document.querySelector('.research-title h1').textContent,research.title);
 assert.deepEqual([...document.querySelectorAll('.research-report h1')].map(h=>h.textContent),['Start','Stop']);
 assert.deepEqual([...document.querySelectorAll('.loom-src-link')].map(b=>b.dataset.status),['valid','missing']);
 assert.match(document.querySelectorAll('.research-turn')[0].textContent,/1 source citation/);
 assert.equal(document.querySelector('.research-chip').textContent,'Earlier questions replayed');
 assert.match(document.querySelector('.research-warnings').textContent,/HEAD changed/);
 await click(document.querySelector('.loom-src-link'));await settle();
 assert.deepEqual(calls.find(c=>c.command==='research_read_source').args,{researchId:research.researchId,path:'src/main.rs',startLine:2,endLine:3});
 assert.deepEqual([...document.querySelectorAll('.source-code-line[data-marked]')].map(l=>l.querySelector('.source-code-number').textContent),['2','3']);
 // Follow-up: Enter sends the trimmed question; the composer locks while it runs.
 const input=document.querySelector('#research-followup-input');
 await act(async()=>setValue(input,'  Why a process group?  '));
 await act(async()=>input.dispatchEvent(new window.KeyboardEvent('keydown',{key:'Enter',bubbles:true})));
 await settle();
 assert.deepEqual(calls.find(c=>c.command==='research_ask').args,{researchId:research.researchId,question:'Why a process group?'});
 assert.equal(document.querySelector('#research-followup-input').disabled,true);
 assert.ok(document.querySelector('.research-running'));
 const stop=[...document.querySelectorAll('.research-actions button')].find(b=>b.textContent==='Stop');
 await click(stop);
 assert.deepEqual(calls.find(c=>c.command==='research_cancel').args,{researchId:research.researchId});
 assert.equal(calls.some(call=>/^chat_send|^learn_/.test(call.command)),false);
});

test('a failed last question shows its error summary and can be retried',async t=>{
 const research={...structuredClone(samples.research),projectPath:undefined,projectName:undefined,turns:[{index:1,question:'Q',status:'failed',jobId:'job-1',startedAtMs:1,finishedAtMs:2,warnings:[],error:'agent exited with status exit status: 1; stderr: boom'}]};
 const {calls,settle,click,render}=setup(t,{research,turns:{}});
 await render();
 assert.equal(document.querySelector('.research-turn .research-error span').textContent,'agent exited with status 1');
 assert.equal(document.querySelector('.research-chip[data-status="failed"]').textContent,'Failed');
 // No project: no source browser.
 assert.equal(document.querySelector('.research-actions [aria-label="Browse source"]'),null);
 await click(document.querySelector('.research-actions .chat-primary-button'));await settle();
 assert.deepEqual(calls.find(c=>c.command==='research_retry').args,{researchId:research.researchId});
});

test('a new research sends the question, agent, trimmed model, project and language',async t=>{
 const {calls,settle,setValue,render}=setup(t,{research:null,turns:{}});
 await render();
 const form=document.querySelector('.research-new');
 assert.ok(form);
 assert.equal(form.querySelector('button[type=submit]').disabled,true);
 await act(async()=>{setValue(form.querySelector('textarea'),'  How is chat stored?  ');setValue(form.querySelector('input'),' gpt-5 ');setValue(form.querySelectorAll('select')[0],'/p');});
 await act(async()=>form.dispatchEvent(new window.Event('submit',{bubbles:true,cancelable:true})));
 await settle();
 assert.deepEqual(calls.find(c=>c.command==='research_create').args,{input:{question:'How is chat stored?',agentId:'agent-codex',model:'gpt-5',projectPath:'/p',language:'en'}});
 assert.ok(document.querySelector('[data-testid="research-view"]'));
});
