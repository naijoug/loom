#!/usr/bin/env python3
"""Real SDK + Rust broker integration against deterministic loopback model APIs.
No model account, user credentials, or external model request is used.
"""
import argparse
import os
import queue
import signal
from datetime import datetime
import hashlib
import platform
from zoneinfo import ZoneInfo
import http.server
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import threading
import time

ROOT = Path(__file__).resolve().parent.parent


def create_fixture_server(provider, cwd, port=0):
    requests = []

    class Handler(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def reply(self, body, stream=False):
            data = body.encode() if isinstance(body, str) else json.dumps(body).encode()
            self.send_response(200)
            self.send_header('Content-Type', 'text/event-stream' if stream else 'application/json')
            self.send_header('Content-Length', str(len(data)))
            self.end_headers()
            self.wfile.write(data)

        def do_POST(self):
            size = int(self.headers.get('Content-Length', '0'))
            if size > 8 * 1024 * 1024:
                self.send_error(413)
                return
            body = json.loads(self.rfile.read(size))
            requests.append({'path': self.path, 'model': body.get('model'), 'stream': body.get('stream')})
            if 'count_tokens' in self.path:
                self.reply({'input_tokens': 20})
                return
            messages = body.get('messages', [])
            if provider == 'claude':
                has_result = any(isinstance(m.get('content'), list) and any(c.get('type') == 'tool_result' for c in m['content']) for m in messages)
                block = {'type': 'text', 'text': 'Fixture tool result received.'} if has_result else {'type': 'tool_use', 'id': 'loom_probe_tool_1', 'name': 'Write', 'input': {'file_path': str(cwd / 'probe.txt'), 'content': 'loom-sdk-probe\n'}}
                stop_reason = 'end_turn' if has_result else 'tool_use'
                base = {'id': 'msg_loom_fixture', 'type': 'message', 'role': 'assistant', 'model': body.get('model'), 'content': [], 'stop_reason': None, 'stop_sequence': None, 'usage': {'input_tokens': 20, 'output_tokens': 1}}
                if not body.get('stream'):
                    self.reply({**base, 'content': [block], 'stop_reason': stop_reason})
                    return
                def event(kind, **data):
                    return f'event: {kind}\ndata: {json.dumps({"type": kind, **data})}\n\n'
                output = event('message_start', message=base)
                output += event('content_block_start', index=0, content_block={'type': 'text', 'text': ''} if has_result else {**block, 'input': {}})
                output += event('content_block_delta', index=0, delta={'type': 'text_delta', 'text': block['text']} if has_result else {'type': 'input_json_delta', 'partial_json': json.dumps(block['input'])})
                output += event('content_block_stop', index=0)
                output += event('message_delta', delta={'stop_reason': stop_reason, 'stop_sequence': None}, usage={'output_tokens': 20})
                output += event('message_stop')
                self.reply(output, True)
            else:
                has_result = any(m.get('role') == 'tool' for m in messages)
                base = {'id': 'chatcmpl_loom_fixture', 'object': 'chat.completion.chunk', 'created': int(time.time()), 'model': 'loom-fixture'}
                def chunk(delta, finish=None):
                    return 'data: ' + json.dumps({**base, 'choices': [{'index': 0, 'delta': delta, 'finish_reason': finish}]}) + '\n\n'
                output = chunk({'role': 'assistant'})
                if has_result:
                    output += chunk({'content': 'Fixture tool result received.'}) + chunk({}, 'stop')
                else:
                    output += chunk({'tool_calls': [{'index': 0, 'id': 'loom_probe_tool_1', 'type': 'function', 'function': {'name': 'write', 'arguments': json.dumps({'path': str(cwd / 'probe.txt'), 'content': 'loom-sdk-probe\n'})}}]})
                    output += chunk({}, 'tool_calls')
                self.reply(output + 'data: [DONE]\n\n', True)

    return http.server.ThreadingHTTPServer(('127.0.0.1', port), Handler), requests


def run_case(provider, decision, core=False, runtime=False):
    with tempfile.TemporaryDirectory(prefix='loom-sdk-probe-') as directory:
        cwd = Path(directory).resolve()
        server, requests = create_fixture_server(provider, cwd)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        url = f'http://127.0.0.1:{server.server_port}' + ('/v1' if provider == 'pi' else '')
        binary = ROOT / ('target/debug/examples/sdk_runtime_probe' if runtime else 'target/debug/examples/sdk_core_approval_probe' if core else 'experiments/runtime-bridge/target/debug/loom-runtime-bridge-probe')
        command = [str(binary), shutil.which('bun'), str(ROOT / ('packages/provider-workers/src/runtime-worker.ts' if runtime else 'packages/provider-workers/src/worker.ts')), provider, url, str(cwd), decision]
        try:
            result = worker_eof_case(provider, url, cwd) if runtime and decision == 'disconnect' else subprocess.run(command, capture_output=True, text=True, timeout=55)
            events = [json.loads(line) for line in result.stdout.splitlines() if line.strip()]
            expected_write = decision in ('allow', 'stale')
            written = (cwd / 'probe.txt').read_text() if (cwd / 'probe.txt').exists() else None
            checks = {
                'worker_exit_ok': result.returncode == 0,
                'real_sdk_requested_tool': any(e.get('kind') in ('approvalRequested', 'tool_request') for e in events),
                'observed_model_api': len(requests) > 0,
                'exact_side_effect': written == ('loom-sdk-probe\n' if expected_write else None),
                'rust_decision_observed': any(e.get('kind') == 'approvalSettled' and e.get('allowed') == expected_write for e in events),
                'stale_response_rejected': decision != 'stale' or any(e.get('kind') == 'protocolError' for e in events),
            }
            if runtime and decision == 'disconnect':
                checks.pop('rust_decision_observed')
                checks['worker_observed_eof'] = any(e.get('kind') == 'completed' and e.get('cancelled') or e.get('kind') == 'failed' and e.get('code') == 'cancelled' for e in events)
            if core or runtime and decision != 'disconnect':
                audits = [event for event in events if event.get('kind') == 'coreAudit']
                expected_status = 'consumed' if expected_write else 'denied' if decision == 'deny' else 'cancelled'
                checks['durable_core_approval'] = len(audits) == 1 and audits[0]['approvalStatuses'] == [expected_status]
                checks['core_turn_settled'] = len(audits) == 1 and audits[0]['activeTurnId'] is None and audits[0]['turnStatus'] in ['completed', 'cancelled', 'interrupted']
            return {'provider': provider, 'scenario': decision, 'transport': 'direct-worker-eof' if runtime and decision == 'disconnect' else 'supervised-runtime' if runtime else 'probe', 'passed': all(checks.values()), 'checks': checks, 'requests': requests, 'events': json.loads(json.dumps(events).replace(str(cwd), '$FIXTURE')), 'stderr': result.stderr.replace(str(cwd), '$FIXTURE')[-6000:]}
        except subprocess.TimeoutExpired:
            return {'provider': provider, 'scenario': decision, 'passed': False, 'error': 'Integration timeout'}
        finally:
            server.shutdown()
            server.server_close()



def worker_eof_case(provider, endpoint, cwd):
    """Own the real worker directly to exercise stdin EOF while its SDK awaits a decision."""
    model = 'claude-sonnet-4-6' if provider == 'claude' else 'loom-fixture'
    config = dict(scope=dict(workspaceId='w', sessionId='s', turnId='t', runId='eof-run'),
                  connection=dict(name='EOF fixture', provider='claude' if provider == 'claude' else 'pi_compat', auth='api_key', endpoint=endpoint, piProvider=None, customApi=None if provider == 'claude' else 'openai-completions', models=[model], defaultModel=model, supportsImages=False, midStreamBehavior='queue'),
                  model=model, thinking=None, cwd=str(cwd), runtimeDirectory=str(cwd / 'sdk-state'),
                  prompt='Write exactly loom-sdk-probe followed by a newline to probe.txt using the write tool.', history=[], tools=['write'], maxTurns=3)
    command = [shutil.which('bun'), str(ROOT / 'packages/provider-workers/src/runtime-worker.ts')]
    env = {key: os.environ[key] for key in ['PATH', 'HOME', 'USER', 'SHELL', 'TMPDIR', 'LANG'] if key in os.environ}
    process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, cwd=cwd, env=env, start_new_session=True)
    messages = queue.Queue()
    def read():
        for line in process.stdout:
            messages.put(line)
        messages.put(None)
    reader = threading.Thread(target=read, daemon=True)
    reader.start()
    events = []
    try:
        process.stdin.write(json.dumps(dict(protocolVersion=1, kind='start', config=config, credential='loom-fixture-not-secret')) + '\n')
        process.stdin.flush()
        deadline = time.monotonic() + 40
        while True:
            line = messages.get(timeout=max(.1, deadline-time.monotonic()))
            if line is None:
                raise RuntimeError('worker ended before a real tool request')
            event = json.loads(line)['event']
            events.append(event)
            if event['kind'] == 'tool_request':
                break
        if (cwd / 'probe.txt').exists():
            raise RuntimeError('side effect before host decision')
        process.stdin.close()
        code = process.wait(timeout=10)
        reader.join(timeout=1)
        while not messages.empty():
            line = messages.get_nowait()
            if line is not None:
                events.append(json.loads(line)['event'])
        return subprocess.CompletedProcess(command, code, '\n'.join(json.dumps(event) for event in events), '')
    finally:
        if process.poll() is None:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait(timeout=5)
        if process.stdin and not process.stdin.closed:
            process.stdin.close()
        process.stdout.close()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--provider', choices=['claude', 'pi', 'all'], default='all')
    parser.add_argument('--scenario', choices=['allow', 'deny', 'cancel', 'stale', 'disconnect', 'all'], default='all')
    parser.add_argument('--output', default='.tmp/runtime-bridge-results.json')
    parser.add_argument('--core', action='store_true', help='Run the production loom-core approval integration harness')
    parser.add_argument('--runtime', action='store_true', help='Run the formal worker and process supervisor')
    parser.add_argument('--serve-fixture', action='store_true', help='Serve a loopback-only model fixture for manual native SDK acceptance')
    parser.add_argument('--fixture-directory', help='Existing disposable directory containing the single probe.txt target')
    parser.add_argument('--port', type=int, default=18765)
    args = parser.parse_args()
    if args.serve_fixture:
        if args.provider == 'all' or not args.fixture_directory:
            parser.error('--serve-fixture requires one --provider and --fixture-directory')
        cwd = Path(args.fixture_directory).resolve(strict=True)
        if not cwd.is_dir():
            parser.error('fixture directory must exist')
        server, requests = create_fixture_server(args.provider, cwd, args.port)
        print(f'Loopback fixture ready: http://127.0.0.1:{server.server_port}', flush=True)
        try:
            server.serve_forever()
        except KeyboardInterrupt:
            pass
        finally:
            server.server_close()
            output = ROOT / args.output
            output.parent.mkdir(parents=True, exist_ok=True)
            output.write_text(json.dumps(dict(kind='native-loopback-model-fixture', provider=args.provider, liveModelVerified=False, requests=requests), indent=2) + '\n')
        return 0
    if args.core and args.runtime:
        parser.error('choose --core or --runtime')
    providers = ['claude', 'pi'] if args.provider == 'all' else [args.provider]
    scenarios = ['allow', 'deny', 'cancel', 'stale', 'disconnect'] if args.scenario == 'all' else [args.scenario]
    results = []
    for provider in providers:
        for decision in scenarios:
            result = run_case(provider, decision, args.core, args.runtime)
            results.append(result)
            print(f'{provider}/{decision}: {"PASS" if result["passed"] else "FAIL"}', flush=True)
            if not result['passed']:
                print(json.dumps(result, ensure_ascii=False, indent=2), flush=True)
    output = ROOT / args.output
    output.parent.mkdir(parents=True, exist_ok=True)
    sources = ['packages/provider-workers/src/worker.ts', 'packages/provider-workers/bun.lock', 'experiments/runtime-bridge/src/main.rs', 'experiments/runtime-bridge/Cargo.lock', 'scripts/runtime-bridge-smoke.py']
    if args.core:
        sources += ['crates/loom-core/examples/sdk_core_approval_probe.rs', 'crates/loom-core/src/approval.rs', 'crates/loom-core/src/repository/approvals.rs', 'crates/loom-core/src/projection.rs']
    if args.runtime:
        sources = ['packages/provider-workers/src/runtime-worker.ts', 'packages/provider-workers/bun.lock', 'scripts/runtime-bridge-smoke.py', 'Cargo.lock']
        sources += [str(path.relative_to(ROOT)) for base in ['crates/loom-runtime/src', 'crates/loom-core/src'] for path in sorted((ROOT / base).rglob('*.rs'))]
        sources += ['crates/loom-runtime/examples/sdk_runtime_probe.rs']
    metadata = {
        'supervisedRuntimeIntegration': args.runtime,
        'coreApprovalIntegration': args.core or args.runtime,
        'evidenceKind': 'real-sdk-local-model-fixture',
        'liveModelVerified': False,
        'recordedAt': datetime.now(ZoneInfo('Asia/Shanghai')).isoformat(),
        'platform': {'system': platform.system(), 'release': platform.release(), 'arch': platform.machine()},
        'scope': 'Single real SDK Write/write tool, local model fixture, Rust decision binding; no broad parity claim.',
        'sourceSha256': {path: hashlib.sha256((ROOT / path).read_bytes()).hexdigest() for path in sources},
        'sdkVersions': {name: json.loads((ROOT / 'packages/provider-workers/node_modules' / name / 'package.json').read_text())['version'] for name in ['@anthropic-ai/claude-agent-sdk', '@earendil-works/pi-coding-agent']},
        'results': results,
    }
    output.write_text(json.dumps(metadata, ensure_ascii=False, indent=2) + '\n')
    return 0 if all(r['passed'] for r in results) else 1


if __name__ == '__main__':
    raise SystemExit(main())
