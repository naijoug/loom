/** M0 SDK boundary probe. Loopback-only; not a production provider adapter. */
import { createHash, randomUUID } from 'node:crypto';
import { mkdirSync } from 'node:fs';
import { isAbsolute, join } from 'node:path';
import { z } from 'zod';

const Context = z.object({ workspaceId: z.string().min(1), sessionId: z.string().min(1), turnId: z.string().min(1) }).strict();
const Start = z.object({
  protocolVersion: z.literal(1), kind: z.literal('start'), context: Context,
  provider: z.enum(['claude', 'pi']), endpoint: z.url(), cwd: z.string(), runtimeDir: z.string(),
  prompt: z.string().min(1),
}).strict();
const Decision = z.object({
  protocolVersion: z.literal(1), kind: z.literal('approvalResponse'), context: Context,
  requestId: z.string(), digest: z.string(), allowed: z.boolean(),
}).strict();
const Cancel = z.object({ protocolVersion: z.literal(1), kind: z.literal('cancel'), context: Context }).strict();
type Config = z.infer<typeof Start>;
type Scope = z.infer<typeof Context>;
let config: Config | undefined;
let active = false;
const abort = new AbortController();
const pending = new Map<string, { digest: string; settle: (allowed: boolean) => void }>();
function emit(kind: string, data: Record<string, unknown> = {}) {
  process.stdout.write(JSON.stringify({ protocolVersion: 1, kind, context: config?.context, ...data }) + '\n');
}
function canonical(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(canonical);
  if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0).map(([key, val]) => [key, canonical(val)]));
  return value;
}
function sameScope(scope: Scope): boolean { return JSON.stringify(scope) === JSON.stringify(config?.context); }
async function approve(toolCallId: string, toolName: string, input: unknown, signal?: AbortSignal): Promise<boolean> {
  if (abort.signal.aborted || signal?.aborted) return false;
  const requestId = randomUUID();
  const binding = { context: config!.context, toolCallId, toolName, input, cwd: config!.cwd };
  const digest = createHash('sha256').update(JSON.stringify(canonical(binding))).digest('hex');
  return new Promise(resolve => {
    const settle = (allowed: boolean) => {
      if (!pending.delete(requestId)) return;
      clearTimeout(timer);
      abort.signal.removeEventListener('abort', cancelled);
      signal?.removeEventListener('abort', cancelled);
      emit('approvalSettled', { requestId, toolCallId, allowed });
      resolve(allowed && !abort.signal.aborted && !signal?.aborted);
    };
    const cancelled = () => settle(false);
    const timer = setTimeout(cancelled, 15000);
    pending.set(requestId, { digest, settle });
    abort.signal.addEventListener('abort', cancelled, { once: true });
    signal?.addEventListener('abort', cancelled, { once: true });
    emit('approvalRequested', { requestId, digest, ...binding });
  });
}
function cancel() { abort.abort(); for (const request of pending.values()) request.settle(false); }

async function runClaude(c: Config) {
  const { query } = await import('@anthropic-ai/claude-agent-sdk');
  const env: Record<string, string> = {};
  for (const key of ['PATH', 'HOME', 'USER', 'SHELL', 'TMPDIR']) if (process.env[key]) env[key] = process.env[key]!;
  Object.assign(env, {
    ANTHROPIC_API_KEY: 'loom-fixture-not-secret', ANTHROPIC_BASE_URL: c.endpoint,
    CLAUDE_CONFIG_DIR: c.runtimeDir, CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC: '1',
    DISABLE_TELEMETRY: '1', DISABLE_ERROR_REPORTING: '1',
  });
  const stream = query({ prompt: c.prompt, options: {
    cwd: c.cwd, env, model: 'claude-sonnet-4-6', maxTurns: 3,
    settingSources: [], systemPrompt: 'You are testing a local fixture. Use the provided Write tool once, then report its result.',
    tools: ['Write'], permissionMode: 'default', abortController: abort,
    includePartialMessages: true,
    canUseTool: async () => ({ behavior: 'deny', message: 'No approval bypass; the pre-tool hook must decide.' }),
    hooks: { PreToolUse: [{ hooks: [async (input, id, options) => {
      if (input.hook_event_name !== 'PreToolUse' || !id) return { continue: false, stopReason: 'Invalid hook identity' };
      const allowed = await approve(id, input.tool_name, input.tool_input, options.signal);
      return { hookSpecificOutput: { hookEventName: 'PreToolUse', permissionDecision: allowed ? 'allow' : 'deny', permissionDecisionReason: allowed ? 'Rust approved this exact call' : 'Rust denied or connection cancelled' } };
    }] }], PostToolUse: [{ hooks: [async (input, id) => {
      emit('toolResult', { toolCallId: id, result: input.hook_event_name === 'PostToolUse' ? input.tool_response : null });
      return {};
    }] }] },
    stderr: () => { /* SDK diagnostics can contain prompts; the probe records typed errors only. */ },
  } });
  try {
    for await (const event of stream) {
      if (event.type === 'stream_event') emit('stream', { event: event.event });
      if (event.type === 'result') {
        emit('providerResult', { subtype: event.subtype, isError: event.is_error, errors: 'errors' in event ? event.errors : undefined });
        if (event.is_error) throw new Error(`Claude result: ${event.subtype}`);
      }
    }
  } finally { stream.close(); }
}

