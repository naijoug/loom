const test=require('node:test'),assert=require('node:assert/strict');
const {JSDOM}=require('jsdom');
const dom=new JSDOM('<div id="root"></div>',{url:'http://localhost'});
global.window=dom.window;global.document=dom.window.document;global.navigator=dom.window.navigator;
global.IS_REACT_ACT_ENVIRONMENT=true;
const React=require('react'),{act}=React,{createRoot}=require('react-dom/client'),{renderToStaticMarkup}=require('react-dom/server');
const {Markdown}=require('../../.tmp/test-build/src/features/chat/MarkdownView.js');
const {isExternalUrl}=require('../../.tmp/test-build/src/features/chat/externalLink.js');

const html=text=>renderToStaticMarkup(React.createElement(Markdown,{text}));

test('GFM headings, emphasis, links, tables, task lists and quotes render as elements',()=>{
 const out=html(`## 结论\n\n**加粗** *斜体* ~~删除~~ \`code\`\n\n> 引用\n\n| 名称 | 值 |\n|---|---|\n| a | 1 |\n\n- [x] 完成\n- [ ] 待办\n\n1. 第一\n2. 第二\n\n---`);
 assert.match(out,/<h2>结论<\/h2>/);
 assert.match(out,/<strong>加粗<\/strong> <em>斜体<\/em> <del>删除<\/del> <code>code<\/code>/);
 assert.match(out,/<blockquote>\s*<p>引用<\/p>\s*<\/blockquote>/);
 assert.match(out,/<table>[\s\S]*<th>名称<\/th>[\s\S]*<td>1<\/td>[\s\S]*<\/table>/);
 assert.match(out,/<input type="checkbox" disabled="" readonly="" checked=""\/> 完成/);
 assert.match(out,/<ol>/);
 assert.match(out,/<hr\/>/);
});

test('CJK punctuation next to bold markers still closes the emphasis',()=>{
 const out=html('适合降低使用成本。**WeKnora**是腾讯开源的框架；**核心：**支持多种检索。');
 assert.match(out,/<strong>WeKnora<\/strong>是腾讯/);
 assert.match(out,/<strong>核心：<\/strong>支持/);
 assert.ok(!out.includes('**'));
});

test('raw HTML stays literal text and unsafe links are not clickable',()=>{
 const out=html('<script>alert(1)</script>\n\n文本 <b>粗</b> <img src=x onerror=alert(1)>\n\n[坏链接](javascript:alert(1)) [文件](./src/main.tsx) [好](https://example.com)');
 assert.ok(!/<script|<b>|<img/.test(out));
 assert.match(out,/&lt;script&gt;alert\(1\)&lt;\/script&gt;/);
 assert.match(out,/&lt;b&gt;粗&lt;\/b&gt;/);
 assert.match(out,/<span class="chat-link-text">坏链接<\/span>/);
 assert.match(out,/<span class="chat-link-text">文件<\/span>/);
 assert.match(out,/<a href="https:\/\/example.com" title="https:\/\/example.com" target="_blank" rel="noopener noreferrer">好<\/a>/);
 assert.equal((out.match(/<a /g)||[]).length,1);
});

test('only http(s) and mailto URLs count as external links',()=>{
 for(const url of ['https://example.com/a?b=1','http://localhost:3000','mailto:a@example.com'])assert.ok(isExternalUrl(url),url);
 for(const url of ['javascript:alert(1)','file:///etc/passwd','./README.md','/abs/path','#top','',undefined,'https://'])assert.ok(!isExternalUrl(url),String(url));
});

test('clicking a link opens it outside the webview without navigating',async t=>{
 const opened=[],original=window.open;window.open=(...args)=>{opened.push(args);return null};
 const root=createRoot(document.getElementById('root'));
 t.after(async()=>{window.open=original;await act(async()=>root.unmount())});
 await act(async()=>root.render(React.createElement(Markdown,{text:'见 [文档](https://example.com/docs) 与 https://example.org'})));
 const links=[...document.querySelectorAll('.chat-markdown a')];
 assert.equal(links.length,2);
 const click=new window.MouseEvent('click',{bubbles:true,cancelable:true});
 await act(async()=>links[0].dispatchEvent(click));
 assert.equal(click.defaultPrevented,true);
 assert.deepEqual(opened,[['https://example.com/docs','_blank','noopener,noreferrer']]);
 assert.equal(window.location.href,'http://localhost/');
});

test('fenced code keeps its language attribute and exact text',()=>{
 const out=html('```ts\nconst a = 1;\n\nconst b = "<b>";\n```\n\n```\nplain\n```');
 assert.match(out,/<pre><code data-language="ts">const a = 1;\n\nconst b = &quot;&lt;b&gt;&quot;;<\/code><\/pre>/);
 assert.match(out,/<pre><code>plain<\/code><\/pre>/);
});
