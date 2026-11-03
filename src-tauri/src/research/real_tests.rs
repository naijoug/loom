//! Opt-in real CLI acceptance for Research. Never runs in the ordinary/offline test suite.
//!
//! `LOOM_REAL_RESEARCH=1 LOOM_REAL_RESEARCH_ROOT=<dir> cargo test research::real_tests::<name> -- --ignored --nocapture`
//! asks one question about a project (this checkout by default) and one follow-up through the
//! same job pipeline as the desktop commands, then prints timings, citations and warnings.
use super::models::Workflow;
use super::{
    jobs::Job,
    models::{Research, Turn, TurnStatus, RESEARCH_SCHEMA_VERSION},
    prompt,
    store::{self, Store},
};
use crate::{
    agent,
    models::{now_ms, AgentConfig},
};
use std::{path::PathBuf, sync::Arc, time::Instant};

#[tokio::test]
#[ignore = "requires LOOM_REAL_RESEARCH=1 and a configured Codex account; consumes model usage"]
async fn codex_researches_a_project() {
    research("codex", agent::ADAPTER_CODEX).await;
}

#[tokio::test]
#[ignore = "requires LOOM_REAL_RESEARCH=1 and a configured Claude account; consumes model usage"]
async fn claude_researches_a_project() {
    research("claude", agent::ADAPTER_CLAUDE).await;
}

async fn research(default_command: &str, adapter: &str) {
    assert_eq!(
        std::env::var("LOOM_REAL_RESEARCH").as_deref(),
        Ok("1"),
        "real model calls require explicit opt-in"
    );
    let env = |name: &str| std::env::var(format!("LOOM_REAL_RESEARCH_{name}")).ok();
    let root = env("ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join(store::next_id("loom-real-research")));
    let project = env("PROJECT").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_path_buf()
    });
    let project = project.canonicalize().unwrap();
    let questions = [
        env("QUESTION").unwrap_or_else(|| {
            "How does Loom start a local agent CLI and stop it when the user cancels?".into()
        }),
        env("FOLLOWUP").unwrap_or_else(|| {
            "Which limits (timeout, output size) apply to that run, and where are they set?".into()
        }),
    ];
    // A CLI binary outside PATH (e.g. a cached Codex) can be pointed to explicitly.
    let command = env(&default_command.to_uppercase()).unwrap_or_else(|| default_command.into());
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
    let store = Store::new(&root);
    let now = now_ms();
    let research = Research {
        active_owner: None,
        workflow: Workflow::Legacy,
        target: None,
        schema_version: RESEARCH_SCHEMA_VERSION,
        research_id: store::next_id("research"),
        title: prompt::title(&questions[0]),
        agent_id: agent.id.clone(),
        agent_name: agent.name.clone(),
        adapter_type: agent.adapter_type.clone(),
        model: env("MODEL"),
        project_path: Some(project.to_string_lossy().into_owned()),
        project_name: project
            .file_name()
            .map(|n| n.to_string_lossy().into_owned()),
        language: env("LANGUAGE").unwrap_or_else(|| "zh-CN".into()),
        prompt_version: prompt::PROMPT_VERSION,
        created_at_ms: now,
        updated_at_ms: now,
        turns: Vec::new(),
        resume_command: None,
        resume_agent_command: None,
    };
    let id = research.research_id.clone();
    store.create(&research).unwrap();
    println!("research dir: {}", store.dir(&id).unwrap().display());
    for question in questions {
        let job_id = store::next_id("job");
        let (_, index) = store
            .update(&id, |research| {
                let index = research.turns.len() as u32 + 1;
                research.turns.push(Turn {
                    index,
                    question: question.clone(),
                    status: TurnStatus::Running,
                    job_id: job_id.clone(),
                    started_at_ms: now_ms(),
                    finished_at_ms: None,
                    error: None,
                    warnings: Vec::new(),
                    refs: None,
                    context: None,
                });
                Ok(index)
            })
            .unwrap();
        let job = Job {
            store: store.clone(),
            research_id: id.clone(),
            job_id: job_id.clone(),
            turn: index,
            cancel: store::claim_job(&store, &id, &job_id).unwrap(),
            agent: agent.clone(),
            emit: Arc::new(|event| println!("event: {}", serde_json::to_string(&event).unwrap())),
        };
        let started = Instant::now();
        job.run().await;
        println!("turn {index}: {:.0}s", started.elapsed().as_secs_f64());
    }
    let finished = store.load(&id).unwrap();
    for turn in &finished.turns {
        println!(
            "turn {} {:?} context={:?} refs={:?} warnings={:?} error={:?}",
            turn.index, turn.status, turn.context, turn.refs, turn.warnings, turn.error
        );
        if let Some(report) = store.read_turn(&id, turn.index).unwrap() {
            println!("---- report {} ----\n{report}\n----", turn.index);
        }
    }
    assert!(
        finished.turns.iter().all(|t| t.status == TurnStatus::Done),
        "every question is answered"
    );
    assert!(finished
        .turns
        .iter()
        .all(|t| t.refs.as_ref().is_some_and(|r| r.valid > 0)));
}
