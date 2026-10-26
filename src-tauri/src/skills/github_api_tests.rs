use super::*;
use crate::skills::{
    installers::{self, ConflictPolicy, InstallMode, InstallRequest, UninstallRequest},
    operations::{self, Phase},
    sources::normalize_git_url,
    targets::TargetContext,
    tests::Fixture,
};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

struct Mock {
    replies: Mutex<VecDeque<(String, Response)>>,
    seen: Mutex<Vec<String>>,
}
impl Transport for Arc<Mock> {
    fn get<'a>(
        &'a self,
        url: reqwest::Url,
        limit: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Response>> + Send + 'a>> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(url.to_string());
            let (expected, response) = self
                .replies
                .lock()
                .unwrap()
                .pop_front()
                .expect("Unexpected request");
            assert_eq!(url.as_str(), expected);
            assert!(
                response.body.len() <= limit,
                "Fixture exceeds request limit"
            );
            Ok(response)
        })
    }
}
fn response(value: Value) -> Response {
    Response {
        status: 200,
        rate_limited: false,
        paginated: false,
        body: serde_json::to_vec(&value).unwrap(),
    }
}
fn hash(bytes: &[u8]) -> String {
    let mut h = Sha1::new();
    h.update(format!("blob {}\0", bytes.len()));
    h.update(bytes);
    format!("{:x}", h.finalize())
}
fn mock(branch: &str, revision: char, files: &[(&str, &[u8], &str)]) -> Arc<Mock> {
    let commit = revision.to_string().repeat(40);
    let tree = "d".repeat(40);
    let prefix = "https://api.github.com/repos/fixture/skills";
    let mut encoded = reqwest::Url::parse(&format!("{prefix}/git/ref/heads/")).unwrap();
    encoded
        .path_segments_mut()
        .unwrap()
        .pop_if_empty()
        .push(branch);
    let mut replies = VecDeque::from([
        (
            prefix.into(),
            response(json!({"default_branch":branch,"full_name":"fixture/skills"})),
        ),
        (
            encoded.to_string(),
            response(
                json!({"ref":format!("refs/heads/{branch}"),"object":{"type":"commit","sha":commit}}),
            ),
        ),
        (
            format!("{prefix}/git/commits/{commit}"),
            response(json!({"sha":commit,"tree":{"sha":tree}})),
        ),
        (
            format!("{prefix}/git/trees/{tree}?recursive=1"),
            response(
                json!({"sha":tree,"truncated":false,"tree":files.iter().map(|(path,bytes,mode)|json!({"path":path,"type":"blob","mode":mode,"sha":hash(bytes),"size":bytes.len()})).collect::<Vec<_>>()}),
            ),
        ),
    ]);
    let mut ordered = files.to_vec();
    ordered.sort_by_key(|(p, _, _)| *p);
    let mut seen = BTreeSet::new();
    for (_, bytes, _) in ordered {
        let sha = hash(bytes);
        if seen.insert(sha.clone()) {
            replies.push_back((format!("{prefix}/git/blobs/{sha}"),response(json!({"sha":sha,"size":bytes.len(),"encoding":"base64","content":base64::engine::general_purpose::STANDARD.encode(bytes)}))));
        }
    }
    Arc::new(Mock {
        replies: Mutex::new(replies),
        seen: Mutex::new(Vec::new()),
    })
}
fn setup() -> (Fixture, Repository, GitSource) {
    let fixture = Fixture::new();
    let repo = Repository::open(&fixture.0.join("data"), &fixture.0.join("coord")).unwrap();
    (
        fixture,
        repo,
        normalize_git_url("https://github.com/fixture/skills").unwrap(),
    )
}
async fn run(
    repo: &Repository,
    source: &GitSource,
    mock: Arc<Mock>,
    previous: Option<String>,
) -> OperationRecord {
    let id = source_id(source).unwrap();
    let op = Operation::begin(repo, &id, "import").unwrap();
    let operation_id = op.record.operation_id.clone();
    import(
        repo.clone(),
        op,
        source.clone(),
        previous,
        Client {
            transport: Box::new(mock),
        },
        |_| {},
    )
    .await;
    operations::list(repo)
        .unwrap()
        .into_iter()
        .find(|r| r.operation_id == operation_id)
        .unwrap()
}
fn alter_tree(mock: &Mock, edit: impl FnOnce(&mut Value)) {
    let mut queue = mock.replies.lock().unwrap();
    let tree = &mut queue[3].1;
    let mut value: Value = serde_json::from_slice(&tree.body).unwrap();
    edit(&mut value);
    tree.body = serde_json::to_vec(&value).unwrap();
}

