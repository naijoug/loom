use super::{
    catalog,
    error::SkillsError,
    files,
    installers::{self, ConflictPolicy, InstallMode, InstallRequest},
    instructions::{self, DesiredResource, ResourceKind},
    operations::Operation,
    package::{self, EntryState},
    repository::Repository,
    targets::TargetContext,
    tests::Fixture,
    transactions::{self, Change, Outcome},
};
use std::{
    fs,
    io::Write,
    process::{Command, Stdio},
};

#[test]
fn malformed_duplicate_and_nested_markers_never_produce_a_span() {
    for text in [
        "<!-- skills-linker:demo:begin -->\nmissing end",
        "<!-- skills-linker:foreign:begin -->\n<!-- skills-linker:demo:begin -->\nowned\n<!-- skills-linker:demo:end -->\n<!-- skills-linker:foreign:end -->\n",
        "<!-- skills-linker:demo:end -->\n<!-- skills-linker:demo:begin -->\n",
        "<!-- skills-linker:demo:begin -->\n<!-- skills-linker:other:begin -->\n<!-- skills-linker:demo:end -->\n",
        "<!-- skills-linker:demo:begin -->\none\n<!-- skills-linker:demo:end -->\n<!-- skills-linker:demo:begin -->\ntwo\n<!-- skills-linker:demo:end -->\n",
    ] { assert!(instructions::block_span(text, "demo").is_err()); }
    assert_eq!(
        instructions::block_span("User rules", "demo").unwrap(),
        None
    );
}

#[test]
fn instruction_ledger_and_manifest_recover_together_after_every_rename() {
    for failure_step in 0..6 {
        let f = Fixture::new();
        fs::create_dir(f.0.join("home")).unwrap();
        fs::create_dir(f.0.join("project")).unwrap();
        let repo = Repository::open(&f.0.join("data"), &f.0.join("coord")).unwrap();
        let context = TargetContext::new(&f.0.join("home"), Some(&f.0.join("project"))).unwrap();
        let target = context
            .selected(&["codex-project".into()])
            .unwrap()
            .remove(0);
        context.ensure(&target).unwrap();
        let path = context.instruction_path(&target).unwrap();
        fs::write(&path, "User rules\n").unwrap();
        package::set_mode(&path, 0o640).unwrap();
        let original = package::state(&path).unwrap();
        let desired = DesiredResource {
            path: path.clone(),
            marker: "demo".into(),
            kind: ResourceKind::Instruction,
            content: "Managed rule".into(),
        };
        let lease = repo.coordinator.enter().unwrap();
        let writer = repo.coordinator.writer(&lease).unwrap();
        let owner = instructions::owner(&target, "demo").unwrap();
        let plan = instructions::plan(&repo, &owner, &[], &[desired]).unwrap();
        let mut changes = plan.stage("resource-crash", 0).unwrap();
        let manifest = owner.manifest_path;
        transactions::prepare_storage(&manifest, "resource-crash").unwrap();
        let staged = transactions::artifact(&manifest, "resource-crash", 2, "staged").unwrap();
        files::atomic_json(&staged, &serde_json::json!({"version":2,"skills":{}})).unwrap();
        changes.push(Change {
            target: manifest.clone(),
            before: EntryState::Missing,
            after: package::state(&staged).unwrap(),
            staged,
            backup: transactions::artifact(&manifest, "resource-crash", 2, "backup").unwrap(),
        });
        let pending =
            transactions::prepare(&repo, &writer, "resource-crash", changes.clone()).unwrap();
        let roots = changes
            .iter()
            .map(|c| c.target.parent().unwrap().to_path_buf())
            .collect::<Vec<_>>();
        assert!(
            transactions::commit_observed(&writer, &pending, &roots, |step| {
                if step == failure_step {
                    Err(SkillsError::invalid("injected interruption"))
                } else {
                    Ok(())
                }
            })
            .is_err()
        );
        drop(writer);
        drop(lease);
        let other = Repository::open(&f.0.join("data-b"), &f.0.join("coord")).unwrap();
        let pending = installers::recovery_items(&other, &context).unwrap();
        assert_eq!(pending.len(), 1);
        assert!(pending[0].can_recover, "{pending:?}");
        assert_eq!(
            installers::recover_one(&other, &context, "resource-crash").unwrap(),
            Outcome::RolledBack
        );
        assert_eq!(package::state(&path).unwrap(), original);
        assert!(!manifest.exists());
        for edit in plan.edits {
            if edit.path != path {
                assert_eq!(package::state(&edit.path).unwrap(), EntryState::Missing);
            }
        }
        assert!(installers::recovery_items(&other, &context)
            .unwrap()
            .is_empty());
    }
}

