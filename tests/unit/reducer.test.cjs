const test = require('node:test');
const assert = require('node:assert/strict');
const { appReducer, initialAppState } = require('../../.tmp/test-build/src/state/reducer.js');
const project = id => ({ id, path: `/${id}`, name: id });
test('projects preserve current selection on refresh and select a valid fallback after removal', () => {
  let state = appReducer(initialAppState, {type:'projects/recentLoaded',projects:[project('a'),project('b')]});
  assert.equal(state.projects.current.id,'a');
  state = appReducer(state,{type:'projects/selected',projectId:'b'});
  state = appReducer(state,{type:'projects/recentLoaded',projects:[project('a'),project('b')]});
  assert.equal(state.projects.current.id,'b');
  state = appReducer(state,{type:'projects/removed',projectId:'b'});
  assert.equal(state.projects.current.id,'a');assert.equal(state.app.activeProjectId,'a');
  assert.equal(state.tasks,undefined);
});
test('registration deduplicates project paths and enters chat without Task state',()=>{
  let state=appReducer(initialAppState,{type:'projects/registered',project:project('a')});
  state=appReducer(state,{type:'projects/registered',project:{...project('b'),path:'/a'}});
  assert.equal(state.projects.recent.length,1);assert.equal(state.projects.current.id,'b');
  assert.equal(state.app.currentView,'chat');
});