#[tokio::test]
async fn complete_api_packages_keep_binary_scripts_nested_roots_and_pinned_installs() {
    let (fixture, repo, source) = setup();
    let skill = b"---\nname: api-demo\ndescription: API fixture\n---\n# API demo\n";
    let other = b"---\nname: outside-demo\n---\n# Outside\n";
    let files = [
        ("skills/auto/demo/SKILL.md", skill.as_slice(), "100644"),
        (
            "skills/auto/demo/agents/tool.yaml",
            b"name: helper".as_slice(),
            "100644",
        ),
        (
            "skills/auto/demo/scripts/run.sh",
            b"#!/bin/sh\necho fixture\n".as_slice(),
            "100755",
        ),
        (
            "skills/auto/demo/references/data.bin",
            b"\x00\xff\x01".as_slice(),
            "100644",
        ),
        ("other/demo/SKILL.md", other.as_slice(), "100644"),
    ];
    let m = mock("feature/skills", 'a', &files);
    let record = run(&repo, &source, m.clone(), None).await;
    assert_eq!(record.phase, Phase::Committed, "{:?}", record.error);
    assert_eq!(record.requests.len(), 9);
    assert!(record
        .requests
        .iter()
        .all(|r| r.status == Some(200) && r.finished_at_ms.is_some()));
    let registered = repo.get_source(&source_id(&source).unwrap()).unwrap();
    assert_eq!(registered.default_branch.as_deref(), Some("feature/skills"));
    let directory = repo.source_directory(&registered).unwrap();
    assert_eq!(
        std::fs::read(directory.join("skills/auto/demo/references/data.bin")).unwrap(),
        b"\x00\xff\x01"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_ne!(
            std::fs::metadata(directory.join("skills/auto/demo/scripts/run.sh"))
                .unwrap()
                .permissions()
                .mode()
                & 0o100,
            0
        );
    }
    let library = repo.list().unwrap();
    assert_eq!(library.skills.len(), 2);
    let entry = library
        .skills
        .iter()
        .find(|s| s.name == "api-demo")
        .unwrap();
    assert_eq!(entry.relative_path, "skills/auto/demo/SKILL.md");
    assert_eq!(entry.category, "auto");
    assert_eq!(repo.detail(&entry.id).unwrap().related_files.len(), 3);
    std::fs::create_dir(fixture.0.join("home")).unwrap();
    std::fs::create_dir(fixture.0.join("project")).unwrap();
    let context =
        TargetContext::new(&fixture.0.join("home"), Some(&fixture.0.join("project"))).unwrap();
    let plan = installers::plan_install(
        &repo,
        &context,
        &InstallRequest {
            skill_id: entry.id.clone(),
            target_ids: vec!["codex-project".into()],
            mode: InstallMode::Copy,
            conflict_policy: ConflictPolicy::Fail,
            project_path: None,
            with_slash_commands: false,
        },
    )
    .unwrap();
    let mut op = Operation::begin(&repo, &registered.id, "install").unwrap();
    assert_eq!(
        installers::apply(&repo, &context, &plan.plan_id, &mut op)
            .unwrap()
            .items[0]
            .status,
        "installed"
    );
    op.finish(Ok(())).unwrap();
    drop(op);
    let destination = fixture.0.join("project/.codex/skills/api-demo");
    assert_eq!(
        std::fs::read(destination.join("references/data.bin")).unwrap(),
        b"\x00\xff\x01"
    );
    let updated = [
        ("skills/auto/demo/SKILL.md", skill.as_slice(), "100644"),
        (
            "skills/auto/demo/references/data.bin",
            b"new bytes".as_slice(),
            "100644",
        ),
        ("other/demo/SKILL.md", other.as_slice(), "100644"),
    ];
    let record = run(
        &repo,
        &source,
        mock("main", 'b', &updated),
        Some("a".repeat(40)),
    )
    .await;
    assert_eq!(record.phase, Phase::Committed, "{:?}", record.error);
    assert_eq!(
        repo.get_source(&registered.id)
            .unwrap()
            .default_branch
            .as_deref(),
        Some("main")
    );
    assert_eq!(
        std::fs::read(destination.join("references/data.bin")).unwrap(),
        b"\x00\xff\x01"
    );
    let statuses = installers::statuses(&repo, &context, &entry.id).unwrap();
    let installed = statuses.iter().find(|s| s.installation.is_some()).unwrap();
    assert!(installed.update_available);
    assert_eq!(
        installed.installation.as_ref().unwrap().revision.as_deref(),
        Some("a".repeat(40).as_str())
    );
    repo.remove_source(&registered.id).unwrap();
    let plan = installers::plan_uninstall(
        &repo,
        &context,
        &UninstallRequest {
            installation_ids: vec![installed
                .installation
                .as_ref()
                .unwrap()
                .installation_id
                .clone()],
            project_path: None,
        },
    )
    .unwrap();
    let mut op = Operation::begin(&repo, "installed", "uninstall").unwrap();
    assert_eq!(
        installers::apply(&repo, &context, &plan.plan_id, &mut op)
            .unwrap()
            .items[0]
            .status,
        "uninstalled"
    );
    op.finish(Ok(())).unwrap();
    assert!(!destination.exists());
    assert!(directory.join("skills/auto/demo/SKILL.md").exists());
}

