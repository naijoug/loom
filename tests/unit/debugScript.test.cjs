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
