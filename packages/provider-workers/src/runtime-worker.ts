/** One-turn SDK worker. Rust owns session state, policy, approvals and lifetime. */
import { createHash, randomUUID } from 'node:crypto';
import { isAbsolute } from 'node:path';
import { mkdirSync, realpathSync, statSync } from 'node:fs';
import { z } from 'zod';
import { createReadToolDefinition, createWriteToolDefinition, createEditToolDefinition, createBashToolDefinition, createGrepToolDefinition, createFindToolDefinition, createLsToolDefinition } from '@earendil-works/pi-coding-agent';
import type { ConnectionConfig, JsonValue } from '../../protocol/src/workbench.ts';
import { RUNTIME_PROTOCOL_VERSION } from '../../protocol/src/runtime.ts';
import type { RunConfig as RunConfigWire, WorkerEvent as WorkerEventWire, Start as StartWire, ToolDecision as ToolDecisionWire, Scope as ScopeWire } from '../../protocol/src/runtime.ts';
const Id = z.string().regex(/^[A-Za-z0-9_-]{1,160}$/);
const Scope: z.ZodType<ScopeWire> = z.object({ workspaceId: Id, sessionId: Id, turnId: Id, runId: Id }).strict();
const Connection: z.ZodType<ConnectionConfig> = z.object({ name: z.string(), provider: z.enum(['claude', 'pi', 'pi_compat']), auth: z.enum(['api_key', 'bearer_token', 'none']), endpoint: z.string().nullable(), piProvider: z.string().nullable(), customApi: z.enum(['anthropic-messages', 'openai-completions']).nullable(), models: z.array(z.string()), defaultModel: z.string(), supportsImages: z.boolean(), midStreamBehavior: z.enum(['queue', 'steer']) }).strict();
const Tool = z.enum(['read', 'write', 'edit', 'bash', 'glob', 'grep', 'ls', 'find']);
const ExternalTool = z.object({ name: z.string().regex(/^mcp__rea__[A-Za-z0-9_-]{1,160}$/), description: z.string().min(1).max(4096), inputSchema: z.object({ type: z.literal('object'), properties: z.record(z.string(), z.unknown()).optional(), required: z.array(z.string()).optional(), additionalProperties: z.boolean().optional(), '$defs': z.record(z.string(), z.unknown()).optional(), '$schema': z.string().optional() }).passthrough() }).strict();
const Config: z.ZodType<RunConfigWire> = z.object({ scope: Scope, connection: Connection, model: z.string().min(1), thinking: z.string().nullable(), cwd: z.string(), runtimeDirectory: z.string(), prompt: z.string().min(1), history: z.array(z.object({ role: z.enum(['user', 'assistant']), text: z.string() }).strict()), tools: z.array(Tool).max(32), externalTools: z.array(ExternalTool).max(8).transform(value => value as RunConfigWire['externalTools']), maxTurns: z.number().int().min(1).max(100) }).strict();
const Start: z.ZodType<StartWire> = z.object({ protocolVersion: z.literal(RUNTIME_PROTOCOL_VERSION), kind: z.literal('start'), config: Config, credential: z.string().min(1).max(65536).nullable() }).strict();
const Cancel = z.object({ protocolVersion: z.literal(RUNTIME_PROTOCOL_VERSION), kind: z.literal('cancel'), scope: Scope }).strict();
const ToolDecision: z.ZodType<ToolDecisionWire> = z.object({ requestId: Id, toolCallId: Id, allowed: z.boolean(), output: z.unknown().nullable().optional() }).strict().transform(value => value as ToolDecisionWire);
const Decision = z.object({ protocolVersion: z.literal(RUNTIME_PROTOCOL_VERSION), kind: z.literal('tool_decision'), scope: Scope, decision: ToolDecision }).strict();
type Run = z.infer<typeof Config>;
let failure: {
    code: string;
    message: string;
} | undefined;
let config: Run | undefined;
let credential: string | null = null;
let seq = 0;
let terminal = false;
let started = false;
const abort = new AbortController();
const pending = new Map<string, {
    toolCallId: string;
    settle: (value: boolean | unknown) => void;
}>();
const seen = new Set<string>();
const allowed = new Set<string>();
const MAX_FRAME = 1024 * 1024;
function clean(text: string): string { if (!credential)
    return text; for (const secret of [credential, JSON.stringify(credential).slice(1, -1), encodeURIComponent(credential)])
    if (secret)
        text = text.replaceAll(secret, '[credential redacted]'); return text; }
