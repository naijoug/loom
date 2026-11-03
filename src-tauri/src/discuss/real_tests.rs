//! Opt-in: the desktop pipeline, two real CLIs, five planned calls. No project is attached.
use super::{
    jobs::Pipeline,
    models::Status,
    store::{self, Store},
    tests::sample,
};
use crate::{agent, models::AgentConfig};
use std::{path::PathBuf, sync::Arc};
#[tokio::test]
#[ignore = "requires LOOM_REAL_DISCUSS=1 and configured Codex + Claude accounts; consumes model usage"]
async fn codex_and_claude_complete_two_rounds_and_summary() {
    assert_eq!(std::env::var("LOOM_REAL_DISCUSS").as_deref(), Ok("1"));
    let env = |s: &str| std::env::var(format!("LOOM_REAL_DISCUSS_{s}")).ok();
    let root = env("ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join(store::next_id("loom-real-discuss")));
    let store = Store::new(&root);
    let id = store::next_id("discussion");
    let mut d = sample(&id);
    d.topic="For a small local desktop app running 2–4 AI command-line tools, compare parallel and sequential discussion rounds. Recommend a bounded concurrency and partial-failure policy. Do not browse or read files. Limit each answer to 150 words; label assumptions. Discuss tradeoffs, not implementation work.".into();
    d.title = "Bounded multi-agent discussion".into();
    d.language = "zh-CN".into();
    d.participants[0].name = "Codex".into();
    d.participants[0].model = env("CODEX_MODEL");
    d.participants[1].name = "Claude Code".into();
    d.participants[1].adapter_type = agent::ADAPTER_CLAUDE.into();
    d.participants[1].model = env("CLAUDE_MODEL");
    let configs = [
        ("a", "codex", agent::ADAPTER_CODEX),
        ("b", "claude", agent::ADAPTER_CLAUDE),
    ]
    .map(|(id, command, adapter)| AgentConfig {
        id: id.into(),
        name: command.into(),
        command: env(&command.to_uppercase()).unwrap_or_else(|| command.into()),
        args: vec![],
        working_directory_policy: "project_root".into(),
        capabilities: vec!["planning".into()],
        adapter_type: adapter.into(),
        can_write_files: false,
        can_run_commands: false,
        enabled: true,
        available: true,
        provider_id: None,
    });
    store.create(&d).unwrap();
    let (_, lease) = store.begin(&id, None).unwrap();
    println!(
        "Discussion evidence: {}",
        store.path(&id, &[]).unwrap().display()
    );
    let pipeline = Pipeline {
        store: store.clone(),
        id: id.clone(),
        agents: configs.into_iter().map(|a| (a.id.clone(), a)).collect(),
        emit: Arc::new(|e| println!("{}", serde_json::to_string(&e).unwrap())),
    };
    pipeline.run(lease, None).await;
    let d = store.load(&id).unwrap();
    println!("Final status: {}", d.status);
    for task in &d.tasks {
        println!(
            "{} {:?}, attempts={}, error={:?}",
            task.id,
            task.status,
            task.attempts.len(),
            task.error
        );
        if task.status == Status::Done {
            println!(
                "---- {} ----\n{}",
                task.id,
                store.read_report(&id, task).unwrap().unwrap()
            );
        }
    }
    assert!(d.tasks.iter().all(|t|t.status==Status::Done),"Both providers and all five calls must pass; inspect persisted errors for blocked providers");
    assert_eq!(d.status, "ready");
    assert!(d.tasks.iter().all(|t| t.attempts.len() == 1));
}
