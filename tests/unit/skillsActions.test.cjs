const test = require('node:test'), assert = require('node:assert/strict');
const { JSDOM } = require('jsdom');
const dom = new JSDOM('<div id="root"></div>', {url:'http://localhost'});
Object.assign(global, {window:dom.window,document:dom.window.document,localStorage:dom.window.localStorage,IS_REACT_ACT_ENVIRONMENT:true});
Object.defineProperty(global,'navigator',{configurable:true,value:dom.window.navigator});
const React = require('react'), {act} = React, {createRoot} = require('react-dom/client');
const {SkillActions, shareText} = require('../../.tmp/test-build/src/features/skills/SkillActions.js');
const detail = {summary:{id:'a',title:'Alpha',relativePath:'manual/alpha/SKILL.md'},content:'# Original\n```sh\necho example\n```',contentDigest:'a',relatedFiles:[{relativePath:'reference.md',content:'Reference text',sizeBytes:14},{relativePath:'image.png',content:null,sizeBytes:42}]};
async function mount(t, props = {}) { const root=createRoot(document.getElementById('root')); t.after(async()=>act(async()=>root.unmount())); const all={detail,sourceName:'Fixture source',onSearch(){},onSources(){},onInstall(){},onTranslate(){},onSettings(){},...props}; await act(async()=>root.render(React.createElement(SkillActions,all))); return {root,all}; }
const button = text => [...document.querySelectorAll('button')].find(node=>node.textContent.includes(text));
async function click(node) {await act(async()=>node.click());}
async function query(text) {const input=document.querySelector('[role=combobox]');await act(async()=>{Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype,'value').set.call(input,text);input.dispatchEvent(new window.Event('input',{bubbles:true}));});return input;}
test('sharing preserves fenced Markdown and related files without invoking providers or publishing', async t=>{
 let copied;Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText:async value=>{copied=value;}}});await mount(t);await click(button('Copy sharing text'));assert.equal(copied,shareText(detail,'Fixture source'));assert.match(copied,/````markdown/);assert.match(copied,/Reference text/);assert.match(copied,/42 bytes/);assert.match(copied,/Source: Fixture source/);
});
test('command keyboard filters, invokes summary navigation only, and escape restores focus',async t=>{
 let translated=0;await mount(t,{onTranslate(){translated++;}});await act(async()=>window.dispatchEvent(new window.KeyboardEvent('keydown',{key:'k',ctrlKey:true,bubbles:true})));const input=await query('>translate');assert.equal(document.querySelectorAll('[role=option]').length,1);await act(async()=>input.dispatchEvent(new window.KeyboardEvent('keydown',{key:'Enter',bubbles:true})));assert.equal(translated,1);assert.equal(document.querySelector('[role=combobox]'),null);await click(button('Commands'));await act(async()=>document.querySelector('input').dispatchEvent(new window.KeyboardEvent('keydown',{key:'Escape',bubbles:true})));assert.equal(document.activeElement,button('Commands'));
});
test('missing selection disables skill commands; clipboard rejection exposes selectable text',async t=>{
 const {root,all}=await mount(t,{detail:null});await click(button('Commands'));await query('gist');assert.equal(document.querySelector('[role=option]').disabled,true);await act(async()=>root.render(React.createElement(SkillActions,{...all,detail})));Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText:async()=>{throw Error('unavailable');}}});await click(button('Copy sharing text'));assert.equal(document.querySelector('textarea').value,shareText(detail,'Fixture source'));
});
test('late clipboard failure does not expose the previous skill text',async t=>{
 let reject;Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText:()=>new Promise((_,no)=>{reject=no;})}});const {root,all}=await mount(t);await click(button('Copy sharing text'));await act(async()=>root.render(React.createElement(SkillActions,{...all,detail:{...detail,summary:{...detail.summary,id:'b'},contentDigest:'b'}})));await act(async()=>reject(Error('late')));assert.equal(document.querySelector('textarea'),null);
});