function safeValue(value: unknown): unknown {
    if (typeof value === 'string')
        return clean(value);
    if (Array.isArray(value))
        return value.map(safeValue);
    if (value && typeof value === 'object')
        return Object.fromEntries(Object.entries(value).map(([key, value]) => [key, safeValue(value)]));
    return value;
}
function emit(event: WorkerEventWire) {
    if (!config || terminal)
        return;
    const frame = JSON.stringify({ protocolVersion: RUNTIME_PROTOCOL_VERSION, scope: config.scope, seq: ++seq, event }) + '\n';
    if (Buffer.byteLength(frame) > MAX_FRAME || process.stdout.writableLength + Buffer.byteLength(frame) > 4 * MAX_FRAME) {
        cancel();
        process.exit(1);
    }
    process.stdout.write(frame);
}
function sameScope(scope: z.infer<typeof Scope>) { const current = config?.scope; return !!current && scope.workspaceId === current.workspaceId && scope.sessionId === current.sessionId && scope.turnId === current.turnId && scope.runId === current.runId; }
function cancel() { abort.abort(); for (const request of pending.values())
    request.settle(false); }
function identity(nativeId: string) { return `tool-${createHash('sha256').update(config!.scope.turnId + '\0' + nativeId).digest('hex')}`; }
async function authorizeValue(nativeId: string, name: string, input: unknown, signal?: AbortSignal): Promise<boolean | unknown> {
    if (abort.signal.aborted || signal?.aborted || !config!.externalTools.some(tool => tool.name === name) && !config!.tools.includes(name as z.infer<typeof Tool>))
        return false;
    const toolCallId = identity(nativeId);
    if (seen.has(toolCallId))
        throw new Error('duplicate SDK tool call');
    seen.add(toolCallId);
    const snapshot = structuredClone(input);
    const before = JSON.stringify(snapshot);
    const result = await new Promise<boolean | unknown>(resolve => {
        const requestId = randomUUID();
        const stop = () => settle(false);
        const settle = (value: boolean | unknown) => { if (!pending.delete(requestId))
            return; clearTimeout(timer); abort.signal.removeEventListener('abort', stop); signal?.removeEventListener('abort', stop); resolve(value === true || value === false ? value && !abort.signal.aborted && !signal?.aborted : value); };
        const timer = setTimeout(stop, 300000);
        pending.set(requestId, { toolCallId, settle });
        abort.signal.addEventListener('abort', stop, { once: true });
        signal?.addEventListener('abort', stop, { once: true });
        emit({ kind: 'tool_request', requestId, toolCallId, name, input: snapshot as JsonValue });
    });
    if (before !== JSON.stringify(input))
        throw new Error('SDK tool arguments changed during approval');
    return result;
}
async function authorize(nativeId: string, name: string, input: unknown, signal?: AbortSignal): Promise<boolean> {
    const result = await authorizeValue(nativeId, name, input, signal);
    if (result !== true) {
        return false;
    }
    const toolCallId = identity(nativeId);
    allowed.add(toolCallId);
    return true;
}
async function authorizeResult(nativeId: string, name: string, input: unknown, signal?: AbortSignal): Promise<unknown | false> {
    const result = await authorizeValue(nativeId, name, input, signal);
    return result === false || result === true ? (result === true ? {} : false) : result;
}
function toolResult(nativeId: string, failed: boolean, output: unknown) {
    const toolCallId = identity(nativeId);
    if (!allowed.delete(toolCallId))
        return;
    const value = safeValue(output ?? null);
    const encoded = JSON.stringify(value);
    // Full SDK output remains in the SDK's model context. Large UI artifacts are
    // explicitly summarized until the content repository handles their storage.
    emit({ kind: 'tool_result', toolCallId, failed, output: Buffer.byteLength(encoded) > 512 * 1024 ? { summary: 'Tool output exceeds the inline display budget', inlineBytes: Buffer.byteLength(encoded) } : value as JsonValue });
}
function prompt(c: Run) { return c.history.length ? `Previous conversation (historical context):\n${JSON.stringify(c.history)}\n\nCurrent user request:\n${c.prompt}` : c.prompt; }