#[tokio::test]
async fn truncated_missing_or_corrupt_blobs_preserve_the_last_completed_source() {
    let (_fixture, repo, source) = setup();
    let files = [(
        "SKILL.md",
        b"---\nname: root-demo\n---\n# Root\n".as_slice(),
        "100644",
    )];
    assert_eq!(
        run(&repo, &source, mock("main", 'a', &files), None)
            .await
            .phase,
        Phase::Committed
    );
    for fault in ["truncated", "missing", "corrupt", "encoding", "pagination"] {
        let m = mock("main", 'b', &files);
        if fault == "truncated" {
            alter_tree(&m, |v| v["truncated"] = json!(true));
        } else if fault == "pagination" {
            m.replies.lock().unwrap()[3].1.paginated = true;
        } else {
            let mut q = m.replies.lock().unwrap();
            let blob = &mut q[4].1;
            if fault == "missing" {
                blob.status = 404;
                blob.body = b"secret-reflected-response".to_vec();
            } else {
                let mut v: Value = serde_json::from_slice(&blob.body).unwrap();
                if fault == "corrupt" {
                    v["content"] =
                        json!(base64::engine::general_purpose::STANDARD.encode(b"wrong"));
                } else {
                    v["encoding"] = json!("utf-8");
                }
                blob.body = serde_json::to_vec(&v).unwrap();
            }
        }
        let record = run(&repo, &source, m, Some("a".repeat(40))).await;
        assert_eq!(record.phase, Phase::Failed, "{fault}");
        assert!(!serde_json::to_string(&record)
            .unwrap()
            .contains("secret-reflected"));
        assert_eq!(
            repo.get_source(&source_id(&source).unwrap())
                .unwrap()
                .revision,
            Some("a".repeat(40))
        );
    }
}

