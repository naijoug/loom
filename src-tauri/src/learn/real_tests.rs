//! Opt-in real CLI acceptance for Learn. Never runs in the ordinary/offline test suite.
//!
//! `LOOM_REAL_LEARN=1 LOOM_REAL_LEARN_ROOT=<dir> cargo test learn::real_tests::<name> -- --ignored --nocapture`
//! clones a public repository into `<dir>/learn/studies/`, runs the same job pipeline as the desktop
//! command (fetch → inventory → architecture → level-1 pages, then every remaining page and the
//! glossary) and prints per-stage timings and reference statistics.
use super::{
    jobs::{plan, Job},
    models::{StageState, StageStatus, Study, STUDY_SCHEMA_VERSION},
    store::{self, Store},
};
use crate::{
    agent,
    models::{now_ms, AgentConfig},
};
use std::{path::PathBuf, sync::Arc, time::Instant};

#[tokio::test]
#[ignore = "requires LOOM_REAL_LEARN=1 and a configured Claude account; consumes model usage"]
async fn claude_studies_hexyl() {
    study("claude", agent::ADAPTER_CLAUDE).await;
}

#[tokio::test]
#[ignore = "requires LOOM_REAL_LEARN=1 and a configured Codex account; consumes model usage"]
async fn codex_studies_hexyl() {
    study("codex", agent::ADAPTER_CODEX).await;
}

async fn study(default_command: &str, adapter: &str) {
    assert_eq!(
        std::env::var("LOOM_REAL_LEARN").as_deref(),
        Ok("1"),
        "real model calls require explicit opt-in"
    );
    let root = std::env::var("LOOM_REAL_LEARN_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join(store::next_id("loom-real-learn")));
    let url = std::env::var("LOOM_REAL_LEARN_URL")
        .unwrap_or_else(|_| "https://github.com/sharkdp/hexyl".into());
    // A CLI binary outside PATH (e.g. a cached Codex) can be pointed to explicitly.
    let command = std::env::var(format!(
        "LOOM_REAL_LEARN_{}",
        default_command.to_uppercase()
    ))
    .unwrap_or_else(|_| default_command.into());
    let model = std::env::var("LOOM_REAL_LEARN_MODEL").ok();
    let language = std::env::var("LOOM_REAL_LEARN_LANGUAGE").unwrap_or_else(|_| "zh-CN".into());
    let agent = AgentConfig {
        id: format!("real-{default_command}"),
        name: format!("{default_command} (real)"),
        command,
        args: Vec::new(),
        working_directory_policy: "project_root".into(),
        capabilities: vec!["planning".into()],
        adapter_type: adapter.into(),
        can_write_files: false,
        can_run_commands: false,
        enabled: true,
        available: true,
    };
    let repo = super::fetch::parse_github_url(&url).unwrap();
    let store = Store::new(&root);
    let now = now_ms();
    let study = Study {
        schema_version: STUDY_SCHEMA_VERSION,
        study_id: store::next_id("study"),
        url: repo.url.clone(),
        owner: repo.owner.clone(),
        repo: repo.repo.clone(),
        git_ref: None,
        commit: None,
        agent_id: agent.id.clone(),
        agent_name: agent.name.clone(),
        adapter_type: agent.adapter_type.clone(),
        model,
        language,
        depth: "standard".into(),
        prompt_version: super::generate::PROMPT_VERSION,
        created_at_ms: now,
        updated_at_ms: now,
        stages: ["fetch", "inventory", "architecture"]
            .into_iter()
            .map(StageState::pending)
            .collect(),
        pages: Vec::new(),
        coverage: None,
        checkout_fingerprint: None,
    };
    store.create(&study).unwrap();
    println!(
        "study dir: {}",
        store.dir(&study.study_id).unwrap().display()
    );
    for stage in ["fetch", "all"] {
        let current = store.load(&study.study_id).unwrap();
        // A failed first job leaves nothing to plan; fall through to the summary.
        let Ok((stages, auto_level1)) = plan(&current, stage) else {
            break;
        };
        let job_id = store::next_id("job");
        let job = Job {
            store: store.clone(),
            study_id: study.study_id.clone(),
            job_id: job_id.clone(),
            cancel: store::claim_job(&study.study_id, &job_id).unwrap(),
            agent: Some(agent.clone()),
            emit: Arc::new(|event| println!("event: {}", serde_json::to_string(&event).unwrap())),
        };
        let started = Instant::now();
        job.run(stages, auto_level1).await;
        println!("job {stage}: {:.0}s", started.elapsed().as_secs_f64());
    }
    let finished = store.load(&study.study_id).unwrap();
    for stage in &finished.stages {
        let seconds = stage
            .finished_at_ms
            .zip(stage.started_at_ms)
            .map(|(end, start)| (end - start) / 1000);
        println!(
            "stage {:<22} {:?} {:>5?}s warnings={:?} error={:?}",
            stage.stage, stage.status, seconds, stage.warnings, stage.error
        );
    }
    for page in &finished.pages {
        println!(
            "page {:<22} ready={} refs={:?}",
            page.page_id, page.ready, page.refs
        );
    }
    println!("coverage: {:?}", finished.coverage);
    assert!(
        finished
            .stages
            .iter()
            .all(|stage| stage.status == StageStatus::Done),
        "every stage finishes"
    );
    assert!(finished.pages.iter().all(|page| page.ready));
}
