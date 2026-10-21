const test=require('node:test'),assert=require('node:assert/strict');
const {JSDOM}=require('jsdom');
const dom=new JSDOM('<div id="root"></div>',{url:'http://localhost'});
global.window=dom.window;global.document=dom.window.document;global.navigator=dom.window.navigator;global.localStorage=dom.window.localStorage;
global.IS_REACT_ACT_ENVIRONMENT=true;
const React=require('react'),{act}=React,{createRoot}=require('react-dom/client'),{renderToStaticMarkup}=require('react-dom/server');
const {ChatTranscript}=require('../../.tmp/test-build/src/features/chat/ChatTranscript.js');
const {inlineCardText}=require('../../.tmp/test-build/src/features/chat/openui/library.js');
const {loadAppSettings}=require('../../.tmp/test-build/src/hooks/useSettingsBridge.js');
const {setInlineCardsEnabled}=require('../../.tmp/test-build/src/features/chat/openui/inlineCardsSetting.js');

const card=`root = CardStack([files, todo])
files = FileList([{path: "src/main.tsx", note: "React entry"}], "Entry")
todo = Checklist([{text: "pnpm build", done: true}], "Verify")`;
const reply=source=>`Here is the summary.\n\n\`\`\`openui-lang\n${source}\n\`\`\``;
const message=(content,status='complete')=>({id:'m1',role:'assistant',content,status,createdAtMs:1,parts:[]});
async function fixture(t){const root=createRoot(document.getElementById('root'));t.after(async()=>{await act(async()=>root.unmount());localStorage.clear()});return async element=>act(async()=>root.render(element));}
// Wait for the lazily loaded OpenUI view to replace the Suspense fallback.
async function settle(selector){for(let i=0;i<50&&!document.querySelector(selector);i++)await act(async()=>new Promise(resolve=>setTimeout(resolve,5)));return document.querySelector(selector);}
const button=label=>[...document.querySelectorAll('.chat-inline-card-actions button')].find(item=>item.textContent===label);

test('cards stay plain source code blocks when the experiment is off',()=>{
 const html=renderToStaticMarkup(React.createElement(ChatTranscript,{messages:[message(reply(card))]}));
 assert.match(html,/<pre><code data-language="openui-lang">root = CardStack/);
 assert.ok(!html.includes('chat-inline-card'));
});

test('a valid finished card renders with Loom-owned source, copy and insert controls',async t=>{
 const render=await fixture(t);const inserted=[];
 await render(React.createElement(ChatTranscript,{messages:[message(reply(card))],inlineCards:true,onInsertText:text=>inserted.push(text)}));
 assert.ok(await settle('.chat-card-stack'));
 assert.equal(document.querySelector('[data-card="files"] code').textContent,'src/main.tsx');
 assert.equal(document.querySelector('[data-card="checklist"] li').dataset.done,'true');
 assert.equal(document.querySelector('.chat-inline-card-note'),null);
 await act(async()=>button('Insert into input').click());
 assert.deepEqual(inserted,['Entry\nsrc/main.tsx — React entry\n\nVerify\n[x] pnpm build']);
 await act(async()=>button('View source').click());
 assert.match(document.querySelector('.chat-inline-card code[data-language="openui-lang"]').textContent,/^root = CardStack/);
 await act(async()=>button('View card').click());
 assert.ok(document.querySelector('.chat-card-stack'));
});

test('invalid or truncated finished cards fall back to their source with a notice',async t=>{
 const render=await fixture(t);
 for(const source of [`root = CardStack([c])\nc = Chart("x")`,`root = CardStack([files])\nfiles = FileList([{path: "src/ma`,'root = CardStack([])']){
  await render(React.createElement(ChatTranscript,{messages:[message(reply(source),'aborted')],inlineCards:true}));
  const note=await settle('.chat-inline-card-note');
  assert.equal(note?.textContent,'This card could not be rendered; showing its source.',source);
  assert.equal(document.querySelector('[data-fallback] code').textContent,source);
  assert.equal(document.querySelector('.chat-inline-card-actions'),null);
 }
});

test('a streaming card renders progressively without falling back, then gains controls when complete',async t=>{
 const render=await fixture(t);
 const partial=`Summary\n\n\`\`\`openui-lang\nroot = CardStack([files])\nfiles = FileList([{path: "src/main.tsx"}, {path: "src/ap`;
 await render(React.createElement(ChatTranscript,{messages:[message(partial,'streaming')],inlineCards:true}));
 assert.ok(await settle('.chat-inline-card[data-streaming]'));
 assert.equal(document.querySelector('.chat-inline-card-note'),null);
 assert.equal(document.querySelector('.chat-inline-card-actions'),null);
 await render(React.createElement(ChatTranscript,{messages:[message(reply(card))],inlineCards:true}));
 assert.ok(await settle('.chat-inline-card-actions'));
 assert.equal(document.querySelector('.chat-inline-card[data-streaming]'),null);
});

test('card plain text covers every component without markup',()=>{
 const text=inlineCardText(`root = CardStack([c, t])
c = ChangeSummary("Changes", [{path: "a.ts", added: 3, removed: 1}], ["Add parser"])
t = DataTable(["Name", "Size"], [["main", "60 KB"]], "Chunks")`);
 assert.equal(text,'Changes\na.ts +3 −1\n- Add parser\n\nChunks\nName | Size\nmain | 60 KB');
 assert.equal(inlineCardText('not a program'),'');
});

test('the setting store persists the opt-in flag',async t=>{
 t.after(()=>localStorage.clear());
 assert.equal((await loadAppSettings()).experimentalInlineCards,false);
 await setInlineCardsEnabled(true);
 assert.equal((await loadAppSettings()).experimentalInlineCards,true);
 await setInlineCardsEnabled(false);
 assert.equal((await loadAppSettings()).experimentalInlineCards,false);
});
