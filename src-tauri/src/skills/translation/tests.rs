use super::*;
use crate::skills::{
    tests::Fixture,
    translation::config::{ConfigInput, KeyChange},
};
use serde_json::json;
fn setup() -> (Fixture, Repository, String) {
    let f = Fixture::new();
    let r = Repository::open(&f.0.join("data"), &f.0.join("coord")).unwrap();
    let source = f.0.join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("SKILL.md"),"---\nname: fixture\ndescription: Fixture description\n---\n# Title\n\nMain body\n\n## References\n\n[Reference](https://example.com)\n\n## Other\nOther body\n").unwrap();
    let id = r.add_local(&source).unwrap().id;
    let skill = super::super::catalog::encode_id(&id, "SKILL.md");
    config::save(
        &r,
        ConfigInput {
            provider_id: ProviderId::OpenAi,
            expected_revision: 0,
            key: KeyChange::Set("fixture-secret".into()),
            model: None,
        },
        &RuntimeConfig::default(),
    )
    .unwrap();
    (f, r, skill)
}
fn input(r: &Repository, id: &str) -> TranslationInput {
    TranslationInput {
        skill_id: id.into(),
        expected_content_digest: r.detail(id).unwrap().content_digest,
        provider_id: ProviderId::OpenAi,
        target_language: "zh-CN".into(),
        source_mode: SourceMode::Markdown,
    }
}
#[test]
fn preparation_pins_content_and_keeps_secrets_and_text_out_of_records() {
    let (f, r, id) = setup();
    let prepared = prepare(r.clone(), input(&r, &id), &RuntimeConfig::default()).unwrap();
    let summary = prepared.summary();
    assert_eq!(summary.context.provider_id, ProviderId::OpenAi);
    assert_eq!(summary.context.model.as_deref(), Some("gpt-5"));
    assert!(prepared.text.contains("Main body"));
    let request =
        fs::read_to_string(location(&r, &summary.context.operation_id, "request").unwrap())
            .unwrap();
    assert!(!request.contains("fixture-secret") && !request.contains("Main body"));
    fs::write(f.0.join("source/SKILL.md"), "# New source\n").unwrap();
    assert!(prepared.text.contains("Main body"));
    assert!(matches!(
        prepare(
            r.clone(),
            TranslationInput {
                expected_content_digest: summary.context.source_digest.clone(),
                ..input(&r, &id)
            },
            &RuntimeConfig::default()
        ),
        Err(SkillsError {
            code: ErrorCode::Conflict,
            ..
        })
    ));
    assert_eq!(history(&r, &id).unwrap().len(), 1);
}
#[test]
fn summary_projection_preserves_title_description_and_references_only() {
    let (_f, r, id) = setup();
    let mut i = input(&r, &id);
    i.source_mode = SourceMode::Summary;
    let p = prepare(r, i, &RuntimeConfig::default()).unwrap();
    assert!(
        p.text.contains("# Title")
            && p.text.contains("Fixture description")
            && p.text.contains("[Reference]")
    );
    assert!(!p.text.contains("Main body") && !p.text.contains("Other body"));
}
#[test]
fn remote_requests_and_complete_responses_match_provider_contracts() {
    let (_f, r, id) = setup();
    let p = prepare(r, input_dummy(&id), &RuntimeConfig::default());
    assert!(p.is_err());
    let (_f, r, id) = setup();
    let p = prepare(r.clone(), input(&r, &id), &RuntimeConfig::default()).unwrap();
    for provider_id in [ProviderId::OpenAi, ProviderId::OpenRouter] {
        let provider = ResolvedProvider {
            id: provider_id,
            model: Some("fixture-model".into()),
            api_key: Some("fixture-secret".into()),
        };
        let b = remote::fixture_body(&provider, &p.context, &p.text);
        assert!(!b.to_string().contains("fixture-secret"));
        assert_eq!(b["model"], "fixture-model");
        let response = if provider_id == ProviderId::OpenAi {
            assert_eq!(b["store"], false);
            assert_eq!(b["input"], p.text);
            json!({"status":"completed","output":[{"type":"reasoning"},{"type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":"# 翻译"}]}]})
        } else {
            assert_eq!(b["messages"][1]["content"], p.text);
            json!({"choices":[{"finish_reason":"stop","message":{"role":"assistant","content":"# 翻译"}}]})
        };
        assert_eq!(
            remote::fixture_parse(provider_id, response).unwrap(),
            "# 翻译"
        );
    }
    for value in [
        json!({"status":"incomplete","output":[]}),
        json!({"status":"completed","output":[{"type":"function_call","name":"danger"}]}),
        json!({"status":"completed","output":[{"type":"message","role":"assistant","status":"completed","content":[{"type":"refusal","refusal":"private secret"}]}]}),
        json!({"status":"completed","output":[]}),
    ] {
        let e = remote::fixture_parse(ProviderId::OpenAi, value).unwrap_err();
        assert!(!e.message.contains("private secret"));
    }
    assert!(remote::fixture_parse(ProviderId::OpenRouter,json!({"choices":[{"finish_reason":"length","message":{"role":"assistant","content":"Partial"}}]})).is_err());
}
fn input_dummy(id: &str) -> TranslationInput {
    TranslationInput {
        skill_id: id.into(),
        expected_content_digest: "bad".into(),
        provider_id: ProviderId::OpenAi,
        target_language: "zh-CN".into(),
        source_mode: SourceMode::Markdown,
    }
}
#[tokio::test]
async fn cancelled_prepared_request_never_calls_remote_or_publishes_result() {
    let (_f, r, id) = setup();
    let p = prepare(r.clone(), input(&r, &id), &RuntimeConfig::default()).unwrap();
    let op_id = p.context.operation_id.clone();
    super::super::operations::cancel(&r, &op_id).unwrap();
    run(p, |_| {}).await;
    let h = history(&r, &id).unwrap();
    assert_eq!(h[0].operation.phase, Phase::Cancelled);
    assert!(h[0].operation.requests.is_empty());
    assert!(result(&r, &op_id).is_err());
    assert!(!location(&r, &op_id, "result").unwrap().exists());
}
#[test]
fn stored_result_requires_matching_committed_context() {
    let (_f, r, id) = setup();
    let mut p = prepare(r.clone(), input(&r, &id), &RuntimeConfig::default()).unwrap();
    let op_id = p.context.operation_id.clone();
    let mut value = TranslationResult {
        context: p.context.clone(),
        markdown: "# Translation".into(),
    };
    files::atomic_json(&location(&r, &op_id, "result").unwrap(), &value).unwrap();
    assert!(result(&r, &op_id).is_err());
    p.operation.finish(Ok(())).unwrap();
    assert_eq!(result(&r, &op_id).unwrap().markdown, "# Translation");
    value.context.target_language = "other".into();
    files::atomic_json(&location(&r, &op_id, "result").unwrap(), &value).unwrap();
    assert!(result(&r, &op_id).is_err());
    assert!(result(&r, "../outside").is_err());
}