const externalFactories = new Map<string, () => import('@earendil-works/pi-coding-agent').ToolDefinition>();
function globalThisFactories(c: Run): Record<string, () => import('@earendil-works/pi-coding-agent').ToolDefinition> {
    const values: Record<string, () => import('@earendil-works/pi-coding-agent').ToolDefinition> = {
        read: () => runtimeTool(createReadToolDefinition(c.cwd)),
        write: () => runtimeTool(createWriteToolDefinition(c.cwd)),
        edit: () => runtimeTool(createEditToolDefinition(c.cwd)),
        bash: () => runtimeTool(createBashToolDefinition(c.cwd)),
        grep: () => runtimeTool(createGrepToolDefinition(c.cwd)),
        ls: () => runtimeTool(createLsToolDefinition(c.cwd)),
    };
    values.find = () => runtimeTool(createFindToolDefinition(c.cwd));
    for (const tool of c.externalTools) {
        if (!externalFactories.has(tool.name))
            externalFactories.set(tool.name, () => externalTool(tool as z.infer<typeof ExternalTool>));
        values[tool.name] = externalFactories.get(tool.name)!;
    }
    return values;
}
function externalTool(tool: z.infer<typeof ExternalTool>): import('@earendil-works/pi-coding-agent').ToolDefinition {
    return {
        name: tool.name,
        label: tool.name,
        description: tool.description,
        parameters: tool.inputSchema as never,
        async execute(nativeId, params, signal) {
            const result = await authorizeResult(nativeId, tool.name, params, signal);
            if (result === false)
                throw new Error('Host denied or cancelled this call');
            const value = structuredClone(result);
            if (!value || typeof value !== 'object' || Array.isArray(value))
                throw new Error('host bridge returned an invalid external tool result');
            allowed.add(identity(nativeId));
            return value as never;
        },
    };
}
function usage(input: number, output: number) { if (Number.isSafeInteger(input) && input >= 0 && Number.isSafeInteger(output) && output >= 0)
    emit({ kind: 'usage', inputTokens: input, outputTokens: output }); }
