//! Serial stage runner for one study. A job runs one or more stages; each
//! stage is recorded in study.json and `jobs/<jobId>/`.
use super::{
    fetch::{self, GitContext},
    generate,
    inventory::{self, Inventory},
    models::*,
    store::{self, Store},
};
use crate::{
    agent_adapter::{self, AdapterInvocationRequest, AgentStage},
    chat::{
        runtime::{self, Limits, Observation},
        ChatMessagePart, STOP_REASON_ABORT, STOP_REASON_TIMEOUT,
    },
    models::{now_ms, AgentConfig},
    process_supervisor::ProcessOwner,
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

pub type Emit = Arc<dyn Fn(LearnEvent) + Send + Sync>;

pub const ARCHITECTURE_TIMEOUT: Duration = Duration::from_secs(30 * 60);
pub const MODULE_TIMEOUT: Duration = Duration::from_secs(15 * 60);
pub const GLOSSARY_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const CANCELLED: &str = "Cancelled";

pub struct Job {
    pub store: Store,
    pub study_id: String,
    pub job_id: String,
    pub cancel: Arc<AtomicBool>,
    /// Agent resolved when the job was requested; `None` for fetch-only work.
    pub agent: Option<AgentConfig>,
    pub emit: Emit,
}

/// Stages a request expands to. `auto_level1` appends level-1 module pages
/// once the architecture exists.
pub fn plan(study: &Study, stage: &str) -> Result<(Vec<String>, bool), String> {
    let modules_missing = |filter: &dyn Fn(&PageInfo) -> bool| {
        study
            .pages
            .iter()
            .filter(|p| p.kind == "module" && !p.ready && filter(p))
            .map(|p| p.page_id.clone())
            .collect::<Vec<_>>()
    };
    Ok(match stage {
        "fetch" => (
            vec!["fetch".into(), "inventory".into(), "architecture".into()],
            true,
        ),
        "inventory" => (vec!["inventory".into(), "architecture".into()], true),
        "architecture" => (vec!["architecture".into()], false),
        "glossary" => (vec!["glossary".into()], false),
        "all" => {
            if !study.stage_done("architecture") {
                return Err("Generate the architecture first".into());
            }
            let mut stages = modules_missing(&|_| true);
            if !study.pages.iter().any(|p| p.kind == "glossary" && p.ready) {
                stages.push("glossary".into());
            }
            if stages.is_empty() {
                return Err("Every page is already generated".into());
            }
            (stages, false)
        }
        _ => {
            let id = stage.strip_prefix("module:").ok_or("Unknown stage")?;
            if !study
                .pages
                .iter()
                .any(|p| p.module_id.as_deref() == Some(id))
            {
                return Err("Unknown module".into());
            }
            (vec![stage.into()], false)
        }
    })
}

impl Job {
    fn event(&self, stage: &str, status: &StageStatus, error: Option<String>) {
        let status = serde_json::to_value(status)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default();
        (self.emit)(LearnEvent {
            study_id: self.study_id.clone(),
            job_id: Some(self.job_id.clone()),
            stage: stage.into(),
            status,
            error,
        });
    }

    fn set_stage(
        &self,
        stage: &str,
        status: StageStatus,
        error: Option<String>,
        warnings: Vec<String>,
    ) -> Result<(), String> {
        let job_id = self.job_id.clone();
        let event_status = status.clone();
        let event_error = error.clone();
        self.store.update(&self.study_id, |study| {
            let state = study.stage_mut(stage);
            if status == StageStatus::Running {
                state.started_at_ms = Some(now_ms());
                state.finished_at_ms = None;
                state.warnings.clear();
            } else {
                state.finished_at_ms = Some(now_ms());
                state.warnings = warnings;
            }
            state.status = status;
            state.job_id = Some(job_id);
            state.error = error;
            Ok(())
        })?;
        self.event(stage, &event_status, event_error);
        Ok(())
    }

    pub async fn run(self, stages: Vec<String>, auto_level1: bool) {
        let mut record = JobRecord {
            job_id: self.job_id.clone(),
            stage: stages.join(","),
            status: StageStatus::Running,
            started_at_ms: now_ms(),
            finished_at_ms: None,
            invocations: Vec::new(),
            model: self
                .agent
                .as_ref()
                .and(self.store.load(&self.study_id).ok().and_then(|s| s.model)),
            tool_calls: 0,
            exit_code: None,
            error: None,
        };
        let _ = self.store.save_job(&self.study_id, &record);
        let mut queue: std::collections::VecDeque<String> = stages.into();
        let mut outcome = StageStatus::Done;
        while let Some(stage) = queue.pop_front() {
            if self
                .set_stage(&stage, StageStatus::Running, None, Vec::new())
                .is_err()
            {
                outcome = StageStatus::Failed;
                break;
            }
            let result = self.run_stage(&stage, &mut record).await;
            let _ = self.store.save_job(&self.study_id, &record);
            match result {
                Ok(warnings) => {
                    let _ = self.set_stage(&stage, StageStatus::Done, None, warnings);
                    if stage == "architecture" && auto_level1 {
                        if let Ok(study) = self.store.load(&self.study_id) {
                            for page in study
                                .pages
                                .iter()
                                .filter(|p| p.level == Some(1) && !p.ready)
                            {
                                queue.push_back(page.page_id.clone());
                            }
                        }
                    }
                }
                Err(error) => {
                    let shutting_down = crate::process_supervisor::supervisor().is_shutting_down();
                    let status = if shutting_down {
                        StageStatus::Interrupted
                    } else if error == CANCELLED || self.cancel.load(Ordering::Relaxed) {
                        StageStatus::Cancelled
                    } else {
                        StageStatus::Failed
                    };
                    let message = (status != StageStatus::Cancelled).then(|| error.clone());
                    let _ = self.set_stage(&stage, status.clone(), message, Vec::new());
                    record.error = Some(error);
                    outcome = status;
                    break;
                }
            }
        }
        record.status = outcome;
        record.finished_at_ms = Some(now_ms());
        let _ = self.store.save_job(&self.study_id, &record);
        store::release_job(&self.study_id, &self.job_id);
        self.event("job", &record.status, record.error.clone());
    }

    async fn run_stage(&self, stage: &str, record: &mut JobRecord) -> Result<Vec<String>, String> {
        match stage {
            "fetch" => self.fetch().await,
            "inventory" => self.inventory().await,
            "architecture" => self.architecture(record).await,
            "glossary" => self.page("glossary", record).await,
            _ if stage.starts_with("module:") => self.page(stage, record).await,
            _ => Err("Unknown stage".into()),
        }
    }

    fn git_context(&self) -> GitContext<'_> {
        GitContext {
            study_id: &self.study_id,
            job_id: &self.job_id,
            cancel: &self.cancel,
        }
    }

    async fn fetch(&self) -> Result<Vec<String>, String> {
        let study = self.store.load(&self.study_id)?;
        let repo = fetch::parse_github_url(&study.url)?;
        let checkout = self.store.checkout(&self.study_id)?;
        let commit = fetch::fetch(
            &self.git_context(),
            &repo,
            study.git_ref.as_deref(),
            &checkout,
        )
        .await?;
        let fingerprint = blocking({
            let checkout = checkout.clone();
            move || fetch::fingerprint(&checkout)
        })
        .await?;
        self.store.update(&self.study_id, |study| {
            study.commit = Some(commit);
            study.checkout_fingerprint = Some(fingerprint);
            Ok(())
        })?;
        Ok(Vec::new())
    }

    async fn inventory(&self) -> Result<Vec<String>, String> {
        let checkout = self.store.checkout(&self.study_id)?;
        let listed = fetch::git(&self.git_context(), &checkout, &["ls-files", "-z"]).await?;
        let built = blocking({
            let checkout = checkout.clone();
            move || Ok(inventory::build(&checkout, &listed))
        })
        .await?;
        self.store
            .write_json(&self.study_id, "inventory.json", &built)?;
        let mut warnings = Vec::new();
        if built.files.is_empty() {
            return Err("No readable text files were found in the repository".into());
        }
        if built.excluded > 0 {
            warnings.push(format!(
                "{} binary, vendored or oversized file(s) excluded",
                built.excluded
            ));
        }
        Ok(warnings)
    }

    async fn verify_checkout(&self, study: &Study) -> Result<(), String> {
        let expected = study
            .checkout_fingerprint
            .clone()
            .ok_or("Fetch the repository first")?;
        let checkout = self.store.checkout(&self.study_id)?;
        let actual = blocking(move || fetch::fingerprint(&checkout)).await?;
        if actual != expected {
            return Err("The checkout changed during generation; the agent did not stay read-only. Fetch again before retrying".into());
        }
        Ok(())
    }

    async fn architecture(&self, record: &mut JobRecord) -> Result<Vec<String>, String> {
        let study = self.store.load(&self.study_id)?;
        let inventory: Inventory = self
            .store
            .read_json(&self.study_id, "inventory.json")?
            .ok_or("Run the inventory first")?;
        let checkout = self.store.checkout(&self.study_id)?;
        let task = generate::architecture_prompt(&study, &inventory::prompt_summary(&inventory));
        let mut prompt = task.clone();
        let mut warnings = Vec::new();
        for attempt in 0..2 {
            let output = self
                .run_agent(&study, &prompt, ARCHITECTURE_TIMEOUT, attempt, record)
                .await?;
            if output.tool_calls == 0 {
                warnings.push("The agent read no files; references may be guessed".into());
            }
            let problems = match generate::extract_json(&output.content) {
                None => vec!["no JSON object was found in the answer".to_string()],
                Some(value) => {
                    let checkout = checkout.clone();
                    match blocking(move || Ok(generate::validate_architecture(value, &checkout)))
                        .await?
                    {
                        Ok(validated) => {
                            warnings.extend(validated.warnings);
                            return self.publish_architecture(
                                &study,
                                validated.architecture,
                                &inventory,
                                warnings,
                            );
                        }
                        Err(problems) => problems,
                    }
                }
            };
            if attempt == 1 {
                return Err(format!(
                    "Architecture output was invalid after one repair: {}",
                    problems.join("; ")
                ));
            }
            prompt = generate::repair_prompt(&task, &output.content, &problems);
        }
        unreachable!()
    }

    fn publish_architecture(
        &self,
        study: &Study,
        architecture: Architecture,
        inventory: &Inventory,
        mut warnings: Vec<String>,
    ) -> Result<Vec<String>, String> {
        let checkout = self.store.checkout(&self.study_id)?;
        let coverage = generate::coverage(&architecture, inventory);
        if coverage.source_files > 0 && coverage.covered * 5 < coverage.source_files * 4 {
            warnings.push(format!(
                "Modules cover {} of {} source files (below 80%)",
                coverage.covered, coverage.source_files
            ));
        }
        let mut pages = generate::page_plan(&architecture, &study.language);
        for (page_id, markdown) in [
            (
                "overview",
                generate::render_overview(&architecture, &study.language),
            ),
            (
                "architecture",
                generate::render_architecture(&architecture, &study.language),
            ),
        ] {
            self.store.write_page(&self.study_id, page_id, &markdown)?;
            let refs = generate::page_refs(&markdown, &checkout);
            if let Some(page) = pages.iter_mut().find(|p| p.page_id == page_id) {
                page.ready = true;
                page.refs = Some(generate::ref_stats(&refs));
            }
        }
        // Keep pages generated earlier for modules that still exist.
        for page in pages
            .iter_mut()
            .filter(|p| p.kind == "module" || p.kind == "glossary")
        {
            if let Some(old) = study
                .pages
                .iter()
                .find(|old| old.page_id == page.page_id && old.ready)
            {
                if self
                    .store
                    .read_page(&self.study_id, &page.page_id)?
                    .is_some()
                {
                    page.ready = true;
                    page.refs = old.refs.clone();
                }
            }
        }
        self.store
            .write_architecture(&self.study_id, &architecture)?;
        self.store.update(&self.study_id, |study| {
            study.pages = pages;
            study.coverage = Some(coverage);
            Ok(())
        })?;
        Ok(warnings)
    }

    async fn page(&self, page_id: &str, record: &mut JobRecord) -> Result<Vec<String>, String> {
        let study = self.store.load(&self.study_id)?;
        let architecture = self
            .store
            .architecture(&self.study_id)?
            .ok_or("Generate the architecture first")?;
        let checkout = self.store.checkout(&self.study_id)?;
        let (task, timeout, min_valid) = if page_id == "glossary" {
            (
                generate::glossary_prompt(&study, &architecture),
                GLOSSARY_TIMEOUT,
                1,
            )
        } else {
            let id = page_id.strip_prefix("module:").unwrap_or_default();
            let module = architecture
                .modules
                .iter()
                .find(|m| m.id == id)
                .ok_or("Unknown module")?;
            (
                generate::module_prompt(&study, &architecture, module),
                MODULE_TIMEOUT,
                3,
            )
        };
        let mut prompt = task.clone();
        let mut warnings = Vec::new();
        let mut best: Option<(String, Vec<SourceRef>)> = None;
        for attempt in 0..2 {
            let output = self
                .run_agent(&study, &prompt, timeout, attempt, record)
                .await?;
            if output.tool_calls == 0 && attempt == 0 {
                warnings.push("The agent read no files; references may be guessed".into());
            }
            let Some(markdown) = generate::extract_page(&output.content) else {
                if attempt == 1 {
                    break;
                }
                prompt = generate::repair_prompt(
                    &task,
                    &output.content,
                    &["no page was found between the required markers".into()],
                );
                continue;
            };
            let refs = generate::page_refs(&markdown, &checkout);
            let problems = generate::page_problems(&markdown, &refs, min_valid);
            let better = best.as_ref().is_none_or(|(_, old)| {
                generate::ref_stats(&refs).valid >= generate::ref_stats(old).valid
            });
            if better {
                best = Some((markdown.clone(), refs));
            }
            if problems.is_empty() || attempt == 1 {
                if !problems.is_empty() {
                    warnings.push(format!(
                        "Accepted after one repair with issues: {}",
                        problems.join("; ")
                    ));
                }
                break;
            }
            prompt = generate::repair_prompt(&task, &markdown, &problems);
        }
        let (markdown, refs) = best.ok_or("The agent did not produce a page")?;
        self.store.write_page(&self.study_id, page_id, &markdown)?;
        let stats = generate::ref_stats(&refs);
        self.store.update(&self.study_id, |study| {
            if let Some(page) = study.pages.iter_mut().find(|p| p.page_id == page_id) {
                page.ready = true;
                page.refs = Some(stats);
            }
            Ok(())
        })?;
        Ok(warnings)
    }

    async fn run_agent(
        &self,
        study: &Study,
        prompt: &str,
        timeout: Duration,
        attempt: usize,
        record: &mut JobRecord,
    ) -> Result<AgentOutput, String> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(CANCELLED.into());
        }
        let agent = self.agent.as_ref().ok_or("No agent selected")?;
        self.verify_checkout(study).await?;
        let checkout = self.store.checkout(&self.study_id)?;
        let job_dir = self.store.job_dir(&self.study_id, &self.job_id)?;
        let separator = if attempt == 0 {
            ""
        } else {
            "\n\n---- repair attempt ----\n\n"
        };
        append(&job_dir.join("prompt.md"), &format!("{separator}{prompt}"));
        let request = AdapterInvocationRequest {
            project_path: &checkout,
            prompt,
            prompt_file: None,
            stage: AgentStage::Planning,
            resume_command: None,
            chat_permission_mode: Some("explore"),
            // Codex and Claude read stdin; Grok and OpenCode take the prompt as an argument.
            embed_prompt: !matches!(
                agent.adapter_type.as_str(),
                agent_adapter::ADAPTER_CODEX | agent_adapter::ADAPTER_CLAUDE
            ),
            system_append: None,
            model: study.model.as_deref(),
        };
        let prepared = agent_adapter::prepare_invocation(agent, &request)?;
        record
            .invocations
            .push(describe(&prepared.program, &prepared.args));
        let run_id = format!("{}-a{attempt}", self.job_id);
        let stdin_text = prepared.stdin_prompt.then(|| prompt.to_string());
        let request = runtime::Request {
            prepared,
            project_key: checkout.to_string_lossy().into_owned(),
            session_id: self.study_id.clone(),
            turn_id: run_id.clone(),
            message_id: run_id,
            stdin_text,
            limits: Limits {
                timeout,
                max_bytes: 64 * 1024 * 1024,
                max_line_bytes: 4 * 1024 * 1024,
                max_lines: 200_000,
                ..Limits::default()
            },
        };
        let cancel = self.cancel.clone();
        let store = self.store.clone();
        let (study_id, job_id) = (self.study_id.clone(), self.job_id.clone());
        let outcome = runtime::run_owned(
            request,
            Some(ProcessOwner::Learn {
                study_id: self.study_id.clone(),
                job_id: self.job_id.clone(),
            }),
            move || cancel.load(Ordering::Relaxed).then_some(STOP_REASON_ABORT),
            |_| Ok(()),
            move |observation| {
                if let Observation::Log { stderr, line } = observation {
                    store.append_log(&study_id, &job_id, stderr, &line);
                }
                Ok(())
            },
        )
        .await?;
        record.exit_code = outcome.exit_code;
        let tool_calls = outcome
            .parts
            .iter()
            .filter(|p| matches!(p, ChatMessagePart::Tool { .. }))
            .count();
        record.tool_calls += tool_calls;
        let result = match outcome.status.as_str() {
            "complete" => Ok(AgentOutput {
                content: outcome.content,
                tool_calls,
            }),
            "aborted" if outcome.termination_reason.as_deref() == Some(STOP_REASON_TIMEOUT) => Err(format!(
                "The agent timed out after {} minutes; retry or choose a faster agent or a smaller depth",
                timeout.as_secs() / 60
            )),
            "aborted" if self.cancel.load(Ordering::Relaxed) => Err(CANCELLED.into()),
            "aborted" => Err("The agent was stopped".into()),
            _ => Err(crate::agents::redact_sensitive_text(
                outcome.error_summary.as_deref().unwrap_or("The agent failed"),
            )),
        };
        self.verify_checkout(study).await?;
        result
    }
}

