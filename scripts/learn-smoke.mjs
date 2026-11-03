import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

// Learn visual acceptance against the DEV-only `learnFixture` API: no IPC, git or model calls.
export async function learnSmoke(client, { evaluate, waitFor, click, clickText, screenshot }) {
  const output = join(process.cwd(), "designs/exports/learn");
  mkdirSync(output, { recursive: true });
  const root = "http://127.0.0.1:1420/?fixture=simple-chat&learnFixture=";
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
      `(localStorage.removeItem('loom-learn-study'),localStorage.setItem('loom-app-settings',JSON.stringify({...JSON.parse(localStorage.getItem('loom-app-settings')||'{}'),locale:${JSON.stringify(locale)},themeMode:${JSON.stringify(theme)}})))`,
    );
    await client.send("Page.navigate", { url: root + scenario });
    await waitFor(
      client,
      `document.documentElement.lang===${JSON.stringify(locale)}&&Boolean(document.querySelector(".activity-rail-item[data-view=learn]"))`,
      "Learn rail item",
    );
    // Repeat the idempotent click until the lazily loaded view appears.
    await waitFor(
      client,
      'Boolean(document.querySelector("[data-testid=learn-page]"))||(document.querySelector(".activity-rail-item[data-view=learn]")?.click(),false)',
      "Learn page",
      300,
    );
  };
  const page = async (title) => {
    await clickText(".learn-wiki-tree button", title);
    await waitFor(
      client,
      `document.querySelector(".learn-article h1")?.textContent===${JSON.stringify(title)}`,
      `${title} page`,
    );
  };

  await open();
  await waitFor(client, 'document.querySelector(".learn-article h1")?.textContent==="hexyl"', "overview");
  if (await evaluate(client, 'getComputedStyle(document.querySelector(".learn-page")).paddingTop!=="0px"'))
    throw Error("Learn root inherited article padding");
  if (
    await evaluate(
      client,
      'Math.abs(document.querySelector(".learn-page").getBoundingClientRect().height-innerHeight)>1',
    )
  )
    throw Error("Learn page does not fill the window");
  if (
    await evaluate(client, 'document.querySelector(".activity-rail-item[aria-current=page]")?.dataset.view!=="learn"')
  )
    throw Error("Rail does not mark Learn as current");
  await screenshot("ready-en-light", output);
  await click(".learn-article .loom-src-link");
  await waitFor(
    client,
    'document.querySelectorAll(".source-code-line[data-marked]").length===5',
    "highlighted source range",
  );
  await screenshot("source-range", output);
  await click(".source-viewer header .chat-icon-button:last-child");
  await waitFor(client, '!document.querySelector(".source-viewer")', "source closed");
  checks.push("overview fills window; source link opens highlighted range");
  await page("Architecture");
  await waitFor(client, 'document.querySelectorAll(".learn-diagram-node").length===4', "diagram nodes");
  await screenshot("architecture", output);
  await evaluate(
    client,
    'document.querySelectorAll(".learn-diagram-node")[1].dispatchEvent(new MouseEvent("click",{bubbles:true}))',
  );
  await waitFor(
    client,
    'document.querySelector(".learn-article h1")?.textContent==="Printer"||Boolean(document.querySelector(".learn-article .chat-primary-button"))',
    "diagram opens module",
  );
  await page("Command line");
  await waitFor(
    client,
    "Boolean(document.querySelector('.loom-src-link[data-status=\"missing\"]'))",
    "stale reference marked",
  );
  await click(".loom-wiki-link");
  await waitFor(
    client,
    'Boolean(document.querySelector(".learn-article .chat-primary-button"))',
    "wiki link opens ungenerated page",
  );
  await screenshot("page-not-generated", output);
  await page("Command line");
  await screenshot("module-page", output);
  checks.push("architecture diagram nodes navigate; module page marks stale refs; wiki link to ungenerated page");
  await evaluate(client, 'document.querySelectorAll(".learn-study-actions .chat-icon-button")[1].click()');
  await waitFor(client, 'Boolean(document.querySelector(".learn-logs pre"))', "job logs");
  await screenshot("logs", output);
  checks.push("job logs");
  for (const [locale, theme] of [
    ["en", "dark"],
    ["zh-CN", "light"],
    ["zh-CN", "dark"],
  ]) {
    await open("ready", locale, theme);
    await waitFor(client, 'Boolean(document.querySelector(".learn-article h1"))', "themed overview");
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
    'getComputedStyle(document.querySelector(".learn-sidebar")).position==="static"',
    "stacked narrow sidebar",
  );
  await screenshot("ready-zh-narrow", output);
  checks.push("en/zh-CN × light/dark; 700px stacked sidebar without overflow");

  await open("running");
  await waitFor(client, 'Boolean(document.querySelector(".learn-pipeline"))', "pipeline while running");
  await screenshot("running", output);
  await open("failed");
  await waitFor(
    client,
    'document.querySelector(".learn-problems .learn-problem-text > span")?.textContent==="agent exited with status 1"&&!document.querySelector(".learn-problems details").open',
    "stage error summary with collapsed stderr",
  );
  await screenshot("failed", output);
  await evaluate(client, 'document.querySelector(".learn-problems details summary").click()');
  await waitFor(
    client,
    'document.querySelector(".learn-problems details[open] pre")?.textContent.includes("Missing optional dependency")',
    "raw stderr behind details",
  );
  await screenshot("failed-details", output);
  await open("error");
  await waitFor(
    client,
    'document.querySelector(".learn-sidebar .learn-error")?.textContent.includes("Synthetic storage error")',
    "storage error",
  );
  await screenshot("error", output);
  checks.push("running pipeline; failed stage summary with raw stderr behind details; storage error");

  await open("empty");
  await waitFor(client, 'Boolean(document.querySelector(".learn-new"))', "new study form");
  await evaluate(
    client,
    `(()=>{const input=document.querySelector('.learn-new input');Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(input,'not a url');input.dispatchEvent(new Event('input',{bubbles:true}));})()`,
  );
  await waitFor(
    client,
    'document.querySelector(".learn-new button[type=submit]")?.disabled',
    "invalid URL blocks submit",
  );
  await evaluate(
    client,
    `(()=>{const input=document.querySelector('.learn-new input');Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(input,'https://github.com/sharkdp/hexyl');input.dispatchEvent(new Event('input',{bubbles:true}));})()`,
  );
  await waitFor(
    client,
    '!document.querySelector(".learn-new button[type=submit]")?.disabled',
    "valid URL enables submit",
  );
  await screenshot("new-study", output);
  await click(".learn-new button[type=submit]");
  await waitFor(client, 'Boolean(document.querySelector(".learn-stages"))', "created study");
  await waitFor(
    client,
    'document.querySelector(".learn-study-status")?.dataset.status==="ready"&&!document.querySelector(".learn-wiki-tree .chat-spin")&&document.querySelectorAll(".learn-wiki-tree .learn-tree-todo").length===3',
    "level 1 pages generated",
    150,
  );
  await screenshot("created-ready", output);
  checks.push("URL validation; create runs fetch → architecture → level-1 pages");

  // The rail replaced the old sidebar "Back to Chat" button.
  await click(".activity-rail-item[data-view=chat]");
  await waitFor(
    client,
    '!document.querySelector("[data-testid=chat-page]").closest("[hidden]")&&Boolean(document.querySelector("[data-testid=learn-page]").closest("[hidden]"))',
    "Chat restored from the rail",
  );
  checks.push("rail returns to Chat");
  writeFileSync(
    join(output, "result.json"),
    JSON.stringify({ status: "passed", kind: "synthetic-web-only", checks }, null, 2),
  );
  console.log(`Learn browser smoke passed: ${output}`);
}
