import { skillsSmoke } from "./skills-smoke.mjs";
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawn } from "node:child_process";
const chrome = process.env.CHROME ?? (process.platform === "darwin" ? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" : "google-chrome");
const baseUrl = "http://127.0.0.1:1420/?fixture=simple-chat";
const port = 9331;
const output = join(process.cwd(), "designs/exports/simple-chat");
const cardsOutput = join(process.cwd(), "designs/exports/openui-inline-cards");
const userDataDir = join(tmpdir(), `loom-chat-smoke-${process.pid}`);
function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function removeDirWithRetries(path) {
  let lastError;
  for (let index = 0; index < 20; index += 1) {
    try {
      rmSync(path, { recursive: true, force: true });
      return;
    } catch (error) {
      lastError = error;
      await delay(100);
    }
  }
  throw lastError;
}

async function killAndWait(child) {
  if (child.exitCode !== null || child.signalCode !== null) {
    return;
  }

  await new Promise((resolve) => {
    const timeout = setTimeout(resolve, 1500);
    child.once("exit", () => {
      clearTimeout(timeout);
      resolve();
    });
    child.kill("SIGKILL");
  });
}

async function waitForJson(url, attempts = 80) {
  let lastError;
  for (let index = 0; index < attempts; index += 1) {
    try {
      const response = await fetch(url);
      if (response.ok) {
        return response.json();
      }
    } catch (error) {
      lastError = error;
    }
    await delay(100);
  }
  throw lastError ?? new Error(`Timed out waiting for ${url}`);
}

function createCdpClient(wsUrl) {
  const socket = new WebSocket(wsUrl);
  let nextId = 1;
  const pending = new Map();

  const listeners = new Map();

  socket.addEventListener("message", (event) => {
    const message = JSON.parse(event.data);
    if (!message.id) {
      listeners.get(message.method)?.(message.params);
      return;
    }
    const entry = pending.get(message.id);
    if (!entry) {
      return;
    }
    pending.delete(message.id);
    if (message.error) {
      entry.reject(new Error(`${message.error.message}: ${JSON.stringify(message.error.data ?? "")}`));
    } else {
      entry.resolve(message.result);
    }
  });

  const opened = new Promise((resolve, reject) => {
    socket.addEventListener("open", resolve, { once: true });
    socket.addEventListener("error", reject, { once: true });
  });

  async function send(method, params = {}) {
    await opened;
    const id = nextId;
    nextId += 1;
    const result = new Promise((resolve, reject) => {
      pending.set(id, { resolve, reject });
    });
    socket.send(JSON.stringify({ id, method, params }));
    return result;
  }

  return {
    send,
    on: (method, listener) => listeners.set(method, listener),
    close: () => socket.close(),
  };
}

async function evaluate(client, expression) {
  const result = await client.send("Runtime.evaluate", {
    expression,
    awaitPromise: true,
    returnByValue: true,
  });
  if (result.exceptionDetails) {
    throw new Error(`Evaluation failed: ${result.exceptionDetails.exception?.description ?? result.exceptionDetails.text}`);
  }
  return result.result.value;
}

// ~20 s at the default: CI macOS runners vary widely in speed, and a failing check only costs the wait.
async function waitFor(client, expression, label, attempts = 200) {
  // A reload or navigation can briefly leave no document; treat a throwing check as "not yet".
  let lastError;
  for (let index = 0; index < attempts; index += 1) {
    try {
      if (await evaluate(client, expression)) {
        return;
      }
    } catch (error) {
      lastError = error;
    }
    await delay(100);
  }
  const debug = await evaluate(client, `({
    href: window.location.href,
    viewport: innerWidth,
    narrow: matchMedia('(max-width: 759px)').matches,
    events: window.__narrowEvents ?? null,
    text: document.body?.innerText?.slice(0, 500) ?? '',
  })`);
  throw new Error(`Timed out waiting for ${label}: ${JSON.stringify(debug)}${lastError ? ` (last error: ${lastError.message})` : ''}`);
}


// OpenUI inline cards (experiment): opt in through Settings, check rendering, fallback and the
// insert-only shell action, and fail on any request that leaves the local dev server.
async function inlineCardsSmoke(client, { click, clickText, screenshot }) {
  mkdirSync(cardsOutput, { recursive: true });
  const external = [];
  client.on('Network.requestWillBeSent', ({ request }) => {
    const { protocol, hostname } = new URL(request.url);
    if (!['data:', 'blob:'].includes(protocol) && !['127.0.0.1', 'localhost'].includes(hostname)) external.push(request.url);
  });
  await client.send('Network.enable');
  await client.send('Emulation.setDeviceMetricsOverride', { width: 1280, height: 820, deviceScaleFactor: 1, mobile: false });
  await evaluate(client, 'localStorage.clear()');
  await client.send('Page.navigate', { url: `${baseUrl}&cards=1` });
  await waitFor(client, 'document.querySelectorAll(".chat-tree-session").length === 3', 'card preview session');
  const openCardSession = async () => {
    // After a reload the tree renders later than the lang/theme attributes; wait for the row itself.
    const row = `Array.from(document.querySelectorAll('.chat-tree-session')).find(e => e.textContent.includes('Inline card preview'))`;
    await waitFor(client, `Boolean(${row})`, 'card preview session row');
    await evaluate(client, `${row}.click()`);
    await waitFor(client, 'document.querySelectorAll(".chat-transcript code[data-language=\\"openui-lang\\"], .chat-inline-card").length >= 2', 'card session transcript');
  };
  await openCardSession();
  if (await evaluate(client, 'document.querySelectorAll(".chat-inline-card").length')) throw Error('Inline cards rendered while the experiment is off');
  await click('.chat-project-tree footer button');
  await waitFor(client, 'Boolean(document.querySelector(".chat-settings"))', 'settings');
  await clickText('.chat-settings nav button', 'Experiments');
  await waitFor(client, 'Boolean(document.querySelector(".settings-toggle-row [role=switch]"))', 'experiments switch');
  await screenshot('settings-experiments', cardsOutput);
  await click('.settings-toggle-row [role=switch]');
  await waitFor(client, 'document.querySelector(".settings-toggle-row [role=switch]").checked', 'experiment enabled');
  await click('.chat-settings aside > button');
  await openCardSession();
  await waitFor(client, 'Boolean(document.querySelector(".chat-card-stack")) && Boolean(document.querySelector(".chat-inline-card-note"))', 'rendered and fallback cards', 300);
  await screenshot('cards-light', cardsOutput);
  await clickText('.chat-inline-card-actions button', 'Insert into input');
  await waitFor(client, 'document.querySelector("textarea").value.includes("src/main.tsx — React entry")', 'card text inserted into the draft');
  if (await evaluate(client, 'document.querySelectorAll(".chat-inline-card").length') !== 2) throw Error('Insert changed the transcript');
  await clickText('.chat-inline-card-actions button', 'View source');
  await waitFor(client, 'Boolean(document.querySelector(".chat-inline-card:not([data-fallback]) code[data-language=\\"openui-lang\\"]"))', 'card source view');
  await screenshot('cards-source', cardsOutput);
  await clickText('.chat-inline-card-actions button', 'View card');
  await evaluate(client, `localStorage.setItem('loom-app-settings', JSON.stringify({ ...JSON.parse(localStorage.getItem('loom-app-settings')), themeMode: 'dark', locale: 'zh-CN' }))`);
  await client.send('Page.reload');
  await waitFor(client, 'document.documentElement.lang === "zh-CN" && document.documentElement.dataset.theme === "dark"', 'dark Chinese appearance');
  await openCardSession();
  await waitFor(client, 'Boolean(document.querySelector(".chat-card-stack")) && document.querySelector(".chat-inline-card-note")?.textContent.includes("源码")', 'Chinese card fallback');
  await screenshot('cards-dark-zh', cardsOutput);
  // Count the events the sidebar relies on so a timeout shows whether the narrow change ever reached the page.
  await evaluate(client, `(() => { window.__narrowEvents = { change: 0, resize: 0 }; matchMedia('(max-width: 759px)').addEventListener('change', () => window.__narrowEvents.change++); addEventListener('resize', () => window.__narrowEvents.resize++); })()`);
  await client.send('Emulation.setDeviceMetricsOverride', { width: 700, height: 820, deviceScaleFactor: 1, mobile: false });
  await waitFor(client, '!document.querySelector(".chat-project-tree")', 'collapsed narrow sidebar');
  await screenshot('cards-narrow', cardsOutput);
  if (external.length) throw Error(`External requests: ${external.join(', ')}`);
  writeFileSync(join(cardsOutput, 'result.json'), JSON.stringify({ status: 'passed', kind: 'synthetic-web-only', externalRequests: 0, checks: ['off by default: openui-lang stays a code block', 'Settings > Experiments switch enables cards', 'valid card renders', 'invalid card falls back to source with notice', 'insert fills the draft without sending', 'source/card toggle', 'dark theme and zh-CN', '700px narrow window', 'no horizontal overflow', 'no external network requests'] }, null, 2));
  console.log(`OpenUI inline cards smoke passed: ${cardsOutput}`);
}

async function main() {
  mkdirSync(output, { recursive: true });
  const child = spawn(chrome, ['--headless=new', '--disable-gpu', '--disable-background-timer-throttling', '--disable-backgrounding-occluded-windows', '--disable-renderer-backgrounding', '--no-first-run', '--no-default-browser-check', `--remote-debugging-port=${port}`, `--user-data-dir=${userDataDir}`, 'about:blank'], { stdio: 'ignore' });
  let client;
  try {
    const pages = await waitForJson(`http://127.0.0.1:${port}/json/list`);
    client = createCdpClient(pages.find(page => page.type === 'page').webSocketDebuggerUrl);
    await client.send('Page.enable'); await client.send('Runtime.enable');
    await client.send('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: 'light' }] });
    await client.send('Emulation.setDeviceMetricsOverride', { width: 1280, height: 820, deviceScaleFactor: 1, mobile: false });
    const click = async selector => evaluate(client, `document.querySelector(${JSON.stringify(selector)})?.click()`);
    const clickText = async (selector, text) => evaluate(client, `Array.from(document.querySelectorAll(${JSON.stringify(selector)})).find(e => e.textContent.trim() === ${JSON.stringify(text)})?.click()`);
    const screenshot = async (name, dir = output) => {
      await delay(150);
      const { data } = await client.send('Page.captureScreenshot', { format: 'png' });
      writeFileSync(join(dir, `${name}.png`), Buffer.from(data, 'base64'));
      const size = await evaluate(client, '({ width: innerWidth, document: document.documentElement.scrollWidth })');
      if (size.document > size.width + 1) throw Error(`Horizontal overflow on ${name}: ${JSON.stringify(size)}`);
    };
    await client.send('Page.navigate', { url: baseUrl });
    await waitFor(client, 'Boolean(document.querySelector(".chat-empty-session h1"))', 'new chat');
    await waitFor(client, 'document.querySelectorAll(".chat-tree-session").length === 2', 'project sessions');
    if (await evaluate(client, 'document.querySelectorAll(".chat-empty-session button").length')) throw Error('New chat page should hold no controls');
    if (!await evaluate(client, 'document.querySelector(".chat-composer-actions > .chat-project-picker > button")?.getAttribute("aria-label")?.startsWith("Project:")')) throw Error('New chat composer should offer the project picker');
    await screenshot('new-chat-light');
    await click('.chat-mode-picker > button');
    await click('.chat-mode-menu [data-mode="ask"]');
    await click('[aria-label="Select Agent"]');
    await evaluate(client, `Array.from(document.querySelectorAll('[role="menuitemradio"]')).find(e => e.textContent.includes('Codex')).click()`);
    await evaluate(client, `(() => { const input = document.querySelector('textarea'); Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(input, 'Preview creation test'); input.dispatchEvent(new Event('input', { bubbles: true })); })()`);
    await waitFor(client, `!document.querySelector('[aria-label="Send message"]').disabled`, "enabled composer");
    await click('[aria-label="Send message"]');
    await waitFor(client, 'Boolean(document.querySelector(".chat-ask-confirm"))', 'Code turn confirmation');
    await clickText('.chat-ask-confirm button', 'Confirm and send');
    await waitFor(client, 'document.querySelector(".chat-session-header h1")?.textContent === "Preview creation test"', 'new chat creation and first send');
    if (!await evaluate(client, 'document.querySelector(".chat-agent-picker > button")?.textContent.includes("Codex")')) throw Error('New chat used the wrong Agent');
    if (await evaluate(client, 'Boolean(document.querySelector(".chat-project-picker"))')) throw Error('Existing chats keep their project; no picker expected');
    await evaluate(client, `Array.from(document.querySelectorAll('.chat-tree-session')).find(e => e.textContent.includes('Refactor the chat entry')).click()`);
    await waitFor(client, 'document.querySelectorAll(".chat-tool-card").length === 3', 'tool cards');
    await click('.chat-tool-card[data-kind="edit"] > button');
    await screenshot('chat-light');
    await click('.chat-project-tree footer button');
    await waitFor(client, 'Boolean(document.querySelector(".chat-settings"))', 'settings');
    await clickText('.chat-settings nav button', 'Appearance');
    await clickText('.chat-settings [role="radio"]', 'Dark');
    await waitFor(client, 'document.documentElement.dataset.theme === "dark"', 'dark theme');
    await screenshot('settings-dark');
    await clickText('.chat-settings [role="radio"]', '简体中文');
    await waitFor(client, 'document.documentElement.lang === "zh-CN"', 'Chinese language');
    await screenshot('settings-chinese');
    await click('.chat-settings aside > button');
    await waitFor(client, '!document.querySelector(".chat-settings")', 'back to active chat');
    if (!await evaluate(client, 'document.querySelector(".chat-session-header h1")?.textContent.includes("Refactor")')) throw Error('Settings lost active chat');
    await screenshot('chat-dark');
    // Reverse both projects' defaults so a reload cannot pass by simply resetting the tree.
    const projectExpansion = `Array.from(document.querySelectorAll('.chat-project-row > button[aria-expanded]')).map(button => ({ path: button.title, expanded: button.getAttribute('aria-expanded') }))`;
    const originalExpansion = await evaluate(client, projectExpansion);
    if (originalExpansion.length !== 2 || new Set(originalExpansion.map(project => project.expanded)).size !== 2) throw Error('Expected one expanded and one collapsed project');
    const expectedExpansion = originalExpansion.map(project => ({ ...project, expanded: project.expanded === 'true' ? 'false' : 'true' }));
    for (const project of expectedExpansion) {
      const button = `Array.from(document.querySelectorAll('.chat-project-row > button[aria-expanded]')).find(button => button.title === ${JSON.stringify(project.path)})`;
      await evaluate(client, `${button}.click()`);
      await waitFor(client, `${button}?.getAttribute('aria-expanded') === ${JSON.stringify(project.expanded)}`, 'toggled project');
    }
    const expansionMatches = `JSON.stringify(${projectExpansion}) === ${JSON.stringify(JSON.stringify(expectedExpansion))}`;
    await waitFor(client, expansionMatches, 'toggled project groups');
    await client.send('Page.reload');
    await waitFor(client, 'document.documentElement.lang === "zh-CN" && document.documentElement.dataset.theme === "dark"', 'persisted appearance');
    await waitFor(client, expansionMatches, 'project expansion persists after reload');
    await client.send('Emulation.setDeviceMetricsOverride', { width: 700, height: 820, deviceScaleFactor: 1, mobile: false });
    await waitFor(client, '!document.querySelector(".chat-project-tree")', 'collapsed narrow sidebar');
    await screenshot('new-chat-narrow');
    writeFileSync(join(output, 'result.json'), JSON.stringify({ status: 'passed', kind: 'synthetic-web-only', checks: ['new chat sends with selected Agent and mode', 'project groups', 'project expansion persists after reload', 'composer mode picker', 'tool cards and diff', 'light/dark', 'language persists after reload', 'Settings preserves selected chat', '700px sidebar collapse', 'no horizontal overflow'] }, null, 2));
    await inlineCardsSmoke(client, { click, clickText, screenshot });
    await skillsSmoke(client, { evaluate, waitFor, click, clickText, screenshot });
    console.log(`Simple Chat browser smoke passed: ${output}`);
  } finally { client?.close(); await killAndWait(child); await removeDirWithRetries(userDataDir); }
}
main().catch(error => { console.error(error); process.exitCode = 1; });
