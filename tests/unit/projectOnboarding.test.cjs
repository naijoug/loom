const test=require('node:test'),assert=require('node:assert/strict'),React=require('react'),{renderToStaticMarkup}=require('react-dom/server');
require.extensions['.css']=()=>undefined;
const {AppStateProvider}=require('../../.tmp/test-build/src/state/AppStateContext.js');
const {AddProjectModal}=require('../../.tmp/test-build/src/components/Sidebar/AddProjectModal.js');
test('project dialog does not invent stack or Git analysis before registration',()=>{
 const html=renderToStaticMarkup(React.createElement(AppStateProvider,null,React.createElement(AddProjectModal,{onClose(){}})));
 assert.match(html,/Not yet detected/);assert.doesNotMatch(html,/React|TypeScript|main branch/);
});
