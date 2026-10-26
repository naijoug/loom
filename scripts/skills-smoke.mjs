import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

export async function skillsSmoke(client, { evaluate, waitFor, click, clickText, screenshot }) {
  const output=join(process.cwd(),'designs/exports/skills');mkdirSync(output,{recursive:true});
  const root='http://127.0.0.1:1420/?fixture=simple-chat&skillsFixture=';
  const checks=[];
  const open=async(scenario='ready',locale='en',theme='light',width=1280)=>{
    await client.send('Emulation.setDeviceMetricsOverride',{width,height:900,deviceScaleFactor:1,mobile:false});
    await evaluate(client,`localStorage.setItem('loom-app-settings',JSON.stringify({...JSON.parse(localStorage.getItem('loom-app-settings')||'{}'),locale:${JSON.stringify(locale)},themeMode:${JSON.stringify(theme)}}))`);
    await client.send('Page.navigate',{url:root+scenario});
    await waitFor(client,'Boolean(document.querySelector(".chat-project-tree footer button"))','chat navigation');
    // Both clicks are idempotent; repeat them until the lazily loaded view appears so a click that lands
    // before the new document or chunk is ready does not stall slow CI runners.
    await waitFor(client,'Boolean(document.querySelector(".chat-settings"))||(document.querySelector(".chat-project-tree footer button")?.click(),false)','settings',300);
    await waitFor(client,`Boolean(document.querySelector(".skills-page"))||(Array.from(document.querySelectorAll(".chat-settings nav button")).find(e=>e.textContent.trim()===${JSON.stringify(locale==='en'?'Skills':'技能')})?.click(),false)`,'Skills page',300);
    if(scenario==='ready'||scenario==='cancel')await waitFor(client,'Boolean(document.querySelector(".skills-detail header h2"))','skill detail');
  };
  const type=async(selector,value)=>evaluate(client,`(()=>{const input=document.querySelector(${JSON.stringify(selector)});Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(input,${JSON.stringify(value)});input.dispatchEvent(new Event('input',{bubbles:true}));})()`);
  await open();await screenshot('library-en-light',output);
  if(await evaluate(client, 'document.querySelector(".skills-detail").scrollWidth > document.querySelector(".skills-detail").clientWidth + 2'))throw Error('Skill Markdown inherits overflowing Settings table width');
  await type('.skills-search input','missing');await waitFor(client,'!document.querySelector(".skills-detail header")','no stale detail');await screenshot('search-empty',output);
  await clickText('.skills-list button','Clear search');await waitFor(client,'Boolean(document.querySelector(".skills-detail header"))','restored detail');
  await clickText('.skills-tabs button','Files');await click('.skills-detail details summary');await screenshot('related-files',output);
  await clickText('.skills-tabs button','Install');await waitFor(client,'Boolean(document.querySelector(".skills-install-targets input"))','install targets');
  if(await evaluate(client,'document.querySelector(".skills-install-targets input").checked'))throw Error('New install target selected implicitly');
  await click('.skills-install-targets input');await waitFor(client,'!document.querySelector(".skills-install-panel > .skills-install-actions button").disabled','enabled install preview');await click('.skills-install-panel > .skills-install-actions button');await waitFor(client,'Boolean(document.querySelector(".skills-plan"))','installation preview');await evaluate(client,'document.querySelector(".skills-plan").scrollIntoView({block:"center"})');await screenshot('install-preview',output);
  await click('.skills-plan .skills-install-actions button');await waitFor(client,'Boolean(document.querySelector(".skills-install-results"))','installed receipt');
  await clickText('.skills-view-tabs button','Installed');await waitFor(client,'Boolean(document.querySelector(".skills-installed-row"))','installed inventory');await screenshot('installed',output);
  await click('.skills-installed-row > button');await waitFor(client,'Boolean(document.querySelector(".skills-plan"))','uninstall preview');await click('.skills-plan .skills-install-actions button');await waitFor(client,'!document.querySelector(".skills-installed-row")','removed installation');
  checks.push('new targets unchecked; install preview/apply; inventory uninstall');
  await clickText('.skills-view-tabs button','Library & discovery');await clickText('.skills-tabs button','Translate');await waitFor(client,'document.querySelector(".skills-translation-panel select")?.value==="codex"','configured provider');
  await clickText('.skills-translation-panel button','Translate');await waitFor(client,'Boolean(document.querySelector(".skills-translation-panel .chat-markdown")) || document.querySelector(".skills-translation-panel")?.textContent.includes("合成翻译结果")','translation result');await evaluate(client,'document.querySelector(".skills-translation-panel").scrollIntoView({block:"start"})');await screenshot('translation-result',output);
  await evaluate(client,'document.querySelectorAll(".skills-list > button")[1].click()');await waitFor(client,'document.querySelector(".skills-detail header h2")?.textContent==="Explain a module"','switch skill');
  if(await evaluate(client,'document.querySelector(".skills-detail").textContent.includes("合成翻译结果")'))throw Error('Translation leaked into another skill');
  await click('.skills-actions > button');await type('.skills-command-list input','>translate');await screenshot('commands',output);await click('.skills-command-list [role=option]');await waitFor(client,'document.querySelectorAll(".skills-translation-panel select")[1]?.value==="summary"','summary command');
  checks.push('translation result; skill switch isolation; command summary navigation');
  for(const [locale,theme] of [['en','dark'],['zh-CN','light'],['zh-CN','dark']]){await open('ready',locale,theme);await screenshot(`library-${locale}-${theme}`,output);}
  await client.send('Emulation.setDeviceMetricsOverride',{width:700,height:900,deviceScaleFactor:1,mobile:false});await screenshot('library-zh-narrow',output);
  await clickText('.skills-tabs button','翻译');await waitFor(client,'Boolean(document.querySelector(".skills-translation-panel"))','narrow translation');await evaluate(client,'document.querySelector(".skills-translation-panel").scrollIntoView({block:"start"})');await screenshot('translation-zh-narrow',output);
  checks.push('en/zh-CN × light/dark; 700px narrow without page overflow');
  for(const scenario of ['empty','loading','error']){await open(scenario);if(scenario==='loading')await waitFor(client,'document.querySelector(".skills-page").getAttribute("aria-busy")==="true"','loading');if(scenario==='error')await waitFor(client,'document.querySelector(".skills-error")?.textContent.includes("Synthetic source unavailable")','source error');await screenshot(scenario,output);}
  await open('cancel');await clickText('.skills-tabs button','Translate');await waitFor(client,'document.querySelector(".skills-translation-panel select")?.value==="codex"','cancel provider');await clickText('.skills-translation-panel button','Translate');await waitFor(client,'document.querySelector(".skills-translation-panel")?.textContent.includes("Cancel")','cancel button');await clickText('.skills-translation-panel button','Cancel');await waitFor(client,'document.querySelector(".skills-translation-panel")?.textContent.includes("Cancelled")','cancelled');await evaluate(client,'document.querySelector(".skills-translation-panel").scrollIntoView({block:"start"})');await screenshot('translation-cancelled',output);
  checks.push('empty/loading/error; inflight translation cancellation');
  await click('.chat-settings aside > button');await waitFor(client,'Boolean(document.querySelector("textarea"))','Chat restored');
  await evaluate(client,`(()=>{const input=document.querySelector('textarea');Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype,'value').set.call(input,'Draft survives Skills');input.dispatchEvent(new Event('input',{bubbles:true}));})()`);
  await click('.chat-project-tree footer button');await waitFor(client,'Boolean(document.querySelector(".chat-settings"))','settings');await clickText('.chat-settings nav button','Skills');await waitFor(client,'Boolean(document.querySelector(".skills-page"))','Skills during draft');await click('.chat-settings aside > button');await waitFor(client,'document.querySelector("textarea")?.value==="Draft survives Skills"','draft preserved');checks.push('Chat draft survives Settings/Skills roundtrip');
  writeFileSync(join(output,'result.json'),JSON.stringify({status:'passed',kind:'synthetic-web-only',checks},null,2));
  console.log(`Skills browser smoke passed: ${output}`);
}
