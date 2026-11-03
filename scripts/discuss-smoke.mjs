import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
// Synthetic DEV fixtures, never IPC or model calls.
export async function discussSmoke(client, { evaluate, waitFor, click, clickText, screenshot }) {
  const output=join(process.cwd(),'designs/exports/discuss');mkdirSync(output,{recursive:true});
  const checks=[];
  const open=async(scenario='ready',locale='en',theme='light',width=1280)=>{
    await client.send('Emulation.setDeviceMetricsOverride',{width,height:820,deviceScaleFactor:1,mobile:false});
    await evaluate(client,`(localStorage.removeItem('loom-discuss-item'),localStorage.setItem('loom-app-settings',JSON.stringify({...JSON.parse(localStorage.getItem('loom-app-settings')||'{}'),locale:${JSON.stringify(locale)},themeMode:${JSON.stringify(theme)}})))`);
    await client.send('Page.navigate',{url:`http://127.0.0.1:1420/?fixture=simple-chat&discussFixture=${scenario}`});
    await waitFor(client,`document.documentElement.lang===${JSON.stringify(locale)}&&Boolean(document.querySelector('.activity-rail-item[data-view=discuss]'))`,'Discuss rail');
    await waitFor(client,`Boolean(document.querySelector('[data-testid=discuss-page]'))||(document.querySelector('.activity-rail-item[data-view=discuss]')?.click(),false)`,'Discuss page',300);
  };
  const reports='document.querySelectorAll(".discuss-report h1").length';
  await open();await waitFor(client,`${reports}===2`,'independent answers');await screenshot('round1-en-light',output);
  await click('.discuss-report .loom-src-link');await waitFor(client,'document.querySelectorAll(".source-code-line[data-marked]").length===3','source range');await screenshot('source-range',output);
  await click('.source-viewer header .chat-icon-button:last-child');
  await click('.discuss-tabs button:nth-child(2)');await waitFor(client,'document.querySelector(".discuss-report h1")?.textContent==="Revised position"','peer review');await screenshot('round2',output);
  await click('.discuss-tabs button:nth-child(3)');await waitFor(client,'document.querySelector(".discuss-report h1")?.textContent==="Consensus"','moderator summary');await screenshot('summary',output);
  await clickText('.discuss-card footer button','Execution logs');await waitFor(client,'document.querySelector(".discuss-logs pre")?.textContent.includes("Synthetic execution")','logs');await screenshot('logs',output);
  checks.push('two independent answers, peer review, moderator summary, source range, execution logs');
  await open('partial');await waitFor(client,'Boolean(document.querySelector("[data-task=r1-p2] .discuss-error"))&&'+reports+'===1','partial failure preserves successful answer');await screenshot('partial',output);
  await clickText('[data-task=r1-p2] footer button','Retry this call');await waitFor(client,'Boolean(document.querySelector("[role=alertdialog]"))','retry confirmation');
  if(await evaluate(client,'document.querySelector("[data-task=r1-p2]").dataset.status!=="failed"'))throw Error('Retry ran before confirmation');
  await clickText('[role=alertdialog] button','Confirm 1 call');await waitFor(client,'document.querySelector("[data-task=r1-p2]")?.dataset.status==="running"','retry running');
  await waitFor(client,'document.querySelector("[data-task=r1-p2]")?.dataset.status==="done"','retry done');
  await click('.discuss-tabs button:nth-child(3)');await waitFor(client,'Boolean(document.querySelector(".discuss-stale"))','summary outdated');await screenshot('outdated-summary',output);
  checks.push('partial result preserved; one-call retry confirmation; downstream summary explicitly outdated');
  await open('running');await waitFor(client,'Boolean(document.querySelector(".discuss-state[data-status=running]"))','running');await screenshot('running',output);await clickText('.discuss-actions button','Stop all');await waitFor(client,'Boolean(document.querySelector(".discuss-state[data-status=cancelled]"))','cancelled');await screenshot('cancelled',output);
  checks.push('stop marks active and queued calls stopped');
  for(const locale of ['en','zh-CN'])for(const theme of ['light','dark']){await open('ready',locale,theme);await waitFor(client,`${reports}===2`,'localized reports');await screenshot(`ready-${locale}-${theme}`,output);}
  await open('ready','zh-CN','dark',700);await waitFor(client,`${reports}===2`,'narrow reports');await screenshot('ready-zh-narrow',output);
  checks.push('en/zh-CN × light/dark; 700px without horizontal overflow');
  await open('error');await waitFor(client,'document.querySelector(".discuss-sidebar [role=alert]")?.textContent.includes("unreadable")','storage error');await screenshot('storage-error',output);
  await open('empty');await waitFor(client,'Boolean(document.querySelector(".discuss-new textarea"))','new form');
  await evaluate(client,`(()=>{const el=document.querySelector('.discuss-new textarea');Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype,'value').set.call(el,'Compare bounded queues');el.dispatchEvent(new Event('input',{bubbles:true}));})()`);
  await click('.discuss-new button[type=submit]');await waitFor(client,'Boolean(document.querySelector("[role=alertdialog]"))','create confirmation');await screenshot('confirm-five-calls',output);
  if(await evaluate(client,'Boolean(document.querySelector("[data-testid=discuss-view]"))'))throw Error('Discussion started before confirmation');
  await clickText('[role=alertdialog] button','Confirm and start');await waitFor(client,'Boolean(document.querySelector("[data-testid=discuss-view]"))','created');await waitFor(client,`${reports}===2`,'new results');await screenshot('created',output);
  await click('[aria-label="Delete discussion"]');await waitFor(client,'Boolean(document.querySelector("[role=alertdialog]"))','delete confirmation');await clickText('[role=alertdialog] button','Cancel');await waitFor(client,'!document.querySelector("[role=alertdialog]")','delete cancelled');
  checks.push('storage error; new discussion confirms five calls; delete confirmation can be cancelled');
  // Switching to Discuss preserves the same Chat draft and session.
  await click('.activity-rail-item[data-view=chat]');await waitFor(client,'!document.querySelector("[data-testid=chat-page]").closest("[hidden]")','back to Chat');
  await evaluate(client,`(()=>{const el=document.querySelector('.chat-composer textarea');Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype,'value').set.call(el,'Keep this draft across Discuss');el.dispatchEvent(new Event('input',{bubbles:true}));})()`);
  await click('.activity-rail-item[data-view=discuss]');await click('.activity-rail-item[data-view=chat]');
  await waitFor(client,'document.querySelector(".chat-composer textarea").value==="Keep this draft across Discuss"','Chat draft preserved');
  checks.push('Chat draft preserved across Discuss');
  writeFileSync(join(output,'result.json'),JSON.stringify({status:'passed',kind:'synthetic-web-only',checks},null,2));
  console.log(`Discuss browser smoke passed: ${output}`);
}