#[test]
fn unsafe_paths_modes_sizes_and_duplicate_entries_are_rejected() {
    let skill = b"# Test\n";
    let entry = json!({"path":"demo/SKILL.md","type":"blob","mode":"100644","sha":hash(skill),"size":skill.len()});
    for fault in [
        "../escape/SKILL.md",
        "/absolute/SKILL.md",
        "demo\\escape/SKILL.md",
        "demo/.git/config",
        "demo/x:y",
    ] {
        let mut malicious = entry.clone();
        malicious["path"] = json!(fault);
        let tree: Tree = serde_json::from_value(
            json!({"sha":"d".repeat(40),"truncated":false,"tree":[entry.clone(),malicious]}),
        )
        .unwrap();
        assert!(entries_to_materialize(tree).is_err());
    }
    for (kind, mode) in [("blob", "120000"), ("commit", "160000")] {
        let mut unsafe_entry = entry.clone();
        unsafe_entry["path"] = json!("demo/resource");
        unsafe_entry["type"] = json!(kind);
        unsafe_entry["mode"] = json!(mode);
        let tree: Tree = serde_json::from_value(
            json!({"sha":"d".repeat(40),"truncated":false,"tree":[entry.clone(),unsafe_entry]}),
        )
        .unwrap();
        assert!(entries_to_materialize(tree).is_err());
    }
    let mut large = entry.clone();
    large["size"] = json!(MAX_TOTAL + 1);
    let tree: Tree =
        serde_json::from_value(json!({"sha":"d".repeat(40),"truncated":false,"tree":[large]}))
            .unwrap();
    assert!(entries_to_materialize(tree).is_err());
    let tree: Tree = serde_json::from_value(
        json!({"sha":"d".repeat(40),"truncated":false,"tree":[entry.clone(),entry]}),
    )
    .unwrap();
    assert!(entries_to_materialize(tree).is_err());
}

#[tokio::test]
async fn auth_rate_limit_redirects_and_cancel_have_distinct_safe_results() {
    let (_fixture, repo, source) = setup();
    for (status, rate, code) in [
        (401, false, ErrorCode::AuthRequired),
        (403, true, ErrorCode::RateLimited),
        (429, false, ErrorCode::RateLimited),
        (301, false, ErrorCode::SourceUnavailable),
    ] {
        let m = Arc::new(Mock {
            replies: Mutex::new(VecDeque::from([(
                "https://api.github.com/repos/fixture/skills".into(),
                Response {
                    status,
                    rate_limited: rate,
                    paginated: false,
                    body: b"token-secret".to_vec(),
                },
            )])),
            seen: Mutex::new(Vec::new()),
        });
        let record = run(&repo, &source, m, None).await;
        assert_eq!(record.error.as_ref().unwrap().code, code);
        assert_eq!(record.requests[0].status, Some(status));
        assert!(!serde_json::to_string(&record)
            .unwrap()
            .contains("token-secret"));
    }
    let op = Operation::begin(&repo, &source_id(&source).unwrap(), "import").unwrap();
    operations::cancel(&repo, &op.record.operation_id).unwrap();
    let m = Arc::new(Mock {
        replies: Mutex::new(VecDeque::new()),
        seen: Mutex::new(Vec::new()),
    });
    import(
        repo.clone(),
        op,
        source,
        None,
        Client {
            transport: Box::new(m.clone()),
        },
        |_| {},
    )
    .await;
    assert!(m.seen.lock().unwrap().is_empty());
    assert_eq!(operations::list(&repo).unwrap()[0].phase, Phase::Cancelled);
    let header = authorization("test-secret").unwrap();
    assert!(header.is_sensitive());
    assert!(!format!("{header:?}").contains("test-secret"));
    assert!(authorization("bad\nheader").is_err());
}

#[tokio::test]
async fn root_skill_and_nested_skill_keep_old_api_discovery_semantics() {
    let (_fixture, repo, source) = setup();
    let root = b"---\nname: root-demo\n---\n# Root\n";
    let nested = b"---\nname: nested-demo\n---\n# Nested\n";
    assert_eq!(
        run(
            &repo,
            &source,
            mock(
                "trunk",
                'a',
                &[
                    ("SKILL.md", root, "100644"),
                    ("skills/demo/SKILL.md", nested, "100644")
                ]
            ),
            None
        )
        .await
        .phase,
        Phase::Committed
    );
    let library = repo.list().unwrap();
    assert_eq!(library.skills.len(), 2);
    assert!(library.skills.iter().any(|s| s.relative_path == "SKILL.md"));
    assert!(library
        .skills
        .iter()
        .any(|s| s.relative_path == "skills/demo/SKILL.md"));
    assert_ne!(source_id(&source).unwrap(), source.source_id);
}

