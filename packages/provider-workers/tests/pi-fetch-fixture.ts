/** Test-only transport injection. No listener or network request is created. */
import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import { join } from 'node:path';
import { createHash } from 'node:crypto';
const observations = join(process.cwd(), 'fetch-observations.json');
globalThis.fetch = async (input, init) => {
    const request = new Request(input, init);
    const url = new URL(request.url);
    if (url.origin !== 'http://127.0.0.1:1' || request.method !== 'POST')
        throw new Error('Unexpected network target in fetch fixture');
    const body = JSON.parse(await request.text()) as {
        messages: Array<{
            role: string;
        }>;
        model: string;
    };
    const log = existsSync(observations) ? JSON.parse(readFileSync(observations, 'utf8')) : [];
    const history = JSON.stringify(body.messages);
    log.push({ authorizationPresent: request.headers.has('authorization'), authorizationDigest: createHash('sha256').update(request.headers.get('authorization') ?? '').digest('hex'), apiKeyPresent: request.headers.has('x-api-key'), model: body.model, historyUserPresent: history.includes('history-user-marker'), historyAnswerPresent: history.includes('history-answer-marker'), futureInputPresent: history.includes('future-queued-marker') });
    writeFileSync(observations, JSON.stringify(log));
    const result = body.messages.some(message => message.role === 'tool');
    const chunk = (delta: unknown, finish: string | null = null) => 'data: ' + JSON.stringify({ id: 'runtime-fixture', object: 'chat.completion.chunk', created: 1, model: body.model, choices: [{ index: 0, delta, finish_reason: finish }] }) + '\n\n';
    let text = chunk({ role: 'assistant' });
    text += result ? chunk({ content: 'Fixture tool result received.' }) + chunk({}, 'stop') : chunk({ tool_calls: [{ index: 0, id: 'fixture-write', type: 'function', function: { name: 'write', arguments: JSON.stringify({ path: join(process.cwd(), 'probe.txt'), content: 'loom-sdk-probe\n' }) } }] }) + chunk({}, 'tool_calls');
    return new Response(text + 'data: [DONE]\n\n', { status: 200, headers: { 'content-type': 'text/event-stream' } });
};
await import('../src/runtime-worker.ts');
