//! Frozen preflight and durable idempotent admission, independent of the renderer.
use super::{
    admission, files,
    models::{Research, Workflow},
    store::{next_id, valid_id, Store},
    target_models::*,
    target_store::{digest, TargetStore, Writer},
};
use crate::{
    models::{now_ms, AgentConfig},
    research_sources,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preflight {
    pub preflight_id: String,
    pub research_id: Option<String>,
    pub input: TargetInput,
    pub budget: Budget,
    pub digest: String,
    pub expires_at_ms: u128,
    pub operations: Vec<String>,
    pub limitations: Vec<String>,
}
#[derive(Serialize, Deserialize)]
struct StoredPreflight {
    public: Preflight,
    agent_digest: String,
    target_identity: String,
}
#[derive(Serialize, Deserialize)]
struct RequestRecord {
    digest: String,
    research_id: String,
    run_id: String,
    error: Option<String>,
}
pub struct Admission {
    pub target: TargetStore,
    pub writer: Option<Writer>,
    pub view: RunView,
}
fn replay(
    store: &Store,
    request_id: &str,
    expected_digest: &str,
) -> Result<Option<RunView>, String> {
    validate_id(request_id)?;
    let path = files::path(&store.root, &format!(".requests/{request_id}.json"))?;
    if !path.exists() {
        return Ok(None);
    }
    let request: RequestRecord = files::read_json(&path, 16384)?;
    if request.digest != expected_digest {
        return Err("request_id_conflict".into());
    }
    if let Some(error) = request.error {
        if error == "admission_pending"
            && valid_id(&request.research_id)
            && valid_id(&request.run_id)
        {
            if let Ok(target) = TargetStore::open(store.clone(), &request.research_id) {
                if let Ok(view) = target.run(&request.run_id) {
                    return Ok(Some(view));
                }
            }
        }
        return Err(error);
    }
    TargetStore::open(store.clone(), &request.research_id)?
        .run(&request.run_id)
        .map(Some)
}
pub fn replay_start(
    store: &Store,
    preflight_id: &str,
    request_id: &str,
) -> Result<Option<RunView>, String> {
    validate_id(preflight_id)?;
    replay(
        store,
        request_id,
        &digest(format!("start:{preflight_id}").as_bytes()),
    )
}
pub fn replay_action(
    store: &Store,
    research_id: &str,
    action: &RunAction,
    request_id: &str,
) -> Result<Option<RunView>, String> {
    replay(
        store,
        request_id,
        &digest(&serde_json::to_vec(&(research_id, action)).map_err(|e| e.to_string())?),
    )
}

fn validate_id(id: &str) -> Result<(), String> {
    if valid_id(id) {
        Ok(())
    } else {
        Err("invalid_request_id".into())
    }
}
fn agent_digest(agent: &AgentConfig) -> Result<String, String> {
    crate::agent::evidence_execution_identity(agent)
}
pub(super) fn target_identity(input: &TargetInput) -> Result<String, String> {
    let Target::App { path } = &input.target else {
        return Err("unsupported_target_collector".into());
    };
    let layout = research_sources::inspect_app(path)?;
    let mut identity = Vec::new();
    for relative in &layout.source_paths {
        let meta =
            std::fs::metadata(layout.root.join(relative)).map_err(|_| "app_target_changed")?;
        identity.push((
            relative.clone(),
            meta.len(),
            meta.modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|v| v.as_nanos()),
        ));
    }
    let meta = std::fs::metadata(&layout.root).map_err(|_| "app_target_changed")?;
    let tools_identity = input
        .tools
        .as_ref()
        .map(|tools| tools.fingerprint())
        .transpose()?;
    let root_modified = meta
        .modified()
        .map_err(|_| "app_target_metadata_unavailable")?
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "app_target_metadata_unavailable")?
        .as_nanos();
    let captured_identity = layout.additional_identity()?;
    Ok(digest(
        &serde_json::to_vec(&(
            captured_identity,
            layout.root,
            meta.len(),
            root_modified,
            identity,
            tools_identity,
        ))
        .map_err(|e| e.to_string())?,
    ))
}
pub fn preflight(
    store: &Store,
    mut input: TargetInput,
    research_id: Option<String>,
    agent: &AgentConfig,
) -> Result<Preflight, String> {
    if let Some(id) = &research_id {
        let record = store.load(id)?;
        if record.workflow != Workflow::Target {
            return Err("workflow_mismatch".into());
        }
    }
    input.question = input.question.trim().into();
    if input.question.is_empty()
        || input.question.chars().count() > 8000
        || !matches!(input.language.as_str(), "en" | "zh-CN")
    {
        return Err("invalid_target_question".into());
    }
    if input.agent_id != agent.id {
        return Err("agent_identity_mismatch".into());
    }
    if input.mode != ResearchMode::Single {
        return Err("research_iteration_not_enabled".into());
    }
    if let Some(model) = &input.model {
        crate::agent::validate_model_name(model)?;
    }
    let budget = input
        .budget
        .clone()
        .unwrap_or_else(|| Budget::defaults(input.mode));
    budget.validate(input.mode)?;
    let metadata = (budget.max_disk_bytes / 4).min(20 * 1024 * 1024);
    let report = (budget.max_disk_bytes / 8).min(4 * 1024 * 1024 + 256 * 1024);
    if budget
        .max_disk_bytes
        .saturating_sub(metadata)
        .saturating_sub(report)
        < 32 * 1024 * 1024
    {
        return Err("target_disk_budget_too_small_for_collector".into());
    }
    match &mut input.target {
        Target::App { path } => {
            *path = research_sources::inspect_app(path)?
                .root
                .to_string_lossy()
                .into_owned();
        }
        _ => return Err("unsupported_target_collector".into()),
    }
    input.tools = Some(
        input
            .tools
            .as_ref()
            .ok_or("rea_runtime_required")?
            .inspect()?,
    );
    let identity = target_identity(&input)?;
    let agent_digest = agent_digest(agent)?;
    let spec_digest = digest(
        &serde_json::to_vec(&(&input, &budget, &identity, &agent_digest))
            .map_err(|e| e.to_string())?,
    );
    let public = Preflight {
        preflight_id: next_id("preflight"),
        research_id,
        input,
        budget,
        digest: spec_digest,
        expires_at_ms: now_ms() + 300_000,
        operations: vec![
            "Inspect selected App statically; for ASAR, read only declared source members and their .unpacked companions".into(),
            "Analyze JavaScript without executing the App".into(),
            "Save source snapshots and redacted evidence; send bounded text to selected CLI".into(),
        ],
        limitations: vec![
            "Native engines are diagnostic only; static observations do not prove runtime behavior"
                .into(),
            "Tool and model readiness are rechecked when starting".into(),
        ],
    };
    files::write_json(
        &files::path(
            &store.root,
            &format!(".preflight/{}.json", public.preflight_id),
        )?,
        &StoredPreflight {
            public: public.clone(),
            agent_digest,
            target_identity: identity,
        },
    )?;
    Ok(public)
}
pub fn read_preflight(store: &Store, id: &str) -> Result<Preflight, String> {
    validate_id(id)?;
    let stored: StoredPreflight = files::read_json(
        &files::path(&store.root, &format!(".preflight/{id}.json"))?,
        1024 * 1024,
    )?;
    if stored.public.preflight_id != id {
        return Err("preflight_identity_mismatch".into());
    }
    Ok(stored.public)
}
pub fn admit(
    store: Store,
    preflight_id: &str,
    request_id: &str,
    agent: &AgentConfig,
) -> Result<Admission, String> {
    validate_id(preflight_id)?;
    validate_id(request_id)?;
    // A short global admission lock protects requests across app instances. It
    // is released before any collector/model starts; per-research lease remains.
    let _request_lease = admission::acquire(&store.root, "target-admission")?;
    let request_path = files::path(&store.root, &format!(".requests/{request_id}.json"))?;
    let request_digest = digest(format!("start:{preflight_id}").as_bytes());
    if request_path.exists() {
        let view =
            replay_start(&store, preflight_id, request_id)?.ok_or("request_record_unavailable")?;
        let target = TargetStore::open(store, &view.spec.research_id)?;
        return Ok(Admission {
            target,
            writer: None,
            view,
        });
    }
    let mut request = RequestRecord {
        digest: request_digest,
        research_id: String::new(),
        run_id: String::new(),
        error: Some("admission_interrupted".into()),
    };
    // Persist an intent first: a process crash cannot turn a repeated request into a new model call.
    files::write_json(&request_path, &request)?;
    let result: Result<Admission, String> = (|| {
        let stored: StoredPreflight = files::read_json(
            &files::path(&store.root, &format!(".preflight/{preflight_id}.json"))?,
            1024 * 1024,
        )?;
        let preflight = stored.public;
        if preflight.preflight_id != preflight_id || preflight.expires_at_ms <= now_ms() {
            return Err("preflight_expired".into());
        }
        if stored.agent_digest != agent_digest(agent)?
            || stored.target_identity != target_identity(&preflight.input)?
        {
            return Err("preflight_changed".into());
        }
        let consumed = files::path(
            &store.root,
            &format!(".preflight/{preflight_id}.consumed.json"),
        )?;
        if consumed.exists() {
            return Err("preflight_already_consumed".into());
        }
        let research_id = preflight
            .research_id
            .clone()
            .unwrap_or_else(|| next_id("research"));
        let kind = if preflight.research_id.is_some() {
            RunKind::Refresh
        } else {
            RunKind::Initial
        };
        let research = if kind == RunKind::Initial {
            let now = now_ms();
            Some(Research {
                active_owner: None,
                workflow: Workflow::Target,
                target: Some(TargetConfig {
                    target: preflight.input.target.clone(),
                    run_ids: vec![],
                }),
                schema_version: 2,
                research_id: research_id.clone(),
                title: preflight.input.question.chars().take(100).collect(),
                agent_id: agent.id.clone(),
                agent_name: agent.name.clone(),
                adapter_type: agent.adapter_type.clone(),
                model: preflight.input.model.clone(),
                project_path: None,
                project_name: None,
                language: preflight.input.language.clone(),
                prompt_version: 1,
                created_at_ms: now,
                updated_at_ms: now,
                turns: vec![],
                resume_command: None,
                resume_agent_command: None,
            })
        } else {
            None
        };
        let run_id = next_id("run");
        let spec = RunSpec {
            schema_version: 2,
            research_id: research_id.clone(),
            run_id: run_id.clone(),
            kind,
            input: preflight.input,
            budget: preflight.budget,
            config_digest: preflight.digest,
            agent_config_digest: stored.agent_digest,
            target_identity: stored.target_identity,
            parent_report_id: None,
            source_snapshots: vec![],
            source_evidence: vec![],
            created_at_ms: now_ms(),
        };
        request.research_id = research_id.clone();
        request.run_id = run_id.clone();
        request.error = Some("admission_pending".into());
        files::write_json(&request_path, &request)?;
        // Consumption precedes publication, so a crash cannot reuse this preflight
        // through a different request ID and launch a second initial run.
        files::write_json(
            &consumed,
            &serde_json::json!({"requestId":request_id,"researchId":research_id,"runId":run_id}),
        )?;
        let (target, writer) = if let Some(research) = research {
            TargetStore::create_initial(store.clone(), &research, &spec)?
        } else {
            let target = TargetStore::open(store.clone(), &research_id)?;
            let writer = target.writer(Some(&run_id))?;
            writer.create_run(&spec)?;
            (target, writer)
        };
        request.error = None;
        let view = target.run(&spec.run_id)?;
        Ok(Admission {
            target,
            writer: Some(writer),
            view,
        })
    })();
    if let Err(error) = &result {
        request.error = Some(error.clone());
    }
    files::write_json(&request_path, &request)?;
    result
}