#[tokio::test]
async fn api_and_git_modes_keep_distinct_sources() {
    let (fixture, repo, source) = setup();
    let files = [(
        "SKILL.md",
        b"---\nname: demo\n---\n# Demo\n".as_slice(),
        "100644",
    )];
    assert_eq!(
        run(&repo, &source, mock("trunk", 'a', &files), None)
            .await
            .phase,
        Phase::Committed
    );
    let checkout = fixture.0.join("git-checkout");
    std::fs::create_dir(&checkout).unwrap();
    std::fs::write(checkout.join("SKILL.md"), files[0].1).unwrap();
    let staging = fixture.0.join("git-snapshot");
    let revision = "a".repeat(40);
    let snapshot = snapshots::prepare(&checkout, &staging, &source.source_id, &revision).unwrap();
    repo.commit_git(
        Source {
            id: source.source_id.clone(),
            name: source.namespace.clone(),
            kind: SourceKind::Git,
            local_path: None,
            git: Some(source.clone()),
            revision: Some(revision),
            updated_at_ms: 1,
            default_branch: None,
        },
        None,
        &staging,
        &snapshot,
    )
    .unwrap();
    assert_eq!(repo.load().unwrap().sources.len(), 2);
    assert_eq!(repo.list().unwrap().skills.len(), 2);
}

#[tokio::test]
async fn active_package_projection_excludes_archived_discovery_and_rejects_lfs() {
    let (_fixture, repo, source) = setup();
    let skill = b"---\nname: active\n---\n# Active\n";
    let archived = b"---\nname: archived\n---\n# Archived\n";
    let m = mock(
        "main",
        'a',
        &[
            ("skills/active/SKILL.md", skill, "100644"),
            ("archive/old/SKILL.md", archived, "100644"),
        ],
    );
    // The transport queue includes old API-only archived bytes; the canonical
    // discovery policy must not request or publish them as active packages.
    m.replies.lock().unwrap().remove(4);
    assert_eq!(
        run(&repo, &source, m.clone(), None).await.phase,
        Phase::Committed
    );
    assert_eq!(repo.list().unwrap().skills.len(), 1);
    assert!(!m
        .seen
        .lock()
        .unwrap()
        .iter()
        .any(|u| u.ends_with(&hash(archived))));
    let lfs = b"version https://git-lfs.github.com/spec/v1\noid sha256:dummy\nsize 1\n";
    let record = run(
        &repo,
        &source,
        mock(
            "main",
            'b',
            &[
                ("skills/active/SKILL.md", skill, "100644"),
                ("skills/active/reference.bin", lfs, "100644"),
            ],
        ),
        Some("a".repeat(40)),
    )
    .await;
    assert_eq!(record.phase, Phase::Failed);
    assert_eq!(
        repo.get_source(&source_id(&source).unwrap())
            .unwrap()
            .revision,
        Some("a".repeat(40))
    );
}

struct PendingTransport;
impl Transport for PendingTransport {
    fn get<'a>(
        &'a self,
        _: reqwest::Url,
        _: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Response>> + Send + 'a>> {
        Box::pin(std::future::pending())
    }
}
#[tokio::test]
async fn cancellation_interrupts_an_inflight_http_request() {
    let (_fixture, repo, source) = setup();
    let op = Operation::begin(&repo, &source_id(&source).unwrap(), "import").unwrap();
    let id = op.record.operation_id.clone();
    let operation = import(
        repo.clone(),
        op,
        source,
        None,
        Client {
            transport: Box::new(PendingTransport),
        },
        |_| {},
    );
    let cancel = async {
        tokio::time::sleep(Duration::from_millis(80)).await;
        operations::cancel(&repo, &id).unwrap();
    };
    tokio::time::timeout(Duration::from_secs(2), async {
        tokio::join!(operation, cancel);
    })
    .await
    .unwrap();
    let record = operations::list(&repo)
        .unwrap()
        .into_iter()
        .find(|r| r.operation_id == id)
        .unwrap();
    assert_eq!(record.phase, Phase::Cancelled);
    assert_eq!(record.requests.len(), 1);
    assert!(repo.load().unwrap().sources.is_empty());
}

