require.extensions['.css']=()=>undefined;
const test=require('node:test'),assert=require('node:assert/strict');
const {JSDOM}=require('jsdom');
const dom=new JSDOM('<div id="root"></div>',{url:'http://localhost'});
global.window=dom.window;global.document=dom.window.document;global.navigator=dom.window.navigator;global.localStorage=dom.window.localStorage;
global.IS_REACT_ACT_ENVIRONMENT=true;
const React=require('react'),{act}=React,{createRoot}=require('react-dom/client');
const {SkillsPanel}=require('../../.tmp/test-build/src/features/skills/SkillsPanel.js');
const {visibleSkills,mergeOperation,skillBody}=require('../../.tmp/test-build/src/features/skills/model.js');
const source={id:'src-one',name:'Repository',kind:'local',localPath:'/fixture',git:null,revision:null,updatedAtMs:1};
const skill=(id,title)=>({id,sourceId:source.id,relativePath:`manual/${id}/SKILL.md`,name:id,title,description:'Useful skill',category:'manual/tool',keywords:'测试 keyword'});
const alpha=skill('a','Alpha'),beta=skill('b','Beta');
const library={sources:[source],skills:[alpha,beta],issues:[]};
const detail=item=>({summary:item,content:`# ${item.title}\n\n${item.title} body`,manifest:{},relatedFiles:[]});
function deferred(){let resolve,reject;const promise=new Promise((yes,no)=>{resolve=yes;reject=no});return{promise,resolve,reject};}
function api(overrides={}){return{list:async()=>library,detail:async id=>detail(id==='a'?alpha:beta),addLocal:async()=>library,remove:async()=>({sources:[],skills:[],issues:[]}),importRepository:async()=>{throw Error('unused')},refresh:async()=>{throw Error('unused')},operations:async()=>[],cancel:async()=>{},subscribe:async()=>()=>{},...overrides};}
async function mount(t,backend){const root=createRoot(document.getElementById('root'));t.after(async()=>{await act(async()=>root.unmount())});await act(async()=>root.render(React.createElement(SkillsPanel,{api:backend,desktop:true})));return root;}
async function click(node){assert.ok(node);await act(async()=>node.dispatchEvent(new window.MouseEvent('click',{bubbles:true})));}
async function input(node,value){await act(async()=>{Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype,'value').set.call(node,value);node.dispatchEvent(new window.Event('input',{bubbles:true}));});}

test('detail responses remain bound to the selected skill',async t=>{
 const old=deferred(),current=deferred();
 await mount(t,api({detail:id=>id==='a'?old.promise:current.promise}));
 await click([...document.querySelectorAll('.skills-list button')].find(node=>node.textContent.includes('Beta')));
 await act(async()=>current.resolve(detail(beta)));
 assert.match(document.querySelector('.skills-detail').textContent,/Beta body/);
 await act(async()=>old.resolve(detail(alpha)));
 assert.doesNotMatch(document.querySelector('.skills-detail').textContent,/Alpha body/);
});

test('filtering to no results clears old detail and clearing search restores selection',async t=>{
 await mount(t,api());
 assert.match(document.querySelector('.skills-detail').textContent,/Alpha body/);
 await input(document.querySelector('input[aria-label="Search skills…"]'),'missing');
 assert.match(document.querySelector('.skills-list').textContent,/No matching skills/);
 assert.doesNotMatch(document.querySelector('.skills-detail').textContent,/Alpha body/);
 await click([...document.querySelectorAll('button')].find(node=>node.textContent==='Clear search'));
 assert.match(document.querySelector('.skills-detail').textContent,/Alpha body/);
});

test('removing a source clears its selected detail and forwards only its identity',async t=>{
 let removed;
 await mount(t,api({remove:async id=>{removed=id;return{sources:[],skills:[],issues:[]}}}));
 await click([...document.querySelectorAll('.skills-sources > button')].find(node=>node.textContent.includes('Repository')));
 await click([...document.querySelectorAll('button')].find(node=>node.textContent==='Remove source'));
 assert.equal(removed,'src-one');
 assert.match(document.querySelector('.skills-list').textContent,/No skills yet/);
 assert.doesNotMatch(document.querySelector('.skills-detail').textContent,/Alpha body/);
});

test('operation subscription is released and stale sequences cannot replace terminal status',async t=>{
 let receive,unsubscribed=false;const old=deferred();
 const root=await mount(t,api({detail:()=>old.promise,subscribe:async handler=>{receive=handler;return()=>{unsubscribed=true}}}));
 const record={operationId:'op',sourceId:'src-one',action:'refresh',phase:'failed',stage:'stopped',sequence:4,startedAtMs:1,updatedAtMs:2,error:{code:'source_unavailable',message:'Fixture failure'},executions:[]};
 await act(async()=>{receive(record);receive({...record,phase:'running',sequence:1,error:null});});
 assert.match(document.querySelector('.skills-operations').textContent,/Fixture failure/);
 assert.doesNotMatch(document.querySelector('.skills-operations').textContent,/In progress/);
 await act(async()=>root.unmount());
 assert.ok(unsubscribed);
 await act(async()=>old.resolve(detail(alpha)));
});

test('search includes metadata keywords and source filters without changing canonical IDs',()=>{
 assert.equal(visibleSkills(library,'src-one','测试')[0].id,'a');
 assert.equal(visibleSkills(library,'another','').length,0);
 const completed={operationId:'op',sequence:5,startedAtMs:1,phase:'committed'};
 assert.deepEqual(mergeOperation([completed],{...completed,sequence:2,phase:'running'}),[completed]);
});

test('overview removes frontmatter while ordinary separators and raw source remain intact',()=>{
 const raw='---\nname: demo\ndescription: Text\n---\n# Body\n';
 assert.equal(skillBody(raw),'# Body\n');
 assert.equal(skillBody('# Body\n\n---\n'), '# Body\n\n---\n');
 assert.equal(raw.startsWith('---'),true);
});
