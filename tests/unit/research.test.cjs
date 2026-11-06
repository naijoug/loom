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

function setup(t,{research,turns,targets=false,runs=[],reports={},evidence={},sources={},evidenceText={}}){
 const dom=new JSDOM('<div id="root"></div>',{url:'http://localhost'});
 global.window=dom.window;global.document=dom.window.document;global.navigator=dom.window.navigator;global.localStorage=dom.window.localStorage;
 window.matchMedia=()=>({matches:false,addEventListener(){},removeEventListener(){}});
 window.HTMLElement.prototype.scrollIntoView=()=>{};
 const project={id:'p',path:'/p',name:'Project',detectedStacks:[],suggestedCommands:[]};
 const agent={id:'agent-codex',name:'Codex',command:'codex',args:[],workingDirectoryPolicy:'project_root',capabilities:[],adapterType:'codex_cli',canWriteFiles:false,canRunCommands:false,enabled:true,available:true};
 const calls=[];const listeners=[];
 const state={research,runs,reports,evidence,sources,evidenceText,preflight:null};
 window.__TAURI_INTERNALS__={transformCallback(callback){listeners.push(callback);return listeners.length},unregisterCallback(){},async invoke(command,args){
  calls.push({command,args});
  if(command==='list_recent_projects')return [project];
  if(command==='chat_list_project_groups')return [{project,sessionCount:0,sessions:[]}];
  if(command==='list_agents')return [agent];
  if(command==='diagnose_agents')return [{agentId:'agent-codex',status:'ready',authStatus:'unknown',detail:'',checkedAtMs:1,capabilities:{structuredStream:true,toolEvents:true,nativeResume:true,inlineCards:false,modelSelection:true,modes:[]}}];
  if(command==='load_app_settings')return {};
  if(command==='health_check')return {version:'test'};
  if(command==='research_target_capabilities')return {singleShot:targets,iteration:false,experiments:false};
  if(command==='research_list'){
    if(!state.research)return {items:[],storageErrors:[]};
    if(state.research.workflow==='target'){const status=state.runs.at(-1)?.state.status??'completed';return {items:[{researchId:state.research.researchId,title:state.research.title,agentName:'Codex',workflow:'target',status,turns:state.runs.length,updatedAtMs:2}],storageErrors:[]};}
    return {items:[{researchId:state.research.researchId,title:state.research.title,agentName:'Codex',projectName:state.research.projectName,status:lastTurnStatus(state.research),turns:state.research.turns.length,updatedAtMs:2}],storageErrors:[]};
  }
  if(command==='research_get')return structuredClone(state.research);
  if(command==='research_read_turn')return turns[args.turn];
  if(command==='research_read_source')return {path:'src/main.rs',text:'fn a() {}\nfn main() {\n    run();\n}\n',firstLine:1,totalLines:4,truncated:false,language:'Rust'};
  if(command==='research_create'){state.research={...structuredClone(samples.research),researchId:'research-new',title:args.input.question,turns:[{index:1,question:args.input.question,status:'running',jobId:'job-n',startedAtMs:1,warnings:[]}]};return state.research;}
  if(command==='research_ask'){state.research.turns.push({index:state.research.turns.length+1,question:args.question,status:'running',jobId:'job-a',startedAtMs:3,warnings:[]});return structuredClone(state.research);}
  if(command==='research_cancel')return true;
  if(command==='research_retry'){const last=state.research.turns.at(-1);Object.assign(last,{status:'running',error:undefined});return structuredClone(state.research);}
  if(command==='research_preflight'){state.preflight={preflightId:'preflight-1',researchId:null,input:args.spec,budget:{maxRounds:1,maxCalls:4,maxCollections:1,maxActiveMs:1,maxDiskBytes:1},digest:'d',expiresAtMs:9,operations:['Pin one commit'],limitations:args.spec.target.kind==='website'?['Loading the page runs its scripts and contacts the network; this is not a read-only action.']:['Pinned snapshot']};return state.preflight;}
  if(command==='research_start_run'){
    const spec=state.preflight.input;const id=`research-${spec.target.kind}`;
    state.research={...structuredClone(samples.research),researchId:id,title:spec.question,workflow:'target',turns:[],projectPath:undefined,projectName:undefined,target:{target:spec.target,runIds:['run-1']}};
    const run={spec:{schemaVersion:2,researchId:id,runId:'run-1',kind:'initial',input:spec,budget:state.preflight.budget,configDigest:'d',parentReportId:null,agentConfigDigest:'a',targetIdentity:id,sourceSnapshots:['snap'],sourceEvidence:[],createdAtMs:1},state:{runId:'run-1',status:'completed',attempt:1,sequence:1,usage:{calls:1,collections:1,rounds:1,activeMs:1000,diskBytes:1,diskReservedBytes:0,reservationMs:0},snapshotIds:['snap'],evidenceIds:['ev-1'],reportIds:['report-1'],stopReason:null,error:null,updatedAtMs:2,exchange:{calls:1,reads:0,corrections:0}}};
    state.runs=[run];state.reports['report-1']={report:{reportId:'report-1',runId:'run-1',revision:1,snapshotIds:['snap'],evidenceIds:['ev-1'],defaultSourceSnapshotId:'snap',markdownSha256:'h',createdAtMs:1},markdown:`# ${spec.target.kind}\n\nDone.`};return structuredClone(run);
  }
  if(command==='research_list_runs')return structuredClone(state.runs);
  if(command==='research_read_report')return structuredClone(state.reports[args.reportId]);
  if(command==='research_list_evidence')return structuredClone(state.evidence[args.reportId]??[]);
  if(command==='research_read_evidence')return {text:state.evidenceText[args.evidenceId]??'redacted model view',nextCursor:null,truncated:false};
  if(command==='research_list_report_tree')return {text:Object.keys(state.sources[args.reportId]??{}).join('\n'),nextCursor:null,truncated:false};
  if(command==='research_read_report_source'){const text=state.sources[args.reportId]?.[args.path];if(text===undefined)throw Error('snapshot_unavailable');return {text,nextCursor:null,truncated:false};}
  if(command==='research_cancel_run'){const run=state.runs.find(item=>item.spec.runId===args.runId);run.state.status='cancelled';return true;}
  if(command==='research_ask_run'){const run={spec:{...state.runs[0].spec,runId:'run-ask',kind:'evidence_followup',parentReportId:args.parentReportId,budget:{...state.runs[0].spec.budget,maxCollections:0}},state:{...state.runs[0].state,runId:'run-ask',status:'collecting',reportIds:[],usage:{...state.runs[0].state.usage,collections:0}}};state.runs.push(run);return structuredClone(run);}
  if(command==='research_retry_run'){const run=state.runs.find(item=>item.spec.runId===args.runId);run.state.status='collecting';run.state.attempt+=1;return structuredClone(run);}
  if(command==='research_resume_run'){const run=state.runs.find(item=>item.spec.runId===args.runId);run.state.status='collecting';return structuredClone(run);}
  if(command==='research_export')return {directory:'/tmp/loom-export',files:['report.md'],includesRaw:args.input.includeRaw,warning:args.input.includeRaw?'Raw evidence can still contain target data.':null};
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
 assert.equal(document.querySelector('[data-workflow=target]'),null);
});

const legacyExecution=['research_create','research_ask','research_retry','research_cancel','research_read_turn','research_read_source','research_list_tree','research_read_job_logs'];
function targetRecord(){return {...structuredClone(samples.research),researchId:'research-target',title:'Pinned commit',workflow:'target',turns:[],projectPath:undefined,projectName:undefined,target:{target:{kind:'github',url:'https://github.com/acme/loom',gitRef:null},runIds:['run-1']}};}
function targetRun(status,reportIds,extra={}){
 const budget={maxRounds:1,maxCalls:4,maxCollections:1,maxActiveMs:1000,maxDiskBytes:1};
 return {spec:{schemaVersion:2,researchId:'research-target',runId:'run-1',kind:'initial',input:{target:{kind:'github',url:'https://github.com/acme/loom',gitRef:null},question:'Pinned commit',agentId:'agent-codex',model:null,language:'en',mode:'single',budget:null,tools:null},budget,configDigest:'d',parentReportId:null,agentConfigDigest:'a',targetIdentity:'github',sourceSnapshots:['snap-b'],sourceEvidence:[],createdAtMs:1},state:{runId:'run-1',status,attempt:1,sequence:1,usage:{calls:1,collections:1,rounds:1,activeMs:1000,diskBytes:1,diskReservedBytes:0,reservationMs:0},snapshotIds:['snap-b'],evidenceIds:['ev-1','ev-shot'],reportIds,stopReason:null,error:status==='failed'?'agent exited with status exit status: 1':null,updatedAtMs:2,exchange:{calls:1,reads:0,corrections:0},...extra}};
}

test('single-shot target form preflights GitHub, website and app without legacy execution',async t=>{
 const {calls,settle,setValue,click,render}=setup(t,{research:null,turns:{},targets:true});
 await render();
 assert.ok(document.querySelector('[data-testid="research-target-form"]'));
 const startKind=async kind=>{
  const form=document.querySelector('.research-new');
  await click(form.querySelector('input[name=research-mode][value=target]'));
  await act(async()=>setValue(form.querySelector('#research-target-kind'),kind));
  if(kind==='github')await act(async()=>setValue(form.querySelector('#research-github-url'),'https://github.com/acme/loom'));
  if(kind==='website')await act(async()=>{setValue(form.querySelector('#research-website-url'),'https://example.test/docs');setValue(form.querySelector('#research-rea-node'),'/preview/node');setValue(form.querySelector('#research-rea-entry'),'/preview/rea.mjs');});
  if(kind==='app')await act(async()=>{setValue(form.querySelector('#research-app-path'),'/preview/app');setValue(form.querySelector('#research-rea-node'),'/preview/node');setValue(form.querySelector('#research-rea-entry'),'/preview/rea.mjs');});
  await act(async()=>setValue(form.querySelector('textarea'),`Question about ${kind}`));
  await act(async()=>form.querySelector('textarea').dispatchEvent(new window.KeyboardEvent('keydown',{key:'Enter',metaKey:true,bubbles:true})));
  await settle();
  if(kind==='website')assert.match(form.textContent,/not a read-only action/);
  await act(async()=>form.dispatchEvent(new window.Event('submit',{bubbles:true,cancelable:true})));
  await settle();
  assert.equal(document.querySelector('[data-workflow=target] h1').textContent,`Question about ${kind}`);
  if(kind!=='app'){await click(document.querySelector('[aria-label="New research"]'));await settle();}
 };
 await startKind('github');await startKind('website');await startKind('app');
 assert.deepEqual(calls.filter(call=>call.command==='research_preflight').map(call=>call.args.spec.target.kind),['github','website','app']);
 assert.deepEqual(calls.filter(call=>call.command==='research_preflight').map(call=>call.args.spec.mode),['single','single','single']);
 assert.equal(calls.find(call=>call.command==='research_preflight'&&call.args.spec.target.kind==='github').args.spec.tools,null);
 assert.deepEqual(calls.find(call=>call.command==='research_preflight'&&call.args.spec.target.kind==='app').args.spec.tools,{node:'/preview/node',entry:'/preview/rea.mjs'});
 assert.equal(calls.filter(call=>call.command==='research_start_run').length,3);
 assert.ok(calls.filter(call=>call.command==='research_start_run').every(call=>/^req-[0-9a-f]{16}$/.test(call.args.requestId)));
 assert.equal(calls.some(call=>legacyExecution.includes(call.command)),false);
});

test('a target report binds source, evidence, follow-up, stop and export to the run',async t=>{
 const reports={
  'report-a':{report:{reportId:'report-a',runId:'run-1',revision:1,snapshotIds:['snap-a'],evidenceIds:['ev-1'],defaultSourceSnapshotId:'snap-a',markdownSha256:'a',createdAtMs:1},markdown:'# Older\n\nSee [file](loom-src:src/main.rs#L1-L1) and [page](loom-evidence:ev-1).'},
  'report-b':{report:{reportId:'report-b',runId:'run-1',revision:1,snapshotIds:['snap-b'],evidenceIds:['ev-1','ev-shot'],defaultSourceSnapshotId:'snap-b',markdownSha256:'b',createdAtMs:2},markdown:'# Newer\n\nSee [file](loom-src:src/main.rs#L1-L1) and [page](loom-evidence:ev-1).'},
 };
 const evidence={'report-a':[{evidenceId:'ev-1',runId:'run-1',sourceId:null,tool:'snapshot',toolVersion:'t',rawSha256:'a',modelSha256:'b',bytes:1,coverage:'pinned commit',limitations:['repository code was not run'],createdAtMs:1}],'report-b':[{evidenceId:'ev-1',runId:'run-1',sourceId:null,tool:'snapshot',toolVersion:'t',rawSha256:'a',modelSha256:'b',bytes:1,coverage:'pinned commit',limitations:[],createdAtMs:1},{evidenceId:'ev-shot',runId:'run-1',sourceId:null,tool:'screenshot',toolVersion:'t',rawSha256:'c',modelSha256:'d',bytes:1,coverage:'viewport',limitations:['not in the text prompt'],createdAtMs:1}]};
 const sources={'report-a':{'src/main.rs':'fn old() {}\n'},'report-b':{'src/main.rs':'fn new() {}\n'}};
 const {calls,settle,setValue,click,render}=setup(t,{research:targetRecord(),turns:{},targets:true,runs:[targetRun('completed',['report-a','report-b'])],reports,evidence,sources,evidenceText:{'ev-1':'redacted manifest','ev-shot':'Screenshot saved outside the text prompt.'}});
 await render();
 assert.equal(document.querySelector('[data-workflow=target] h1').textContent,'Pinned commit');
 assert.equal(document.querySelector('.research-report').dataset.reportId,'report-b');
 await click(document.querySelector('.loom-src-link'));await settle();
 assert.equal(calls.filter(call=>call.command==='research_read_report_source').at(-1).args.reportId,'report-b');
 assert.match(document.querySelector('.source-code').textContent,/fn new/);
 await click([...document.querySelectorAll('.research-evidence-item')].find(button=>button.textContent.includes('screenshot')));await settle();
 assert.match(document.body.textContent,/not shown in the text prompt/);
 await act(async()=>setValue(document.querySelector('select[aria-label="Report"]'),'report-a'));await settle();
 assert.equal(document.querySelector('.research-report').dataset.reportId,'report-a');
 await click(document.querySelector('.loom-src-link'));await settle();
 assert.equal(calls.filter(call=>call.command==='research_read_report_source').at(-1).args.reportId,'report-a');
 assert.match(document.querySelector('.source-code').textContent,/fn old/);
 assert.equal(document.querySelector('.source-code').textContent.includes('fn new'),false);
 await click(document.querySelector('.loom-evidence-link'));await settle();
 assert.equal(calls.filter(call=>call.command==='research_read_evidence').at(-1).args.reportId,'report-a');
 assert.match(document.querySelector('.research-evidence pre').textContent,/redacted manifest/);
 const input=document.querySelector('#research-followup-input');
 await act(async()=>setValue(input,'Why this commit?'));
 await act(async()=>input.dispatchEvent(new window.KeyboardEvent('keydown',{key:'Enter',bubbles:true})));
 await settle();
 const ask=calls.find(call=>call.command==='research_ask_run');
 assert.equal(ask.args.parentReportId,'report-a');
 assert.equal(ask.args.question,'Why this commit?');
 assert.match(ask.args.requestId,/^req-[0-9a-f]{16}$/);
 const stop=[...document.querySelectorAll('.research-actions button')].find(button=>button.textContent==='Stop');
 await click(stop);await settle();
 assert.equal(calls.find(call=>call.command==='research_cancel_run').args.runId,'run-ask');
 await click(document.querySelector('.research-export input'));
 await click(document.querySelector('.research-export button'));await settle();
 assert.deepEqual(calls.find(call=>call.command==='research_export').args,{researchId:'research-target',input:{reportId:'report-a',evidenceIds:[],includeRaw:true}});
 assert.match(document.body.textContent,/Raw evidence can still contain target data/);
 assert.equal(calls.some(call=>legacyExecution.includes(call.command)),false);
});

test('target stop still reaches the run when single-shot admission is off, and follow-up does not',async t=>{
 const {calls,click,render}=setup(t,{research:targetRecord(),turns:{},targets:false,runs:[targetRun('collecting',[])]});
 await render();
 assert.equal(document.querySelector('#research-followup-input'),null);
 assert.match(document.body.textContent,/Single-shot target research is off/);
 const stop=[...document.querySelectorAll('.research-actions button')].find(button=>button.textContent==='Stop');
 await click(stop);
 assert.deepEqual(calls.find(call=>call.command==='research_cancel_run').args,{researchId:'research-target',runId:'run-1'});
 assert.equal(calls.some(call=>legacyExecution.includes(call.command)),false);
});

test('a failed target run retries through research_retry_run',async t=>{
 const {calls,settle,click,render}=setup(t,{research:targetRecord(),turns:{},targets:true,runs:[targetRun('failed',[])]});
 await render();
 await click([...document.querySelectorAll('.research-actions button')].find(button=>button.textContent==='Retry'));await settle();
 const retry=calls.find(call=>call.command==='research_retry_run');
 assert.equal(retry.args.runId,'run-1');
 assert.match(retry.args.requestId,/^req-[0-9a-f]{16}$/);
 assert.equal(calls.some(call=>call.command==='research_retry'),false);
});
