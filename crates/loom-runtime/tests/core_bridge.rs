#![cfg(unix)]
use loom_core::*;
use loom_runtime::{
    run_session, Authorization, RunOutcome, RuntimeError, SessionRunOptions, ToolPolicy,
    WorkerCommand,
};
use serde_json::Value;
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
type TestResult<T> = std::result::Result<T, RuntimeError>;
const SECRET: &str = "private-fixture-甲🔒";
fn id(s: &str) -> Id {
    Id::new(s).unwrap()
}
struct Fixture {
    path: PathBuf,
    core: Core,
    key: SessionKey,
    connection: ModelConnection,
}
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "loom-core-bridge-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        let path = path.canonicalize().unwrap();
        fs::write(path.join("worker.py"), PYTHON).unwrap();
        let core = Core::open(path.join("data")).unwrap();
        core.create_workspace(id("w"), "Fixture".into()).unwrap();
        let connection = core
            .save_connection_metadata(SaveConnectionMetadata {
                id: id("c"),
                operation_id: id("create"),
                expected_revision: None,
                credential_ref: Some(id("fake-key")),
                config: ConnectionConfig {
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
            })
            .unwrap();
        let key = SessionKey {
            workspace_id: id("w"),
            session_id: id("s"),
        };
        core.create_session(CreateSession {
            key: key.clone(),
            title: None,
            project_id: None,
            permission_mode: PermissionMode::Ask,
            connection: ConnectionSelection {
                connection_id: Some(id("c")),
                ..Default::default()
            },
        })
        .unwrap();
        Self {
            path,
            core,
            key,
            connection,
        }
    }
    fn lease(&self, scenario: &str) -> TurnLease {
        self.core
            .send_message(SendMessage {
                key: self.key.clone(),
                client_request_id: id(scenario),
                text: scenario.into(),
                attachment_ids: vec![],
            })
            .unwrap();
        self.core
            .claim_next_turn(
                &self.key,
                RuntimeBinding {
                    adapter: "test-worker".into(),
                    connection: ConnectionSelection {
                        connection_id: Some(id("c")),
                        model: Some("fixture".into()),
                        thinking: None,
                    },
                    permission_mode: PermissionMode::Ask,
                    policy_version: 1,
                    working_directory: Some(
                        self.core.runtime_working_directory(&self.key).unwrap(),
                    ),
                    connection_snapshot_digest: Some(self.connection.snapshot_digest().unwrap()),
                    policy_snapshot_digest: None,
                },
            )
            .unwrap()
    }
    fn run(&self, lease: TurnLease, policy: &str, budget: usize) -> TestResult<RunOutcome> {
        self.run_policy(lease, &mut Policy(policy), budget)
    }
    fn run_policy(
        &self,
        lease: TurnLease,
        policy: &mut impl ToolPolicy,
        budget: usize,
    ) -> TestResult<RunOutcome> {
        run_session(
            &self.core,
            lease,
            &WorkerCommand {
                executable: PathBuf::from("/usr/bin/python3"),
                script: self.path.join("worker.py"),
                timeout: Duration::from_secs(3),
                exit_grace: Duration::from_millis(100),
            },
            SessionRunOptions {
                run_id: id(&"r".repeat(160)),
                tools: vec!["write".into()],
                external_tools: Vec::new(),
                max_turns: 3,
                context_bytes: budget,
            },
            Some(SECRET),
            Arc::new(AtomicBool::new(false)),
            policy,
            None,
        )
    }
    fn view(&self) -> SessionView {
        self.core.get_session(&self.key).unwrap()
    }
    fn effect(&self) -> PathBuf {
        PathBuf::from(self.core.runtime_working_directory(&self.key).unwrap()).join("effect")
    }
}

#[test]
fn production_policy_rejects_a_lease_without_its_frozen_snapshot_before_tool_execution() {
    let f = Fixture::new();
    let lease = f.lease("tool");
    let cwd = PathBuf::from(f.core.runtime_working_directory(&f.key).unwrap());
    let mut policy = loom_runtime::policy::PermissionPolicy::new(
        Default::default(),
        loom_runtime::policy::PolicyScope::native(cwd, f.path.clone()),
    )
    .unwrap();
    let result = f.run_policy(lease, &mut policy, 100_000).unwrap_err();
    assert_eq!(result.code, "policy");
    assert!(result.message.contains("snapshot"));
    assert!(!f.effect().exists());
    assert!(!f.effect().with_file_name("started").exists());
    assert!(f.view().approvals.is_empty());
    assert_eq!(f.view().turns[0].status, TurnStatus::Failed);
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
struct Policy<'a>(&'a str);
impl ToolPolicy for Policy<'_> {
    fn evaluate(
        &mut self,
        context: &RuntimeContext,
        name: &str,
        input: &Value,
    ) -> TestResult<Authorization> {
        assert_eq!(context.binding.permission_mode, PermissionMode::Ask);
        assert_eq!(name, "write");
        assert_eq!(input["path"], "effect");
        // Raw parameters remain available for classification/digest comparison.
        assert_eq!(input[SECRET], SECRET);
        match self.0 {
            "allow" => Ok(Authorization::Allow {
                reason: format!("fixture allow {SECRET}"),
            }),
            "deny" => Ok(Authorization::Deny {
                reason: format!("fixture deny {SECRET}"),
            }),
            "ask" => Ok(Authorization::Ask),
            _ => Err(RuntimeError::new(
                "policy",
                format!("failed classification {SECRET}"),
            )),
        }
    }
}

