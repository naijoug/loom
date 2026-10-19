const test=require('node:test');const assert=require('node:assert/strict');const {execFileSync}=require('node:child_process');const path=require('node:path');
const {SessionReplica}=require('../../.tmp/test-build/src/workbench/projection.js');
const {SessionController}=require('../../.tmp/test-build/src/workbench/sessionController.js');
const root=path.resolve(__dirname,'../..');
// Produce events and expected snapshots from the real Rust core on every run.
const fixture=JSON.parse(execFileSync('cargo',['run','--quiet','--offline','-p','loom-core','--example','projection_fixture'],{cwd:root,encoding:'utf8'}));
const clone=value=>JSON.parse(JSON.stringify(value));
const replica=()=>new SessionReplica(fixture.initial.metadata.key);
const deferred=()=>{let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b;});return{promise,resolve,reject};};

test('frontend replay equals real Rust projections including tools, cancellation and notes',()=>{
 const r=replica();r.snapshot(fixture.initial);for(const event of fixture.events)r.receive(event);assert.deepEqual(clone(r.view),fixture.expected);assert.equal(Object.getPrototypeOf(r.view.tools),Object.prototype);assert.ok(Object.prototype.hasOwnProperty.call(r.view.tools,'__proto__'));
});
test('events arriving before a snapshot, out of order and duplicated do not lose output',()=>{
 const r=replica();for(const event of [...fixture.events].reverse())r.receive(event);r.receive(fixture.foreign);assert.equal(r.view,null);r.snapshot(fixture.initial);for(const event of fixture.events)r.receive(event);assert.deepEqual(clone(r.view),fixture.expected);
 r.snapshot(fixture.intermediate);assert.deepEqual(clone(r.view),fixture.expected);
});
test('a missing event stays a gap until replay fills it; a wrong-scope snapshot is rejected',()=>{
 const r=replica();r.snapshot(fixture.initial);for(const event of fixture.events.slice(1))r.receive(event);assert.equal(r.after,fixture.initial.lastSeq);assert.equal(r.hasGap,true);r.receive(fixture.events[0]);assert.deepEqual(clone(r.view),fixture.expected);
 const bad=clone(fixture.initial);bad.metadata.key.sessionId='other';assert.throws(()=>r.snapshot(bad));
});
test('buffer pressure requests a fresh snapshot rather than consuming unbounded history',()=>{
 const r=replica();for(let i=0;i<520;i++)r.receive({...fixture.events[0],seq:1000+i});assert.equal(r.needsSnapshot,true);r.snapshot(fixture.expected);assert.equal(r.view.lastSeq,fixture.expected.lastSeq);
});
test('controller survives StrictMode stop/start and ignores old generation responses',async()=>{
 const first=deferred(),second=deferred();const callbacks=[];let watchCount=0,closed=0;
 const client={watch(_key,onMessage){callbacks.push(onMessage);return ++watchCount===1?first.promise:second.promise;},async request(){return{events:[],hasMore:false,lastSeq:fixture.expected.lastSeq};}};
 const controller=new SessionController(fixture.initial.metadata.key,client);const old=controller.start();await controller.dispose();const current=controller.start();
 second.resolve({snapshot:fixture.initial,close:async()=>{closed++;}});await current;
 callbacks[1]({kind:'event',event:fixture.events[0]});
 first.resolve({snapshot:fixture.expected,close:async()=>{closed++;}});await old;callbacks[0]({kind:'event',event:fixture.events[fixture.events.length-1]});
 assert.equal(closed,1);assert.equal(controller.getSnapshot().view.lastSeq,fixture.events[0].seq);await controller.dispose();assert.equal(closed,2);
});
test('controller repairs missing delivery through the production event page shape',async()=>{
 let notify;let pages=0;const client={async watch(_key,fn){notify=fn;return{snapshot:fixture.initial,close:async()=>{}};},async request(command){if(command.method==='read_events'){pages++;return{events:fixture.events.filter(event=>event.seq>command.params.after),lastSeq:fixture.expected.lastSeq,hasMore:false};}return fixture.expected;}};
 const controller=new SessionController(fixture.initial.metadata.key,client);await controller.start();notify({kind:'event',event:fixture.events[2]});await controller.sync();assert.ok(pages>=1);assert.deepEqual(clone(controller.getSnapshot().view),fixture.expected);await controller.dispose();
});
test('backpressure before the initial watch response does not strand a dead subscription',async()=>{
 const first=deferred();let notify,count=0,closed=0;
 const client={watch(_key,handler){notify=handler;count++;return count===1?first.promise:Promise.resolve({snapshot:fixture.expected,close:async()=>{closed++;}});},async request(command){return command.method==='get_session'?fixture.expected:{events:[],lastSeq:fixture.expected.lastSeq,hasMore:false};}};
 const controller=new SessionController(fixture.initial.metadata.key,client);const starting=controller.start();notify({kind:'resync_required'});await controller.sync();first.resolve({snapshot:fixture.initial,close:async()=>{closed++;}});await starting;await controller.sync();await new Promise(resolve=>setImmediate(resolve));assert.ok(count>=2);assert.deepEqual(clone(controller.getSnapshot().view),fixture.expected);assert.equal(closed,1);await controller.dispose();
});
