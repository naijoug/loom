const test=require('node:test'),assert=require('node:assert/strict');
const {JSDOM}=require('jsdom');const dom=new JSDOM('<div id="root"></div>',{url:'http://localhost'});
global.window=dom.window;global.document=dom.window.document;global.navigator=dom.window.navigator;global.localStorage=dom.window.localStorage;global.IS_REACT_ACT_ENVIRONMENT=true;
const React=require('react'),{act}=React,{createRoot}=require('react-dom/client');
const {TranslationSettings}=require('../../.tmp/test-build/src/features/skills/TranslationSettings.js');
const data={revision:7,providers:[{id:'openai',label:'OpenAI',configured:true,supportsApiKey:true,storedKey:true,keySource:'environment',model:'gpt-5-mini',storedModel:'gpt-5',modelFromEnvironment:true},{id:'codex',label:'Local Codex',configured:true,supportsApiKey:false,storedKey:false,keySource:'local_cli',model:null,storedModel:null,modelFromEnvironment:false}]};
function deferred(){let resolve;const promise=new Promise(r=>resolve=r);return{promise,resolve};}
async function mount(t,api){const root=createRoot(document.getElementById('root'));t.after(async()=>{await act(async()=>root.unmount())});await act(async()=>root.render(React.createElement(TranslationSettings,{api})));return root;}
async function input(node,value){await act(async()=>{Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype,'value').set.call(node,value);node.dispatchEvent(new window.Event('input',{bubbles:true}));});}
async function select(node,value){await act(async()=>{node.value=value;node.dispatchEvent(new window.Event('change',{bubbles:true}));});}
async function submit(form){await act(async()=>form.dispatchEvent(new window.Event('submit',{bubbles:true,cancelable:true})));}
function form(){return document.querySelector('form[aria-label="OpenAI"]');}
test('provider form preserves hidden key and submits only explicit changes with revision',async t=>{
 let request;await mount(t,{translationProviders:async()=>data,saveTranslationConfig:async r=>{request=r;return{...data,revision:8}}});
 assert.match(form().textContent,/environment key/);assert.equal(document.querySelector('input[type="password"]'),null);
 assert.equal(document.querySelector('form[aria-label="Local Codex"] select'),null);
 await input(form().querySelector('input'),'gpt-5-nano');await submit(form());
 assert.deepEqual(request,{providerId:'openai',expectedRevision:7,key:{action:'keep'},model:'gpt-5-nano'});
 assert.match(document.body.textContent,/settings saved/);assert.equal(form().querySelector('button').disabled,false);
});
test('replace clears password after failure and leaves key action explicit',async t=>{
 let request;await mount(t,{translationProviders:async()=>data,saveTranslationConfig:async r=>{request=r;throw{code:'conflict',message:'Settings changed; reload'}}});
 await select(form().querySelector('select'),'set');await input(form().querySelector('input[type="password"]'),'private-marker');await submit(form());
 assert.deepEqual(request.key,{action:'set',value:'private-marker'});assert.match(form().textContent,/Settings changed/);assert.equal(form().querySelector('input[type="password"]'),null);assert.equal(form().querySelector('select').value,'keep');assert.doesNotMatch(document.body.innerHTML,/private-marker/);
});
test('clear warns that environment key remains and serializes clear without a value',async t=>{
 let request;await mount(t,{translationProviders:async()=>data,saveTranslationConfig:async r=>{request=r;return{...data,revision:8}}});await select(form().querySelector('select'),'clear');assert.match(form().textContent,/does not clear the environment/);await submit(form());assert.deepEqual(request.key,{action:'clear'});
});
test('stale provider load cannot overwrite a new backend and pending save disables all forms',async t=>{
 const old=deferred(),save=deferred();const root=await mount(t,{translationProviders:()=>old.promise,saveTranslationConfig:async()=>data});
 const api={translationProviders:async()=>data,saveTranslationConfig:()=>save.promise};await act(async()=>root.render(React.createElement(TranslationSettings,{api})));
 await act(async()=>old.resolve({revision:0,providers:[]}));assert.ok(form());await submit(form());assert.ok([...document.querySelectorAll('form button')].every(b=>b.disabled));
 await act(async()=>save.resolve({...data,revision:8}));assert.ok([...document.querySelectorAll('form button')].every(b=>!b.disabled));
});

test('saving one provider retains unsaved model changes in another provider',async t=>{
 await mount(t,{translationProviders:async()=>data,saveTranslationConfig:async()=>({...data,revision:8})});
 const other=document.querySelector('form[aria-label="Local Codex"] input');await input(other,'local-draft-model');await submit(form());assert.equal(document.querySelector('form[aria-label="Local Codex"] input').value,'local-draft-model');
});