#[cfg(test)]
mod tests {
    use super::super::store::tests::temp_root;
    use super::*;
    fn fixture() -> (std::path::PathBuf, Store, TargetInput, AgentConfig) {
        let root = temp_root();
        let app = root.join("app");
        std::fs::create_dir(&app).unwrap();
        std::fs::write(app.join("package.json"), "{}").unwrap();
        std::fs::write(app.join("index.js"), "export const value = 42;").unwrap();
        let package = root.join("rea");
        std::fs::create_dir_all(package.join("scripts")).unwrap();
        std::fs::write(
            package.join("package.json"),
            r#"{"name":"rea-agents","version":"6.8.0"}"#,
        )
        .unwrap();
        std::fs::write(package.join("scripts/rea.mjs"), "// never executed").unwrap();
        let input = TargetInput {
            target: Target::App {
                path: app.to_string_lossy().into_owned(),
            },
            question: "Explain".into(),
            agent_id: "agent".into(),
            model: None,
            language: "en".into(),
            mode: ResearchMode::Single,
            budget: None,
            tools: Some(crate::rea::Config {
                node: "/bin/sh".into(),
                entry: package.join("scripts/rea.mjs"),
            }),
        };
        let agent = AgentConfig {
            id: "agent".into(),
            name: "Fixture".into(),
            command: "/bin/sh".into(),
            args: vec![],
            working_directory_policy: "project_root".into(),
            capabilities: vec![],
            adapter_type: "claude_code_cli".into(),
            can_write_files: false,
            can_run_commands: false,
            enabled: true,
            available: true,
        };
        let store = Store::new(&root);
        (root, store, input, agent)
    }
    #[test]
    fn repeated_start_returns_same_run_and_does_not_claim_second_writer() {
        let (root, store, input, agent) = fixture();
        let preflight = preflight(&store, input.clone(), None, &agent).unwrap();
        let first = admit(
            store.clone(),
            &preflight.preflight_id,
            "request-first",
            &agent,
        )
        .unwrap();
        assert!(first.writer.is_some());
        let second = admit(
            store.clone(),
            &preflight.preflight_id,
            "request-first",
            &agent,
        )
        .unwrap();
        assert!(second.writer.is_none());
        assert_eq!(first.view.spec.run_id, second.view.spec.run_id);
        assert_eq!(store.list().items.len(), 1);
        assert!(first.target.writer(None).is_err());
        assert!(admit(
            store.clone(),
            &preflight.preflight_id,
            "request-other",
            &agent
        )
        .is_err());
        let other = super::preflight(&store, input, None, &agent).unwrap();
        assert!(
            matches!(admit(store.clone(), &other.preflight_id, "request-first", &agent), Err(e) if e == "request_id_conflict")
        );
        drop(first);
        drop(second);
        assert_eq!(store.list().items[0].status, "interrupted");
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn retry_preserves_usage_and_followup_freezes_materials_without_target_access() {
        let (root, store, input, agent) = fixture();
        let preflight = preflight(&store, input, None, &agent).unwrap();
        let mut first = admit(
            store.clone(),
            &preflight.preflight_id,
            "request-first",
            &agent,
        )
        .unwrap();
        let research_id = first.view.spec.research_id.clone();
        let run_id = first.view.spec.run_id.clone();
        let writer = first.writer.take().unwrap();
        let mut state = first.target.run(&run_id).unwrap().state;
        state.usage.calls = 1;
        state.usage.active_ms = 42;
        state.status = RunStatus::Failed;
        writer.save_state(state, "failed").unwrap();
        drop(writer);
        let retry = continue_run(
            store.clone(),
            &research_id,
            RunAction::Retry {
                run_id: run_id.clone(),
            },
            "request-retry",
            &agent,
        )
        .unwrap();
        assert_eq!(retry.view.state.attempt, 2);
        assert_eq!(retry.view.state.usage.calls, 1);
        assert_eq!(retry.view.state.usage.active_ms, 42);
        let repeated = continue_run(
            store.clone(),
            &research_id,
            RunAction::Retry {
                run_id: run_id.clone(),
            },
            "request-retry",
            &agent,
        )
        .unwrap();
        assert!(repeated.writer.is_none());
        let writer = retry.writer.unwrap();
        let report = writer
            .publish_report(&run_id, "Only retained evidence is available.")
            .unwrap();
        let mut state = retry.target.run(&run_id).unwrap().state;
        state.status = RunStatus::Completed;
        writer.save_state(state, "done").unwrap();
        drop(writer);
        std::fs::remove_dir_all(root.join("app")).unwrap();
        std::fs::remove_dir_all(root.join("rea")).unwrap();
        let followup = continue_run(
            store.clone(),
            &research_id,
            RunAction::Followup {
                parent_report_id: report.report_id,
                question: "What remains unknown?".into(),
            },
            "request-followup",
            &agent,
        )
        .unwrap();
        assert_eq!(followup.view.spec.kind, RunKind::EvidenceFollowup);
        assert_eq!(followup.view.spec.budget.max_collections, 0);
        assert_eq!(
            followup.view.state.snapshot_ids,
            followup.view.spec.source_snapshots
        );
        assert_eq!(
            followup.view.state.evidence_ids,
            followup.view.spec.source_evidence
        );
        drop(followup);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn initial_creation_is_atomic_when_run_spec_write_fails() {
        let (root, store, input, agent) = fixture();
        let preflight = preflight(&store, input, None, &agent).unwrap();
        files::fail_next_write_to("spec.json");
        assert!(
            matches!(admit(store.clone(),&preflight.preflight_id,"request-full-disk",&agent),Err(e) if e.contains("28"))
        );
        let listed = store.list();
        assert!(listed.items.is_empty());
        assert!(listed.storage_errors.is_empty());
        assert!(
            matches!(admit(store.clone(),&preflight.preflight_id,"request-full-disk",&agent),Err(e) if e.contains("28"))
        );
        assert!(
            matches!(admit(store.clone(),&preflight.preflight_id,"request-different",&agent),Err(e) if e=="preflight_already_consumed")
        );
        assert!(!std::fs::read_dir(&store.root).unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".creating-")));
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn pending_request_after_publication_replays_without_dispatch() {
        let (root, store, input, agent) = fixture();
        let preflight = preflight(&store, input, None, &agent).unwrap();
        let first = admit(
            store.clone(),
            &preflight.preflight_id,
            "request-pending",
            &agent,
        )
        .unwrap();
        let run_id = first.view.spec.run_id.clone();
        let path = files::path(&store.root, ".requests/request-pending.json").unwrap();
        let mut record: RequestRecord = files::read_json(&path, 16384).unwrap();
        record.error = Some("admission_pending".into());
        files::write_json(&path, &record).unwrap();
        let active = admit(
            store.clone(),
            &preflight.preflight_id,
            "request-pending",
            &agent,
        )
        .unwrap();
        assert!(active.writer.is_none());
        assert_eq!(active.view.spec.run_id, run_id);
        drop(first);
        drop(active);
        let replayed = admit(
            store.clone(),
            &preflight.preflight_id,
            "request-pending",
            &agent,
        )
        .unwrap();
        assert!(replayed.writer.is_none());
        assert_eq!(replayed.view.state.status, RunStatus::Interrupted);
        assert_eq!(replayed.view.spec.run_id, run_id);
        assert_eq!(store.list().items.len(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_requests_stay_failed_and_target_changes_invalidate_preflight() {
        let (root, store, input, agent) = fixture();
        let preflight = preflight(&store, input, None, &agent).unwrap();
        std::fs::write(root.join("app/index.js"), "changed source and length").unwrap();
        for _ in 0..2 {
            assert!(
                matches!(admit(store.clone(), &preflight.preflight_id, "request-first", &agent),Err(e) if e == "preflight_changed")
            );
        }
        assert!(store.list().items.is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RunAction {
    Followup {
        parent_report_id: String,
        question: String,
    },
    Retry {
        run_id: String,
    },
    Resume {
        run_id: String,
    },
}

pub fn continue_run(
    store: Store,
    research_id: &str,
    action: RunAction,
    request_id: &str,
    agent: &AgentConfig,
) -> Result<Admission, String> {
    validate_id(request_id)?;
    let target = TargetStore::open(store.clone(), research_id)?;
    let _request_lease = admission::acquire(&store.root, "target-admission")?;
    let path = files::path(&store.root, &format!(".requests/{request_id}.json"))?;
    let request_digest =
        digest(&serde_json::to_vec(&(research_id, &action)).map_err(|e| e.to_string())?);
    if path.exists() {
        let existing: RequestRecord = files::read_json(&path, 16384)?;
        if existing.digest != request_digest {
            return Err("request_id_conflict".into());
        }
        if let Some(error) = existing.error {
            return Err(error);
        }
        let view = target.run(&existing.run_id)?;
        return Ok(Admission {
            target,
            writer: None,
            view,
        });
    }
    let mut request = RequestRecord {
        digest: request_digest,
        research_id: research_id.into(),
        run_id: String::new(),
        error: Some("admission_interrupted".into()),
    };
    files::write_json(&path, &request)?;
    let result: Result<Admission, String> = (|| {
        let mut writer = target.writer(None)?;
        let view = match &action {
            RunAction::Followup {
                parent_report_id,
                question,
            } => {
                let question = question.trim();
                if question.is_empty() || question.chars().count() > 8000 {
                    return Err("invalid_target_question".into());
                }
                let report = target.report(parent_report_id)?.report;
                let mut spec = target.run(&report.run_id)?.spec;
                if spec.input.agent_id != agent.id {
                    return Err("agent_identity_mismatch".into());
                }
                // A follow-up is a new run with a fresh verified execution profile,
                // while the parent report's material set remains immutable.
                spec.agent_config_digest = agent_digest(agent)?;
                spec.run_id = next_id("run");
                spec.kind = RunKind::EvidenceFollowup;
                spec.input.question = question.into();
                spec.input.mode = ResearchMode::Single;
                spec.budget = Budget::defaults(ResearchMode::Single);
                spec.budget.max_collections = 0;
                spec.input.budget = Some(spec.budget.clone());
                spec.parent_report_id = Some(parent_report_id.clone());
                spec.source_snapshots = report.snapshot_ids;
                spec.source_evidence = report.evidence_ids;
                spec.created_at_ms = now_ms();
                spec.config_digest = digest(
                    &serde_json::to_vec(&(
                        &spec.input,
                        &spec.parent_report_id,
                        &spec.source_snapshots,
                        &spec.source_evidence,
                        &spec.agent_config_digest,
                    ))
                    .map_err(|e| e.to_string())?,
                );
                writer.bind(&spec.run_id)?;
                writer.create_run(&spec)?;
                target.run(&spec.run_id)?
            }
            RunAction::Retry { run_id } | RunAction::Resume { run_id } => {
                let run = target.run(run_id)?;
                let valid = match &action {
                    RunAction::Retry { .. } => {
                        matches!(run.state.status, RunStatus::Failed | RunStatus::Cancelled)
                    }
                    _ => matches!(
                        run.state.status,
                        RunStatus::Interrupted | RunStatus::AwaitingUser
                    ),
                };
                if !valid {
                    return Err("run_state_mismatch".into());
                }
                if run.spec.agent_config_digest != agent_digest(agent)? {
                    return Err("agent_configuration_changed".into());
                }
                if run.spec.kind != RunKind::EvidenceFollowup
                    && run.spec.target_identity != target_identity(&run.spec.input)?
                {
                    return Err("target_changed_repeat_preflight".into());
                }
                let mut state = writer.recover(run_id)?;
                if state.usage.active_ms >= run.spec.budget.max_active_ms
                    || state.usage.calls >= run.spec.budget.max_calls
                {
                    return Err("budget_exhausted".into());
                }
                state.attempt = state.attempt.checked_add(1).ok_or("attempt_limit")?;
                state.status = RunStatus::Queued;
                state.error = None;
                state.stop_reason = None;
                writer.bind(run_id)?;
                writer.save_state(state, &next_id("attempt"))?;
                target.run(run_id)?
            }
        };
        request.run_id = view.spec.run_id.clone();
        request.error = None;
        Ok(Admission {
            target: target.clone(),
            writer: Some(writer),
            view,
        })
    })();
    if let Err(error) = &result {
        request.error = Some(error.clone());
    }
    files::write_json(&path, &request)?;
    result
}
