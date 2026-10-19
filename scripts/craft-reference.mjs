// M0 only: run the fixed upstream app in its own data directories.
import { spawn, execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
const root = dirname(dirname(fileURLToPath(import.meta.url)));
const ref = join(root, '.ref/repos/craft-agents-oss');
const state = join(root, '.ref/state/craft-v0.14.0');
const config = join(state, 'config');
const sha = execFileSync('git', ['-C', ref, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
if (sha !== '73bd9c2a3573158bea880984eb8d5fdb41e0cac2') throw new Error('Reference SHA does not match M0 baseline');
for (const path of [config, join(state, 'chromium'), join(state, 'logs')]) mkdirSync(path, { recursive: true });
const env = Object.fromEntries(['PATH', 'HOME', 'USER', 'SHELL', 'TMPDIR', 'LANG', 'TERM'].filter(key => process.env[key]).map(key => [key, process.env[key]]));
Object.assign(env, {
  CRAFT_CONFIG_DIR: config,
  CRAFT_APP_NAME: 'Loom Craft Reference',
  CRAFT_DEEPLINK_SCHEME: 'loom-craft-reference',
  VITE_DEV_SERVER_URL: 'http://127.0.0.1:1420',
  CRAFT_VITE_PORT: '1420',
  SENTRY_ELECTRON_INGEST_URL: '',
});
// The pinned upstream migrates ~/.craft-agent credentials into an empty config
// directory. Seed a VALID empty encrypted store without invoking its load path.
// This private API is confined to the fixed reference harness, never Loom runtime.
if (!existsSync(join(config, 'credentials.enc'))) {
  execFileSync('bun', ['-e', `
    import { SecureStorageBackend } from ${JSON.stringify(join(ref, 'packages/shared/src/credentials/backends/secure-storage.ts'))};
    const backend = new SecureStorageBackend();
    backend.saveStoreSync({version: 1, credentials: {}, metadata: {createdAt: Date.now(), updatedAt: Date.now()}});
  `], { cwd: ref, env, stdio: 'inherit' });
}
const main = join(ref, 'apps/electron/dist/main.cjs');
if (!existsSync(main)) throw new Error('Build reference main/preload/renderer first');
const entry = join(state, 'entry.cjs');
writeFileSync(join(state, 'package.json'), JSON.stringify({ name: 'loom-craft-reference', version: '0.14.0', main: 'entry.cjs' }));
writeFileSync(entry, `const {app}=require('electron');\napp.setPath('userData',${JSON.stringify(join(state, 'chromium'))});\napp.setPath('logs',${JSON.stringify(join(state, 'logs'))});\nrequire(${JSON.stringify(main)});\n`);
const electron = execFileSync('node', ['-p', 'require("electron")'], { cwd: ref, encoding: 'utf8' }).trim();
const child = spawn(electron, [state], { cwd: ref, env, stdio: 'inherit' });
let stopping = false;
function stop() {
  if (stopping) return;
  stopping = true;
  child.kill('SIGTERM');
  const timer = setTimeout(() => child.kill('SIGKILL'), 8000);
  timer.unref();
}
process.on('SIGTERM', stop);
process.on('SIGINT', stop);
child.on('error', error => { console.error(error.message); process.exitCode = 1; });
child.on('exit', code => { process.exitCode = stopping ? 0 : (code ?? 1); });
