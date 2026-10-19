import { readFileSync, writeFileSync, mkdirSync, readdirSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import ts from 'typescript';

// Static inventory only. Passing this check does not prove functional parity.
const root = dirname(dirname(fileURLToPath(import.meta.url)));
const reference = join(root, '.ref/repos/craft-agents-oss');
const expectedSha = '73bd9c2a3573158bea880984eb8d5fdb41e0cac2';
const sha = execFileSync('git', ['-C', reference, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
if (sha !== expectedSha) throw new Error(`Reference moved: expected ${expectedSha}, got ${sha}`);
const inputs = new Map();
function source(path) {
  const text = readFileSync(join(reference, path), 'utf8');
  inputs.set(path, createHash('sha256').update(text).digest('hex'));
  return ts.createSourceFile(path, text, ts.ScriptTarget.Latest, true);
}
function find(node, predicate) {
  if (predicate(node)) return node;
  return ts.forEachChild(node, child => find(child, predicate));
}
function declaration(path, name, predicate = ts.isVariableDeclaration) {
  const found = find(source(path), node => predicate(node) && node.name?.getText() === name);
  if (!found) throw new Error(`${path}: missing ${name}`);
  return found;
}
function literal(node) {
  if (ts.isStringLiteral(node)) return node.text;
  if (node.kind === ts.SyntaxKind.TrueKeyword) return true;
  if (node.kind === ts.SyntaxKind.FalseKeyword) return false;
  throw new Error(`Expected literal, got ${node.getText()}`);
}
function union(path, name) {
  const node = declaration(path, name, ts.isTypeAliasDeclaration);
  if (!ts.isUnionTypeNode(node.type)) throw new Error(`Expected union: ${name}`);
  return node.type.types.map(type => literal(type.literal));
}
const toolPath = 'packages/session-tools-core/src/tool-defs.ts';
const tools = declaration(toolPath, 'SESSION_TOOL_DEFS').initializer.elements.map(node => {
  const fields = Object.fromEntries(node.properties.map(prop => [prop.name.getText(), prop.initializer]));
  return {
    name: literal(fields.name),
    schema: fields.inputSchema.getText(),
    executionMode: literal(fields.executionMode),
    safeMode: literal(fields.safeMode),
    readOnly: fields.readOnly ? literal(fields.readOnly) : false,
    handler: fields.handler.getText(),
    source: toolPath,
  };
});
if (new Set(tools.map(tool => tool.name)).size !== tools.length) throw new Error('Duplicate tool name');
const settingsPath = 'apps/electron/src/renderer/pages/settings/settings-pages.ts';
const settings = declaration(settingsPath, 'SETTINGS_PAGE_COMPONENTS').initializer.properties.map(prop => ({
  id: prop.name.getText(), component: prop.initializer.getText(), source: settingsPath,
}));
const configPath = 'packages/shared/src/config/llm-connections.ts';
const plan = readFileSync(join(root, 'docs/plans/2026-09-30/13:17-craft-parity-rebuild.md'), 'utf8');
// Acceptance status/evidence columns are maintained separately from the static inventory.
const features = [...plan.matchAll(/^\| (F\d\d) \| ([^|\n]+?) \| ([^|\n]+?) \| ([^|\n]+?) \|.*$/gm)].map(([, id, scope, locations, acceptance]) => ({
  id, scope, locations, acceptance,
}));
if (features.length !== 23) throw new Error('Expected the 23 planned feature areas');
const endpoints = [];
for (const path of [
  'packages/shared/src/branding.ts',
  'packages/shared/src/pages/publisher.ts',
  'packages/shared/src/auth/oauth-relay.ts',
]) {
  // URLs are public source constants, not environment values or user configuration.
  const text = source(path).text;
  const urls = [...new Set(text.match(/https:\/\/[A-Za-z0-9._/:-]+/g) ?? [])].sort();
  endpoints.push({ source: path, urls });
}
const pkg = JSON.parse(source('package.json').text);
const files = path => readdirSync(join(reference, path), { withFileTypes: true })
  .filter(entry => entry.isFile() && /\.(ts|tsx)$/.test(entry.name)).map(entry => `${path}/${entry.name}`).sort();
const output = {
  schemaVersion: 1,
  evidenceKind: 'static-source-inventory',
  upstream: { repository: 'https://github.com/craft-ai-agents/craft-agents-oss', sha, version: pkg.version },
  scope: 'Runtime behavior, screenshots, credentials and cross-platform parity require separate acceptance evidence.',
  features, tools, settings,
  providerTypes: union(configPath, 'LlmProviderType'),
  authTypes: union(configPath, 'LlmAuthType'),
  permissionModes: union('packages/shared/src/agent/mode-types.ts', 'PermissionMode'),
  sdkVersions: Object.fromEntries(['@anthropic-ai/claude-agent-sdk', '@earendil-works/pi-coding-agent', '@earendil-works/pi-ai'].map(name => [name, pkg.dependencies[name]])),
  rpcHandlers: files('packages/server-core/src/handlers/rpc'),
  contentViewers: files('packages/ui/src/components/overlay'),
  externalServiceSources: endpoints,
  sourceHashes: Object.fromEntries([...inputs].sort(([a], [b]) => a.localeCompare(b))),
};
const destination = join(root, 'designs/craft-v0.14.0/inventory.json');
const rendered = JSON.stringify(output, null, 2) + '\n';
if (process.argv.includes('--check')) {
  if (readFileSync(destination, 'utf8') !== rendered) throw new Error('Inventory changed; review before regenerating');
} else {
  mkdirSync(dirname(destination), { recursive: true });
  writeFileSync(destination, rendered);
}
console.log(`Craft ${pkg.version}: ${features.length} areas, ${tools.length} tools, ${settings.length} settings, ${output.authTypes.length} auth types; ${process.argv.includes('--check') ? 'verified' : 'written'}.`);
