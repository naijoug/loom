require.extensions['.css']=()=>undefined;
const test=require('node:test'),assert=require('node:assert/strict');
const {JSDOM}=require('jsdom');const dom=new JSDOM('<div id="root"></div>',{url:'http://localhost'});
global.window=dom.window;global.document=dom.window.document;global.navigator=dom.window.navigator;global.localStorage=dom.window.localStorage;global.IS_REACT_ACT_ENVIRONMENT=true;
const React=require('react'),{act}=React,{createRoot}=require('react-dom/client');
const {InstallPanel}=require('../../.tmp/test-build/src/features/skills/InstallPanel.js');
const {InstalledSkills}=require('../../.tmp/test-build/src/features/skills/InstalledSkills.js');
const samples=require('../../contracts/tauri-contract.json').modelSamples;
const status={target:samples.skillsInstallTarget,skillId:'skill-a',destination:'/fixture/project/.codex/skills/fixture-demo',state:'absent',installation:null,updateAvailable:false,message:null};
const plan={...samples.skillsInstallPlan,projectPath:'/one'};
const receipt=samples.skillsInstallReceipt;
function defer(){let resolve;const promise=new Promise(r=>resolve=r);return{resolve,promise};}
function api(overrides={}){return{installStatus:async()=>[status],planInstall:async()=>plan,planUninstall:async()=>({...plan,action:'uninstall',source:null,items:plan.items.map(item=>({...item,decision:'uninstall'}))}),applyInstall:async()=>receipt,installations:async()=>[],pending:async()=>[],recover:async()=> 'rolled_back',...overrides};}
async function mount(t,Component,props){const root=createRoot(document.getElementById('root'));t.after(async()=>{await act(async()=>root.unmount())});await act(async()=>root.render(React.createElement(Component,props)));return root;}
async function click(node){assert.ok(node);await act(async()=>node.dispatchEvent(new window.MouseEvent('click',{bubbles:true})));}
const button=text=>[...document.querySelectorAll('button')].find(node=>node.textContent===text);

test('new targets start unchecked and selecting one produces a concrete plan before any write',async t=>{
 const calls=[];const backend=api({planInstall:async input=>{calls.push(['plan',input]);return plan;},applyInstall:async(id,project)=>{calls.push(['apply',id,project]);return receipt;}});
 await mount(t,InstallPanel,{api:backend,skillId:'skill-a',projectPath:'/one'});
 assert.equal(document.querySelector('.skills-install-targets input[type="checkbox"]').checked,false);assert.equal(button('Preview install / update').disabled,true);
 await click(document.querySelector('.skills-install-targets input[type="checkbox"]'));await click(button('Preview install / update'));
 assert.equal(calls.length,1);assert.equal(calls[0][1].projectPath,'/one');assert.equal(calls[0][1].mode,'copy');assert.equal(calls[0][1].conflictPolicy,'fail');
 assert.match(document.querySelector('.skills-plan').textContent,/\/fixture\/project\/\.codex\/skills\/fixture-demo/);
 await click(button('Apply these changes'));assert.deepEqual(calls[1],['apply','install-fixture','/one']);assert.match(document.querySelector('.skills-install-results').textContent,/Installed/);
});

