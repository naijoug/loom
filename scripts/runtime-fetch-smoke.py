#!/usr/bin/env python3
"""Real Pi SDK + formal worker + Rust/core with an in-process fake fetch.
No network permissions/listener; does not replace real HTTP integration tests.
"""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT=Path(__file__).resolve().parent.parent
results=[]
for scenario,auth in [('allow','key'),('deny','key'),('cancel','key'),('stale','key'),('allow','none'),('allow','literal'),('explore','key'),('execute','key'),('allowlist','key')]:
    anonymous=auth=='none'
    with tempfile.TemporaryDirectory(prefix='loom-runtime-fetch-') as directory:
        cwd=Path(directory).resolve()
        args=[str(ROOT/'target/debug/examples/sdk_runtime_probe'),shutil.which('bun'),str(ROOT/'packages/provider-workers/tests/pi-fetch-fixture.ts'),'pi','http://127.0.0.1:1',str(cwd),scenario]
        if auth!='key':args.append(auth)
        run=subprocess.run(args,capture_output=True,text=True,timeout=55)
        events=[json.loads(line) for line in run.stdout.splitlines() if line.strip()]
        writes=scenario in ('allow','stale','execute','allowlist')
        value=(cwd/'probe.txt').read_text() if (cwd/'probe.txt').exists() else None
        observations=json.loads((cwd/'fetch-observations.json').read_text()) if (cwd/'fetch-observations.json').exists() else []
        audits=[e for e in events if e.get('kind')=='coreAudit']
        checks=dict(exit=run.returncode==0,actualSdkTool=any(e.get('kind')=='approvalRequested' for e in events),sideEffect=value==('loom-sdk-probe\n' if writes else None),coreTerminal=len(audits)==1 and audits[0]['activeTurnId'] is None,requestObserved=bool(observations),noAuthHeaders=not anonymous or all(not item['authorizationPresent'] and not item['apiKeyPresent'] for item in observations))
        checks['credentialNotExecuted']=not (cwd/'credential-effect').exists()
        if scenario in ('explore','execute','allowlist'):
            checks['actualSdkTool']=any(e.get('kind')=='policyDecision' and e['allowed']==writes for e in events)
            checks['automaticPolicy']=not any(e.get('kind')=='approvalRequested' for e in events) and len(audits)==1 and audits[0]['approvalStatuses']==[]
        checks['frozenContext']=bool(observations) and all(item['model']=='loom-fixture' and item['historyUserPresent'] and item['historyAnswerPresent'] and not item['futureInputPresent'] for item in observations)
        checks['usagePersisted']=scenario=='cancel' or (len(audits)==1 and audits[0].get('usage') is not None)
        checks['credentialLiteral']=auth!='literal' or all(item['authorizationDigest']==hashlib.sha256(b'Bearer !printf should-not-execute > credential-effect').hexdigest() for item in observations)
        result=dict(scenario=scenario,auth=auth,anonymous=anonymous,passed=all(checks.values()),checks=checks,events=events,requests=observations,stderr=run.stderr[-4000:].replace(str(cwd),'$FIXTURE'))
        results.append(result);print(f'{scenario}/anonymous={anonymous}: {"PASS" if result["passed"] else "FAIL"}',flush=True)
        if not result['passed']:print(json.dumps(result,ensure_ascii=False,indent=2),flush=True)
paths=['packages/provider-workers/src/runtime-worker.ts','packages/provider-workers/tests/pi-fetch-fixture.ts','crates/loom-runtime/src/supervisor.rs','crates/loom-runtime/src/core_bridge.rs','crates/loom-core/src/repository/runtime_context.rs','crates/loom-core/src/repository/approvals.rs','crates/loom-runtime/src/protocol.rs','crates/loom-runtime/src/policy.rs','crates/loom-runtime/src/policy/config.rs','crates/loom-runtime/src/policy/paths.rs','crates/loom-runtime/src/policy/mcp.rs','crates/loom-runtime/examples/sdk_runtime_probe.rs','scripts/runtime-fetch-smoke.py']
output=ROOT/'.tmp/runtime-fetch-results.json';output.parent.mkdir(exist_ok=True)
output.write_text(json.dumps(dict(kind='real-pi-sdk-injected-fetch',realHttpVerified=False,sourceSha256={p:hashlib.sha256((ROOT/p).read_bytes()).hexdigest() for p in paths},results=results),ensure_ascii=False,indent=2)+'\n')
raise SystemExit(0 if all(item['passed'] for item in results) else 1)