struct AgentOutput {
    content: String,
    tool_calls: usize,
}

/// Redacted command line for the job record; long arguments (prompts) are elided.
fn describe(program: &str, args: &[String]) -> String {
    let args: Vec<String> = args
        .iter()
        .map(|arg| {
            if arg.chars().count() > 160 {
                format!("<{} chars>", arg.chars().count())
            } else {
                crate::agents::redact_sensitive_text(arg)
            }
        })
        .collect();
    format!("{program} {}", args.join(" "))
}

fn append(path: &std::path::Path, text: &str) {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = file.write_all(text.as_bytes());
    }
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|_| "Learn worker interrupted".to_string())?
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt, sync::Mutex};

    fn fake_agent(root: &std::path::Path, script: &str) -> AgentConfig {
        let path = root.join("fake-codex");
        fs::write(&path, format!("#!/bin/sh\ncat >/dev/null\n{script}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        AgentConfig {
            id: "agent-fake".into(),
            name: "Fake".into(),
            command: path.to_string_lossy().into_owned(),
            args: Vec::new(),
            working_directory_policy: "project_root".into(),
            capabilities: vec!["planning".into()],
            adapter_type: agent_adapter::ADAPTER_CODEX.into(),
            can_write_files: false,
            can_run_commands: false,
            enabled: true,
            available: true,
        }
    }

    fn codex_message(text: &str) -> String {
        let line = serde_json::json!({"type": "item.completed", "item": {"type": "agent_message", "text": text}});
        format!(
            "printf '%s\\n' '{}'",
            line.to_string().replace('\'', "'\\''")
        )
    }

    fn setup(id: &str, script: &str) -> (std::path::PathBuf, Job, Arc<Mutex<Vec<LearnEvent>>>) {
        let root = super::super::store::tests::temp_root();
        let store = Store::new(&root);
        let mut study = super::super::store::tests::sample_study(id);
        let checkout_src = super::super::generate::tests::fixture_checkout();
        let checkout = store.dir(id).unwrap().join("checkout");
        store.create(&study).unwrap();
        fs::rename(&checkout_src, &checkout).unwrap();
        let inventory = inventory::build(&checkout, "src/main.rs\0src/core/mod.rs\0");
        store.write_json(id, "inventory.json", &inventory).unwrap();
        study.checkout_fingerprint = Some(fetch::fingerprint(&checkout).unwrap());
        study.stage_mut("fetch").status = StageStatus::Done;
        store
            .update(id, |s| {
                *s = study.clone();
                Ok(())
            })
            .unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = events.clone();
        let job_id = store::next_id("job");
        let cancel = store::claim_job(id, &job_id).unwrap();
        let job = Job {
            store,
            study_id: id.into(),
            job_id,
            cancel,
            agent: Some(fake_agent(&root, script)),
            emit: Arc::new(move |event| sink.lock().unwrap().push(event)),
        };
        (root, job, events)
    }

    #[tokio::test]
    async fn architecture_then_level1_module_pages_are_generated_and_validated() {
        let architecture = format!(
            "```json\n{}\n```",
            super::super::generate::tests::architecture_json()
        );
        let page = "<<<LOOM_PAGE\n# CLI\n[a](loom-src:src/main.rs#L1-L3) [b](loom-src:src/core/mod.rs#L1-L9) [c](loom-src:src/core/mod.rs#L10-L20)\nLOOM_PAGE>>>";
        // The fake CLI answers the architecture prompt first, then module pages.
        let script = format!(
            "if [ -f \"$0.done\" ]; then {}; else touch \"$0.done\"; {}; fi",
            codex_message(page),
            codex_message(&architecture)
        );
        let (root, job, events) = setup("study-ja", &script);
        let job_study = String::from("study-ja");
        let store = job.store.clone();
        job.run(vec!["architecture".into()], true).await;
        let study = store.load(&job_study).unwrap();
        assert!(study.stage_done("architecture"), "{:?}", study.stages);
        assert!(study.stage_done("module:cli"), "{:?}", study.stages);
        assert!(!study.stage_done("module:core"), "level 2 is on demand");
        let cli = study
            .pages
            .iter()
            .find(|p| p.page_id == "module:cli")
            .unwrap();
        assert_eq!(
            cli.refs,
            Some(RefStats {
                valid: 3,
                invalid: 0
            })
        );
        assert!(store
            .read_page(&job_study, "overview")
            .unwrap()
            .unwrap()
            .contains("loom-src:src/main.rs"));
        assert_eq!(study.coverage.as_ref().unwrap().covered, 2);
        let events = events.lock().unwrap();
        assert!(events
            .iter()
            .any(|e| e.stage == "job" && e.status == "done"));
        assert!(store::running_job(&job_study).is_none());
        let logs = fs::read_dir(store.dir(&job_study).unwrap().join("jobs"))
            .unwrap()
            .count();
        assert_eq!(logs, 1);
        fs::remove_dir_all(root).unwrap();
    }

    /// Characterization for the agent layer move (P1): the event order, job
    /// record and log files a successful job leaves behind.
    #[tokio::test]
    async fn job_events_record_and_logs_are_stable() {
        let architecture = format!(
            "```json\n{}\n```",
            super::super::generate::tests::architecture_json()
        );
        let page = "<<<LOOM_PAGE\n# CLI\n[a](loom-src:src/main.rs#L1-L3) [b](loom-src:src/core/mod.rs#L1-L9) [c](loom-src:src/core/mod.rs#L10-L20)\nLOOM_PAGE>>>";
        let script = format!(
            "echo 'warming up' >&2; if [ -f \"$0.done\" ]; then {}; else touch \"$0.done\"; {}; fi",
            codex_message(page),
            codex_message(&architecture)
        );
        let (root, job, events) = setup("study-jk", &script);
        let (store, job_id) = (job.store.clone(), job.job_id.clone());
        job.run(vec!["architecture".into()], true).await;
        let events = events
            .lock()
            .unwrap()
            .iter()
            .map(|e| format!("{}:{}", e.stage, e.status))
            .collect::<Vec<_>>();
        assert_eq!(
            events,
            [
                "architecture:running",
                "architecture:done",
                "module:cli:running",
                "module:cli:done",
                "job:done"
            ]
        );
        let job_dir = store.job_dir("study-jk", &job_id).unwrap();
        let mut files = fs::read_dir(&job_dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        files.sort();
        assert_eq!(files, ["job.json", "prompt.md", "stderr.log", "stdout.log"]);
        let record: JobRecord =
            serde_json::from_str(&fs::read_to_string(job_dir.join("job.json")).unwrap()).unwrap();
        // The record keeps the first stage of the job; every agent attempt is listed.
        assert_eq!(record.stage, "architecture");
        assert_eq!(record.status, StageStatus::Done);
        assert_eq!((record.exit_code, record.tool_calls), (Some(0), 0));
        assert!(record.error.is_none() && record.model.is_none());
        let checkout = store.checkout("study-jk").unwrap();
        let expected = format!(
            "{} exec --json --skip-git-repo-check --cd {} --sandbox read-only --config approval_policy=\"never\" -- -",
            root.join("fake-codex").display(),
            checkout.display()
        );
        assert_eq!(record.invocations, [expected.clone(), expected]);
        let read = |name: &str| fs::read_to_string(job_dir.join(name)).unwrap();
        assert_eq!(read("stderr.log"), "warming up\nwarming up\n");
        let stdout = read("stdout.log");
        assert_eq!(stdout.lines().count(), 2);
        assert!(stdout.lines().all(|line| line.contains("agent_message")));
        let prompt = read("prompt.md");
        assert!(!prompt.contains("repair attempt"));
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn invalid_json_gets_one_repair_then_fails_and_writes_are_detected() {
        let (root, job, _) = setup("study-jb", &codex_message("no json at all"));
        let job_study = String::from("study-jb");
        let store = job.store.clone();
        job.run(vec!["architecture".into()], false).await;
        let study = store.load(&job_study).unwrap();
        let state = study.stage("architecture").unwrap();
        assert_eq!(state.status, StageStatus::Failed);
        assert!(state.error.as_deref().unwrap().contains("after one repair"));
        let prompt = fs::read_to_string(
            store
                .dir(&job_study)
                .unwrap()
                .join("jobs")
                .read_dir()
                .unwrap()
                .next()
                .unwrap()
                .unwrap()
                .path()
                .join("prompt.md"),
        )
        .unwrap();
        assert!(prompt.contains("repair attempt"));
        fs::remove_dir_all(&root).unwrap();

        let (root, job, _) = setup("study-jc", "echo leaked > src/new.rs");
        let job_study = String::from("study-jc");
        let store = job.store.clone();
        job.run(vec!["architecture".into()], false).await;
        let state = store
            .load(&job_study)
            .unwrap()
            .stage("architecture")
            .unwrap()
            .clone();
        assert_eq!(state.status, StageStatus::Failed);
        assert!(state.error.unwrap().contains("read-only"));
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn cancel_stops_the_agent_and_marks_the_stage_cancelled() {
        let (root, job, _) = setup("study-jd", "sleep 30");
        let job_study = String::from("study-jd");
        let store = job.store.clone();
        let cancel = job.cancel.clone();
        let handle = tokio::spawn(job.run(vec!["architecture".into()], false));
        tokio::time::sleep(Duration::from_millis(300)).await;
        cancel.store(true, Ordering::Relaxed);
        tokio::time::timeout(Duration::from_secs(10), handle)
            .await
            .unwrap()
            .unwrap();
        let state = store
            .load(&job_study)
            .unwrap()
            .stage("architecture")
            .unwrap()
            .clone();
        assert_eq!(state.status, StageStatus::Cancelled);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn plans_expand_requests() {
        let mut study = super::super::store::tests::sample_study("study-x");
        assert_eq!(plan(&study, "fetch").unwrap().0.len(), 3);
        assert!(plan(&study, "all").is_err());
        assert!(plan(&study, "module:nope").is_err());
        study.stage_mut("architecture").status = StageStatus::Done;
        study.pages = super::super::generate::page_plan(
            &serde_json::from_value(super::super::generate::tests::architecture_json()).unwrap(),
            "en",
        );
        assert_eq!(
            plan(&study, "all").unwrap().0,
            ["module:cli", "module:core", "glossary"]
        );
        assert_eq!(plan(&study, "module:core").unwrap().0, ["module:core"]);
        assert!(describe("codex", &["x".repeat(200)]).contains("<200 chars>"));
    }
}
