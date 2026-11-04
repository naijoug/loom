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
        provider_id: None,
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

/// `LOOM_REAL_LEARN=1 cargo test learn::real_tests::github_trending_live -- --ignored --nocapture`
/// calls the public GitHub search API (no model usage).
#[tokio::test]
#[ignore = "requires LOOM_REAL_LEARN=1 and network access to api.github.com"]
async fn github_trending_live() {
    assert_eq!(std::env::var("LOOM_REAL_LEARN").as_deref(), Ok("1"));
    for (period, language) in [
        ("week", None),
        ("day", Some("Rust")),
        ("month", Some("C++")),
    ] {
        let (query, _, cached, repos) = super::github::trending(period, language, false)
            .await
            .unwrap_or_else(|error| panic!("{period}/{language:?}: {error}"));
        assert!(!cached, "first call is not cached");
        assert!(!repos.is_empty(), "{query} returned no repositories");
        for repo in &repos {
            assert_eq!(repo.url, format!("https://github.com/{}", repo.full_name));
            assert!(super::github::valid_full_name(&repo.full_name));
        }
        if let Some(language) = language {
            let matching = repos
                .iter()
                .filter(|repo| repo.language.as_deref() == Some(language))
                .count();
            assert!(matching * 2 >= repos.len(), "{query}: mostly {language}");
        }
        println!(
            "{query}: {} repos, top {} ★{}",
            repos.len(),
            repos[0].full_name,
            repos[0].stars
        );
        let (_, _, cached, again) = super::github::trending(period, language, false)
            .await
            .unwrap();
        assert!(
            cached && again.len() == repos.len(),
            "second call is cached"
        );
    }
}

/// `LOOM_REAL_LEARN=1 cargo test learn::real_tests::claude_discovers_repositories -- --ignored --nocapture`
/// runs two real Discover turns (consumes model usage) and the GitHub lookups behind them.
#[tokio::test]
#[ignore = "requires LOOM_REAL_LEARN=1 and a configured Claude account; consumes model usage"]
async fn claude_discovers_repositories() {
    discover("claude", agent::ADAPTER_CLAUDE).await;
}

#[tokio::test]
#[ignore = "requires LOOM_REAL_LEARN=1 and a configured Codex account; consumes model usage"]
async fn codex_discovers_repositories() {
    discover("codex", agent::ADAPTER_CODEX).await;
}

async fn discover(command: &str, adapter: &str) {
    use super::models::{DiscoverInput, DiscoverMessage};
    assert_eq!(std::env::var("LOOM_REAL_LEARN").as_deref(), Ok("1"));
    let root = std::env::temp_dir().join(store::next_id("loom-real-discover"));
    let agent = AgentConfig {
        id: format!("real-{command}"),
        name: format!("{command} (real)"),
        command: command.into(),
        args: Vec::new(),
        working_directory_policy: "project_root".into(),
        capabilities: vec!["planning".into()],
        adapter_type: adapter.into(),
        can_write_files: false,
        can_run_commands: false,
        enabled: true,
        available: true,
        provider_id: None,
    };
    let discover = super::discover::Discover::new(&root);
    let language = std::env::var("LOOM_REAL_LEARN_LANGUAGE").unwrap_or_else(|_| "zh-CN".into());
    let mut messages = vec![DiscoverMessage {
        role: "user".into(),
        text: "I want to learn how a small, well-written terminal text editor is built. Suggest a few repositories.".into(),
    }];
    for turn in 0..2 {
        let input = DiscoverInput {
            run_id: format!("real-{command}-{turn}"),
            agent_id: agent.id.clone(),
            model: None,
            language: language.clone(),
            messages: messages.clone(),
        };
        let started = Instant::now();
        let reply = discover.send(&agent, &input, None).await.unwrap();
        println!(
            "turn {turn} ({:.1}s)\n{}\nrepos: {:?}\nqueries: {:?}\nwarnings: {:?}\n",
            started.elapsed().as_secs_f64(),
            reply.reply,
            reply.repos.iter().map(|r| &r.full_name).collect::<Vec<_>>(),
            reply.queries,
            reply.warnings
        );
        assert!(!reply.reply.trim().is_empty());
        assert!(!reply.reply.contains("```json"), "JSON block is stripped");
        assert!(!reply.repos.is_empty(), "lookup found real repositories");
        let run = root.join(format!("learn/discover/runs/{}", input.run_id));
        let record: serde_json::Value =
            serde_json::from_slice(&std::fs::read(run.join("run.json")).unwrap()).unwrap();
        assert_eq!(record["status"], "complete", "{record}");
        assert!(run.join("prompt.md").is_file() && run.join("stdout.log").is_file());
        messages.push(DiscoverMessage {
            role: "assistant".into(),
            text: format!(
                "{}\n\n(Found on GitHub: {})",
                reply.reply,
                reply
                    .repos
                    .iter()
                    .map(|r| r.full_name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        });
        messages.push(DiscoverMessage {
            role: "user".into(),
            text: "Something similar but written in Rust, please.".into(),
        });
    }
    println!("records: {}", root.display());
}
