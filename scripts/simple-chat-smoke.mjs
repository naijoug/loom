import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawn } from "node:child_process";
const chrome = process.env.CHROME ?? (process.platform === "darwin" ? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" : "google-chrome");
const baseUrl = "http://127.0.0.1:1420/?fixture=simple-chat";
const port = 9331;
const output = join(process.cwd(), "designs/exports/simple-chat");
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

  socket.addEventListener("message", (event) => {
    const message = JSON.parse(event.data);
    if (!message.id) {
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
    throw new Error(`Evaluation failed: ${result.exceptionDetails.text}`);
  }
  return result.result.value;
}

async function waitFor(client, expression, label) {
  for (let index = 0; index < 80; index += 1) {
    if (await evaluate(client, expression)) {
      return;
    }
    await delay(100);
  }
  const debug = await evaluate(client, `({
    href: window.location.href,
    text: document.body?.innerText?.slice(0, 500) ?? '',
  })`);
  throw new Error(`Timed out waiting for ${label}: ${JSON.stringify(debug)}`);
}


async function main() {
  mkdirSync(output, { recursive: true });
  const child = spawn(chrome, ['--headless=new', '--disable-gpu', '--no-first-run', '--no-default-browser-check', `--remote-debugging-port=${port}`, `--user-data-dir=${userDataDir}`, 'about:blank'], { stdio: 'ignore' });
  let client;
  try {
    const pages = await waitForJson(`http://127.0.0.1:${port}/json/list`);
    client = createCdpClient(pages.find(page => page.type === 'page').webSocketDebuggerUrl);
    await client.send('Page.enable'); await client.send('Runtime.enable');
    await client.send('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: 'light' }] });
    await client.send('Emulation.setDeviceMetricsOverride', { width: 1280, height: 820, deviceScaleFactor: 1, mobile: false });
    const click = async selector => evaluate(client, `document.querySelector(${JSON.stringify(selector)})?.click()`);
    const clickText = async (selector, text) => evaluate(client, `Array.from(document.querySelectorAll(${JSON.stringify(selector)})).find(e => e.textContent.trim() === ${JSON.stringify(text)})?.click()`);
    const screenshot = async name => {
      await delay(150);
      const { data } = await client.send('Page.captureScreenshot', { format: 'png' });
      writeFileSync(join(output, `${name}.png`), Buffer.from(data, 'base64'));
      const size = await evaluate(client, '({ width: innerWidth, document: document.documentElement.scrollWidth })');
      if (size.document > size.width + 1) throw Error(`Horizontal overflow on ${name}: ${JSON.stringify(size)}`);
    };
    await client.send('Page.navigate', { url: baseUrl });
    await waitFor(client, 'Boolean(document.querySelector(".chat-mode-cards"))', 'new chat');
    await waitFor(client, 'document.querySelectorAll(".chat-tree-session").length === 2', 'project sessions');
    if (await evaluate(client, 'document.querySelectorAll(".chat-empty-session .chat-agent-menu").length')) throw Error('Agent selection leaked into mode cards');
    await screenshot('new-chat-light');
    await click('.chat-mode-cards button:nth-child(2)');
    await click('[aria-label="Select Agent"]');
    await evaluate(client, `Array.from(document.querySelectorAll('[role="menuitemradio"]')).find(e => e.textContent.includes('Codex')).click()`);
    await evaluate(client, `(() => { const input = document.querySelector('textarea'); Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(input, 'Preview creation test'); input.dispatchEvent(new Event('input', { bubbles: true })); })()`);
    await waitFor(client, `!document.querySelector('[aria-label="Send message"]').disabled`, "enabled composer");
    await click('[aria-label="Send message"]');
    await waitFor(client, 'Boolean(document.querySelector(".chat-ask-confirm"))', 'Code turn confirmation');
    await clickText('.chat-ask-confirm button', 'Confirm and send');
    await waitFor(client, 'document.querySelector(".chat-session-header h1")?.textContent === "Preview creation test"', 'new chat creation and first send');
    if (!await evaluate(client, 'document.querySelector(".chat-agent-picker > button")?.textContent.includes("Codex")')) throw Error('New chat used the wrong Agent');
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
    await client.send('Page.reload');
    await waitFor(client, 'document.documentElement.lang === "zh-CN" && document.documentElement.dataset.theme === "dark"', 'persisted appearance');
    await client.send('Emulation.setDeviceMetricsOverride', { width: 700, height: 820, deviceScaleFactor: 1, mobile: false });
    await waitFor(client, '!document.querySelector(".chat-project-tree")', 'collapsed narrow sidebar');
    await screenshot('new-chat-narrow');
    writeFileSync(join(output, 'result.json'), JSON.stringify({ status: 'passed', kind: 'synthetic-web-only', checks: ['new chat sends with selected Agent and mode', 'project groups', 'new chat mode-only cards', 'tool cards and diff', 'light/dark', 'language persists after reload', 'Settings preserves selected chat', '700px sidebar collapse', 'no horizontal overflow'] }, null, 2));
    console.log(`Simple Chat browser smoke passed: ${output}`);
  } finally { client?.close(); await killAndWait(child); await removeDirWithRetries(userDataDir); }
}
main().catch(error => { console.error(error); process.exitCode = 1; });
