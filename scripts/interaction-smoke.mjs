import { mkdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawn } from "node:child_process";

const chrome = process.env.CHROME ?? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const baseUrl = process.env.LOOM_INTERACTION_BASE_URL ?? "http://127.0.0.1:1420/preview/planning";
const debuggingPort = Number(process.env.LOOM_INTERACTION_CDP_PORT ?? "9323");
const userDataDir = join(tmpdir(), "loom-interaction-smoke-chrome");

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

async function navigate(client, url) {
  await client.send("Page.navigate", { url });
  await waitFor(client, "document.readyState === 'complete' && Boolean(document.querySelector('.app-layout, .settings-page'))", url);
}

function textIncludes(...terms) {
  const checks = terms
    .map((term) => `text.includes(${JSON.stringify(term.toLowerCase())})`)
    .join(" && ");
  return `(() => { const text = document.body.innerText.toLowerCase(); return ${checks}; })()`;
}

function clickElement(selector) {
  return `
    (() => {
      const element = document.querySelector(${JSON.stringify(selector)});
      if (!element) return { ok: false, reason: 'missing' };
      if (element.disabled) return { ok: false, reason: 'disabled' };
      element.scrollIntoView({ block: 'center', inline: 'center' });
      for (const type of ['pointerdown', 'mousedown', 'pointerup', 'mouseup', 'click']) {
        element.dispatchEvent(new MouseEvent(type, { bubbles: true, cancelable: true, view: window }));
      }
      element.click();
      return {
        ok: true,
        modalOpen: Boolean(document.querySelector('.project-modal, .task-modal')),
        text: element.textContent.trim(),
      };
    })()
  `;
}

async function clickRequired(client, selector, label) {
  const result = await evaluate(client, clickElement(selector));
  if (!result?.ok) {
    throw new Error(`Could not click ${label}: ${JSON.stringify(result)}`);
  }
  return result;
}

async function main() {
  await removeDirWithRetries(userDataDir);
  mkdirSync(userDataDir, { recursive: true });

  const child = spawn(chrome, [
    "--headless=new",
    "--disable-gpu",
    "--no-first-run",
    "--window-size=1440,1000",
    `--user-data-dir=${userDataDir}`,
    `--remote-debugging-port=${debuggingPort}`,
    `${baseUrl}?screen=board`,
  ], {
    stdio: ["ignore", "ignore", "pipe"],
  });

  child.stderr.on("data", () => undefined);

  try {
    const targets = await waitForJson(`http://127.0.0.1:${debuggingPort}/json/list`);
    const page = targets.find((target) => target.type === "page");
    if (!page?.webSocketDebuggerUrl) {
      throw new Error("Chrome did not expose a page target");
    }

    const client = createCdpClient(page.webSocketDebuggerUrl);
    try {
      await client.send("Page.enable");
      await client.send("Runtime.enable");
      await client.send("Emulation.setDeviceMetricsOverride", {
        width: 1440,
        height: 1000,
        deviceScaleFactor: 1,
        mobile: false,
      });
      await navigate(client, `${baseUrl}?screen=board`);
      await waitFor(client, textIncludes("Wire stream filter into reducer", "LOOM-12"), "board screen");

      await clickRequired(client, ".project-add-button", "add project button");
      await waitFor(client, textIncludes("Open a local directory", "Detected"), "add project modal");
      await clickRequired(client, ".project-modal-close", "add project close button");
      await waitFor(client, `(${textIncludes("Open a local directory")}) === false`, "add project modal close");

      await evaluate(client, `
        (() => {
          const element = [...document.querySelectorAll('button')].find((button) => button.textContent.includes('新建任务'));
          if (!element || element.disabled) return false;
          element.scrollIntoView({ block: 'center', inline: 'center' });
          for (const type of ['pointerdown', 'mousedown', 'pointerup', 'mouseup', 'click']) {
            element.dispatchEvent(new MouseEvent(type, { bubbles: true, cancelable: true, view: window }));
          }
          return true;
        })()
      `);
      await waitFor(client, textIncludes("Invite agents to discuss", "Suggested primary"), "new task modal");
      await evaluate(client, `
        const title = document.querySelector('.task-modal input');
        const desc = document.querySelector('.task-modal textarea');
        const setValue = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set;
        const setTextArea = Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, 'value').set;
        setValue.call(title, 'Interaction smoke task');
        title.dispatchEvent(new Event('input', { bubbles: true }));
        setTextArea.call(desc, 'Verify New Task modal inputs are usable.');
        desc.dispatchEvent(new Event('input', { bubbles: true }));
        true;
      `);
      await waitFor(client, `
        const submit = [...document.querySelectorAll('button')].find((button) => button.textContent.includes('Start discussion'));
        submit && !submit.disabled;
      `, "new task submit enabled");
      await clickRequired(client, ".task-modal-close", "new task close button");
      await waitFor(client, `(${textIncludes("Invite agents to discuss")}) === false`, "new task modal close");
      await navigate(client, `${baseUrl}?screen=board`);
      await waitFor(client, textIncludes("Wire stream filter into reducer", "LOOM-12"), "board screen after modal");

      await navigate(client, `${baseUrl}?screen=board-speaker`);
      await waitFor(client, textIncludes("speaker", "SPK-4", "Voice preset"), "speaker board");

      await clickRequired(client, ".sidebar-settings-entry", "settings button");
      await waitFor(client, textIncludes("Current project", "Runtime model"), "settings general");
      await evaluate(client, "[...document.querySelectorAll('.settings-nav-item')].find((item) => item.textContent.includes('Agents'))?.click(); true");
      await waitFor(client, textIncludes("Installed agents", "Add custom Agent"), "settings agents tab");

      await navigate(client, `${baseUrl}?screen=session`);
      await waitFor(client, textIncludes("标记为可测试", "子任务"), "session screen");
      await navigate(client, `${baseUrl}?screen=testing`);
      await waitFor(client, textIncludes("调试验收", "验收门禁"), "testing screen");

      client.close();
    } finally {
      client.close();
    }
  } finally {
    await killAndWait(child);
    await removeDirWithRetries(userDataDir);
  }

  console.log("Interaction smoke passed");
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
});
