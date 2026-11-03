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
      `(Object.keys(localStorage).filter(k=>k.startsWith('loom-learn-read:')).forEach(k=>localStorage.removeItem(k)),localStorage.removeItem('loom-learn-study'),localStorage.setItem('loom-app-settings',JSON.stringify({...JSON.parse(localStorage.getItem('loom-app-settings')||'{}'),locale:${JSON.stringify(locale)},themeMode:${JSON.stringify(theme)}})))`,
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

  // Real key events: focus emulation lets an unfocused headless page receive focus and keyboard input.
  const KEYS = { Enter: 13, Escape: 27, Tab: 9, ArrowDown: 40, ArrowUp: 38 };
  const press = async (key) => {
    const base = { key, code: key, windowsVirtualKeyCode: KEYS[key], nativeVirtualKeyCode: KEYS[key] };
    await client.send("Input.dispatchKeyEvent", { type: "rawKeyDown", ...base });
    if (key === "Enter") await client.send("Input.dispatchKeyEvent", { type: "char", ...base, text: "\r" });
    await client.send("Input.dispatchKeyEvent", { type: "keyUp", ...base });
  };
  const focused = (selector) => `Boolean(document.activeElement?.matches(${JSON.stringify(selector)}))`;
  const focus = (selector) => evaluate(client, `document.querySelector(${JSON.stringify(selector)}).focus()`);

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
  if (
    !(await evaluate(
      client,
      'document.querySelector(".learn-sidebar .learn-switcher-button strong")?.textContent==="hexyl"&&Boolean(document.querySelector(".learn-sidebar .learn-wiki-tree"))&&!document.querySelector(".learn-body .learn-wiki-tree")',
    ))
  )
    throw Error("Outline is not in the left column under the study switcher");
  await screenshot("ready-en-light", output);
  await click(".learn-progress-pill");
  await waitFor(
    client,
    'document.querySelectorAll(".learn-progress-pop .learn-stage").length===4',
    "progress popover with stages",
  );
  await screenshot("progress-popover", output);
  await click(".learn-progress-pill");
  await waitFor(client, '!document.querySelector(".learn-progress-pop")', "progress popover closed");
  await click(".learn-switcher-button");
  await waitFor(
    client,
    'document.querySelectorAll(".learn-switcher-menu [role=menuitemradio]").length===1',
    "switcher menu",
  );
  await screenshot("switcher", output);
  await click(".learn-switcher-button");
  checks.push("two columns: switcher + outline; progress popover; switcher menu");
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
  await waitFor(
    client,
    'document.querySelectorAll(".learn-diagram-node").length===4&&!document.querySelector(".learn-diagram-edge")&&!document.querySelector(".learn-article .chat-markdown h2")',
    "diagram nodes without edges or the duplicate layer list",
  );
  await screenshot("architecture", output);
  await evaluate(
    client,
    'document.querySelector(".learn-diagram-node").dispatchEvent(new MouseEvent("mouseover",{bubbles:true,relatedTarget:document.body}))',
  );
  await waitFor(
    client,
    'document.querySelectorAll(".learn-diagram-edge-dep").length===2&&document.querySelectorAll(".learn-diagram-node[data-role=dim]").length===1',
    "hovered module shows its dependencies",
  );
  await screenshot("architecture-focus", output);
  await evaluate(
    client,
    'document.querySelector(".learn-diagram-node").dispatchEvent(new MouseEvent("mouseout",{bubbles:true,relatedTarget:document.body}))',
  );
  await waitFor(client, '!document.querySelector(".learn-diagram-edge")', "edges hidden after hover ends");
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
  await click(".learn-pending .chat-primary-button");
  await waitFor(
    client,
    'Boolean(document.querySelector(".learn-wiki-tree .learn-icon-running"))',
    "outline marks the generating page",
  );
  await screenshot("page-generating", output);
  await waitFor(
    client,
    'document.querySelector(".learn-article h1")?.textContent==="Printer"',
    "generated page opens",
    150,
  );
  await page("Command line");
  await screenshot("module-page", output);
  await clickText(".learn-wiki-tree button", "Overview");
  await waitFor(
    client,
    'document.querySelectorAll(".learn-wiki-tree .learn-icon-read").length===4&&document.querySelector(".learn-continue strong")?.textContent==="Input"',
    "read marks and next unread page",
  );
  await screenshot("overview-progress", output);
  await click(".learn-continue .chat-primary-button");
  await waitFor(
    client,
    'document.querySelector(".learn-pending h2")?.textContent==="Input"&&[...document.querySelectorAll(".learn-pager strong")].map(e=>e.textContent).join()==="Printer,Colours"',
    "continue opens the next page with prev/next",
  );
  await screenshot("pending-with-pager", output);
  checks.push(
    "architecture diagram: no default edges, hover highlights dependencies, nodes navigate; module page marks stale refs; wiki link to ungenerated page; generate this page (outline spinner); read marks, continue reading, prev/next",
  );
  await click('.learn-more > button[aria-haspopup="menu"]');
  await waitFor(client, 'document.querySelectorAll(".learn-menu [role^=menuitem]").length===3', "More menu");
  await screenshot("more-menu", output);
  await click('.learn-menu [role="menuitemcheckbox"]');
  await waitFor(client, 'Boolean(document.querySelector(".learn-logs pre"))', "job logs");
  await screenshot("logs", output);
  checks.push("More menu opens job logs");

  await client.send("Emulation.setFocusEmulationEnabled", { enabled: true });
  await focus('.learn-more > button[aria-haspopup="menu"]');
  await press("Enter");
  await waitFor(client, focused('.learn-menu [role="menuitem"]'), "More menu focuses its first item");
  await press("ArrowDown");
  await waitFor(client, focused('.learn-menu [role="menuitemcheckbox"]'), "ArrowDown moves in the More menu");
  await press("ArrowUp");
  await press("ArrowUp");
  await waitFor(client, focused(".learn-menu .learn-menu-danger"), "ArrowUp wraps to the last item");
  await press("Escape");
  await waitFor(
    client,
    `!document.querySelector(".learn-menu")&&${focused(".learn-more > button")}`,
    "Escape closes the More menu and refocuses its button",
  );
  await focus(".learn-progress-pill");
  await press("Enter");
  await waitFor(client, 'Boolean(document.querySelector(".learn-progress-pop"))', "Enter opens the progress popover");
  await press("Escape");
  await waitFor(
    client,
    `!document.querySelector(".learn-progress-pop")&&${focused(".learn-progress-pill")}`,
    "Escape closes the progress popover",
  );
  await focus(".learn-switcher-button");
  await press("Enter");
  await waitFor(
    client,
    focused('.learn-switcher-menu [role="menuitemradio"][aria-checked="true"]'),
    "switcher focuses the current study",
  );
  await press("Escape");
  await waitFor(
    client,
    `!document.querySelector(".learn-switcher-menu")&&${focused(".learn-switcher-button")}`,
    "Escape closes the switcher",
  );
  await clickText(".learn-wiki-tree button", "Architecture");
  await waitFor(client, 'document.querySelectorAll(".learn-diagram-node").length===4', "architecture for keyboard");
  await focus(".learn-diagram-node");
  await waitFor(client, 'document.querySelectorAll(".learn-diagram-edge-dep").length===2', "focus shows edges");
  await press("Tab");
  await waitFor(
    client,
    `${focused('[data-module="printer"]')}&&document.querySelectorAll(".learn-diagram-edge-dep").length===1&&document.querySelectorAll(".learn-diagram-edge-user").length===1`,
    "Tab moves focus and edges to the next module",
  );
  await screenshot("architecture-keyboard", output);
  await press("Enter");
  await waitFor(
    client,
    'document.querySelector(".learn-article h1")?.textContent==="Printer"',
    "Enter opens the module",
  );
  await focus(".learn-pager-next");
  await press("Enter");
  await waitFor(
    client,
    'document.querySelector(".learn-pending h2")?.textContent==="Input"',
    "Enter on Next pages forward",
  );
  await client.send("Emulation.setFocusEmulationEnabled", { enabled: false });
  checks.push(
    "keyboard: More menu arrows/wrap/Escape, progress popover and switcher Enter/Escape with focus return, diagram focus + Tab edges, Enter opens module, pager Next",
  );
  for (const [locale, theme] of [
    ["en", "dark"],
    ["zh-CN", "light"],
    ["zh-CN", "dark"],
  ]) {
    await open("ready", locale, theme);
    await waitFor(client, 'Boolean(document.querySelector(".learn-article h1"))', "themed overview");
    await screenshot(`ready-${locale}-${theme}`, output);
    await clickText(".learn-wiki-tree button", "Architecture");
    await waitFor(client, 'document.querySelectorAll(".learn-diagram-node").length===4', "themed architecture");
    await evaluate(
      client,
      'document.querySelector(".learn-diagram-node").dispatchEvent(new MouseEvent("mouseover",{bubbles:true,relatedTarget:document.body}))',
    );
    await waitFor(client, 'document.querySelectorAll(".learn-diagram-edge-dep").length===2', "themed diagram hover");
    await screenshot(`architecture-${locale}-${theme}`, output);
    await clickText(".learn-wiki-tree button", "Input");
    await waitFor(client, 'document.querySelector(".learn-pending h2")?.textContent==="Input"', "themed pending page");
    await screenshot(`pending-${locale}-${theme}`, output);
    await clickText(".learn-wiki-tree button", "Overview");
    await waitFor(client, 'Boolean(document.querySelector(".learn-entry"))', "back to overview");
  }
  await client.send("Emulation.setDeviceMetricsOverride", {
    width: 700,
    height: 820,
    deviceScaleFactor: 1,
    mobile: false,
  });
  await waitFor(
    client,
    'getComputedStyle(document.querySelector(".learn-sidebar")).visibility==="hidden"&&document.documentElement.scrollWidth<=innerWidth',
    "narrow window hides the left column without overflow",
  );
  await screenshot("ready-zh-narrow", output);
  await click(".learn-drawer-toggle");
  await waitFor(
    client,
    'getComputedStyle(document.querySelector(".learn-sidebar")).visibility==="visible"&&Boolean(document.querySelector(".learn-scrim"))',
    "outline drawer",
  );
  await screenshot("ready-zh-narrow-drawer", output);
  await clickText(".learn-wiki-tree button", "Architecture");
  await waitFor(client, '!document.querySelector(".learn-page[data-drawer]")', "choosing a page closes the drawer");
  if (await evaluate(client, "document.documentElement.scrollWidth>innerWidth"))
    throw Error("Narrow architecture page overflows");
  await screenshot("architecture-zh-narrow", output);
  await clickText(".learn-wiki-tree button", "Input");
  await waitFor(
    client,
    'document.querySelector(".learn-pending h2")?.textContent==="Input"&&document.documentElement.scrollWidth<=innerWidth',
    "narrow pending page without overflow",
  );
  await screenshot("pending-zh-narrow", output);
  await client.send("Emulation.setFocusEmulationEnabled", { enabled: true });
  await focus(".learn-drawer-toggle");
  await press("Enter");
  await waitFor(client, 'Boolean(document.querySelector(".learn-page[data-drawer]"))', "Enter opens the drawer");
  await click(".learn-switcher-button");
  await waitFor(client, 'Boolean(document.querySelector(".learn-switcher-menu"))', "switcher inside the drawer");
  await press("Escape");
  await waitFor(
    client,
    '!document.querySelector(".learn-switcher-menu")&&Boolean(document.querySelector(".learn-page[data-drawer]"))',
    "first Escape closes only the switcher",
  );
  await press("Escape");
  await waitFor(
    client,
    `!document.querySelector(".learn-page[data-drawer]")&&${focused(".learn-drawer-toggle")}`,
    "second Escape closes the drawer and refocuses its toggle",
  );
  await client.send("Emulation.setFocusEmulationEnabled", { enabled: false });
  checks.push(
    "en/zh-CN × light/dark overview, architecture hover and pending page; 700px drawer, architecture and pending page without overflow; drawer Escape layering",
  );

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

  // mini-swe-agent scale: 10 modules in 4 layers with long names must show in full, with no edges until focus.
  const fits =
    '[...document.querySelectorAll(".learn-diagram-node strong,.learn-diagram-layer h4")].every(e=>e.scrollWidth<=e.clientWidth+1&&e.scrollHeight<=e.clientHeight+1)';
  for (const [theme, width] of [
    ["light", 1280],
    ["dark", 1280],
    ["light", 700],
  ]) {
    await open("large", "en", theme, width);
    await clickText(".learn-wiki-tree button", "Architecture");
    await waitFor(
      client,
      `document.querySelectorAll(".learn-diagram-node").length===10&&document.querySelectorAll(".learn-diagram-layer").length===4&&!document.querySelector(".learn-diagram-edge")&&${fits}&&document.documentElement.scrollWidth<=innerWidth`,
      `large diagram in full (${theme}, ${width}px)`,
    );
    await screenshot(`architecture-large-${theme}-${width}`, output);
    await evaluate(
      client,
      'document.querySelector("[data-module=agent]").dispatchEvent(new MouseEvent("mouseover",{bubbles:true,relatedTarget:document.body}))',
    );
    await waitFor(
      client,
      'document.querySelectorAll(".learn-diagram-edge-dep").length===3&&document.querySelectorAll(".learn-diagram-edge-user").length===3',
      "large diagram focus edges",
    );
    await screenshot(`architecture-large-focus-${theme}-${width}`, output);
  }
  checks.push(
    "architecture at mini-swe-agent scale (10 modules / 4 layers): full names, no default edges, hover edges; light/dark; 700px",
  );

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
  await waitFor(client, 'Boolean(document.querySelector(".learn-progress-pill"))', "created study");
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
