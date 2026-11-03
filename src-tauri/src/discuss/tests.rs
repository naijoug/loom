use super::{
    commands,
    jobs::Pipeline,
    models::*,
    prompt,
    store::{self, Store},
};
use crate::{
    agent,
    models::{now_ms, AgentConfig},
};
use std::{
    collections::HashMap,
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

pub(super) fn root() -> PathBuf {
    let p = std::env::temp_dir().join(store::next_id("loom-discuss-test"));
    fs::create_dir_all(&p).unwrap();
    p
}
pub(super) fn sample(id: &str) -> Discussion {
    let ps = vec![
        Participant {
            id: "p1".into(),
            agent_id: "a".into(),
            name: "Alpha".into(),
            adapter_type: agent::ADAPTER_CODEX.into(),
            model: None,
        },
        Participant {
            id: "p2".into(),
            agent_id: "b".into(),
            name: "Beta".into(),
            adapter_type: agent::ADAPTER_CODEX.into(),
            model: None,
        },
    ];
    Discussion {
        schema_version: SCHEMA_VERSION,
        discussion_id: id.into(),
        title: "Choose a queue".into(),
        topic: "Compare a bounded queue and an unbounded queue".into(),
        language: "en".into(),
        project_path: None,
        project_name: None,
        tasks: tasks(&ps),
        participants: ps,
        status: "ready".into(),
        active_run_id: None,
        created_at_ms: now_ms(),
        updated_at_ms: now_ms(),
        prompt_version: prompt::VERSION,
    }
}
fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}
fn response(text: &str) -> String {
    format!("printf '%s\\n' {}",quote(&serde_json::json!({"type":"item.completed","item":{"type":"agent_message","text":text}}).to_string()))
}
fn fake(root: &Path, id: &str, script: &str) -> AgentConfig {
    let p = root.join(format!("fake-{id}"));
    fs::write(&p,format!("#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'codex 1'; exit 0; fi\ninput=$(cat)\n{script}\n")).unwrap();
    fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
    AgentConfig {
        id: id.into(),
        name: id.into(),
        command: p.to_string_lossy().into_owned(),
        args: vec![],
        working_directory_policy: "project_root".into(),
        capabilities: vec!["planning".into()],
        adapter_type: agent::ADAPTER_CODEX.into(),
        can_write_files: false,
        can_run_commands: false,
        enabled: true,
        available: true,
    }
}
fn pipeline(root: &Path, scripts: [&str; 2]) -> Pipeline {
    let store = Store::new(root);
    let id = store::next_id("discussion");
    store.create(&sample(&id)).unwrap();
    Pipeline {
        store,
        id,
        agents: [fake(root, "a", scripts[0]), fake(root, "b", scripts[1])]
            .into_iter()
            .map(|a| (a.id.clone(), a))
            .collect::<HashMap<_, _>>(),
        emit: Arc::new(|_| {}),
    }
}
async fn run(p: &Pipeline, retry: Option<&str>) -> Discussion {
    let (_, lease) = p.store.begin(&p.id, retry).unwrap();
    tokio::time::timeout(
        Duration::from_secs(15),
        p.clone().run(lease, retry.map(str::to_string)),
    )
    .await
    .expect("bounded pipeline");
    p.store.load(&p.id).unwrap()
}

