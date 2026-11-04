//! One job answers one question (turn). The agent runs read-only in the project
//! or in an empty Loom-managed directory; the report, its citations and the job
//! logs are written under the research directory only.
use super::{
    models::*,
    prompt::{self, PastTurn},
    store::{self, Store},
};
use crate::{
    agent::{
        self, AdapterInvocationRequest, AgentStage, Limits, MessagePart, Observation, RunSpec,
        STOP_REASON_ABORT, STOP_REASON_TIMEOUT,
    },
    agent::{ProcessKind, ProcessOwner},
    models::{now_ms, AgentConfig},
    source_files,
};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

pub type Emit = Arc<dyn Fn(ResearchEvent) + Send + Sync>;

pub const TURN_TIMEOUT: Duration = Duration::from_secs(20 * 60);
const CANCELLED: &str = "Cancelled";

pub struct Job {
    pub store: Store,
    pub research_id: String,
    pub job_id: String,
    /// 1-based index of the turn this job answers; the turn is already `running`.
    pub turn: u32,
    pub cancel: Arc<AtomicBool>,
    pub agent: AgentConfig,
    pub emit: Emit,
}

struct Answer {
    report: String,
    refs: RefStats,
    warnings: Vec<String>,
    context: Option<String>,
    resume_command: Option<String>,
}

struct AgentOutput {
    content: String,
    tool_calls: usize,
    resume_command: Option<String>,
}

impl Job {
    fn event(&self, status: &TurnStatus, error: Option<String>) {
        let status = serde_json::to_value(status)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default();
        (self.emit)(ResearchEvent {
            research_id: self.research_id.clone(),
            job_id: Some(self.job_id.clone()),
            turn: Some(self.turn),
            status,
            error,
        });
    }

    pub async fn run(self) {
        let mut record = JobRecord {
            job_id: self.job_id.clone(),
            turn: self.turn,
            status: TurnStatus::Running,
            started_at_ms: now_ms(),
            finished_at_ms: None,
            invocations: Vec::new(),
            model: None,
            tool_calls: 0,
            exit_code: None,
            error: None,
        };
        let _ = self.store.save_job(&self.research_id, &record);
        self.event(&TurnStatus::Running, None);
        let result = self.answer(&mut record).await;
        let (status, error) = match result {
            Ok(answer) => match self.publish(answer) {
                Ok(()) => (TurnStatus::Done, None),
                Err(error) => (TurnStatus::Failed, Some(error)),
            },
            Err(error) => {
                let status = if crate::agent::is_shutting_down() {
                    TurnStatus::Interrupted
                } else if error == CANCELLED || self.cancel.load(Ordering::Relaxed) {
                    TurnStatus::Cancelled
                } else {
                    TurnStatus::Failed
                };
                (status, Some(error))
            }
        };
        if status != TurnStatus::Done {
            let message = error.clone().filter(|_| status != TurnStatus::Cancelled);
            let turn = self.turn;
            let finished = status.clone();
            let _ = self.store.update(&self.research_id, |research| {
                if let Some(state) = research.turns.iter_mut().find(|t| t.index == turn) {
                    state.status = finished;
                    state.error = message;
                    state.finished_at_ms = Some(now_ms());
                }
                Ok(())
            });
        }
        record.status = status.clone();
        record.error = error.clone();
        record.finished_at_ms = Some(now_ms());
        let _ = self.store.save_job(&self.research_id, &record);
        store::release_job(&self.store, &self.research_id, &self.job_id);
        self.event(&status, error.filter(|_| status != TurnStatus::Cancelled));
    }

    fn publish(&self, answer: Answer) -> Result<(), String> {
        self.store
            .write_turn(&self.research_id, self.turn, &answer.report)?;
        let (turn, command) = (self.turn, self.agent.command.clone());
        self.store.update(&self.research_id, |research| {
            let state = research
                .turns
                .iter_mut()
                .find(|t| t.index == turn)
                .ok_or("Turn not found")?;
            state.status = TurnStatus::Done;
            state.error = None;
            state.finished_at_ms = Some(now_ms());
            state.warnings = answer.warnings;
            state.refs = Some(answer.refs);
            state.context = answer.context;
            research.resume_agent_command = answer.resume_command.as_ref().map(|_| command);
            research.resume_command = answer.resume_command;
            Ok(())
        })?;
        Ok(())
    }

