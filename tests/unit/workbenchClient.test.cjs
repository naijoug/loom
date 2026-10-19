const test=require('node:test');const assert=require('node:assert/strict');const {createWorkbenchClient}=require('../../.tmp/test-build/src/api/workbenchClient.js');
test('secret-bearing saves use the dedicated host boundary and reject another operation response',async()=>{
 const request={id:'connection',operationId:'save-id',expectedRevision:null,config:{name:'Fixture'},credential:{kind:'replace',value:'synthetic-private-fixture'}};let observed;
 const client=createWorkbenchClient(async(command,args)=>{observed={command,args};return{id:'connection',operationId:'save-id',revision:1,credentialRef:'opaque-reference'};},fn=>fn);
 const saved=await client.saveConnection(request);assert.equal(observed.command,'workbench_save_connection');assert.equal(observed.args.request,request);assert.ok(!JSON.stringify(saved).includes('synthetic-private-fixture'));
 const stale=createWorkbenchClient(async()=>({id:'connection',operationId:'other-save',revision:1}),fn=>fn);await assert.rejects(()=>stale.saveConnection(request),/不匹配/);
});
test('wire client uses generated command shape and checks request ownership and response type',async()=>{
 const calls=[];const client=createWorkbenchClient(async(command,args)=>{calls.push({command,args});return{protocolVersion:1,requestId:'request',status:'success',result:{type:'workspaces',data:[]}};},fn=>fn,()=> 'request');
 assert.deepEqual(await client.request({method:'list_workspaces'},'workspaces'),[]);assert.equal(calls[0].command,'workbench_invoke');assert.deepEqual(calls[0].args.request,{protocolVersion:1,requestId:'request',command:{method:'list_workspaces'}});await assert.rejects(()=>client.request({method:'list_workspaces'},'session'),/类型不匹配/);
 const wrong=createWorkbenchClient(async()=>({protocolVersion:1,requestId:'other',status:'success',result:{type:'workspaces',data:[]}}),fn=>fn,()=> 'request');await assert.rejects(()=>wrong.request({method:'list_workspaces'},'workspaces'),/归属/);
});
test('watch uses one channel, closes once, and drops delayed messages after close',async()=>{
 const calls=[];let callback;let observed=0;const key={workspaceId:'w',sessionId:'s'};
 const client=createWorkbenchClient(async(command,args)=>{calls.push({command,args});return{metadata:{key}};},fn=>{callback=fn;return'channel';},()=> 'watch');
 const watch=await client.watch(key,()=>{observed++;});callback({kind:'resync_required'});await watch.close();await watch.close();callback({kind:'resync_required'});assert.equal(observed,1);assert.deepEqual(calls.map(item=>item.command),['workbench_watch','workbench_unwatch']);assert.deepEqual(calls[0].args,{key,watchId:'watch',onEvent:'channel'});
});
test('workspace watcher invalidates only its workspace and remains closable',async()=>{
 let callback,changed=0;const calls=[];const client=createWorkbenchClient(async(command,args)=>{calls.push({command,args});},fn=>{callback=fn;return'channel';},()=> 'workspace-watch');
 const watch=await client.watchWorkspace('w',()=>{changed++;});callback({kind:'workspace_changed',notice:{workspaceId:'other',sessionId:'s',seq:4}});callback({kind:'workspace_changed',notice:{workspaceId:'w',sessionId:'s',seq:4}});callback({kind:'resync_required'});await watch.close();callback({kind:'workspace_changed',notice:{workspaceId:'w',sessionId:'s',seq:5}});
 assert.equal(changed,2);assert.equal(calls[0].command,'workbench_watch_workspace');assert.equal(calls[1].command,'workbench_unwatch');
});

test('permission writes use a dedicated validated host command and compare scope independent of key order',async()=>{
 const request={scope:{workspaceId:'w',kind:'workspace'},operationId:'save-policy',expectedRevision:null,config:{allowedWritePaths:['/tmp/test/**']}};
 const calls=[];const client=createWorkbenchClient(async(command,args)=>{calls.push({command,args});return{scope:{kind:'workspace',workspaceId:'w'},revision:1,operationId:'save-policy',config:request.config,updatedAtMs:1};},fn=>fn);
 const saved=await client.savePermissions(request);assert.equal(saved.revision,1);assert.equal(calls[0].command,'workbench_save_permissions');assert.equal(calls[0].args.request,request);
 const wrong=createWorkbenchClient(async()=>({...saved,scope:{kind:'workspace',workspaceId:'other'}}),fn=>fn);await assert.rejects(()=>wrong.savePermissions(request),/不匹配/);
});
