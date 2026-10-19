#![cfg(unix)]
use loom_core::{
    ConnectionAuth, ConnectionConfig, ConnectionProvider, EndpointApi, Id, MidStreamBehavior,
};
use loom_runtime::*;
use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
static NEXT: AtomicU64 = AtomicU64::new(1);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "loom-worker-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        fs::write(p.join("worker.py"), PYTHON).unwrap();
        Self(p.canonicalize().unwrap())
    }
    fn command(&self) -> WorkerCommand {
        WorkerCommand {
            executable: PathBuf::from("/usr/bin/python3"),
            script: self.0.join("worker.py"),
            timeout: Duration::from_secs(3),
            exit_grace: Duration::from_millis(150),
        }
    }
    fn config(&self, scenario: &str) -> RunConfig {
        RunConfig {
            scope: Scope {
                workspace_id: id("w"),
                session_id: id("s"),
                turn_id: id("t"),
                run_id: id("r"),
            },
            connection: ConnectionConfig {
                name: "Fixture".into(),
                provider: ConnectionProvider::PiCompat,
                auth: ConnectionAuth::ApiKey,
                endpoint: Some("http://127.0.0.1:1".into()),
                pi_provider: None,
                custom_api: Some(EndpointApi::OpenaiCompletions),
                models: vec!["fixture".into()],
                default_model: "fixture".into(),
                supports_images: false,
                mid_stream_behavior: MidStreamBehavior::Queue,
            },
            model: "fixture".into(),
            thinking: None,
            cwd: self.0.to_string_lossy().into(),
            runtime_directory: self.0.join("state").to_string_lossy().into(),
            prompt: scenario.into(),
            history: vec![],
            tools: vec!["write".into()],
            external_tools: Vec::new(),
            max_turns: 3,
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn id(s: &str) -> Id {
    Id::new(s).unwrap()
}
#[derive(Default)]
struct Capture {
    decision: Option<bool>,
    bad_decision: bool,
    queued: Vec<ToolDecision>,
    text: String,
    tools: usize,
    completed: usize,
    ready_stop: Option<Arc<AtomicBool>>,
    ready_delay: Duration,
    ready_seen: bool,
}
impl Handler for Capture {
    fn event(&mut self, event: &WorkerEvent) -> Result<(), RuntimeError> {
        match event {
            WorkerEvent::Ready => {
                self.ready_seen = true;
                std::thread::sleep(self.ready_delay);
                if let Some(stop) = &self.ready_stop {
                    stop.store(true, Ordering::Release);
                }
            }
            WorkerEvent::Text { delta, .. } => self.text.push_str(delta),
            WorkerEvent::ToolRequest {
                request_id,
                tool_call_id,
                ..
            } => {
                if let Some(allowed) = self.decision {
                    self.queued.push(ToolDecision {
                        request_id: if self.bad_decision {
                            id("stale")
                        } else {
                            request_id.clone()
                        },
                        tool_call_id: tool_call_id.clone(),
                        allowed,
                        output: None,
                    });
                }
            }
            WorkerEvent::ToolResult { .. } => self.tools += 1,
            WorkerEvent::Completed { .. } => self.completed += 1,
            _ => {}
        }
        Ok(())
    }
    fn decisions(&mut self) -> Result<Vec<ToolDecision>, RuntimeError> {
        Ok(std::mem::take(&mut self.queued))
    }
}
fn execute(f: &Fixture, scenario: &str, handler: &mut Capture) -> Result<RunOutcome, RuntimeError> {
    run(
        &f.command(),
        &f.config(scenario),
        Some("private-key-甲🔒"),
        Arc::new(AtomicBool::new(false)),
        handler,
    )
}

#[test]
fn completed_requires_clean_exit_and_text_secrets_are_redacted_across_deltas() {
    let f = Fixture::new();
    let mut h = Capture::default();
    assert_eq!(
        execute(&f, "secret", &mut h).unwrap(),
        RunOutcome::Completed
    );
    assert_eq!(h.text, "before [credential redacted] after");
    assert_eq!(h.completed, 1);
    let mut h = Capture::default();
    assert_eq!(
        execute(&f, "bad-exit", &mut h).unwrap_err().code,
        "worker_exit"
    );
    assert_eq!(h.completed, 0);
}
#[test]
fn tool_cannot_proceed_without_a_matching_decision() {
    for allowed in [true, false] {
        let f = Fixture::new();
        let mut h = Capture {
            decision: Some(allowed),
            ..Default::default()
        };
        assert_eq!(execute(&f, "tool", &mut h).unwrap(), RunOutcome::Completed);
        assert_eq!(f.0.join("effect").exists(), allowed);
        assert_eq!(h.tools, usize::from(allowed));
    }
}
#[test]
fn stale_decisions_and_unapproved_results_fail_closed() {
    let f = Fixture::new();
    let mut h = Capture {
        decision: Some(true),
        bad_decision: true,
        ..Default::default()
    };
    assert_eq!(
        execute(&f, "tool", &mut h).unwrap_err().code,
        "stale_decision"
    );
    assert!(!f.0.join("effect").exists());
    assert_eq!(
        execute(&f, "unapproved", &mut Capture::default())
            .unwrap_err()
            .code,
        "unapproved_tool"
    );
}
#[test]
fn malformed_scope_sequence_and_terminal_boundaries_are_rejected() {
    for (case, code) in [
        ("scope", "protocol_scope"),
        ("sequence", "protocol_scope"),
        ("late", "protocol_scope"),
        ("bad-json", "invalid_frame"),
        ("tail", "invalid_frame"),
        ("oversize", "invalid_frame"),
        ("missing", "missing_terminal"),
    ] {
        let f = Fixture::new();
        assert_eq!(
            execute(&f, case, &mut Capture::default()).unwrap_err().code,
            code,
            "{case}"
        );
    }
}
#[test]
fn cancellation_stops_a_worker_that_ignores_term() {
    let f = Fixture::new();
    let stop = Arc::new(AtomicBool::new(false));
    let mut h = Capture {
        ready_stop: Some(stop.clone()),
        ..Default::default()
    };
    let started = Instant::now();
    assert_eq!(
        run(
            &f.command(),
            &f.config("hang"),
            Some("private"),
            stop,
            &mut h
        )
        .unwrap(),
        RunOutcome::Cancelled
    );
    assert!(started.elapsed() < Duration::from_secs(2));
}
#[test]
fn terminal_worker_with_a_lingering_child_is_not_success_and_group_is_reaped() {
    let f = Fixture::new();
    assert_eq!(
        execute(&f, "child", &mut Capture::default())
            .unwrap_err()
            .code,
        "worker_hung"
    );
    std::thread::sleep(Duration::from_millis(700));
    assert!(!f.0.join("child-effect").exists());
}
#[test]
fn timeout_bounds_an_unresponsive_worker() {
    let f = Fixture::new();
    let mut command = f.command();
    command.timeout = Duration::from_millis(250);
    let now = Instant::now();
    assert_eq!(
        run(
            &command,
            &f.config("hang"),
            Some("private"),
            Arc::new(AtomicBool::new(false)),
            &mut Capture::default()
        )
        .unwrap_err()
        .code,
        "timeout"
    );
    assert!(now.elapsed() < Duration::from_secs(2));
}

#[test]
fn watchdog_stops_side_effects_even_while_a_host_callback_is_blocked() {
    let f = Fixture::new();
    let mut command = f.command();
    command.timeout = Duration::from_millis(500);
    let mut handler = Capture {
        ready_delay: Duration::from_secs(1),
        ..Default::default()
    };
    let result = run(
        &command,
        &f.config("blocked-handler"),
        Some("private"),
        Arc::new(AtomicBool::new(false)),
        &mut handler,
    );
    assert!(handler.ready_seen);
    assert_eq!(result.unwrap_err().code, "timeout");
    assert!(!f.0.join("handler-effect").exists());
}
#[test]
fn large_stderr_is_drained_without_exposing_credentials() {
    let f = Fixture::new();
    let mut h = Capture::default();
    assert_eq!(
        execute(&f, "stderr", &mut h).unwrap(),
        RunOutcome::Completed
    );
    assert_eq!(h.text, "visible");
}
#[test]
fn credentials_never_appear_in_argv_or_inherited_environment() {
    let f = Fixture::new();
    let mut h = Capture::default();
    execute(&f, "private-start", &mut h).unwrap();
    assert_eq!(h.text, "private stdin only");
}
#[test]
fn oversize_start_and_pre_cancel_do_not_execute_the_worker() {
    let f = Fixture::new();
    let mut config = f.config("tool");
    config.prompt = "x".repeat(MAX_FRAME);
    assert_eq!(
        run(
            &f.command(),
            &config,
            Some("private"),
            Arc::new(AtomicBool::new(false)),
            &mut Capture::default()
        )
        .unwrap_err()
        .code,
        "frame_limit"
    );
    assert!(!f.0.join("effect").exists());
    assert_eq!(
        run(
            &f.command(),
            &f.config("tool"),
            Some("private"),
            Arc::new(AtomicBool::new(true)),
            &mut Capture::default()
        )
        .unwrap(),
        RunOutcome::Cancelled
    );
}

const PYTHON: &str = r#"
import json,os,sys,time,signal
value=json.loads(sys.stdin.readline());config=value['config'];scope=config['scope'];seq=0
def emit(kind,**fields):
 global seq
 seq+=1
 print(json.dumps(dict(protocolVersion=2,scope=scope,seq=seq,event=dict(kind=kind,**fields))),flush=True)
case=config['prompt']
if case=='hang':signal.signal(signal.SIGTERM,signal.SIG_IGN)
emit('ready')
if case=='secret':
 text='before '+value['credential']+' after'
 for ch in text:emit('text',partId='reply',textKind='text',delta=ch)
elif case=='tool':
 emit('tool_request',requestId='request',toolCallId='call',name='write',input={'path':'effect'})
 answer=json.loads(sys.stdin.readline());assert answer['scope']==scope
 if answer['decision']['allowed']:
  open('effect','w').write('allowed')
  emit('tool_result',toolCallId='call',failed=False,output={'ok':True})
elif case=='unapproved':emit('tool_result',toolCallId='call',failed=False,output={})
elif case=='scope':scope['turnId']='foreign';emit('text',partId='reply',textKind='text',delta='wrong')
elif case=='sequence':seq+=1;emit('text',partId='reply',textKind='text',delta='wrong')
elif case=='bad-json':print('{broken',flush=True);sys.exit(0)
elif case=='tail':sys.stdout.write('{partial');sys.stdout.flush();sys.exit(0)
elif case=='oversize':print('x'*1048577,flush=True);sys.exit(0)
elif case=='missing':sys.exit(0)
elif case=='hang':signal.signal(signal.SIGTERM,signal.SIG_IGN);time.sleep(30)
elif case=='blocked-handler':time.sleep(.7);open('handler-effect','w').write('late');time.sleep(30)
elif case=='child':
 pid=os.fork()
 if pid==0:
  signal.signal(signal.SIGTERM,signal.SIG_IGN);time.sleep(.65);open('child-effect','w').write('orphan');os._exit(0)
elif case=='stderr':
 sys.stderr.write((value['credential']+'\n')*100000);sys.stderr.flush();emit('text',partId='reply',textKind='text',delta='visible')
elif case=='private-start':
 assert value['credential'] not in ' '.join(sys.argv)
 assert value['credential'] not in str(dict(os.environ))
 emit('text',partId='reply',textKind='text',delta='private stdin only')
emit('completed',cancelled=False)
if case=='late':emit('text',partId='reply',textKind='text',delta='late')
if case=='bad-exit':sys.exit(9)
"#;
