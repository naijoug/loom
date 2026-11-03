//! Bounded parallel rounds with a barrier between rounds and explicit, single-call retry.
use super::{
    models::*,
    prompt,
    store::{self, Lease, Store},
};
use crate::{
    agent::{
        self, AdapterInvocationRequest, AgentStage, Limits, MessagePart, Observation, ProcessKind,
        ProcessOwner, RunSpec, STOP_REASON_ABORT,
    },
    models::{now_ms, AgentConfig},
    source_files,
};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
pub type Emit = Arc<dyn Fn(DiscussEvent) + Send + Sync>;
#[derive(Clone)]
pub struct Pipeline {
    pub store: Store,
    pub id: String,
    pub agents: HashMap<String, AgentConfig>,
    pub emit: Emit,
}
impl Pipeline {
    fn event(&self, task_id: Option<String>, status: &str) {
        (self.emit)(DiscussEvent {
            discussion_id: self.id.clone(),
            task_id,
            status: status.into(),
        });
    }
    /// Always release the lease, including on a panic. Disk recovery never automatically invokes models.
    pub async fn run(self, lease: Lease, retry: Option<String>) {
        self.event(None, "running");
        let result = self.execute(&lease, retry.as_deref()).await;
        let final_state = self.store.update(&self.id, |d| {
            let interrupted = agent::is_shutting_down();
            let stopped = lease.cancel.load(Ordering::Relaxed);
            for t in &mut d.tasks {
                if matches!(t.status, Status::Pending | Status::Running)
                    && (retry.is_none() || retry.as_deref() == Some(&t.id))
                {
                    t.status = if interrupted {
                        Status::Interrupted
                    } else if stopped && result.is_ok() {
                        Status::Cancelled
                    } else {
                        Status::Failed
                    };
                    t.error = result
                        .as_ref()
                        .err()
                        .cloned()
                        .or_else(|| Some("Call did not complete".into()));
                }
            }
            d.active_run_id = None;
            d.status = if interrupted {
                "interrupted"
            } else if stopped && result.is_ok() {
                "cancelled"
            } else if d.tasks.iter().all(|t| t.status == Status::Done && !t.stale) {
                "ready"
            } else if d.tasks.iter().any(|t| t.status == Status::Done) {
                "partial"
            } else {
                "failed"
            }
            .into();
            Ok(())
        });
        // Release before emitting, so consumers can immediately retry after a terminal event.
        drop(lease);
        match final_state {
            Ok((d, _)) => self.event(None, &d.status),
            Err(_) => self.event(None, "failed"),
        }
    }
    async fn execute(&self, lease: &Lease, retry: Option<&str>) -> Result<(), String> {
        let d = self.store.load(&self.id)?;
        if let Some(id) = retry {
            self.call(&d, d.task(id)?.clone(), lease).await?;
            return Ok(());
        }
        for round in 1..=3 {
            if lease.cancel.load(Ordering::Relaxed) || agent::is_shutting_down() {
                break;
            }
            let snapshot = self.store.load(&self.id)?;
            let mut tasks = tokio::task::JoinSet::new();
            for task in snapshot.tasks.iter().filter(|t| t.round == round) {
                let (pipeline, d, t, cancel, run_id) = (
                    self.clone(),
                    snapshot.clone(),
                    task.clone(),
                    lease.cancel.clone(),
                    lease.run_id.clone(),
                );
                tasks.spawn(async move { pipeline.call_inner(&d, t, &run_id, cancel).await });
            }
            while let Some(result) = tasks.join_next().await {
                // A persisted per-participant failure is Ok; infrastructure/persistence failure stops the pipeline.
                let error = match result {
                    Ok(Ok(())) => None,
                    Ok(Err(e)) => Some(e),
                    Err(e) => Some(format!("Discussion worker interrupted: {e}")),
                };
                if let Some(error) = error {
                    lease.cancel.store(true, Ordering::Relaxed);
                    while tasks.join_next().await.is_some() {}
                    return Err(error);
                }
            }
        }
        Ok(())
    }
    async fn call(&self, d: &Discussion, t: Task, lease: &Lease) -> Result<(), String> {
        self.call_inner(d, t, &lease.run_id, lease.cancel.clone())
            .await
    }
    async fn call_inner(
        &self,
        d: &Discussion,
        t: Task,
        run_id: &str,
        cancel: Arc<AtomicBool>,
    ) -> Result<(), String> {
        let participant = d.participant(&t.participant_id)?;
        let mut record = JobRecord {
            job_id: store::next_id("job"),
            task_id: t.id.clone(),
            run_id: run_id.into(),
            status: Status::Running,
            started_at_ms: now_ms(),
            finished_at_ms: None,
            invocation: None,
            model: participant.model.clone(),
            exit_code: None,
            tool_calls: 0,
            error: None,
        };
        self.store.save_job(&self.id, &record)?;
        self.store
            .write_job_text(&self.id, &record.job_id, "stdout.log", "")?;
        self.store
            .write_job_text(&self.id, &record.job_id, "stderr.log", "")?;
        self.store.update(&self.id, |d| {
            let task = d.task_mut(&t.id)?;
            task.status = Status::Running;
            task.error = None;
            task.attempts.push(record.job_id.clone());
            Ok(())
        })?;
        self.event(Some(t.id.clone()), "running");
        let answer = self.answer(d, &t, &cancel, &mut record).await;
        let status = if cancel.load(Ordering::Relaxed) {
            Status::Cancelled
        } else if agent::is_shutting_down() {
            Status::Interrupted
        } else if answer.is_ok() {
            Status::Done
        } else {
            Status::Failed
        };
        let mut error = answer
            .as_ref()
            .err()
            .cloned()
            .map(|e| agent::redact_sensitive_text(&e));
        let mut warnings = Vec::new();
        let mut refs = None;
        let mut final_status = status;
        if let Ok((text, stats, notes)) = answer {
            if final_status == Status::Done {
                self.store
                    .write_job_text(&self.id, &record.job_id, "result.md", &text)?;
                match self.store.write_report(&self.id, &t, &text) {
                    Ok(()) => {
                        refs = Some(stats);
                        warnings = notes;
                    }
                    Err(e) => {
                        final_status = Status::Failed;
                        error = Some(e);
                    }
                }
            }
        }
        record.status = final_status.clone();
        record.error = error.clone();
        record.finished_at_ms = Some(now_ms());
        self.store.save_job(&self.id, &record)?;
        self.store.update(&self.id, |d| {
            let task = d.task_mut(&t.id)?;
            task.status = final_status;
            task.error = error;
            task.warnings = warnings;
            task.refs = refs;
            if task.status == Status::Done {
                task.stale = false;
            }
            Ok(())
        })?;
        self.event(Some(t.id), "updated");
        Ok(())
    }
    async fn answer(
        &self,
        d: &Discussion,
        t: &Task,
        cancel: &Arc<AtomicBool>,
        record: &mut JobRecord,
    ) -> Result<(String, RefStats, Vec<String>), String> {
        if cancel.load(Ordering::Relaxed) {
            return Err("Cancelled".into());
        }
        let p = d.participant(&t.participant_id)?;
        let a = self
            .agents
            .get(&p.agent_id)
            .ok_or("Participant's Agent configuration is no longer available")?;
        if !a.enabled || !a.available {
            return Err(format!("{} is disabled or unavailable", p.name));
        }
        if a.adapter_type != p.adapter_type {
            return Err("Agent adapter changed; create a new discussion".into());
        }
        agent::check_startable(a).await?;
        let cwd = match &d.project_path {
            Some(path) => {
                let path = PathBuf::from(path);
                if !path.is_dir() {
                    return Err("Project folder is no longer available".into());
                }
                path
            }
            None => self.store.workspace(&self.id)?,
        };
        let project = d.project_path.as_ref().map(PathBuf::from);
        let before = worktree_state(project.clone()).await;
        let prompt = prompt::build(d, t, &self.store)?;
        self.store.write_job_text(
            &self.id,
            &record.job_id,
            "prompt.md",
            &agent::redact_sensitive_text(&prompt),
        )?;
        let prepared = agent::prepare_invocation(
            a,
            &AdapterInvocationRequest {
                execution_profile: None,
                project_path: &cwd,
                prompt: &prompt,
                prompt_file: None,
                stage: AgentStage::Planning,
                resume_command: None,
                chat_permission_mode: Some("explore"),
                embed_prompt: !matches!(
                    a.adapter_type.as_str(),
                    agent::ADAPTER_CODEX | agent::ADAPTER_CLAUDE
                ),
                system_append: None,
                model: p.model.as_deref(),
            },
        )?;
        let args: Vec<String> = prepared
            .args
            .iter()
            .map(|s| {
                if s.chars().count() > 160 {
                    format!("<{} chars>", s.chars().count())
                } else {
                    agent::redact_sensitive_text(s)
                }
            })
            .collect();
        record.invocation = Some(format!(
            "{} {}",
            agent::redact_sensitive_text(&prepared.program),
            args.join(" ")
        ));
        self.store.save_job(&self.id, record)?;
        let spec = RunSpec {
            stdin_text: prepared.stdin_prompt.then_some(prompt),
            prepared,
            run_id: record.job_id.clone(),
            owner: ProcessOwner::Feature {
                feature: "discuss".into(),
                scope_id: self.id.clone(),
                run_id: record.job_id.clone(),
            },
            kind: ProcessKind::Feature,
            limits: Limits {
                timeout: Duration::from_secs(15 * 60),
                ..Limits::default()
            },
            enrich: None,
        };
        let (cancel, store, id, job) = (
            cancel.clone(),
            self.store.clone(),
            self.id.clone(),
            record.job_id.clone(),
        );
        let result = agent::run(
            spec,
            move || cancel.load(Ordering::Relaxed).then_some(STOP_REASON_ABORT),
            |_| Ok(()),
            move |observation| {
                if let Observation::Log { stderr, line } = observation {
                    store.append_log(&id, &job, stderr, &line)?;
                }
                Ok(())
            },
        )
        .await?;
        record.exit_code = result.exit_code;
        record.tool_calls = result
            .parts
            .iter()
            .filter(|p| matches!(p, MessagePart::Tool { .. }))
            .count();
        if result.status != "complete" {
            agent::forget_startable(Some(&a.id));
            return Err(result.error_summary.unwrap_or_else(|| {
                format!(
                    "Agent stopped: {}",
                    result.termination_reason.unwrap_or(result.status)
                )
            }));
        }
        let text = prompt::extract(&result.content).ok_or("The Agent returned an empty answer")?;
        let refs = source_files::ref_stats(&source_files::page_refs(&text, &cwd));
        let mut warnings = Vec::new();
        if refs.invalid > 0 {
            warnings.push(format!(
                "{} source citation(s) could not be verified",
                refs.invalid
            ));
        }
        if before.is_some() && before != worktree_state(project).await {
            warnings.push("Project Git status or HEAD changed during the call; another program may also have changed it".into());
        }
        if d.project_path.is_some() && record.tool_calls == 0 {
            warnings.push("The Agent used no file tools; check its project claims".into());
        }
        Ok((text, refs, warnings))
    }
}
async fn worktree_state(project: Option<PathBuf>) -> Option<String> {
    let p = project?;
    tokio::task::spawn_blocking(move || crate::projects::worktree_state(&p))
        .await
        .ok()
        .flatten()
}