    fn cwd(&self, research: &Research) -> Result<PathBuf, String> {
        match &research.project_path {
            Some(path) => {
                let path = PathBuf::from(path);
                if !path.is_dir() {
                    return Err("The project folder is no longer available".into());
                }
                Ok(path)
            }
            None => self.store.workspace(&self.research_id),
        }
    }

    async fn answer(&self, record: &mut JobRecord) -> Result<Answer, String> {
        let research = self.store.load(&self.research_id)?;
        record.model = research.model.clone();
        let question = research
            .turns
            .iter()
            .find(|t| t.index == self.turn)
            .map(|t| t.question.clone())
            .ok_or("Turn not found")?;
        let cwd = self.cwd(&research)?;
        let project = research.project_path.as_ref().map(PathBuf::from);
        let before = worktree_state(project.clone()).await;

        let mut warnings = Vec::new();
        let earlier: Vec<&Turn> = research
            .turns
            .iter()
            .filter(|t| t.index < self.turn && t.status == TurnStatus::Done)
            .collect();
        // Native resume only continues the session of the turn right before this one.
        let resume = research
            .resume_command
            .clone()
            .filter(|_| research.resume_agent_command.as_deref() == Some(&self.agent.command))
            .filter(|_| earlier.last().is_some_and(|t| t.index + 1 == self.turn));
        let (output, context) = if earlier.is_empty() {
            let prompt = prompt::report_prompt(&research, &question);
            (
                self.run_agent(&research, &cwd, &prompt, None, 0, record)
                    .await?,
                None,
            )
        } else {
            let native = match &resume {
                Some(command) => {
                    let prompt = prompt::followup_prompt(&research, &question, &[]);
                    match self
                        .run_agent(&research, &cwd, &prompt, Some(command), 0, record)
                        .await
                    {
                        Ok(output) => Some(output),
                        Err(error) if self.stopped(&error) => return Err(error),
                        Err(error) => {
                            warnings.push(format!(
                                "Resuming the agent session failed ({error}); earlier questions were replayed instead"
                            ));
                            None
                        }
                    }
                }
                None => None,
            };
            match native {
                Some(output) => (output, Some("native".to_string())),
                None => {
                    let history = self.history(&earlier)?;
                    let prompt = prompt::followup_prompt(&research, &question, &history);
                    let attempt = usize::from(resume.is_some());
                    let output = self
                        .run_agent(&research, &cwd, &prompt, None, attempt, record)
                        .await?;
                    (output, Some("replay".to_string()))
                }
            }
        };

        if before.is_some() && worktree_state(project.clone()).await != before {
            warnings.push(
                "The project's Git status or HEAD changed while the agent ran; it may not have stayed read-only, or another program changed the project".into(),
            );
        }
        let report =
            prompt::extract_report(&output.content).ok_or("The agent returned an empty report")?;
        let refs = {
            let (report, cwd) = (report.clone(), cwd.clone());
            blocking(move || Ok(source_files::page_refs(&report, &cwd))).await?
        };
        let stats = source_files::ref_stats(&refs);
        if stats.invalid > 0 {
            warnings.push(format!(
                "{} source citation(s) could not be verified",
                stats.invalid
            ));
        }
        if project.is_some() && output.tool_calls == 0 {
            warnings.push("The agent read no project files; citations may be guessed".into());
        }
        Ok(Answer {
            report,
            refs: stats,
            warnings,
            context,
            resume_command: output.resume_command,
        })
    }

    fn stopped(&self, error: &str) -> bool {
        error == CANCELLED
            || self.cancel.load(Ordering::Relaxed)
            || crate::agent::is_shutting_down()
    }

    fn history(&self, earlier: &[&Turn]) -> Result<Vec<PastTurn>, String> {
        earlier
            .iter()
            .map(|turn| {
                Ok(PastTurn {
                    question: turn.question.clone(),
                    report: self
                        .store
                        .read_turn(&self.research_id, turn.index)?
                        .unwrap_or_default(),
                })
            })
            .collect()
    }