async function claude(c: Run) {
    const factories = globalThisFactories(c);
    const { query } = await import('@anthropic-ai/claude-agent-sdk');
    const names: Record<string, string> = { read: 'Read', write: 'Write', edit: 'Edit', bash: 'Bash', glob: 'Glob', grep: 'Grep' };
    if (c.tools.some(tool => !names[tool]))
        throw new Error('tool unavailable for Claude adapter');
    const reverse = { ...Object.fromEntries(Object.entries(names).map(([key, value]) => [value, key])), ...Object.fromEntries(c.externalTools.map(tool => [tool.name, tool.name])) };
    const env: Record<string, string> = {};
    for (const key of ['PATH', 'HOME', 'USER', 'SHELL', 'TMPDIR', 'LANG'])
        if (process.env[key])
            env[key] = process.env[key]!;
    Object.assign(env, { CLAUDE_CONFIG_DIR: c.runtimeDirectory, CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC: '1', DISABLE_TELEMETRY: '1', DISABLE_ERROR_REPORTING: '1' });
    if (c.connection.endpoint)
        env.ANTHROPIC_BASE_URL = c.connection.endpoint;
    if (c.connection.auth === 'api_key')
        env.ANTHROPIC_API_KEY = credential!;
    else if (c.connection.auth === 'bearer_token')
        env.ANTHROPIC_AUTH_TOKEN = credential!;
    else
        throw new Error('Claude authentication unavailable');
    const directory = realpathSync(c.cwd);
    const identityAtStart = statSync(directory);
    let messageIndex = 0;
    const effort = c.thinking && c.thinking !== 'off' ? z.enum(['low', 'medium', 'high', 'xhigh', 'max']).parse(c.thinking) : undefined;
    const stream = query({ prompt: prompt(c), options: { cwd: c.cwd, env, model: c.model, settingSources: [], persistSession: false, tools: [...c.tools.map(tool => names[tool]), ...c.externalTools.map(tool => tool.name)], maxTurns: c.maxTurns, abortController: abort, includePartialMessages: true, permissionMode: 'default', thinking: c.thinking === 'off' ? { type: 'disabled' } : undefined, effort,
            systemPrompt: 'You are Loom, a local assistant. Follow the user request. Tool execution is controlled by the host; a denied tool call must not be retried with a bypass.',
            canUseTool: async () => ({ behavior: 'deny', message: 'The Rust tool gate must approve this invocation.' }),
            hooks: { PreToolUse: [{ hooks: [async (input, id, options) => {
                                if (input.hook_event_name !== 'PreToolUse' || !id)
                                    return { continue: false, stopReason: 'invalid tool identity' };
                                const actual = realpathSync(input.cwd);
                                const metadata = statSync(actual);
                                if (actual !== directory || metadata.dev !== identityAtStart.dev || metadata.ino !== identityAtStart.ino) {
                                    failure = { code: 'working_directory_changed', message: 'SDK working directory changed; a new execution binding is required' };
                                    cancel();
                                    return { continue: false, stopReason: failure.message };
                                }
                                const name = reverse[input.tool_name];
                                const permit = !!name && await authorize(id, name, input.tool_input, options.signal);
                                return { hookSpecificOutput: { hookEventName: 'PreToolUse', permissionDecision: permit ? 'allow' : 'deny', permissionDecisionReason: permit ? 'Rust authorized this exact call' : 'Host denied or cancelled this call' } };
                            }] }], PostToolUse: [{ hooks: [async (input, id) => { if (input.hook_event_name === 'PostToolUse' && id)
                                toolResult(id, false, input.tool_response); return {}; }] }], PostToolUseFailure: [{ hooks: [async (input, id) => { if (input.hook_event_name === 'PostToolUseFailure' && id)
                                toolResult(id, true, { error: clean(input.error) }); return {}; }] }] }, stderr: () => { },
        } });
    try {
        for await (const message of stream) {
            if (message.type === 'stream_event') {
                const event = message.event;
                if (event.type === 'message_start')
                    messageIndex++;
                if (event.type === 'content_block_delta') {
                    if (event.delta.type === 'text_delta')
                        emit({ kind: 'text', partId: `text-${messageIndex}-${event.index}`, textKind: 'text', delta: event.delta.text });
                    else if (event.delta.type === 'thinking_delta')
                        emit({ kind: 'text', partId: `thinking-${messageIndex}-${event.index}`, textKind: 'thinking', delta: event.delta.thinking });
                }
            }
            if (message.type === 'result') {
                usage(message.usage.input_tokens, message.usage.output_tokens);
                if (message.is_error && !abort.signal.aborted)
                    throw new Error(`Claude request failed: ${message.subtype}`);
            }
        }
    }
    finally {
        stream.close();
    }
}
function runtimeTool(tool: {
    name: string;
    label: string;
    description: string;
    parameters: import('@earendil-works/pi-coding-agent').ToolDefinition['parameters'];
    execute: unknown;
}): import('@earendil-works/pi-coding-agent').ToolDefinition {
    // Preserve the SDK's argument schema; omit UI renderer callbacks whose generic
    // parameter types are irrelevant to the headless worker.
    return { name: tool.name, label: tool.label, description: tool.description, parameters: tool.parameters, execute: tool.execute as import('@earendil-works/pi-coding-agent').ToolDefinition['execute'] };
}
async function pi(c: Run) {
    const sdk = await import('@earendil-works/pi-coding-agent');
    const ai = await import('@earendil-works/pi-ai');
    const runtime = await sdk.ModelRuntime.create({ credentials: new ai.InMemoryCredentialStore(), modelsStore: new ai.InMemoryModelsStore(), modelsPath: null, refreshOnCreate: false, allowModelNetwork: false });
    const provider = c.connection.provider === 'pi' ? (c.connection.piProvider ?? '') : 'loom-custom';
    if (c.connection.provider === 'pi_compat') {
        if (!c.connection.endpoint || !c.connection.customApi)
            throw new Error('custom endpoint configuration missing');
        if (c.connection.auth === 'bearer_token' && c.connection.customApi !== 'openai-completions')
            throw new Error('bearer auth is not implemented for this Pi protocol');
        runtime.registerProvider(provider, { api: c.connection.customApi, baseUrl: c.connection.endpoint, authHeader: c.connection.auth !== 'none', models: [{ id: c.model, name: c.model, reasoning: false, input: c.connection.supportsImages ? ['text', 'image'] : ['text'], contextWindow: 128000, maxTokens: 8192, cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 }, compat: { supportsStore: false } }] });
    }
    else {
        if (c.connection.auth === 'none')
            throw new Error('native Pi provider requires explicit credentials; ambient cloud authentication is disabled');
        const existing = runtime.getModel(provider, c.model);
        if (!existing || !['anthropic-messages', 'openai-completions', 'openai-responses', 'google-generative-ai', 'mistral-conversations'].includes(existing.api))
            throw new Error('native provider/model or authentication family is not available');
        if (c.connection.auth === 'bearer_token' && !['openai-completions', 'openai-responses'].includes(existing.api))
            throw new Error('bearer auth is not implemented for this native Pi provider');
        if (c.connection.endpoint)
            runtime.registerProvider(provider, { baseUrl: c.connection.endpoint });
    }
    // RuntimeCredentials stores the value literally. Provider config apiKey fields
    // are expressions and must never receive an untrusted credential string.
    await runtime.setRuntimeApiKey(provider, credential ?? 'no-auth');
    const model = runtime.getModel(provider, c.model);
    if (!model)
        throw new Error('configured model unavailable');
    if (c.thinking && c.thinking !== 'off' && !model.reasoning)
        throw new Error('configured model does not declare reasoning support');
    const factories = globalThisFactories(c);
    if (c.tools.some(tool => !factories[tool]))
        throw new Error('tool unavailable for Pi adapter');
    const tools = [...c.tools, ...c.externalTools.map(tool => tool.name)].map(name => {
        const tool = factories[name]();
        return { ...tool, execute: async (id, params, signal, onUpdate, context) => {
                if (!await authorize(id, name, params, signal))
                    throw new Error('Host denied or cancelled this call');
                if (abort.signal.aborted || signal?.aborted)
                    throw new Error('cancelled');
                try {
                    const result = await tool.execute(id, params, signal, onUpdate, context);
                    toolResult(id, false, result);
                    return result;
                }
                catch (error) {
                    toolResult(id, true, { error: clean(error instanceof Error ? error.message : 'tool failed') });
                    throw error;
                }
            } } satisfies import('@earendil-works/pi-coding-agent').ToolDefinition;
    });
    const settings = sdk.SettingsManager.inMemory({ retry: { enabled: false }, compaction: { enabled: false } });
    const loader = new sdk.DefaultResourceLoader({ cwd: c.cwd, agentDir: c.runtimeDirectory, settingsManager: settings, noExtensions: true, noSkills: true, noPromptTemplates: true, noThemes: true, noContextFiles: true, systemPrompt: 'You are Loom, a local assistant. Follow the user request. The host controls every tool invocation; do not bypass a denied call.' });
    await loader.reload();
    const levels = ['off', 'minimal', 'low', 'medium', 'high', 'xhigh'];
    if (c.thinking && !levels.includes(c.thinking))
        throw new Error('unsupported thinking level');
    const { session } = await sdk.createAgentSession({ cwd: c.cwd, agentDir: c.runtimeDirectory, modelRuntime: runtime, model, thinkingLevel: (c.thinking ?? 'off') as 'off', tools: c.tools, customTools: tools, settingsManager: settings, resourceLoader: loader, sessionManager: sdk.SessionManager.inMemory(c.cwd) });
    const stop = () => { void session.abort(); };
    abort.signal.addEventListener('abort', stop, { once: true });
    let messageIndex = 0, inputTokens = 0, outputTokens = 0, turns = 0;
    let failed: string | undefined;
    session.subscribe(event => {
        if (event.type === 'message_start' && event.message.role === 'assistant') {
            messageIndex++;
            if (++turns > c.maxTurns) {
                failed = 'Pi turn limit exceeded';
                cancel();
            }
        }
        if (event.type === 'message_update') {
            const delta = event.assistantMessageEvent;
            if (delta.type === 'text_delta' || delta.type === 'thinking_delta')
                emit({ kind: 'text', partId: `${delta.type}-${messageIndex}-${delta.contentIndex}`, textKind: delta.type === 'text_delta' ? 'text' : 'thinking', delta: delta.delta });
        }
        if (event.type === 'message_end' && event.message.role === 'assistant') {
            inputTokens += event.message.usage.input;
            outputTokens += event.message.usage.output;
            usage(inputTokens, outputTokens);
            if (event.message.stopReason === 'error')
                failed = clean(event.message.errorMessage ?? 'Pi request failed');
        }
    });
    try {
        await session.prompt(prompt(c));
        if (failed)
            throw new Error(failed);
    }
    finally {
        abort.signal.removeEventListener('abort', stop);
        session.dispose();
    }
}
async function start(value: unknown) {
    if (started)
        throw new Error('worker accepts one run');
    started = true;
    const input = Start.parse(value);
    config = input.config;
    credential = input.credential;
    if (!isAbsolute(config.cwd) || !isAbsolute(config.runtimeDirectory))
        throw new Error('absolute directories required');
    if ((config.connection.auth === 'none') !== (credential === null) || !config.connection.models.includes(config.model))
        throw new Error('model or credential mismatch');
    if (config.connection.auth === 'none') {
        const fetch = globalThis.fetch.bind(globalThis);
        globalThis.fetch = async (input, init) => { const request = new Request(input, init); const headers = new Headers(request.headers); headers.delete('authorization'); headers.delete('x-api-key'); return fetch(new Request(request, { headers })); };
    }
    mkdirSync(config.runtimeDirectory, { recursive: true, mode: 0o700 });
    emit({ kind: 'ready' });
    try {
        await (config.connection.provider === 'claude' ? claude(config) : pi(config));
        if (failure)
            throw new Error(failure.message);
        emit({ kind: 'completed', cancelled: abort.signal.aborted });
    }
    catch (error) {
        emit({ kind: 'failed', code: failure?.code ?? (abort.signal.aborted ? 'cancelled' : 'provider_error'), message: clean(failure?.message ?? (error instanceof Error ? error.message : 'SDK request failed')).slice(0, 4096) });
        process.exitCode = abort.signal.aborted ? 0 : 1;
    }
    finally {
        terminal = true;
        cancel();
        credential = null;
        process.stdin.destroy();
    }
}
function receive(line: string) {
    try {
        const input = JSON.parse(line);
        if (input.kind === 'start') {
            void start(input).catch(() => { cancel(); process.exit(1); });
            return;
        }
        if (input.kind === 'cancel') {
            const value = Cancel.parse(input);
            if (!sameScope(value.scope))
                throw new Error('scope mismatch');
            cancel();
            return;
        }
        const value = Decision.parse(input);
        const request = pending.get(value.decision.requestId);
        if (!sameScope(value.scope) || !request || request.toolCallId !== value.decision.toolCallId)
            throw new Error('stale tool decision');
        request.settle(value.decision.allowed ? (value.decision.output ?? true) : false);
    }
    catch {
        cancel();
        process.exit(1);
    }
}
let buffered = '';
process.stdin.setEncoding('utf8');
process.stdin.on('data', (chunk: string) => { buffered += chunk; if (Buffer.byteLength(buffered) > MAX_FRAME) {
    cancel();
    process.exit(1);
} let end; while ((end = buffered.indexOf('\n')) >= 0) {
    const line = buffered.slice(0, end);
    buffered = buffered.slice(end + 1);
    receive(line);
} });
process.stdin.on('end', cancel);
process.on('SIGTERM', cancel);
process.on('SIGINT', cancel);