#[test]
fn recovery_rejects_unrelated_project_file_even_with_a_valid_journal() {
    let f = Fixture::new();
    fs::create_dir(f.0.join("home")).unwrap();
    fs::create_dir(f.0.join("project")).unwrap();
    let repo = Repository::open(&f.0.join("data"), &f.0.join("coord")).unwrap();
    let context = TargetContext::new(&f.0.join("home"), Some(&f.0.join("project"))).unwrap();
    let target = f.0.join("project/unrelated.txt");
    transactions::prepare_storage(&target, "unrelated").unwrap();
    let staged = transactions::artifact(&target, "unrelated", 0, "staged").unwrap();
    fs::write(&staged, "Not an instruction").unwrap();
    let lease = repo.coordinator.enter().unwrap();
    let writer = repo.coordinator.writer(&lease).unwrap();
    transactions::prepare(
        &repo,
        &writer,
        "unrelated",
        vec![Change {
            target: target.clone(),
            before: EntryState::Missing,
            after: package::state(&staged).unwrap(),
            staged,
            backup: transactions::artifact(&target, "unrelated", 0, "backup").unwrap(),
        }],
    )
    .unwrap();
    drop(writer);
    drop(lease);
    assert!(!installers::recovery_items(&repo, &context).unwrap()[0].can_recover);
    assert!(installers::recover_one(&repo, &context, "unrelated").is_err());
    assert!(!target.exists());
}

#[test]
fn separate_processes_with_distinct_libraries_preserve_both_instruction_blocks() {
    let f = Fixture::new();
    fs::create_dir(f.0.join("home")).unwrap();
    fs::create_dir(f.0.join("project")).unwrap();
    fs::write(f.0.join("project/AGENTS.md"), "User rules\n").unwrap();
    for name in ["first", "second"] {
        let dir = f.0.join("source/auto").join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: {name}\n---\n# {name}\n"),
        )
        .unwrap();
        fs::write(dir.join("inject.md"), format!("Rule for {name}\n")).unwrap();
    }
    let mut children = Vec::new();
    for (tool, name) in [("codex-project", "first"), ("amp-project", "second")] {
        children.push(
            Command::new(std::env::current_exe().unwrap())
                .args([
                    "--ignored",
                    "--exact",
                    "skills::instructions_tests::instruction_process_child",
                    "--nocapture",
                ])
                .env("LOOM_INSTRUCTION_TEST_ROOT", &f.0)
                .env("LOOM_INSTRUCTION_TEST_TOOL", tool)
                .env("LOOM_INSTRUCTION_TEST_NAME", name)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        );
    }
    for child in &mut children {
        child.stdin.take().unwrap().write_all(b"go\n").unwrap();
    }
    for child in children {
        let out = child.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let text = fs::read_to_string(f.0.join("project/AGENTS.md")).unwrap();
    assert!(text.starts_with("User rules\n"));
    assert_eq!(text.matches("skills-linker:first:begin").count(), 1);
    assert_eq!(text.matches("skills-linker:second:begin").count(), 1);
}

#[test]
#[ignore = "spawned by the parent with an isolated explicit fixture"]
fn instruction_process_child() {
    use std::io::BufRead;
    let root = std::path::PathBuf::from(std::env::var_os("LOOM_INSTRUCTION_TEST_ROOT").unwrap());
    let tool = std::env::var("LOOM_INSTRUCTION_TEST_TOOL").unwrap();
    let name = std::env::var("LOOM_INSTRUCTION_TEST_NAME").unwrap();
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line).unwrap();
    let repo = Repository::open(&root.join(format!("data-{name}")), &root.join("coord")).unwrap();
    fn retry_lock<T>(mut work: impl FnMut() -> super::error::Result<T>) -> T {
        for _ in 0..20 {
            match work() {
                Ok(value) => return value,
                Err(error) if error.code == super::error::ErrorCode::LockBusy => {
                    std::thread::sleep(std::time::Duration::from_millis(10))
                }
                Err(error) => panic!("Unexpected concurrent operation error: {error:?}"),
            }
        }
        panic!("Timed out waiting for another test client commit");
    }
    let source = retry_lock(|| repo.add_local(&root.join("source")));
    let context = TargetContext::new(&root.join("home"), Some(&root.join("project"))).unwrap();
    let request = InstallRequest {
        skill_id: catalog::encode_id(&source.id, &format!("auto/{name}/SKILL.md")),
        target_ids: vec![tool],
        mode: InstallMode::Copy,
        conflict_policy: ConflictPolicy::Fail,
        project_path: None,
        with_slash_commands: false,
    };
    // Another process may commit between review and apply. Replan on a detected
    // conflict rather than overwriting its newly committed block.
    for _ in 0..4 {
        let plan = retry_lock(|| installers::plan_install(&repo, &context, &request));
        let mut operation = retry_lock(|| Operation::begin(&repo, &source.id, "install"));
        let result =
            retry_lock(|| installers::apply(&repo, &context, &plan.plan_id, &mut operation));
        retry_lock(|| operation.finish(Ok(())));
        if result.items[0].status == "installed" {
            return;
        }
        assert_eq!(result.items[0].status, "conflict", "{result:?}");
    }
    panic!("Concurrent instruction install did not converge");
}
