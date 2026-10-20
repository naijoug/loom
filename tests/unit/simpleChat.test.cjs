const test=require('node:test'),assert=require('node:assert/strict');
const {JSDOM}=require('jsdom');
const dom=new JSDOM('<div id="root"></div>',{url:'http://localhost'});
global.window=dom.window;global.document=dom.window.document;global.navigator=dom.window.navigator;global.localStorage=dom.window.localStorage;
global.IS_REACT_ACT_ENVIRONMENT=true;
const React=require('react'),{act}=React,{createRoot}=require('react-dom/client'),{renderToStaticMarkup}=require('react-dom/server');
const {ChatComposer}=require('../../.tmp/test-build/src/features/chat/ChatComposer.js');
const {ChatTranscript}=require('../../.tmp/test-build/src/features/chat/ChatTranscript.js');
const {ToolCallCard}=require('../../.tmp/test-build/src/features/chat/ToolCallCard.js');
const {defaultAgentId}=require('../../.tmp/test-build/src/features/chat/AgentPicker.js');
const {parseSlashCommand,filterSlashCommands}=require('../../.tmp/test-build/src/features/chat/slashCommands.js');
const {toWorkMode,toPermissionMode}=require('../../.tmp/test-build/src/features/chat/workMode.js');
const {mergeChatPart}=require('../../.tmp/test-build/src/features/chat/state/ChatProjection.js');
const {ErrorBoundary}=require('../../.tmp/test-build/src/components/common/ErrorBoundary.js');
const {I18nProvider,useLocale,useT,translate,normalizeLocale}=require('../../.tmp/test-build/src/i18n/index.js');
const {en}=require('../../.tmp/test-build/src/i18n/en.js');const {zhCN}=require('../../.tmp/test-build/src/i18n/zh-CN.js');
const agent={id:'claude',name:'Claude Code',adapterType:'claude_code_cli',enabled:true,available:true};
const props={draft:'hello',sending:false,error:null,useBackend:true,agentId:'claude',agents:[agent],permissionMode:'explore',resumeHint:false,diagnostics:[],diagnosticsLoading:false,onRefreshDiagnostics(){},onDraftChange(){},onSend(){},onAbort(){},onAgentChange(){},onPermissionChange(){}};
async function fixture(t){const root=createRoot(document.getElementById('root'));t.after(async()=>{await act(async()=>root.unmount());localStorage.clear()});return async element=>act(async()=>root.render(element));}
const key=async(key,extra={})=>act(async()=>document.querySelector('textarea').dispatchEvent(new window.KeyboardEvent('keydown',{key,bubbles:true,...extra})));
test('work modes normalize legacy and unknown values without granting auto mode',()=>{
 for(const value of ['explore','read_only','unknown',null])assert.equal(toWorkMode(value),'research');
 for(const value of ['ask','auto','read_write'])assert.equal(toWorkMode(value),'code');
 assert.equal(toPermissionMode('code'),'ask');assert.equal(toPermissionMode('code',true),'auto');assert.equal(toPermissionMode('unknown',true),'explore');
});
test('slash commands validate arguments and never accept trailing tokens',()=>{
 assert.deepEqual(parseSlashCommand('/mode auto'),{command:'mode',argument:'auto'});
 assert.deepEqual(parseSlashCommand('/agent agent-codex'),{command:'agent',argument:'agent-codex'});
 for(const value of ['/mode root','/new extra','/agent','hello','/export now'])assert.equal(parseSlashCommand(value),null);
 assert.deepEqual(filterSlashCommands('/cl'),['/clear-resume']);
});
test('composer respects readiness, IME, newline, and focused Esc; running mode and agent are locked',async t=>{
 const render=await fixture(t);let sent=0,stopped=0;
 await render(React.createElement(ChatComposer,{...props,canSend:false,onSend(){sent++}}));
 await key('Enter');assert.equal(sent,0);
 await render(React.createElement(ChatComposer,{...props,onSend(){sent++}}));
 await key('Enter',{isComposing:true});await key('Enter',{keyCode:229});await key('Enter',{shiftKey:true});assert.equal(sent,0);
 await key('Enter');assert.equal(sent,1);
 await render(React.createElement(ChatComposer,{...props,sending:true,onAbort(){stopped++}}));
 assert.equal(document.querySelector('[aria-label="Select Agent"]').disabled,true);
 await act(async()=>document.body.dispatchEvent(new window.KeyboardEvent('keydown',{key:'Escape',bubbles:true})));assert.equal(stopped,0);
 await key('Escape');assert.equal(stopped,1);
 await render(React.createElement(ChatComposer,{...props,sending:true,aborting:true,onAbort(){stopped++}}));await key('Escape');assert.equal(stopped,1);
});
test('Code confirmation is per turn, while slash navigation executes the selected command',async t=>{
 const render=await fixture(t);let sent=0;const commands=[];
 await render(React.createElement(ChatComposer,{...props,permissionMode:'ask',onSend(){sent++}}));
 await key('Enter');assert.equal(sent,0);assert.ok(document.querySelector('[role="dialog"]'));
 await act(async()=>[...document.querySelectorAll('button')].find(button=>button.textContent==='Cancel').click());assert.equal(sent,0);
 await key('Enter');await act(async()=>[...document.querySelectorAll('button')].find(button=>button.textContent==='Confirm and send').click());assert.equal(sent,1);
 await render(React.createElement(ChatComposer,{...props,draft:'/',onCommand(command){commands.push(command)}}));
 await key('ArrowDown');await key('ArrowDown');await key('ArrowDown');await key('Enter');assert.deepEqual(commands,['/clear-resume']);
});
test('Agent picker disables unavailable CLIs and remembers only an installed enabled Agent',async t=>{
 assert.equal(defaultAgentId([{...agent,id:'missing',available:false},agent],'missing'),'claude');
 const render=await fixture(t);let selected='';
 await render(React.createElement(ChatComposer,{...props,agents:[agent,{...agent,id:'missing',name:'Missing',available:false}],onAgentChange(id){selected=id}}));
 await act(async()=>document.querySelector('[aria-label="Select Agent"]').click());
 const choices=[...document.querySelectorAll('[role="menuitemradio"]')];assert.equal(choices[1].disabled,true);
 await act(async()=>choices[0].click());assert.equal(selected,'claude');
});
test('tool cards pair by id, preserve metadata, and collapse after completion',async t=>{
 const running={type:'tool',name:'Edit',callId:'c1',kind:'edit',target:'src/a.ts',status:'running',diffStats:{added:2,removed:1},diffPreview:'-old\n+new'};
 const result=mergeChatPart([running],{type:'tool',name:'c1',callId:'c1',outputSummary:'done',status:'done'});
 assert.equal(result.length,1);assert.equal(result[0].name,'Edit');assert.equal(result[0].kind,'edit');
 assert.equal(mergeChatPart([{type:'tool',name:'Read',callId:'legacy'}],{type:'tool',name:'Read',callId:'legacy',status:'running'})[0].status,'running');
 const render=await fixture(t);await render(React.createElement(ToolCallCard,{part:running}));assert.equal(document.querySelector('button').getAttribute('aria-expanded'),'true');
 await render(React.createElement(ToolCallCard,{part:result[0]}));assert.equal(document.querySelector('button').getAttribute('aria-expanded'),'false');
 await act(async()=>document.querySelector('button').click());assert.match(document.body.textContent,/-old/);
});
test('transcript escapes HTML and hides tools when capability is absent',()=>{
 const message={id:'a',role:'assistant',content:'<script>alert(1)</script>\n\n`code`\n\n```html\n<img src=x onerror=alert(1)>\n```',status:'complete',createdAtMs:1,parts:[{type:'tool',name:'Unsafe card',status:'done'}]};
 const html=renderToStaticMarkup(React.createElement(ChatTranscript,{messages:[message],toolEvents:false}));
 assert.doesNotMatch(html,/<script|<img|Unsafe card/);assert.match(html,/&lt;script&gt;/);assert.match(html,/<code/);
});
test('ErrorBoundary contains child failures and retries the view',async t=>{
 const render=await fixture(t);let broken=true;const previous=console.error;console.error=()=>{};t.after(()=>console.error=previous);
 function Child(){if(broken)throw Error('bad message');return React.createElement('p',null,'Recovered')}
 await render(React.createElement(ErrorBoundary,null,React.createElement(Child)));
 assert.match(document.querySelector('[role="alert"]').textContent,/Something went wrong/);
 broken=false;await act(async()=>document.querySelector('button').click());assert.match(document.body.textContent,/Recovered/);
});
test('dictionaries have identical keys, interpolation is text, and locale persists after remount',async t=>{
 assert.deepEqual(Object.keys(en).sort(),Object.keys(zhCN).sort());assert.equal(normalizeLocale('fr'),'en');
 assert.equal(translate('en','exitCode',{code:'<img>'}),'Exit <img>');
 const render=await fixture(t);let locale;
 function Harness(){locale=useLocale();const translate=useT();return React.createElement('p',null,translate('settings'))}
 await render(React.createElement(I18nProvider,null,React.createElement(Harness)));assert.match(document.body.textContent,/Settings/);
 await act(async()=>locale.setLocale('zh-CN'));assert.equal(document.documentElement.lang,'zh-CN');assert.match(document.body.textContent,/设置/);
 await render(null);await render(React.createElement(I18nProvider,null,React.createElement(Harness)));assert.equal(locale.locale,'zh-CN');
});
