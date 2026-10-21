const test=require('node:test'),assert=require('node:assert/strict');
const React=require('react'),{renderToStaticMarkup}=require('react-dom/server');
const {Renderer}=require('@openuidev/react-lang');
const {inlineCardLibrary,inlineCardPrompt,checkInlineCard}=require('../../.tmp/test-build/src/features/chat/openui/library.js');

const valid=`root = CardStack([files, todo])
files = FileList([{path: "src/main.tsx", note: "React entry"}], "Entry")
todo = Checklist([{text: "pnpm build", done: true}, {text: "cargo test", done: false}], "Verify")`;
const render=source=>renderToStaticMarkup(React.createElement(Renderer,{response:source,library:inlineCardLibrary,isStreaming:false}));

test('inline card prompt keeps standalone programs, required-first signatures and no tool workflow',()=>{
 const prompt=inlineCardPrompt();
 assert.match(prompt,/## Inline Mode/);
 assert.match(prompt,/Each openui-lang block is a complete, standalone program/);
 assert.match(prompt,/FileList\(files: .*title\?: string\)/);
 assert.match(prompt,/Checklist\(items: .*title\?: string\)/);
 assert.match(prompt,/DataTable\(columns: string\[\], rows: string\[\]\[\], title\?: string\)/);
 for(const absent of ['Query(','Mutation(','## Data Workflow','$variables'])assert.ok(!prompt.includes(absent),absent);
});

test('component schemas expose no action, url or html props',()=>{
 const schema=JSON.stringify(inlineCardLibrary.toJSONSchema());
 for(const absent of ['"action"','"uri"','"url"','"html"','"href"'])assert.ok(!schema.includes(absent),absent);
});

test('a complete valid card passes the render check',()=>{
 assert.deepEqual(checkInlineCard(valid),{ok:true,problems:[],cardCount:2});
});

test('invalid finished cards report problems so the UI can fall back to source',()=>{
 const cases={
  'unknown-component':`root = CardStack([c])\nc = Chart("x", [1, 2])`,
  'type-mismatch':`root = CardStack([t])\nt = DataTable("not-an-array", 5)`,
  'unresolved':`root = CardStack([files, missing])\nfiles = FileList([{path: "a"}])`,
  'empty':'root = CardStack([])',
  'no-root':'files = FileList([{path: "a"}])',
  'incomplete':`root = CardStack([files])\nfiles = FileList([{path: "src/ma`,
 };
 for(const [problem,source] of Object.entries(cases)){
  const result=checkInlineCard(source);
  assert.equal(result.ok,false,problem);
  assert.ok(result.problems.includes(problem),`${problem}: ${result.problems.join(',')}`);
 }
});

test('renderer outputs card text as escaped text nodes',()=>{
 const html=render(valid);
 assert.match(html,/<code>src\/main\.tsx<\/code>/);
 assert.match(html,/data-done="true">pnpm build</);
 const hostile=render(`root = CardStack([t])\nt = DataTable(["<img src=x onerror=alert(1)>"], [["<script>alert(1)</script>"]])`);
 assert.ok(!hostile.includes('<script>')&&!hostile.includes('<img'));
 assert.match(hostile,/&lt;script&gt;alert\(1\)&lt;\/script&gt;/);
});