#[tokio::test]
#[ignore = "explicit network acceptance: anonymous read of vercel-labs/agent-browser, installs only into a disposable project"]
async fn real_public_github_api_materializes_and_installs_without_git() {
    let fixture = Fixture::new();
    let repo = Repository::open(&fixture.0.join("data"), &fixture.0.join("coord")).unwrap();
    let source = normalize_git_url("https://github.com/vercel-labs/agent-browser").unwrap();
    let op = Operation::begin(&repo, &source_id(&source).unwrap(), "import").unwrap();
    let id = op.record.operation_id.clone();
    import(
        repo.clone(),
        op,
        source.clone(),
        None,
        Client::anonymous().unwrap(),
        |_| {},
    )
    .await;
    let record = operations::list(&repo)
        .unwrap()
        .into_iter()
        .find(|r| r.operation_id == id)
        .unwrap();
    assert_eq!(record.phase, Phase::Committed, "{:?}", record.error);
    assert!(record.executions.is_empty());
    let library = repo.list().unwrap();
    assert!(!library.skills.is_empty());
    let source_record = repo.get_source(&source_id(&source).unwrap()).unwrap();
    let directory = repo.source_directory(&source_record).unwrap();
    let snapshot = snapshots::verify(directory.parent().unwrap()).unwrap();
    std::fs::create_dir(fixture.0.join("home")).unwrap();
    std::fs::create_dir(fixture.0.join("project")).unwrap();
    let context =
        TargetContext::new(&fixture.0.join("home"), Some(&fixture.0.join("project"))).unwrap();
    let skill = &library.skills[0];
    let plan = installers::plan_install(
        &repo,
        &context,
        &InstallRequest {
            skill_id: skill.id.clone(),
            target_ids: vec!["codex-project".into()],
            mode: InstallMode::Copy,
            conflict_policy: ConflictPolicy::Fail,
            project_path: None,
            with_slash_commands: false,
        },
    )
    .unwrap();
    let mut op = Operation::begin(&repo, &source_record.id, "install").unwrap();
    let installed = installers::apply(&repo, &context, &plan.plan_id, &mut op).unwrap();
    assert_eq!(installed.items[0].status, "installed");
    op.finish(Ok(())).unwrap();
    drop(op);
    let entry = installers::inventory(&repo, &context)
        .unwrap()
        .into_iter()
        .find_map(|s| s.installation)
        .unwrap();
    let removal = installers::plan_uninstall(
        &repo,
        &context,
        &UninstallRequest {
            installation_ids: vec![entry.installation_id],
            project_path: None,
        },
    )
    .unwrap();
    let mut op = Operation::begin(&repo, &source_record.id, "uninstall").unwrap();
    assert_eq!(
        installers::apply(&repo, &context, &removal.plan_id, &mut op)
            .unwrap()
            .items[0]
            .status,
        "uninstalled"
    );
    op.finish(Ok(())).unwrap();
    println!(
        "GITHUB_API_ACCEPTANCE {}",
        json!({"repository":source.url,"revision":source_record.revision,"defaultBranch":source_record.default_branch,"skillCount":library.skills.len(),"snapshotFiles":snapshot.file_count,"httpRequests":record.requests.len(),"gitProcesses":record.executions.len(),"authentication":"anonymous","installedAndRemovedInTemporaryProject":true,"agentInvoked":false})
    );
}

#[test]
fn secondary_limit_detection_does_not_return_untrusted_server_text() {
    assert!(secondary_rate_limit(
        br#"{"message":"You have exceeded a secondary rate limit. reflected-secret"}"#
    ));
    assert!(!secondary_rate_limit(
        br#"{"message":"Resource not accessible by integration"}"#
    ));
    assert!(!secondary_rate_limit(b"not json"));
}
