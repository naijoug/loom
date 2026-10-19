const test = require('node:test');
const assert = require('node:assert/strict');
const { spawnSync } = require('node:child_process');
const path = require('node:path');
const fs = require('node:fs');
const os = require('node:os');
const script = path.resolve(__dirname, '../../scripts/debug.sh');
const repo = path.dirname(path.dirname(script));
function run(code, ...args) {
  return spawnSync('/bin/bash', ['-c', 'source "$1"; ' + code, 'test', script, ...args], { encoding: 'utf8' });
}
test('debug stop recognizes dev executables, never build/compiler commands containing cfg(dev)', () => {
  for (const command of ['pnpm tauri dev', `node ${repo}/node_modules/@tauri-apps/cli/tauri.js dev`, 'target/debug/loom', `${repo}/src-tauri/target/debug/loom`, `${repo}/src-tauri/target/debug/bundle/macos/Loom.app/Contents/MacOS/loom`, `node ${repo}/scripts/craft-reference.mjs`, `node ${repo}/scripts/browser-probe.mjs`]) {
    assert.equal(run('is_desktop_dev_command "$2"', command).status, 0, command);
  }
  for (const command of [
    `rustc --out-dir ${repo}/src-tauri/target/release --cfg desktop --check-cfg cfg(dev)`,
    `rustc --out-dir ${repo}/src-tauri/target/debug/loom --cfg desktop`,
    `pnpm tauri build --bundles app`,
    `node ${repo}/node_modules/@tauri-apps/cli/tauri.js build`,
    `node unrelated.js --note=tauri-dev`,
    `node /unrelated/scripts/craft-reference.mjs`,
    `node /unrelated/scripts/browser-probe.mjs`,
  ]) assert.equal(run('is_desktop_dev_command "$2"', command).status, 1, command);
});
test('debug stop succeeds with no dev process and a PID disappearing before inspection', () => {
  const empty = run('ps() { :; }; rm() { :; }; stop_desktop_dev');
  assert.equal(empty.status, 0, empty.stderr);
  const disappeared = run('kill() { return 0; }; ps() { :; }; stop_pid 12345');
  assert.equal(disappeared.status, 0, disappeared.stderr);
});
test('unavailable process inspection fails shutdown and preserves the PID hint', () => {
  const result = run('ps() { return 1; }; rm() { echo PID_HINT_REMOVED; }; main stop && echo FALSE_SUCCESS');
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /shutdown is unverified/);
  assert.ok(!result.stdout.includes('PID_HINT_REMOVED'));
  assert.ok(!result.stdout.includes('FALSE_SUCCESS'));
  assert.ok(!result.stdout.includes('Loom local dev stopped'));
});
test('bundled SDK preview configures and validates Bun before launching the app', (t) => {
  const fixture = fs.mkdtempSync(path.join(os.tmpdir(), 'loom-debug-bundle-'));
  t.after(() => fs.rmSync(fixture, { recursive: true, force: true }));
  const binary = path.join(fixture, 'src-tauri/target/debug/bundle/macos/Loom.app/Contents/MacOS/loom');
  fs.mkdirSync(path.dirname(binary), { recursive: true });
  fs.writeFileSync(binary, '#!/bin/sh\nexit 0\n', { mode: 0o700 });
  const bun = path.join(fixture, 'bin/bun');
  fs.mkdirSync(path.dirname(bun));
  fs.writeFileSync(bun, '#!/bin/sh\nexit 0\n', { mode: 0o700 });
  const code = 'stop_all() { :; }; ROOT_DIR="$3"; DESKTOP_PID_FILE=/dev/null; LOOM_PREVIEW_TARGET=workbench-bundle; LOOM_WORKBENCH_SDK=1; LOOM_WORKBENCH_BUN="$2"; exec() { printf "SDK_BUN=%s\\nLAUNCH=%s\\n" "$LOOM_WORKBENCH_BUN" "$1"; }; start_desktop';
  const valid = run(code, process.execPath, fixture);
  assert.equal(valid.status, 0, valid.stderr);
  assert.ok(valid.stdout.includes(`SDK_BUN=${process.execPath}`));
  assert.ok(valid.stdout.includes(`LAUNCH=${binary}`));
  const discovered = run('PATH="$3/bin:$PATH"; ' + code, '', fixture);
  assert.equal(discovered.status, 0, discovered.stderr);
  assert.ok(discovered.stdout.includes(`SDK_BUN=${bun}`));
  const invalid = run(code, '/nonexistent/loom-test-bun', fixture);
  assert.notEqual(invalid.status, 0);
  assert.match(invalid.stderr, /absolute Bun executable/);
  assert.ok(!invalid.stdout.includes('LAUNCH='));
});
