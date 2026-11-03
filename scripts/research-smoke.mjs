import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

// Research visual acceptance against the DEV-only `researchFixture` API: no IPC, git or model calls.
export async function researchSmoke(client, { evaluate, waitFor, click, screenshot }) {
  const output = join(process.cwd(), "designs/exports/research");
  mkdirSync(output, { recursive: true });
  const root = "http://127.0.0.1:1420/?fixture=simple-chat&researchFixture=";
  const checks = [];
  const open = async (scenario = "ready", locale = "en", theme = "light", width = 1280) => {
    await client.send("Emulation.setDeviceMetricsOverride", {
      width,
      height: 820,
      deviceScaleFactor: 1,
      mobile: false,
    });
    await evaluate(
      client,
      `(localStorage.removeItem('loom-research-item'),localStorage.setItem('loom-app-settings',JSON.stringify({...JSON.parse(localStorage.getItem('loom-app-settings')||'{}'),locale:${JSON.stringify(locale)},themeMode:${JSON.stringify(theme)}})))`,
    );
    await client.send("Page.navigate", { url: root + scenario });
    await waitFor(
      client,
      `document.documentElement.lang===${JSON.stringify(locale)}&&Boolean(document.querySelector(".activity-rail-item[data-view=research]"))`,
      "Research rail item",
    );
    // Repeat the idempotent click until the lazily loaded view appears.
    await waitFor(
      client,
      'Boolean(document.querySelector("[data-testid=research-page]"))||(document.querySelector(".activity-rail-item[data-view=research]")?.click(),false)',
      "Research page",
      300,
    );
  };
  const setText = (selector, value) =>
    evaluate(
      client,
      `(()=>{const el=document.querySelector(${JSON.stringify(selector)});Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype,'value').set.call(el,${JSON.stringify(value)});el.dispatchEvent(new Event('input',{bubbles:true}));})()`,
    );
  const reports = 'document.querySelectorAll(".research-report h1").length';

  await open();
  await waitFor(client, `${reports}===2`, "two reports");
  if (
    await evaluate(
      client,
      'Math.abs(document.querySelector(".research-page").getBoundingClientRect().height-innerHeight)>1',
    )
  )
    throw Error("Research page does not fill the window");
  if (
    await evaluate(
      client,
      'document.querySelector(".activity-rail-item[aria-current=page]")?.dataset.view!=="research"',
    )
  )
    throw Error("Rail does not mark Research as current");
  if (await evaluate(client, 'document.querySelectorAll(".research-chip").length!==1'))
    throw Error("Follow-up context chip missing");
  await waitFor(
    client,
    'Boolean(document.querySelector(\'.loom-src-link[data-status="missing"]\'))&&document.querySelector(".research-turn .research-note")?.textContent.includes("1")',
    "stale citation marked and counted",
  );
  await screenshot("ready-en-light", output);
  await click(".research-report .loom-src-link");
  await waitFor(
    client,
    'document.querySelectorAll(".source-code-line[data-marked]").length===21',
    "highlighted source range",
  );
  await screenshot("source-range", output);
  await click(".source-viewer header .chat-icon-button:last-child");
  await waitFor(client, '!document.querySelector(".source-viewer")', "source closed");
  checks.push(
    "reports fill window; follow-up context chip; stale citation marked; source link opens highlighted range",
  );
  await click('.research-actions .chat-icon-button[aria-label="Run logs"]');
  await waitFor(client, 'Boolean(document.querySelector(".research-logs pre"))', "job logs");
  await screenshot("logs", output);
  checks.push("job logs");

  await setText("#research-followup-input", "Is the process group killed on Windows too?");
  await click(".research-followup button[type=submit]");
  await waitFor(
    client,
    'Boolean(document.querySelector(".research-running"))&&document.querySelector("#research-followup-input").disabled',
    "follow-up running",
  );
  await screenshot("followup-running", output);
  await waitFor(client, `${reports}===3&&!document.querySelector(".research-running")`, "follow-up answered");
  checks.push("follow-up runs and renders a third report");

  // Web-only research: no source browser button.
  await evaluate(
    client,
    'Array.from(document.querySelectorAll(".research-sidebar .chat-tree-session")).find(e=>e.textContent.includes("async runtime"))?.click()',
  );
  await waitFor(
    client,
    'document.querySelector(".research-title h1")?.textContent.includes("async runtime")&&!document.querySelector(".research-actions [aria-label=\'Browse source\']")',
    "web-only research",
  );
  await screenshot("no-project", output);
  checks.push("web-only research hides the source browser");

  for (const [locale, theme] of [
    ["en", "dark"],
    ["zh-CN", "light"],
    ["zh-CN", "dark"],
  ]) {
    await open("ready", locale, theme);
    await waitFor(client, `${reports}===2`, "themed reports");
    await screenshot(`ready-${locale}-${theme}`, output);
  }
  await client.send("Emulation.setDeviceMetricsOverride", {
    width: 700,
    height: 820,
    deviceScaleFactor: 1,
    mobile: false,
  });
  await waitFor(
    client,
    'getComputedStyle(document.querySelector(".research-sidebar")).position==="static"',
    "stacked narrow sidebar",
  );
  await screenshot("ready-zh-narrow", output);
  checks.push("en/zh-CN × light/dark; 700px stacked sidebar without overflow");

  await open("running");
  await waitFor(
    client,
    'Boolean(document.querySelector(".research-running")&&document.querySelector(".research-actions button:not(.chat-icon-button)"))',
    "running with Stop",
  );
  await screenshot("running", output);
  await click(".research-actions button:not(.chat-icon-button)");
  await waitFor(
    client,
    'Boolean(document.querySelector(".research-chip[data-status=cancelled]")&&document.querySelector(".research-actions .chat-primary-button"))',
    "stopped with Retry",
  );
  await screenshot("cancelled", output);
  await click(".research-actions .chat-primary-button");
  await waitFor(client, `${reports}===3`, "retried question answered");
  checks.push("Stop marks the question stopped; Retry answers it");
  await open("failed");
  await waitFor(
    client,
    'document.querySelector(".research-turn[data-status=failed] .research-error span")?.textContent==="agent exited with status 1"&&Boolean(document.querySelector(".research-warnings"))',
    "failed turn summary with fallback warning",
  );
  await screenshot("failed", output);
  await open("error");
  await waitFor(
    client,
    'document.querySelector(".research-sidebar .research-error")?.textContent.includes("Synthetic storage error")',
    "storage error",
  );
  await screenshot("error", output);
  checks.push("failed question with error summary and replay warning; storage error");

  await open("empty");
  await waitFor(client, 'Boolean(document.querySelector(".research-new"))', "new research form");
  await waitFor(
    client,
    'document.querySelector(".research-new button[type=submit]")?.disabled',
    "empty question blocks submit",
  );
  await setText(".research-new textarea", "How are chat sessions stored on disk?");
  await evaluate(
    client,
    `(()=>{const select=document.querySelector('.research-new select');select.value='/preview/loom';select.dispatchEvent(new Event('change',{bubbles:true}));})()`,
  );
  await waitFor(
    client,
    '!document.querySelector(".research-new button[type=submit]")?.disabled',
    "question enables submit",
  );
  await screenshot("new-research", output);
  await click(".research-new button[type=submit]");
  await waitFor(
    client,
    'document.querySelector(".research-title small")?.textContent.includes("loom")&&document.querySelectorAll(".research-report h1").length===1',
    "created research answered",
  );
  await waitFor(client, 'document.querySelector(".research-status")?.dataset.status==="ready"', "list marks ready");
  await screenshot("created-ready", output);
  checks.push("question required; create with project runs and answers");

  await click(".activity-rail-item[data-view=chat]");
  await waitFor(
    client,
    '!document.querySelector("[data-testid=chat-page]").closest("[hidden]")',
    "Chat restored from the rail",
  );
  checks.push("rail returns to Chat");
  writeFileSync(
    join(output, "result.json"),
    JSON.stringify({ status: "passed", kind: "synthetic-web-only", checks }, null, 2),
  );
  console.log(`Research browser smoke passed: ${output}`);
}