#[test]
fn automatic_policy_precedes_execution_and_durable_audit_omits_credentials() {
    for allowed in [true, false] {
        let f = Fixture::new();
        let lease = f.lease("tool");
        assert_eq!(
            f.run(lease, if allowed { "allow" } else { "deny" }, 100_000)
                .unwrap(),
            RunOutcome::Completed
        );
        assert_eq!(f.effect().exists(), allowed);
        let view = f.view();
        assert_eq!(view.turns[0].status, TurnStatus::Completed);
        assert!(view.active_turn_id.is_none());
        assert!(view.approvals.is_empty());
        assert_eq!(
            view.tools[&id("tool")].status,
            if allowed {
                ToolStatus::Succeeded
            } else {
                ToolStatus::Denied
            }
        );
        assert_eq!(
            view.turns[0].usage,
            Some(TokenUsage {
                input_tokens: 12,
                output_tokens: 5
            })
        );
        let events = f.core.read_events(&f.key, 0, 100).unwrap();
        let audit = serde_json::to_string(&events).unwrap();
        assert!(!audit.contains(SECRET));
        assert!(!serde_json::to_string(&view).unwrap().contains(SECRET));
        assert!(events.events.iter().any(|e| matches!(&e.payload, SessionEvent::RuntimeApplied { update: RuntimeUpdate::ToolPolicy { arguments_digest, .. }, .. } if arguments_digest.len()==64)));
        assert!(view.messages[1].parts.iter().any(
            |p| matches!(p, MessagePart::Text { text, .. } if text.contains("credential redacted"))
        ));
    }
}

#[test]
fn interactive_decision_uses_real_pending_records_and_consumes_allow_once() {
    for allowed in [true, false] {
        let f = Fixture::new();
        let lease = f.lease("tool");
        std::thread::scope(|threads| {
            let runner = threads.spawn(|| f.run(lease, "ask", 100_000));
            let deadline = Instant::now() + Duration::from_secs(2);
            let approval = loop {
                if let Some(a) = f.view().approvals.values().next().cloned() {
                    break a;
                }
                assert!(Instant::now() < deadline, "approval never arrived");
                std::thread::sleep(Duration::from_millis(5));
            };
            assert!(!f.effect().exists());
            assert!(!approval.preview.to_string().contains(SECRET));
            assert!(f
                .core
                .respond_tool_approval(&f.key, &approval.id, "wrong", true)
                .is_err());
            f.core
                .respond_tool_approval(&f.key, &approval.id, &approval.binding_digest, allowed)
                .unwrap();
            assert_eq!(runner.join().unwrap().unwrap(), RunOutcome::Completed);
        });
        assert_eq!(f.effect().exists(), allowed);
        assert_eq!(
            f.view().approvals[&id("request")].status,
            if allowed {
                ApprovalStatus::Consumed
            } else {
                ApprovalStatus::Denied
            }
        );
    }
}

#[test]
fn cancelling_pending_approval_revokes_authority_and_persists_cancelled_turn() {
    let f = Fixture::new();
    let lease = f.lease("tool");
    let turn = lease.turn_id().clone();
    std::thread::scope(|threads| {
        let runner = threads.spawn(|| f.run(lease, "ask", 100_000));
        let deadline = Instant::now() + Duration::from_secs(2);
        while f.view().approvals.is_empty() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        f.core.cancel_turn(&f.key, &turn).unwrap();
        assert_eq!(runner.join().unwrap().unwrap(), RunOutcome::Cancelled);
    });
    assert!(!f.effect().exists());
    let view = f.view();
    assert_eq!(
        view.approvals[&id("request")].status,
        ApprovalStatus::Cancelled
    );
    assert_eq!(view.turns[0].status, TurnStatus::Cancelled);
    assert!(view.active_turn_id.is_none());
}

#[test]
fn context_policy_protocol_and_exit_failures_are_saved_without_false_completion() {
    for (scenario, policy, budget, code) in [
        ("tool", "error", 100_000, "policy"),
        ("tool", "allow", 1, "core"),
        ("usage-regressed", "allow", 100_000, "core"),
        ("bad-exit", "allow", 100_000, "worker_exit"),
    ] {
        let f = Fixture::new();
        let lease = f.lease(scenario);
        let error = f.run(lease, policy, budget).unwrap_err();
        assert_eq!(error.code, code);
        assert!(!error.message.contains(SECRET));
        let view = f.view();
        assert!(matches!(
            view.turns[0].status,
            TurnStatus::Failed | TurnStatus::Interrupted
        ));
        assert!(view.active_turn_id.is_none());
        assert!(!f.effect().exists());
        assert!(!serde_json::to_string(&view).unwrap().contains(SECRET));
        if budget == 1 {
            assert!(!f.effect().with_file_name("started").exists());
        }
    }
}

const PYTHON: &str = r#"
import json,sys,os
start=json.loads(sys.stdin.readline());config=start['config'];secret=start['credential'];scope=config['scope'];seq=0
assert config['cwd']==os.getcwd()
open('started','w').write('started')
def emit(kind,**fields):
 global seq
 seq+=1
 print(json.dumps(dict(protocolVersion=2,scope=scope,seq=seq,event=dict(kind=kind,**fields))),flush=True)
emit('ready')
emit('usage',inputTokens=12,outputTokens=5)
case=config['prompt']
if case=='tool':
 emit('tool_request',requestId='request',toolCallId='tool',name='write',input={'path':'effect',secret:secret})
 decision=json.loads(sys.stdin.readline())
 if decision['kind']=='cancel':sys.exit(0)
 if decision['decision']['allowed']:
  open('effect','w').write('authorized')
  emit('tool_result',toolCallId='tool',failed=False,output={secret:secret})
 for ch in 'reply '+secret:emit('text',partId='reply',textKind='text',delta=ch)
elif case=='usage-regressed':emit('usage',inputTokens=1,outputTokens=1)
emit('completed',cancelled=False)
if case=='bad-exit':sys.exit(7)
"#;