struct FixtureExecutor {
    program: PathBuf,
}
impl Executor for FixtureExecutor {
    fn execute<'a>(
        &'a self,
        _repo: &'a Repository,
        op: &'a mut Operation,
        provider: &'a ResolvedProvider,
        context: &'a Context,
        text: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String>> + Send + 'a>> {
        Box::pin(async move {
            if provider.id.remote() {
                remote::fixture_parse(
                    provider.id,
                    if provider.id == ProviderId::OpenAi {
                        json!({"status":"completed","output":[{"type":"message","status":"completed","role":"assistant","content":[{"type":"output_text","text":"# Fixture translation"}]}]})
                    } else {
                        json!({"choices":[{"finish_reason":"stop","message":{"role":"assistant","content":"# Fixture translation"}}]})
                    },
                )
            } else {
                local::fixture_translate(
                    op,
                    provider,
                    context,
                    text,
                    self.program.to_str().unwrap(),
                )
                .await
            }
        })
    }
}
#[cfg(unix)]
#[tokio::test]
async fn four_provider_workflows_persist_versioned_results_without_mutating_source() {
    use std::os::unix::fs::PermissionsExt;
    let (f, r, id) = setup();
    let program = f.0.join("fixture-cli");
    fs::write(
        &program,
        r##"#!/bin/sh
out=''
while [ "$#" -gt 0 ]; do
 if [ "$1" = '--output-last-message' ]; then shift; out="$1"; fi
 shift
done
cat >/dev/null
if [ -n "$out" ]; then
 printf '# Fixture translation' > "$out"
 printf '{"type":"turn.completed"}\n'
else
 printf '{"type":"result","subtype":"success","is_error":false,"result":"# Fixture translation"}'
fi
"##,
    )
    .unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o700)).unwrap();
    config::save(
        &r,
        ConfigInput {
            provider_id: ProviderId::OpenRouter,
            expected_revision: 1,
            key: KeyChange::Set("fixture-router-secret".into()),
            model: None,
        },
        &RuntimeConfig::default(),
    )
    .unwrap();
    let runtime = RuntimeConfig::default()
        .fixture_local(ProviderId::Codex)
        .fixture_local(ProviderId::ClaudeCode);
    let original = fs::read(f.0.join("source/SKILL.md")).unwrap();
    for provider_id in ProviderId::ALL {
        let mut request = input(&r, &id);
        request.provider_id = provider_id;
        let prepared = prepare(r.clone(), request, &runtime).unwrap();
        let operation_id = prepared.context.operation_id.clone();
        run_using(
            prepared,
            |_| {},
            FixtureExecutor {
                program: program.clone(),
            },
        )
        .await;
        let output = result(&r, &operation_id).unwrap();
        assert_eq!(output.markdown, "# Fixture translation");
        assert_eq!(output.context.provider_id, provider_id);
        assert_eq!(output.context.skill_id, id);
        let logs = fs::read_to_string(
            r.root
                .join("operations")
                .join(format!("{operation_id}.json")),
        )
        .unwrap();
        for forbidden in ["fixture-secret", "fixture-router-secret", "Main body"] {
            assert!(!logs.contains(forbidden));
        }
        assert!(!std::env::temp_dir()
            .join(format!("loom-translation-{operation_id}"))
            .exists());
    }
    assert_eq!(history(&r, &id).unwrap().len(), 4);
    assert_eq!(fs::read(f.0.join("source/SKILL.md")).unwrap(), original);
}
struct UntilCancelled;
impl Executor for UntilCancelled {
    fn execute<'a>(
        &'a self,
        _repo: &'a Repository,
        op: &'a mut Operation,
        _provider: &'a ResolvedProvider,
        _context: &'a Context,
        _text: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String>> + Send + 'a>> {
        Box::pin(async move {
            loop {
                op.check_cancelled()?;
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
    }
}
#[tokio::test]
async fn cancellation_during_execution_persists_cancelled_without_a_result() {
    let (_f, r, id) = setup();
    let p = prepare(r.clone(), input(&r, &id), &RuntimeConfig::default()).unwrap();
    let op = p.context.operation_id.clone();
    let execution = run_using(p, |_| {}, UntilCancelled);
    let cancel = async {
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        super::super::operations::cancel(&r, &op).unwrap();
    };
    tokio::join!(execution, cancel);
    assert_eq!(
        history(&r, &id).unwrap()[0].operation.phase,
        Phase::Cancelled
    );
    assert!(!location(&r, &op, "result").unwrap().exists());
}

#[test]
fn large_history_keeps_recent_results_and_old_active_requests_without_deleting_records() {
    let (_f, r, id) = setup();
    let active = prepare(r.clone(), input(&r, &id), &RuntimeConfig::default()).unwrap();
    let mut context = active.context.clone();
    let mut record = active.operation.record.clone();
    record.phase = Phase::Committed;
    // Unrelated records must not be read by the per-skill history endpoint.
    for index in 0..1001 {
        fs::write(
            r.root
                .join("operations")
                .join(format!("unrelated-{index}.json")),
            b"unrelated fixture",
        )
        .unwrap();
    }
    for index in 0..1601 {
        let op_id = format!("history-{index:04}");
        context.operation_id = op_id.clone();
        context.created_at_ms = active.context.created_at_ms + index + 1;
        record.operation_id = op_id.clone();
        fs::write(
            location(&r, &op_id, "request").unwrap(),
            serde_json::to_vec(&context).unwrap(),
        )
        .unwrap();
        fs::write(location(&r, &op_id, "result").unwrap(), b"retained result").unwrap();
        fs::write(
            r.root.join("operations").join(format!("{op_id}.json")),
            serde_json::to_vec(&record).unwrap(),
        )
        .unwrap();
    }
    let entries_before = fs::read_dir(r.root.join("translations")).unwrap().count();
    let values = history(&r, &id).unwrap();
    assert_eq!(values.len(), 21);
    assert_eq!(values[0].context.operation_id, "history-1600");
    assert_eq!(values[19].context.operation_id, "history-1581");
    assert_eq!(values[20].context.operation_id, active.context.operation_id);
    assert_eq!(values[20].operation.phase, Phase::Running);
    assert_eq!(
        fs::read_dir(r.root.join("translations")).unwrap().count(),
        entries_before
    );
    assert!(prepare(r.clone(), input(&r, &id), &RuntimeConfig::default()).is_ok());
    drop(active);
    let values = history(&r, &id).unwrap();
    assert!(values.iter().all(|v| v.operation.phase != Phase::Running));
}

#[test]
fn history_reconciles_interrupted_requests_and_rejects_mismatched_operation_identity() {
    let (_f, r, id) = setup();
    let prepared = prepare(r.clone(), input(&r, &id), &RuntimeConfig::default()).unwrap();
    let op_id = prepared.context.operation_id.clone();
    drop(prepared);
    let values = history(&r, &id).unwrap();
    assert_eq!(values[0].operation.phase, Phase::Interrupted);
    let mut record = values[0].operation.clone();
    record.source_id = "different-source".into();
    files::atomic_json(
        &r.root.join("operations").join(format!("{op_id}.json")),
        &record,
    )
    .unwrap();
    assert_eq!(
        history(&r, &id).unwrap_err().code,
        ErrorCode::RecoveryRequired
    );
}

#[test]
fn operation_list_bounds_returned_terminal_records_not_persisted_history() {
    let (_f, r, id) = setup();
    let active = prepare(r.clone(), input(&r, &id), &RuntimeConfig::default()).unwrap();
    let mut record = active.operation.record.clone();
    record.phase = Phase::Committed;
    for index in 0..1005 {
        record.operation_id = format!("operation-{index:04}");
        record.started_at_ms = active.operation.record.started_at_ms + index + 1;
        fs::write(
            r.root
                .join("operations")
                .join(format!("{}.json", record.operation_id)),
            serde_json::to_vec(&record).unwrap(),
        )
        .unwrap();
    }
    let records = super::super::operations::list(&r).unwrap();
    assert_eq!(records.len(), 1001);
    assert_eq!(records[0].operation_id, "operation-1004");
    assert_eq!(records.last().unwrap().phase, Phase::Running);
    assert!(r.root.join("operations/operation-0000.json").exists());
}

#[tokio::test]
#[ignore = "explicit account-confirmed model call with synthetic documentation only"]
async fn real_translation_provider_acceptance() {
    assert_eq!(
        std::env::var("LOOM_SKILLS_REAL_ACCOUNT_CONFIRMED").as_deref(),
        Ok("1"),
        "Verify the personal account or obtain an explicit account exception before running"
    );
    let name = std::env::var("LOOM_SKILLS_REAL_TRANSLATION_PROVIDER").expect("Choose one provider");
    let provider_id: ProviderId =
        serde_json::from_value(serde_json::Value::String(name.clone())).unwrap();
    let f = Fixture::new();
    let repo = Repository::open(&f.0.join("data"), &f.0.join("coord")).unwrap();
    let source = f.0.join("synthetic-source");
    fs::create_dir(&source).unwrap();
    let document = "---\nname: translation-acceptance\ndescription: Synthetic translation acceptance document\n---\n# Translation fixture\n\nKeep the source file unchanged. This paragraph is only test documentation.\n\n```text\nLOOM_TRANSLATION_LITERAL_42\n```\n";
    fs::write(source.join("SKILL.md"), document).unwrap();
    let source_id = repo.add_local(&source).unwrap().id;
    let skill_id = super::super::catalog::encode_id(&source_id, "SKILL.md");
    let request = TranslationInput {
        skill_id: skill_id.clone(),
        expected_content_digest: repo.detail(&skill_id).unwrap().content_digest,
        provider_id,
        target_language: "Simplified Chinese".into(),
        source_mode: SourceMode::Markdown,
    };
    let prepared = prepare(repo.clone(), request, &RuntimeConfig::capture().unwrap()).unwrap();
    let id = prepared.context.operation_id.clone();
    run(prepared, |_| {}).await;
    let record = super::super::operations::get(&repo, &id).unwrap();
    assert_eq!(record.phase, Phase::Committed, "{:?}", record.error);
    let output = result(&repo, &id).unwrap();
    assert!(output.markdown.contains("LOOM_TRANSLATION_LITERAL_42"));
    assert!(output
        .markdown
        .chars()
        .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)));
    assert_eq!(
        fs::read_to_string(source.join("SKILL.md")).unwrap(),
        document
    );
    println!(
        "TRANSLATION_ACCEPTANCE {}",
        serde_json::json!({"provider":name,"phase":record.phase,"resultBytes":output.markdown.len(),"containsChinese":true,"literalPreserved":true,"sourceUnchanged":true,"input":"synthetic-only","httpRequests":record.requests.len(),"cliExecutions":record.executions.len()})
    );
}
