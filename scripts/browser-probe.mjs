/** Automated product probe of the isolated Rust CEF host, through its own stdio API. */
import { spawn } from 'node:child_process';
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import assert from 'node:assert/strict';
const root = dirname(dirname(fileURLToPath(import.meta.url)));
const binary = join(root, 'experiments/browser-host/target/bundle/loom-browser-host.app/Contents/MacOS/loom-browser-host');
const profile = join(root, '.ref/state/browser-probe/workspace-a');
const report = { evidenceKind: 'native-rust-cef-loopback-probe', recordedAt: new Date().toISOString(), platform: `${process.platform}-${process.arch}`, steps: [], complete: false };
const artifact = join(root, 'docs/dogfood/craft-v014/browser-probe-results.json');
mkdirSync(profile, { recursive: true });
let current;
let nextId = 1;
let stopping = false;
function persist() {
  report.sourceSha256 = Object.fromEntries(['experiments/browser-host/src/bridge.rs', 'experiments/browser-host/src/mac/mod.rs', 'experiments/browser-host/src/shared/mod.rs', 'experiments/browser-host/src/shared/simple_app.rs', 'experiments/browser-host/src/shared/simple_handler/mod.rs', 'experiments/browser-host/Cargo.lock', 'scripts/browser-probe.mjs', 'experiments/browser-host/web/index.html'].map(file => [file, createHash('sha256').update(readFileSync(join(root, file))).digest('hex')]));
  writeFileSync(artifact, JSON.stringify(report, null, 2) + '\n');
}
function step(name, detail = {}) { report.steps.push({ name, passed: true, ...detail }); console.log(`PASS ${name}`); }
function launch() {
  const env = Object.fromEntries(['PATH', 'HOME', 'USER', 'SHELL', 'TMPDIR', 'LANG'].filter(key => process.env[key]).map(key => [key, process.env[key]]));
  env.LOOM_BROWSER_PROFILE = profile;
  const child = spawn(binary, ['--url=http://127.0.0.1:1420/', '--use-alloy-style'], { env, stdio: ['pipe', 'pipe', 'pipe'] });
  const waits = new Map();
  const backlog = new Map();
  let buffer = '';
  let stderr = '';
  child.stderr.on('data', bytes => { stderr = (stderr + bytes.toString()).slice(-8000); });
  function deliver(event) {
    const key = event.kind === 'cdpResult' ? `cdp:${event.id}` : event.kind;
    const queue = waits.get(key);
    if (queue?.length) queue.shift().resolve(event);
    else if (event.kind === 'loaded' || event.kind === 'browserCreated') backlog.set(key, [...(backlog.get(key) ?? []), event]);
  }
  child.stdout.on('data', bytes => {
    buffer += bytes.toString();
    let at;
    while ((at = buffer.indexOf('\n')) >= 0) {
      const line = buffer.slice(0, at); buffer = buffer.slice(at + 1);
      try { deliver(JSON.parse(line)); } catch { /* upstream startup diagnostics are not protocol frames */ }
    }
  });
  const exited = new Promise((resolve, reject) => {
    child.once('error', reject);
    child.once('exit', (code, signal) => {
      for (const queue of waits.values()) for (const pending of queue) pending.reject(new Error(`Browser exited ${code ?? signal}: ${stderr}`));
      resolve({ code, signal });
    });
  });
  function wait(key, timeout = 20000) {
    const cached = backlog.get(key); if (cached?.length) return Promise.resolve(cached.shift());
    return new Promise((resolve, reject) => {
      const entry = { resolve: event => { clearTimeout(timer); resolve(event); }, reject: error => { clearTimeout(timer); reject(error); } };
      const queue = waits.get(key) ?? []; queue.push(entry); waits.set(key, queue);
      const timer = setTimeout(() => { queue.splice(queue.indexOf(entry), 1); reject(new Error(`Timed out awaiting ${key}: ${stderr}`)); }, timeout);
    });
  }
  function send(value) { child.stdin.write(JSON.stringify(value) + '\n'); }
  async function cdp(method, params = {}) {
    const id = nextId++; const done = wait(`cdp:${id}`); send({ kind: 'cdp', id, method, params });
    const result = await done; assert.equal(result.success, true, JSON.stringify(result)); return result.result;
  }
  async function evaluate(expression) { const response = await cdp('Runtime.evaluate', { expression, returnByValue: true }); assert.ok(!response.exceptionDetails, JSON.stringify(response)); return response.result.value; }
  async function close() {
    if (child.exitCode !== null || child.signalCode !== null || child.stdin.destroyed) return exited;
    send({ kind: 'close' });
    const timer = setTimeout(() => child.kill('SIGKILL'), 8000); timer.unref();
    const result = await exited; clearTimeout(timer); return result;
  }
  return { child, wait, send, cdp, evaluate, close, exited };
}
async function stop() { if (stopping) return; stopping = true; if (current) await current.close(); }
process.on('SIGTERM', () => { void stop(); });
process.on('SIGINT', () => { void stop(); });
try {
  current = launch();
  const loaded = await current.wait('loaded'); assert.equal(loaded.status, 200);
  assert.equal(await current.evaluate('document.title'), 'Loom Browser Probe');
  step('native_window_and_local_page');
  const ax = await current.cdp('Accessibility.getFullAXTree');
  assert.ok(ax.nodes.some(node => node.role?.value === 'textbox' && node.name?.value === 'Name'));
  step('cdp_accessibility_tree', { nodeCount: ax.nodes.length });
  const text = await current.evaluate(`document.querySelector('#name').value='Ada'; document.querySelector('#form').requestSubmit(); document.querySelector('#result').textContent`);
  assert.equal(text, 'Hello Ada'); step('cdp_form_input_and_result');
  const hidden = current.wait('hidden'); current.send({ kind: 'hide' }); await hidden;
  assert.equal(await current.evaluate(`document.querySelector('#result').textContent`), 'Hello Ada');
  const shown = current.wait('shown'); current.send({ kind: 'show' }); await shown;
  step('native_hide_show_keeps_page');
  const windowClosed = current.wait('hidden'); current.send({ kind: 'windowClose' }); await windowClosed;
  assert.equal(await current.evaluate(`document.querySelector('#result').textContent`), 'Hello Ada');
  const reopened = current.wait('shown'); current.send({ kind: 'show' }); await reopened;
  step('native_window_close_hides_without_destroying');
  const seed = await current.evaluate(`localStorage.setItem('loom-probe','persisted'); document.cookie='loom_probe=1; Max-Age=3600; Path=/; SameSite=Lax'; ({stored:localStorage.getItem('loom-probe'),cookie:document.cookie})`);
  assert.equal(seed.stored, 'persisted'); assert.ok(seed.cookie.includes('loom_probe=1'));
  step('profile_storage_and_cookie_written');
  let closed = await current.close(); assert.equal(closed.code, 0); step('graceful_native_shutdown');
  current = launch(); await current.wait('loaded');
  const restored = await current.evaluate(`({stored:localStorage.getItem('loom-probe'),cookie:document.cookie})`);
  assert.equal(restored.stored, 'persisted'); assert.ok(restored.cookie.includes('loom_probe=1'));
  step('profile_storage_and_cookie_survive_restart');
  if (process.env.LOOM_BROWSER_KEEP_OPEN === '1') {
    step('ready_for_manual_native_input'); report.complete = true; persist();
    console.log('Browser remains open for native input verification. Use scripts/debug.sh stop when finished.');
    await current.exited;
  } else { closed = await current.close(); assert.equal(closed.code, 0); report.complete = true; persist(); }
} catch (error) {
  report.error = error instanceof Error ? error.message : String(error);
  persist(); console.error(report.error); process.exitCode = 1;
  await stop();
}
