const test = require('node:test');
const assert = require('node:assert/strict');
const { JSDOM } = require('jsdom');
const dom = new JSDOM('<div id="root"></div>', {url: 'http://localhost'});
global.window = dom.window; global.document = dom.window.document; global.navigator = dom.window.navigator;
global.IS_REACT_ACT_ENVIRONMENT = true;
dom.window.HTMLDialogElement.prototype.showModal = function () { this.open = true; };
dom.window.HTMLDialogElement.prototype.close = function () { this.open = false; };
const React = require('react'); const {act} = React; const {createRoot} = require('react-dom/client');
const {workbenchClient: client} = require('../../.tmp/test-build/src/api/workbenchClient.js');
const {LegacyPreviewDialog} = require('../../.tmp/test-build/src/workbench/LegacyPreviewDialog.js');
const deferred = () => { let resolve; const promise = new Promise(r => {resolve = r;}); return {promise,resolve}; };
const project = id => ({id,workspaceId:'w',name:id,directory:`/projects/${id}`,createdAtMs:1});
const report = id => ({projectDirectory:`/projects/${id}`,fingerprint:'abc',files:[],issues:[],items:[{kind:'chat',sourceId:id,sourcePath:'.loom/chat/sessions/a.json',sourceVersion:1,title:`<img src=x onerror=alert(1)> ${id}`,messageCount:2,contentHash:'hash',issues:[]}]});

test('preview uses scoped project IDs, ignores stale response, escapes source labels and retries without importing', async () => {
  const original=client.request;const pending=deferred();const requests=[];let closed=0;
  client.request=async command=>{requests.push(command);return command.params.projectId==='a'?pending.promise:report('b');};
  const root=createRoot(document.getElementById('root'));
  const render=id=>root.render(React.createElement(LegacyPreviewDialog,{project:project(id),onClose:()=>{closed++;}}));
  try {
    await act(async()=>render('a'));
    assert.equal(document.activeElement.getAttribute('aria-label'),'关闭预检');
    await act(async()=>render('b'));await act(async()=>pending.resolve(report('stale-a')));
    assert.match(document.body.textContent,/可读取 1 项/);assert.doesNotMatch(document.body.textContent,/stale-a/);
    assert.equal(document.querySelector('img'),null);
    const retry=[...document.querySelectorAll('button')].find(node=>node.textContent==='重新检查');
    await act(async()=>retry.click());assert.equal(requests.length,3);
    assert.ok(requests.every(command=>command.method==='preview_legacy_import'&&command.params.workspaceId==='w'));
    assert.equal(requests[1].params.projectId,'b');assert.match(document.body.textContent,/不会执行导入/);
    let prepares=0,executes=0;const execution=deferred();
    client.request=async command=>{
      if(command.method==='prepare_legacy_import'){prepares++;assert.equal(command.params.expectedFingerprint,'abc');return{id:'plan',workspaceId:'w',projectId:'b',createdAtMs:1,preview:report('b')};}
      if(command.method==='execute_legacy_import'){executes++;assert.equal(command.params.planId,'plan');return execution.promise;}
      throw new Error(`unexpected ${command.method}`);
    };
    const importButton=[...document.querySelectorAll('button')].find(node=>node.textContent==='导入可读取的记录');
    await act(async()=>{importButton.click();importButton.click();});assert.equal(prepares,1);assert.equal(executes,1);
    await act(async()=>execution.resolve({planId:'plan',workspaceId:'w',completedAtMs:2,results:[{kind:'chat',sourceId:'b',sessionKey:{workspaceId:'w',sessionId:'imported'},status:'imported',error:null,issues:[]}]}));
    assert.match(document.body.textContent,/新增 1 项/);
    await act(async()=>[...document.querySelectorAll('button')].find(node=>node.textContent==='重试导入').click());assert.equal(prepares,1);assert.equal(executes,2);
    await act(async()=>document.querySelector('[aria-label="关闭预检"]').click());assert.equal(closed,1);
  } finally { await act(async()=>root.unmount());client.request=original;dom.window.close(); }
});