    async fn run_agent(
        &self,
        research: &Research,
        cwd: &Path,
        prompt: &str,
        resume_command: Option<&str>,
        attempt: usize,
        record: &mut JobRecord,
    ) -> Result<AgentOutput, String> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(CANCELLED.into());
        }
        let agent = &self.agent;
        let separator = if attempt == 0 {
            ""
        } else {
            "\n\n---- replayed history attempt ----\n\n"
        };
        self.store.append_prompt(
            &self.research_id,
            &self.job_id,
            &format!("{separator}{prompt}"),
        );
        let request = AdapterInvocationRequest {
            execution_profile: None,
            project_path: cwd,
            prompt,
            prompt_file: None,
            stage: AgentStage::Planning,
            resume_command,
            chat_permission_mode: Some("explore"),
            // Codex and Claude read stdin; Grok and OpenCode take the prompt as an argument.
            embed_prompt: !matches!(
                agent.adapter_type.as_str(),
                agent::ADAPTER_CODEX | agent::ADAPTER_CLAUDE
            ),
            system_append: None,
            model: research.model.as_deref(),
            needs_tools: true,
        };
        let prepared = agent::prepare_invocation(agent, &request)?;
        record
            .invocations
            .push(describe(&prepared.program, &prepared.args));
        let stdin_text = prepared.stdin_prompt.then(|| prompt.to_string());
        let spec = RunSpec {
            prepared,
            run_id: format!("{}-a{attempt}", self.job_id),
            owner: ProcessOwner::Feature {
                feature: "research".into(),
                scope_id: self.research_id.clone(),
                run_id: self.job_id.clone(),
            },
            kind: ProcessKind::Feature,
            stdin_text,
            limits: Limits {
                timeout: TURN_TIMEOUT,
                max_bytes: 64 * 1024 * 1024,
                max_line_bytes: 4 * 1024 * 1024,
                max_lines: 200_000,
                ..Limits::default()
            },
            enrich: None,
        };
        let cancel = self.cancel.clone();
        let store = self.store.clone();
        let (research_id, job_id) = (self.research_id.clone(), self.job_id.clone());
        let cancel_store = self.store.clone();
        let cancel_research_id = self.research_id.clone();
        let cancel_job_id = self.job_id.clone();
        let outcome = agent::run(
            spec,
            move || {
                if cancel_store.is_cancelled(&cancel_research_id, &cancel_job_id) {
                    cancel.store(true, Ordering::Relaxed);
                }
                cancel.load(Ordering::Relaxed).then_some(STOP_REASON_ABORT)
            },
            |_| Ok(()),
            move |observation| {
                if let Observation::Log { stderr, line } = observation {
                    store.append_log(&research_id, &job_id, stderr, &line);
                }
                Ok(())
            },
        )
        .await?;
        record.exit_code = outcome.exit_code;
        let tool_calls = outcome
            .parts
            .iter()
            .filter(|p| matches!(p, MessagePart::Tool { .. }))
            .count();
        record.tool_calls += tool_calls;
        match outcome.status.as_str() {
            "complete" => Ok(AgentOutput {
                content: outcome.content,
                tool_calls,
                resume_command: outcome.resume_command,
            }),
            "aborted" if outcome.termination_reason.as_deref() == Some(STOP_REASON_TIMEOUT) => {
                Err(format!(
                    "The agent timed out after {} minutes; retry or ask a narrower question",
                    TURN_TIMEOUT.as_secs() / 60
                ))
            }
            "aborted" if self.cancel.load(Ordering::Relaxed) => Err(CANCELLED.into()),
            "aborted" => Err("The agent was stopped".into()),
            _ => {
                agent::forget_startable(Some(&agent.id));
                Err(agent::redact_sensitive_text(
                    outcome
                        .error_summary
                        .as_deref()
                        .unwrap_or("The agent failed"),
                ))
            }
        }
    }
}

/// Redacted command line for the job record; long arguments (prompts) are elided.
fn describe(program: &str, args: &[String]) -> String {
    let args: Vec<String> = args
        .iter()
        .map(|arg| {
            if arg.chars().count() > 160 {
                format!("<{} chars>", arg.chars().count())
            } else {
                agent::redact_sensitive_text(arg)
            }
        })
        .collect();
    format!("{program} {}", args.join(" "))
}