test('late status and plan responses cannot carry selections into another project',async t=>{
 const late=defer(),latePlan=defer();let project='/one';
 const backend=api({installStatus:async(_id,path)=>path==='/one'?late.promise:[{...status,target:{...status.target,skillsDir:'/two/.codex/skills'},destination:'/two/.codex/skills/fixture-demo'}],planInstall:()=>latePlan.promise});
 const root=await mount(t,InstallPanel,{api:backend,skillId:'skill-a',projectPath:project});
 project='/two';await act(async()=>root.render(React.createElement(InstallPanel,{api:backend,skillId:'skill-b',projectPath:project})));
 await act(async()=>late.resolve([{...status,state:'installed',installation:samples.skillsInstallation}]));
 assert.equal(document.querySelector('.skills-install-targets input[type="checkbox"]').checked,false);assert.match(document.querySelector('.skills-install-targets').textContent,/\/two\//);
 await click(document.querySelector('.skills-install-targets input[type="checkbox"]'));await act(async()=>button('Preview install / update').dispatchEvent(new window.MouseEvent('click',{bubbles:true})));
 await act(async()=>root.render(React.createElement(InstallPanel,{api:backend,skillId:'skill-c',projectPath:'/three'})));
 await act(async()=>latePlan.resolve(plan));assert.equal(document.querySelector('.skills-plan'),null);
});

test('an in-flight apply keeps its reviewed project and does not overwrite a new panel result',async t=>{
 const applied=defer();const calls=[];const backend=api({applyInstall:(id,path)=>{calls.push([id,path]);return applied.promise;}});
 const root=await mount(t,InstallPanel,{api:backend,skillId:'skill-a',projectPath:'/one'});
 await click(document.querySelector('.skills-install-targets input[type="checkbox"]'));await click(button('Preview install / update'));
 await act(async()=>button('Apply these changes').dispatchEvent(new window.MouseEvent('click',{bubbles:true})));
 await act(async()=>root.render(React.createElement(InstallPanel,{api:backend,skillId:'skill-b',projectPath:'/two'})));
 await act(async()=>applied.resolve(receipt));assert.deepEqual(calls,[['install-fixture','/one']]);assert.equal(document.querySelector('.skills-install-results'),null);
});

test('legacy and blocked targets cannot be selected and a blocked plan cannot be applied',async t=>{
 const backend=api({installStatus:async()=>[{...status,state:'legacy',message:'Migration required'}, {...status,target:{...status.target,id:'amp-global',blockedReason:'Path escapes target'},state:'unavailable'}]});
 await mount(t,InstallPanel,{api:backend,skillId:'skill-a',projectPath:null});
 assert.ok([...document.querySelectorAll('.skills-install-targets input[type="checkbox"]')].every(node=>node.disabled));assert.equal(button('Preview install / update').disabled,true);
});

test('managed inventory removes an orphan installation by its saved identity without needing its source',async t=>{
 let removed=false;const calls=[];const backend=api({installations:async()=>removed?[]:[{...status,state:'installed',installation:samples.skillsInstallation}],planUninstall:async(ids,path)=>{calls.push([ids,path]);return{...plan,action:'uninstall',source:null,items:plan.items.map(item=>({...item,decision:'uninstall'}))};},applyInstall:async()=>{removed=true;return{...receipt,items:receipt.items.map(item=>({...item,status:'uninstalled'}))};}});
 await mount(t,InstalledSkills,{api:backend,projectPath:'/one',revision:0});await click(button('Preview uninstall'));await click(button('Apply these changes'));
 assert.deepEqual(calls,[[[samples.skillsInstallation.installationId],'/one']]);assert.match(document.body.textContent,/No managed installations/);assert.match(document.querySelector('.skills-install-results').textContent,/Uninstalled/);
});

test('a transient status-lock failure can be refreshed without losing the panel',async t=>{
 let attempts=0;const backend=api({installStatus:async()=>{if(attempts++===0)throw {code:'lock_busy',message:'Another client is committing'};return[status];}});
 await mount(t,InstallPanel,{api:backend,skillId:'skill-a',projectPath:'/one'});assert.match(document.body.textContent,/Another client is committing/);
 await click(button('Reload status'));assert.doesNotMatch(document.body.textContent,/Another client is committing/);assert.equal(document.querySelector('.skills-install-targets input[type="checkbox"]').checked,false);
});

test('event refresh and action refresh serialize status reads and preserve initial selection',async t=>{
 const first=defer();let calls=0;const backend=api({installStatus:()=>{calls++;return calls===1?first.promise:Promise.resolve([{...status,state:'installed',installation:samples.skillsInstallation}]);}});
 const root=await mount(t,InstallPanel,{api:backend,skillId:'skill-a',projectPath:'/one',revision:0});assert.equal(calls,1);
 await act(async()=>root.render(React.createElement(InstallPanel,{api:backend,skillId:'skill-a',projectPath:'/one',revision:1})));assert.equal(calls,1,'second read waits for the first lock owner');
 await act(async()=>first.resolve([status]));assert.equal(calls,2);assert.equal(document.querySelector('.skills-install-targets input[type="checkbox"]').checked,true);
});


test('instruction review lists affected files and wrappers require an explicit selection',async t=>{
 const calls=[];const backend=api({planInstall:async input=>{calls.push(input);return {...plan,items:plan.items.map(item=>({...item,resourceChanges:[{path:'/one/AGENTS.md',before:{kind:'missing'}},{path:'/private/coord/shared-instructions/ledger.json',before:{kind:'missing'}}]}))};}});
 await mount(t,InstallPanel,{api:backend,skillId:'skill-a',projectPath:'/one'});
 assert.equal(document.querySelector('.skills-wrapper-option input').checked,false);
 await click(document.querySelector('.skills-wrapper-option input'));
 assert.equal(button('Preview install / update').disabled,true,'wrapper choice does not select an installation target');
 await click(document.querySelector('.skills-install-targets input'));await click(button('Preview install / update'));
 assert.equal(calls[0].withSlashCommands,true);assert.match(document.querySelector('.skills-plan').textContent,/AGENTS.md/);
 assert.doesNotMatch(document.querySelector('.skills-plan').textContent,/ledger.json/);
});

test('instruction diagnostics display stale and malformed state without triggering a write',async t=>{
 const checks=[{path:'/one/AGENTS.md',state:'stale',message:null},{path:'/one/.claude/commands/demo.md',state:'malformed',message:'Owner unavailable'}];
 let writes=0;const backend=api({installations:async()=>[{...status,state:'installed',installation:samples.skillsInstallation,instructionChecks:checks}],applyInstall:async()=>{writes++;return receipt;}});
 await mount(t,InstalledSkills,{api:backend,projectPath:'/one',revision:0});
 assert.match(document.body.textContent,/changed; review/);assert.match(document.body.textContent,/Owner unavailable/);
 await click(button('Reload status'));assert.equal(writes,0);
});
