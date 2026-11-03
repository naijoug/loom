require.extensions['.css']=()=>undefined;
const test=require('node:test'),assert=require('node:assert/strict'),{JSDOM}=require('jsdom');
const initial=new JSDOM('<div/>',{url:'http://localhost'});global.window=initial.window;global.document=initial.window.document;global.navigator=initial.window.navigator;
const React=require('react'),{act}=React,{createRoot}=require('react-dom/client');
const {NewDiscussForm}=require('../../.tmp/test-build/src/features/discuss/NewDiscussForm.js');
const {DiscussView}=require('../../.tmp/test-build/src/features/discuss/DiscussView.js');
const {I18nProvider}=require('../../.tmp/test-build/src/i18n/index.js');
const sample=require('../../contracts/tauri-contract.json').modelSamples.discussion;
global.IS_REACT_ACT_ENVIRONMENT=true;
function setup(t){const dom=new JSDOM('<div id="root"/>',{url:'http://localhost'});global.window=dom.window;global.document=dom.window.document;global.navigator=dom.window.navigator;global.localStorage=dom.window.localStorage;const root=createRoot(document.getElementById('root'));t.after(async()=>{await act(async()=>root.unmount());dom.window.close()});return {
 render:async node=>act(async()=>{root.render(React.createElement(I18nProvider,null,node));await new Promise(r=>setTimeout(r,10));}),
 click:async el=>act(async()=>el.dispatchEvent(new window.MouseEvent('click',{bubbles:true}))),
 input:async(el,value)=>act(async()=>{Object.getOwnPropertyDescriptor(Object.getPrototypeOf(el),'value').set.call(el,value);el.dispatchEvent(new window.Event('input',{bubbles:true}));el.dispatchEvent(new window.Event('change',{bubbles:true}));}),
 settle:async()=>act(async()=>{await new Promise(r=>setTimeout(r,20))}),
};}
const agents=sample.participants.map(p=>({id:p.agentId,name:p.name,adapterType:p.adapterType,enabled:true,available:true}));
const diagnostics=agents.map(a=>({agentId:a.id,status:'ready',capabilities:{modelSelection:true}}));
const button=text=>[...document.querySelectorAll('button')].find(b=>b.textContent===text);

test('starting a discussion requires reviewing and confirming the exact call count',async t=>{
 const ui=setup(t),calls=[];let created;
 const api={create:async input=>{calls.push(input);return structuredClone(sample)}};
 await ui.render(React.createElement(NewDiscussForm,{api,agents,diagnostics,projects:[],onCreated:id=>created=id}));
 assert.equal(document.querySelector('button[type=submit]').disabled,true);
 await ui.input(document.querySelector('textarea'),'  Compare queues  ');
 await act(async()=>document.querySelector('form').dispatchEvent(new window.Event('submit',{bubbles:true,cancelable:true})));
 assert.equal(calls.length,0);assert.match(document.querySelector('[role=alertdialog]').textContent,/5 Agent calls/);
 assert.equal(document.querySelector('fieldset').disabled,true);
 await ui.click(button('Cancel'));assert.equal(calls.length,0);
 await ui.click(button('Add participant'));assert.match(document.querySelector('.discuss-new').textContent,/Expected: 7/);
 // New third row reuses a provider and must get a distinct model.
 assert.equal(document.querySelector('button[type=submit]').disabled,true);
 const models=[...document.querySelectorAll('.agent-model-input')];
 await ui.input(models[2],' model-variant ');
 await act(async()=>document.querySelector('form').dispatchEvent(new window.Event('submit',{bubbles:true,cancelable:true})));
 await ui.click(button('Confirm and start'));
 assert.equal(calls.length,1);assert.equal(calls[0].confirmedCalls,7);assert.equal(calls[0].participants.length,3);assert.equal(calls[0].participants[2].model,'model-variant');assert.equal(calls[0].topic,'Compare queues');assert.equal(created,sample.discussionId);
});
function viewApi(d,calls){return {get:async()=>structuredClone(d),readReport:async(_id,taskId)=>({taskId,markdown:'# Answer\n\nUse bounded queues.',refs:[]}),retry:async(id,taskId,n)=>{calls.push(['retry',id,taskId,n]);d.status='running';d.tasks.find(t=>t.id===taskId).status='running';return structuredClone(d)},cancel:async id=>{calls.push(['cancel',id]);return true},remove:async id=>{calls.push(['delete',id])},readLogs:async()=>({text:'record',nextOffset:6,hasMore:false})};}

test('partial results remain readable and only the confirmed failed task is retried',async t=>{
 const ui=setup(t),calls=[],d=structuredClone(sample);d.status='partial';d.tasks[1].status='failed';d.tasks[1].error='Provider failed\n\nAuthentication required';
 const api=viewApi(d,calls);
 await ui.render(React.createElement(DiscussView,{api,discussionId:d.discussionId,revision:0,onDeleted(){}}));await ui.settle();
 assert.equal(document.querySelectorAll('.discuss-report h1').length,1);assert.match(document.querySelector('[data-task=r1-p2]').textContent,/Provider failed/);
 await ui.click(document.querySelector('[data-task=r1-p2] footer button'));assert.equal(calls.length,0);
 await ui.click(button('Confirm 1 call'));assert.deepEqual(calls,[['retry',d.discussionId,'r1-p2',1]]);
 await ui.click(button('Stop all'));assert.equal(calls.at(-1)[0],'cancel');
});

test('outdated summary is clearly marked and delete requires a separate confirmation',async t=>{
 const ui=setup(t),calls=[],d=structuredClone(sample);d.status='partial';d.tasks[4].stale=true;const api=viewApi(d,calls);let deleted=false;
 await ui.render(React.createElement(DiscussView,{api,discussionId:d.discussionId,revision:0,onDeleted(){deleted=true}}));await ui.settle();
 await ui.click(document.querySelectorAll('[role=tab]')[2]);await ui.settle();
 assert.match(document.querySelector('.discuss-stale').textContent,/Outdated/);assert.ok(button('Update this result'));assert.equal(document.querySelectorAll('.discuss-report').length,1);
 await ui.click(document.querySelector('[aria-label="Delete discussion"]'));assert.equal(calls.length,0);
 await ui.click(button('Cancel'));assert.equal(calls.length,0);
 await ui.click(document.querySelector('[aria-label="Delete discussion"]'));await ui.click(button('Delete discussion'));assert.equal(deleted,true);assert.deepEqual(calls,[['delete',d.discussionId]]);
});