async fn worktree_state(project: Option<PathBuf>) -> Option<String> {
    let project = project?;
    blocking(move || Ok(crate::projects::worktree_state(&project)))
        .await
        .ok()
        .flatten()
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|_| "Research worker interrupted".to_string())?
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::research::store::tests::{sample_research, temp_root, turn};
    use std::{fs, os::unix::fs::PermissionsExt, process::Command, sync::Mutex};

    fn fake_agent(root: &Path, script: &str) -> AgentConfig {
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
            adapter_type: agent::ADAPTER_CODEX.into(),
            can_write_files: false,
            can_run_commands: false,
            enabled: true,
            available: true,
            provider_id: None,
        }
    }

    fn codex_lines(thread: &str, text: &str) -> String {
        let started = serde_json::json!({"type": "thread.started", "thread_id": thread});
        let message = serde_json::json!({"type": "item.completed", "item": {"type": "agent_message", "text": text}});
        let quote = |v: serde_json::Value| v.to_string().replace('\'', "'\\''");
        format!("printf '%s\\n' '{}' '{}'", quote(started), quote(message))
    }

    /// A Git project with two source files, registered nowhere (jobs do not check).
    fn project() -> PathBuf {
        let root = temp_root();
        fs::create_dir_all(root.join("src/core")).unwrap();
        fs::write(
            root.join("src/main.rs"),
            "fn main() {\n    core::run();\n}\n",
        )
        .unwrap();
        fs::write(
            root.join("src/core/mod.rs"),
            (1..=50).map(|i| format!("// {i}\n")).collect::<String>(),
        )
        .unwrap();
        let git = |args: &[&str]| {
            assert!(Command::new("git")
                .arg("-C")
                .arg(&root)
                .args(args)
                .output()
                .unwrap()
                .status
                .success());
        };
        git(&["init", "-q"]);
        git(&["add", "."]);
        git(&[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.invalid",
            "commit",
            "-qm",
            "init",
        ]);
        root
    }

    struct Setup {
        root: PathBuf,
        store: Store,
        agent: AgentConfig,
        events: Arc<Mutex<Vec<ResearchEvent>>>,
    }

    impl Setup {
        fn new(id: &str, project: Option<&Path>, script: &str) -> Self {
            let root = temp_root();
            let store = Store::new(&root);
            let mut research = sample_research(id);
            if let Some(project) = project {
                research.project_path = Some(project.to_string_lossy().into_owned());
                research.project_name = Some("demo".into());
            }
            store.create(&research).unwrap();
            Self {
                agent: fake_agent(&root, script),
                root,
                store,
                events: Arc::new(Mutex::new(Vec::new())),
            }
        }

        /// Adds a running turn with `question` and returns its job.
        fn ask(&self, id: &str, question: &str) -> Job {
            let job_id = store::next_id("job");
            let (_, index) = self
                .store
                .update(id, |research| {
                    let index = research.turns.len() as u32 + 1;
                    let mut state = turn(index, TurnStatus::Running, &job_id);
                    state.question = question.into();
                    research.turns.push(state);
                    Ok(index)
                })
                .unwrap();
            let cancel = store::claim_job(&self.store, id, &job_id).unwrap();
            let sink = self.events.clone();
            Job {
                store: self.store.clone(),
                research_id: id.into(),
                job_id,
                turn: index,
                cancel,
                agent: self.agent.clone(),
                emit: Arc::new(move |event| sink.lock().unwrap().push(event)),
            }
        }
    }

    const REPORT: &str = "<<<LOOM_REPORT\n# Entry\n## Conclusion\nIt starts in [main](loom-src:src/main.rs#L1-L3), see [core](loom-src:src/core/mod.rs#L1-L9) and [gone](loom-src:src/gone.rs#L1-L2).\nLOOM_REPORT>>>";

    #[tokio::test]
    async fn first_question_writes_a_report_with_checked_citations() {
        let project = project();
        let setup = Setup::new(
            "research-ja",
            Some(&project),
            &codex_lines("thread-a", REPORT),
        );
        let job = setup.ask("research-ja", "Where does it start?");
        let job_id = job.job_id.clone();
        job.run().await;
        let research = setup.store.load("research-ja").unwrap();
        let state = &research.turns[0];
        assert_eq!(state.status, TurnStatus::Done, "{:?}", state.error);
        assert_eq!(
            state.refs,
            Some(RefStats {
                valid: 2,
                invalid: 1
            })
        );
        assert!(state
            .warnings
            .iter()
            .any(|w| w.contains("1 source citation")));
        assert!(state
            .warnings
            .iter()
            .any(|w| w.contains("read no project files")));
        assert!(!state.warnings.iter().any(|w| w.contains("Git status")));
        assert!(state.context.is_none());
        let report = setup.store.read_turn("research-ja", 1).unwrap().unwrap();
        assert!(report.starts_with("# Entry") && !report.contains("LOOM_REPORT"));
        assert_eq!(
            research.resume_command.as_deref(),
            Some(format!("{} resume thread-a", setup.agent.command).as_str())
        );
        let events: Vec<String> = setup
            .events
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.status.clone())
            .collect();
        assert_eq!(events, ["running", "done"]);
        let job_dir = setup.store.job_dir("research-ja", &job_id).unwrap();
        let record: JobRecord =
            serde_json::from_str(&fs::read_to_string(job_dir.join("job.json")).unwrap()).unwrap();
        assert_eq!(record.status, TurnStatus::Done);
        assert!(record.invocations[0].contains("--sandbox read-only"));
        assert!(record.invocations[0].contains(&project.display().to_string()));
        assert!(fs::read_to_string(job_dir.join("prompt.md"))
            .unwrap()
            .contains("Where does it start?"));
        assert!(store::running_job(&setup.store, "research-ja").is_none());
        // Nothing was written into the project.
        let state = crate::projects::worktree_state(&project).unwrap();
        assert!(state.lines().skip(1).all(str::is_empty), "{state}");
        assert!(!project.join("turns").exists());
        fs::remove_dir_all(&setup.root).unwrap();
        fs::remove_dir_all(project).unwrap();
    }

    #[tokio::test]
    async fn without_a_project_the_agent_runs_in_the_research_workspace() {
        let setup = Setup::new(
            "research-jw",
            None,
            &codex_lines(
                "thread-w",
                "# Plain\nNo markers, [web](https://example.com).",
            ),
        );
        let job = setup.ask("research-jw", "What is X?");
        let job_id = job.job_id.clone();
        job.run().await;
        let research = setup.store.load("research-jw").unwrap();
        assert_eq!(research.turns[0].status, TurnStatus::Done);
        assert_eq!(research.turns[0].refs, Some(RefStats::default()));
        assert!(
            research.turns[0].warnings.is_empty(),
            "{:?}",
            research.turns[0].warnings
        );
        let workspace = setup.store.workspace("research-jw").unwrap();
        let record = fs::read_to_string(
            setup
                .store
                .job_dir("research-jw", &job_id)
                .unwrap()
                .join("job.json"),
        )
        .unwrap();
        assert!(record.contains(&workspace.display().to_string()));
        assert!(fs::read_to_string(
            setup
                .store
                .job_dir("research-jw", &job_id)
                .unwrap()
                .join("prompt.md")
        )
        .unwrap()
        .contains("do not use `loom-src:`"));
        fs::remove_dir_all(&setup.root).unwrap();
    }

    #[tokio::test]
    async fn follow_up_resumes_natively_then_replays_when_resume_fails() {
        // Fails whenever invoked with `resume` once the marker file exists.
        let script = format!(
            "case \"$*\" in *resume*) if [ -f \"$0.broken\" ]; then echo 'session expired' >&2; exit 3; fi;; esac; {}",
            codex_lines("thread-f", REPORT)
        );
        let setup = Setup::new("research-jf", None, &script);
        setup.ask("research-jf", "First?").run().await;
        let job = setup.ask("research-jf", "Second?");
        let job_id = job.job_id.clone();
        job.run().await;
        let research = setup.store.load("research-jf").unwrap();
        assert_eq!(
            research.turns[1].status,
            TurnStatus::Done,
            "{:?}",
            research.turns[1].error
        );
        assert_eq!(research.turns[1].context.as_deref(), Some("native"));
        let prompt = fs::read_to_string(
            setup
                .store
                .job_dir("research-jf", &job_id)
                .unwrap()
                .join("prompt.md"),
        )
        .unwrap();
        assert!(prompt.contains("Second?") && !prompt.contains("Earlier in this research"));
        let record = fs::read_to_string(
            setup
                .store
                .job_dir("research-jf", &job_id)
                .unwrap()
                .join("job.json"),
        )
        .unwrap();
        assert!(
            record.contains(" resume ") && record.contains("thread-f"),
            "{record}"
        );

        fs::write(format!("{}.broken", setup.agent.command), "").unwrap();
        let job = setup.ask("research-jf", "Third?");
        let job_id = job.job_id.clone();
        job.run().await;
        let research = setup.store.load("research-jf").unwrap();
        let third = &research.turns[2];
        assert_eq!(third.status, TurnStatus::Done, "{:?}", third.error);
        assert_eq!(third.context.as_deref(), Some("replay"));
        assert!(third.warnings.iter().any(|w| w.contains("replayed")));
        let prompt = fs::read_to_string(
            setup
                .store
                .job_dir("research-jf", &job_id)
                .unwrap()
                .join("prompt.md"),
        )
        .unwrap();
        assert!(prompt.contains("replayed history attempt"));
        assert!(
            prompt.contains("### Earlier question 1\nFirst?")
                && prompt.contains("### Earlier question 2\nSecond?")
        );
        fs::remove_dir_all(&setup.root).unwrap();
    }

    #[tokio::test]
    async fn changed_agent_command_replays_instead_of_resuming() {
        let setup = Setup::new(
            "research-jr",
            None,
            &codex_lines("thread-r", "# Plain\nanswer"),
        );
        setup.ask("research-jr", "First?").run().await;
        setup
            .store
            .update("research-jr", |r| {
                r.resume_agent_command = Some("/old/codex".into());
                Ok(())
            })
            .unwrap();
        setup.ask("research-jr", "Second?").run().await;
        let research = setup.store.load("research-jr").unwrap();
        assert_eq!(research.turns[1].context.as_deref(), Some("replay"));
        assert!(research.turns[1].warnings.is_empty());
        fs::remove_dir_all(&setup.root).unwrap();
    }

    #[tokio::test]
    async fn project_writes_are_flagged_without_failing_the_turn() {
        let project = project();
        let script = format!(
            "echo leaked > \"{}/src/new.rs\"; {}",
            project.display(),
            codex_lines("thread-x", REPORT)
        );
        let setup = Setup::new("research-jx", Some(&project), &script);
        setup.ask("research-jx", "Q?").run().await;
        let state = setup.store.load("research-jx").unwrap().turns[0].clone();
        assert_eq!(state.status, TurnStatus::Done);
        assert!(state
            .warnings
            .iter()
            .any(|w| w.contains("Git status or HEAD changed")));
        fs::remove_dir_all(&setup.root).unwrap();
        fs::remove_dir_all(project).unwrap();
    }

    #[tokio::test]
    async fn failures_and_cancel_are_recorded_on_the_turn() {
        let setup = Setup::new("research-jc", None, "echo 'boom' >&2; exit 2");
        setup.ask("research-jc", "Q?").run().await;
        let state = setup.store.load("research-jc").unwrap().turns[0].clone();
        assert_eq!(state.status, TurnStatus::Failed);
        assert!(state.error.is_some());
        assert_eq!(
            store::summary(&setup.store.load("research-jc").unwrap()).status,
            "failed"
        );
        fs::remove_dir_all(&setup.root).unwrap();

        let setup = Setup::new("research-jd", None, "sleep 30");
        let job = setup.ask("research-jd", "Q?");
        let cancel = job.cancel.clone();
        let handle = tokio::spawn(job.run());
        tokio::time::sleep(Duration::from_millis(300)).await;
        cancel.store(true, Ordering::Relaxed);
        tokio::time::timeout(Duration::from_secs(10), handle)
            .await
            .unwrap()
            .unwrap();
        let research = setup.store.load("research-jd").unwrap();
        assert_eq!(research.turns[0].status, TurnStatus::Cancelled);
        assert!(research.turns[0].error.is_none());
        assert_eq!(
            setup.events.lock().unwrap().last().unwrap().status,
            "cancelled"
        );
        assert!(store::running_job(&setup.store, "research-jd").is_none());
        fs::remove_dir_all(&setup.root).unwrap();
    }

    #[test]
    fn long_arguments_are_elided() {
        assert!(describe("codex", &["x".repeat(200)]).contains("<200 chars>"));
    }
}