async function runPi(c: Config) {
  const { createAgentSession, SessionManager, SettingsManager, ModelRuntime, DefaultResourceLoader, createWriteToolDefinition } = await import('@earendil-works/pi-coding-agent');
  const { InMemoryCredentialStore, InMemoryModelsStore } = await import('@earendil-works/pi-ai');
  const modelRuntime = await ModelRuntime.create({ credentials: new InMemoryCredentialStore(), modelsStore: new InMemoryModelsStore(), modelsPath: null, refreshOnCreate: false, allowModelNetwork: false });
  modelRuntime.registerProvider('loom-fixture', {
    api: 'openai-completions', apiKey: 'loom-fixture-not-secret', baseUrl: c.endpoint,
    models: [{ id: 'loom-fixture', name: 'Loom fixture', reasoning: false, input: ['text'], contextWindow: 32000, maxTokens: 1000, cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 }, compat: { supportsStore: false } }],
  });
  const model = modelRuntime.getModel('loom-fixture', 'loom-fixture');
  if (!model) throw new Error('Fixture model registration failed');
  const settingsManager = SettingsManager.inMemory({ retry: { enabled: false }, compaction: { enabled: false } });
  const resourceLoader = new DefaultResourceLoader({ cwd: c.cwd, agentDir: c.runtimeDir, settingsManager,
    noExtensions: true, noSkills: true, noPromptTemplates: true, noThemes: true, noContextFiles: true,
    systemPrompt: 'You are testing a local fixture. Use write once then report its result.',
  });
  await resourceLoader.reload();
  const write = createWriteToolDefinition(c.cwd);
  const gated: import('@earendil-works/pi-coding-agent').ToolDefinition = {
    name: write.name, label: write.label, description: write.description, parameters: write.parameters,
    execute: async (toolCallId, raw, signal, onUpdate, context) => {
    const params = z.object({ path: z.string(), content: z.string() }).strict().parse(raw);
    if (!await approve(toolCallId, write.name, params, signal)) throw new Error('Rust denied or connection cancelled');
    if (abort.signal.aborted || signal?.aborted) throw new Error('Cancelled');
    const result = await write.execute(toolCallId, params, signal, onUpdate, context);
    emit('toolResult', { toolCallId, result });
    return result;
  } };
  const { session } = await createAgentSession({ cwd: c.cwd, agentDir: c.runtimeDir, modelRuntime, model,
    thinkingLevel: 'off', tools: ['write'], customTools: [gated], settingsManager, resourceLoader,
    sessionManager: SessionManager.inMemory(c.cwd),
  });
  const stop = () => { void session.abort(); };
  abort.signal.addEventListener('abort', stop, { once: true });
  session.subscribe(event => {
    if (event.type === 'message_update') emit('stream', { event: event.assistantMessageEvent });
    if (event.type === 'tool_execution_end') emit('toolExecutionEnd', { toolCallId: event.toolCallId, isError: event.isError });
  });
  try { await session.prompt(c.prompt); }
  finally { abort.signal.removeEventListener('abort', stop); session.dispose(); }
}
async function start(value: unknown) {
  const c = Start.parse(value);
  if (config) throw new Error('A probe worker only owns one turn');
  const url = new URL(c.endpoint);
  if (url.protocol !== 'http:' || url.hostname !== '127.0.0.1' || !url.port || url.username || url.password) throw new Error('M0 probe accepts only explicit loopback HTTP endpoints');
  if (!isAbsolute(c.cwd) || !isAbsolute(c.runtimeDir)) throw new Error('Absolute fixture directories required');
  config = c; active = true;
  mkdirSync(c.runtimeDir, { recursive: true });
  emit('started', { provider: c.provider });
  try {
    await (c.provider === 'claude' ? runClaude(c) : runPi(c));
    emit('completed', { cancelled: abort.signal.aborted });
  } catch (error) {
    emit('error', { message: error instanceof Error ? error.message : String(error), cancelled: abort.signal.aborted });
    process.exitCode = abort.signal.aborted ? 0 : 1;
  } finally { active = false; cancel(); process.stdin.destroy(); }
}
function receive(line: string) {
  try {
    const value = JSON.parse(line);
    if (value.kind === 'start') { void start(value).catch(error => { emit('protocolError', { message: String(error) }); process.exitCode = 1; process.stdin.destroy(); }); return; }
    if (value.kind === 'cancel') { const command = Cancel.parse(value); if (!sameScope(command.context)) throw new Error('Scope mismatch'); cancel(); return; }
    const response = Decision.parse(value);
    const request = pending.get(response.requestId);
    if (!sameScope(response.context) || !request || request.digest !== response.digest) throw new Error('Stale, unknown or mismatched approval');
    request.settle(response.allowed);
  } catch (error) { emit('protocolError', { message: error instanceof Error ? error.message : String(error) }); }
}
let buffered = '';
process.stdin.setEncoding('utf8');
process.stdin.on('data', (chunk: string) => {
  buffered += chunk;
  if (Buffer.byteLength(buffered) > 1024 * 1024) { emit('protocolError', { message: 'Frame exceeds 1 MiB' }); cancel(); process.stdin.destroy(); return; }
  let newline;
  while ((newline = buffered.indexOf('\n')) >= 0) { const line = buffered.slice(0, newline); buffered = buffered.slice(newline + 1); if (line.trim()) receive(line); }
});
process.stdin.on('end', () => { if (active) cancel(); });
process.on('SIGTERM', cancel);
process.on('SIGINT', cancel);