#[tokio::test]
async fn parallel_rounds_have_barriers_and_five_persisted_calls() {
    let root = root();
    // Each peer must see the other's start marker. A serial implementation cannot pass.
    let script = |own: &str, peer: &str| {
        format!("case \"$input\" in *'Round 1:'*) touch {}; n=0; while [ ! -f {} ]; do n=$((n+1)); [ $n -lt 150 ] || exit 9; sleep 0.02; done;; esac\n{}",quote(root.join(own).to_str().unwrap()),quote(root.join(peer).to_str().unwrap()),response("# Answer\nBound the queue."))
    };
    let p = pipeline(
        &root,
        [&script("a-start", "b-start"), &script("b-start", "a-start")],
    );
    let d = run(&p, None).await;
    assert_eq!(d.status, "ready");
    assert_eq!(d.tasks.len(), 5);
    for t in &d.tasks {
        assert_eq!(t.status, Status::Done);
        assert_eq!(t.attempts.len(), 1);
        assert!(p
            .store
            .read_report(&p.id, t)
            .unwrap()
            .unwrap()
            .contains("Bound the queue"));
    }
    let records: Vec<JobRecord> = d
        .tasks
        .iter()
        .map(|t| {
            serde_json::from_str(
                &p.store
                    .read_log(&p.id, &t.attempts[0], "record", 0)
                    .unwrap()
                    .text,
            )
            .unwrap()
        })
        .collect();
    let first_end = records
        .iter()
        .filter(|j| j.task_id.starts_with("r1"))
        .map(|j| j.finished_at_ms.unwrap())
        .max()
        .unwrap();
    assert!(records
        .iter()
        .filter(|j| j.task_id.starts_with("r2"))
        .all(|j| j.started_at_ms >= first_end));
    let second_end = records
        .iter()
        .filter(|j| j.task_id.starts_with("r2"))
        .map(|j| j.finished_at_ms.unwrap())
        .max()
        .unwrap();
    assert!(records.last().unwrap().started_at_ms >= second_end);
    let text = p
        .store
        .read_log(&p.id, &d.tasks.last().unwrap().attempts[0], "prompt", 0)
        .unwrap()
        .text;
    assert!(text.contains("Participant p1") && text.contains("Participant p2"));
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn partial_failure_preserves_peers_and_single_retry_marks_downstream_stale() {
    let root = root();
    let ok = response("# Answer\nA bounded queue.");
    let p = pipeline(&root, [&ok, "echo 'provider unavailable' >&2; exit 2"]);
    let d = run(&p, None).await;
    assert_eq!(d.status, "partial");
    assert_eq!(d.task("r1-p2").unwrap().status, Status::Failed);
    assert_eq!(d.task("r2-p2").unwrap().status, Status::Failed);
    assert_eq!(d.task("summary").unwrap().status, Status::Done);
    let peer = p
        .store
        .read_report(&p.id, d.task("r1-p1").unwrap())
        .unwrap();
    let summary_job = &d.task("summary").unwrap().attempts[0];
    assert!(p
        .store
        .read_log(&p.id, summary_job, "prompt", 0)
        .unwrap()
        .text
        .contains("No current successful answer"));
    let mut fixed = p.clone();
    fixed.agents.insert("b".into(), fake(&root, "b-fixed", &ok));
    fixed.agents.get_mut("b").unwrap().id = "b".into();
    let retry = run(&fixed, Some("r1-p2")).await;
    assert_eq!(retry.task("r1-p2").unwrap().attempts.len(), 2);
    assert_eq!(retry.task("r1-p1").unwrap().attempts.len(), 1);
    assert!(retry.tasks.iter().filter(|t| t.round > 1).all(|t| t.stale));
    assert_eq!(
        peer,
        p.store
            .read_report(&p.id, retry.task("r1-p1").unwrap())
            .unwrap()
    );
    assert_eq!(
        retry.tasks.iter().map(|t| t.attempts.len()).sum::<usize>(),
        6
    );
    // Summary cannot silently use stale second-round reports.
    let premature = run(&fixed, Some("summary")).await;
    assert_eq!(premature.task("summary").unwrap().status, Status::Failed);
    run(&fixed, Some("r2-p1")).await;
    run(&fixed, Some("r2-p2")).await;
    let finished = run(&fixed, Some("summary")).await;
    assert_eq!(finished.status, "ready");
    assert_eq!(finished.task("summary").unwrap().attempts.len(), 3);
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn startup_failure_is_per_participant_and_does_not_abort_other_calls() {
    let root = root();
    let ok = response("Healthy answer");
    let mut p = pipeline(&root, [&ok, &ok]);
    p.agents.get_mut("b").unwrap().command =
        root.join("missing-binary").to_string_lossy().into_owned();
    let d = run(&p, None).await;
    assert_eq!(d.status, "partial");
    assert_eq!(d.task("summary").unwrap().status, Status::Done);
    assert_eq!(d.task("r1-p2").unwrap().status, Status::Failed);
    assert!(d.task("r1-p2").unwrap().error.is_some());
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn cancel_stops_parallel_processes_and_does_not_start_later_rounds() {
    let root = root();
    let p = pipeline(&root, ["sleep 30", "sleep 30"]);
    let (_, lease) = p.store.begin(&p.id, None).unwrap();
    let handle = tokio::spawn(p.clone().run(lease, None));
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if p.store
                .load(&p.id)
                .unwrap()
                .tasks
                .iter()
                .filter(|t| t.status == Status::Running)
                .count()
                == 2
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert!(store::cancel(&p.id));
    tokio::time::timeout(Duration::from_secs(8), handle)
        .await
        .unwrap()
        .unwrap();
    let d = p.store.load(&p.id).unwrap();
    assert_eq!(d.status, "cancelled");
    assert!(d.tasks.iter().all(|t| t.status == Status::Cancelled));
    assert!(d
        .tasks
        .iter()
        .filter(|t| t.round > 1)
        .all(|t| t.attempts.is_empty()));
    assert!(!store::cancel(&p.id));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn restart_recovers_without_calls_and_active_lease_blocks_delete_and_second_start() {
    let root = root();
    let s = Store::new(&root);
    let id = store::next_id("discussion");
    s.create(&sample(&id)).unwrap();
    let (_, lease) = s.begin(&id, None).unwrap();
    assert!(s.begin(&id, None).is_err());
    assert!(s.delete(&id).is_err());
    assert_eq!(s.load(&id).unwrap().status, "running");
    drop(lease);
    let d = s.load(&id).unwrap();
    assert_eq!(d.status, "interrupted");
    assert!(d
        .tasks
        .iter()
        .all(|t| t.status == Status::Interrupted && t.attempts.is_empty()));
    assert!(s.begin(&id, None).is_err());
    let (_, lease) = s.begin(&id, Some("r1-p1")).unwrap();
    drop(lease);
    s.load(&id).unwrap();
    s.delete(&id).unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn validates_confirmation_participant_count_models_and_duplicate_choices() {
    let mut i = CreateDiscussionInput {
        topic: " topic ".into(),
        participants: vec![
            ParticipantInput {
                agent_id: "a".into(),
                model: None,
            },
            ParticipantInput {
                agent_id: "b".into(),
                model: Some(" model ".into()),
            },
        ],
        project_path: None,
        language: "en".into(),
        confirmed_calls: 5,
    };
    commands::validate(&mut i).unwrap();
    assert_eq!(i.topic, "topic");
    assert_eq!(i.participants[1].model.as_deref(), Some("model"));
    i.confirmed_calls = 9;
    assert!(commands::validate(&mut i).is_err());
    i.confirmed_calls = 5;
    i.participants[1] = i.participants[0].clone();
    assert!(commands::validate(&mut i).is_err());
    i.participants[1].model = Some("--bad".into());
    assert!(commands::validate(&mut i).is_err());
    i.participants = vec![i.participants[0].clone(); 5];
    i.confirmed_calls = 11;
    assert!(commands::validate(&mut i).is_err());
}

#[test]
fn storage_rejects_escape_symlinks_and_incompatible_schema() {
    let root = root();
    let s = Store::new(&root);
    let id = store::next_id("discussion");
    s.create(&sample(&id)).unwrap();
    assert!(s.load("../outside").is_err());
    assert!(s.path(&id, &["..", "outside"]).is_err());
    let outside = root.join("outside");
    fs::create_dir(&outside).unwrap();
    symlink(&outside, s.path(&id, &["jobs"]).unwrap()).unwrap();
    assert!(s.path(&id, &["jobs", "job-1", "stdout.log"]).is_err());
    assert!(s.read_log(&id, "job-1", "stdout", 0).is_err());
    let mut d = sample(&id);
    d.schema_version = 99;
    crate::storage::atomic_write_json(&s.path(&id, &["discussion.json"]).unwrap(), &d).unwrap();
    assert!(s.load(&id).is_err());
    assert_eq!(s.list().storage_errors.len(), 1);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn prompt_bounds_peer_material_and_marks_missing_answers() {
    let root = root();
    let s = Store::new(&root);
    let id = store::next_id("discussion");
    let mut d = sample(&id);
    s.create(&d).unwrap();
    d.tasks[0].status = Status::Done;
    s.write_report(&id, &d.tasks[0], &"界".repeat(50_000))
        .unwrap();
    d.tasks[1].status = Status::Failed;
    let prompt = prompt::build(&d, &d.tasks[2], &s).unwrap();
    assert!(prompt.contains("truncated"));
    assert!(prompt.contains("No current successful answer"));
    assert!(prompt.chars().count() < 12_000);
    assert!(prompt::extract(" ").is_none());
    assert_eq!(
        prompt::extract("intro <<<LOOM_DISCUSS\nanswer\nLOOM_DISCUSS>>> outro").unwrap(),
        "answer"
    );
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn empty_answers_fail_instead_of_manufacturing_a_successful_summary() {
    let root = root();
    let p = pipeline(&root, ["exit 0", "exit 0"]);
    let d = run(&p, None).await;
    assert_eq!(d.status, "failed");
    assert!(d.tasks.iter().all(|t| t.status == Status::Failed));
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn four_participants_complete_nine_calls_and_all_receive_read_only_arguments() {
    let root = root();
    let answer = response("Read-only answer");
    let script = format!("case \"$*\" in *'--sandbox read-only'*) ;; *) exit 7;; esac\n{answer}");
    let mut p = pipeline(&root, [&script, &script]);
    p.store
        .update(&p.id, |d| {
            for (id, agent_id) in [("p3", "c"), ("p4", "d")] {
                d.participants.push(Participant {
                    id: id.into(),
                    agent_id: agent_id.into(),
                    name: agent_id.into(),
                    adapter_type: agent::ADAPTER_CODEX.into(),
                    model: None,
                });
            }
            d.tasks = tasks(&d.participants);
            Ok(())
        })
        .unwrap();
    for id in ["c", "d"] {
        p.agents.insert(id.into(), fake(&root, id, &script));
    }
    let d = run(&p, None).await;
    assert_eq!(d.status, "ready");
    assert_eq!(d.tasks.len(), 9);
    assert_eq!(d.tasks.iter().map(|t| t.attempts.len()).sum::<usize>(), 9);
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn project_report_records_valid_and_escaping_source_references() {
    let root = root();
    fs::write(root.join("source.rs"), "fn main() {\n}\n").unwrap();
    let script = response(
        "See [source](loom-src:source.rs#L1-L2) and [escape](loom-src:../outside.rs#L1-L2).",
    );
    let p = pipeline(&root, [&script, &script]);
    p.store
        .update(&p.id, |d| {
            d.project_path = Some(root.to_string_lossy().into_owned());
            Ok(())
        })
        .unwrap();
    let d = run(&p, None).await;
    assert_eq!(d.status, "ready");
    assert!(d.tasks.iter().all(|t| t
        .refs
        .as_ref()
        .is_some_and(|r| r.valid == 1 && r.invalid == 1)));
    assert!(d.tasks.iter().all(|t| !t.warnings.is_empty()));
    assert_eq!(
        fs::read_to_string(root.join("source.rs")).unwrap(),
        "fn main() {\n}\n"
    );
    fs::remove_dir_all(root).unwrap();
}
