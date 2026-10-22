use super::{
    catalog,
    error::ErrorCode,
    files,
    installers::{self, ConflictPolicy, InstallMode, InstallRequest, PlanAction, UninstallRequest},
    operations::Operation,
    package,
    repository::Repository,
    targets::TargetContext,
    tests::Fixture,
};
use std::{fs, path::PathBuf};

struct Setup {
    fixture: Fixture,
    repo: Repository,
    context: TargetContext,
    skill_id: String,
}
impl Setup {
    fn new() -> Self {
        let fixture = Fixture::new();
        let root = &fixture.0;
        fs::create_dir(root.join("home")).unwrap();
        fs::create_dir(root.join("project")).unwrap();
        fs::create_dir_all(root.join("source/manual/demo/references")).unwrap();
        fs::write(
            root.join("source/manual/demo/SKILL.md"),
            "---\nname: fixture-demo\ndescription: Test fixture.\n---\n# Demo\n",
        )
        .unwrap();
        fs::write(
            root.join("source/manual/demo/references/guide.md"),
            "original reference",
        )
        .unwrap();
        let repo = Repository::open(&root.join("data"), &root.join("coord")).unwrap();
        let source = repo.add_local(&root.join("source")).unwrap();
        let context = TargetContext::new(&root.join("home"), Some(&root.join("project"))).unwrap();
        let skill_id = catalog::encode_id(&source.id, "manual/demo/SKILL.md");
        Self {
            fixture,
            repo,
            context,
            skill_id,
        }
    }
    fn request(
        &self,
        mode: InstallMode,
        policy: ConflictPolicy,
        targets: &[&str],
    ) -> InstallRequest {
        InstallRequest {
            skill_id: self.skill_id.clone(),
            mode,
            conflict_policy: policy,
            target_ids: targets.iter().map(|s| s.to_string()).collect(),
            project_path: self
                .context
                .project()
                .map(|p| p.to_string_lossy().into_owned()),
        }
    }
    fn apply(&self, plan_id: &str) -> installers::InstallReceipt {
        let mut op = Operation::begin(&self.repo, "src-fixture", "install").unwrap();
        let result = installers::apply(&self.repo, &self.context, plan_id, &mut op).unwrap();
        op.finish(Ok(())).unwrap();
        result
    }
    fn destination(&self, tool: &str, scope: &str) -> PathBuf {
        self.context.selected(&[format!("{tool}-{scope}")]).unwrap()[0]
            .skills_dir
            .join("fixture-demo")
    }
}

#[test]
fn copy_install_is_idempotent_and_uninstalls_after_source_removal() {
    let s = Setup::new();
    let plan = installers::plan_install(
        &s.repo,
        &s.context,
        &s.request(
            InstallMode::Copy,
            ConflictPolicy::Fail,
            &["codex-project", "claude-code-global", "amp-project"],
        ),
    )
    .unwrap();
    assert_eq!(plan.items.len(), 3);
    assert!(plan.items.iter().all(|i| i.decision == "install"));
    let result = s.apply(&plan.plan_id);
    assert!(
        result.items.iter().all(|i| i.status == "installed"),
        "{result:?}"
    );
    assert_eq!(
        s.apply(&plan.plan_id).operation_id,
        result.operation_id,
        "retry returns original receipt"
    );
    assert!(s
        .destination("codex", "project")
        .join("references/guide.md")
        .is_file());
    let entries = installers::inventory(&s.repo, &s.context).unwrap();
    assert_eq!(
        entries.iter().filter(|s| s.installation.is_some()).count(),
        3
    );
    let ids = entries
        .iter()
        .filter_map(|s| s.installation.as_ref().map(|e| e.installation_id.clone()))
        .collect();
    s.repo
        .remove_source(&s.repo.load().unwrap().sources[0].id)
        .unwrap();
    let removal = installers::plan_uninstall(
        &s.repo,
        &s.context,
        &UninstallRequest {
            installation_ids: ids,
            project_path: None,
        },
    )
    .unwrap();
    assert_eq!(removal.action, PlanAction::Uninstall);
    assert!(s
        .apply(&removal.plan_id)
        .items
        .iter()
        .all(|i| i.status == "uninstalled"));
    assert!(!s.destination("codex", "project").exists());
    assert!(s.fixture.0.join("source/manual/demo/SKILL.md").exists());
}

#[cfg(unix)]
#[test]
fn symlink_install_removes_only_the_link_and_keeps_backups_outside_discovery() {
    let s = Setup::new();
    let plan = installers::plan_install(
        &s.repo,
        &s.context,
        &s.request(
            InstallMode::Symlink,
            ConflictPolicy::Fail,
            &["codex-project", "chatgpt-project"],
        ),
    )
    .unwrap();
    assert_eq!(plan.items.len(), 1);
    assert_eq!(s.apply(&plan.plan_id).items[0].status, "installed");
    let dest = s.destination("codex", "project");
    assert!(dest.is_symlink());
    fs::write(
        s.fixture.0.join("source/manual/demo/references/guide.md"),
        "live edit",
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(dest.join("references/guide.md")).unwrap(),
        "live edit"
    );
    let entry = installers::inventory(&s.repo, &s.context)
        .unwrap()
        .into_iter()
        .find_map(|s| s.installation)
        .unwrap();
    let removal = installers::plan_uninstall(
        &s.repo,
        &s.context,
        &UninstallRequest {
            installation_ids: vec![entry.installation_id],
            project_path: None,
        },
    )
    .unwrap();
    assert_eq!(s.apply(&removal.plan_id).items[0].status, "uninstalled");
    assert!(fs::symlink_metadata(&dest).is_err());
    assert!(s.fixture.0.join("source/manual/demo/SKILL.md").exists());
    assert!(catalog::scan("src-installed", dest.parent().unwrap())
        .unwrap()
        .skills
        .is_empty());
}

#[test]
fn unmanaged_content_and_edits_after_preview_cannot_be_overwritten() {
    let s = Setup::new();
    let dest = s.destination("codex", "project");
    fs::create_dir_all(&dest).unwrap();
    fs::write(dest.join("SKILL.md"), "user content").unwrap();
    let plan = installers::plan_install(
        &s.repo,
        &s.context,
        &s.request(
            InstallMode::Copy,
            ConflictPolicy::Overwrite,
            &["codex-project"],
        ),
    )
    .unwrap();
    assert_eq!(plan.items[0].decision, "blocked");
    assert_eq!(s.apply(&plan.plan_id).items[0].status, "conflict");
    assert_eq!(
        fs::read_to_string(dest.join("SKILL.md")).unwrap(),
        "user content"
    );
    let plan = installers::plan_install(
        &s.repo,
        &s.context,
        &s.request(InstallMode::Copy, ConflictPolicy::Fail, &["amp-project"]),
    )
    .unwrap();
    fs::write(
        s.fixture.0.join("source/manual/demo/references/guide.md"),
        "changed after preview",
    )
    .unwrap();
    assert_eq!(s.apply(&plan.plan_id).items[0].status, "conflict");
    assert!(!s.destination("amp", "project").exists());
}

#[test]
fn modified_copy_is_protected_until_explicit_replacement_and_backup_is_retained() {
    let s = Setup::new();
    let first = installers::plan_install(
        &s.repo,
        &s.context,
        &s.request(InstallMode::Copy, ConflictPolicy::Fail, &["codex-project"]),
    )
    .unwrap();
    assert_eq!(s.apply(&first.plan_id).items[0].status, "installed");
    let dest = s.destination("codex", "project");
    fs::write(dest.join("my-notes.md"), "retain user notes").unwrap();
    let status = installers::statuses(&s.repo, &s.context, &s.skill_id)
        .unwrap()
        .into_iter()
        .find(|s| s.target.id == "codex-project")
        .unwrap();
    assert_eq!(status.state, "modified");
    let removal = installers::plan_uninstall(
        &s.repo,
        &s.context,
        &UninstallRequest {
            installation_ids: vec![status.installation.unwrap().installation_id],
            project_path: None,
        },
    )
    .unwrap();
    assert_eq!(removal.items[0].decision, "blocked");
    let replacement = installers::plan_install(
        &s.repo,
        &s.context,
        &s.request(
            InstallMode::Copy,
            ConflictPolicy::Overwrite,
            &["codex-project"],
        ),
    )
    .unwrap();
    assert!(replacement.items[0]
        .message
        .as_ref()
        .unwrap()
        .contains("Modified"));
    assert_eq!(s.apply(&replacement.plan_id).items[0].status, "installed");
    let backup =
        super::transactions::artifact(&dest, &format!("{}-0", replacement.plan_id), 0, "backup")
            .unwrap();
    assert_eq!(
        fs::read_to_string(backup.join("my-notes.md")).unwrap(),
        "retain user notes"
    );
    assert!(!backup.starts_with(dest.parent().unwrap()));
}

#[test]
fn legacy_manifests_are_not_implicitly_adopted() {
    let s = Setup::new();
    let dir = s
        .destination("codex", "project")
        .parent()
        .unwrap()
        .to_path_buf();
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(".skills-linker-manifest.json");
    let original = include_str!("../../tests/fixtures/skills/legacy-install-manifest.json");
    fs::write(&path, original).unwrap();
    let plan = installers::plan_install(
        &s.repo,
        &s.context,
        &s.request(
            InstallMode::Copy,
            ConflictPolicy::Overwrite,
            &["codex-project"],
        ),
    )
    .unwrap();
    assert_eq!(plan.items[0].decision, "blocked");
    assert_eq!(s.apply(&plan.plan_id).items[0].status, "conflict");
    assert_eq!(fs::read_to_string(path).unwrap(), original);
}

#[cfg(unix)]
#[test]
fn escaping_target_is_disabled_without_hiding_safe_targets() {
    let s = Setup::new();
    let outside = files::private_directory(&s.fixture.0.join("outside")).unwrap();
    std::os::unix::fs::symlink(&outside, s.fixture.0.join("home/.codex")).unwrap();
    let targets = s.context.targets().unwrap();
    assert!(targets
        .iter()
        .find(|t| t.id == "codex-global")
        .unwrap()
        .blocked_reason
        .is_some());
    assert!(s.context.selected(&["codex-global".into()]).is_err());
    assert!(s.context.selected(&["claude-code-project".into()]).is_ok());
}

#[test]
fn fingerprints_distinguish_directory_structure_and_user_added_links() {
    let f = Fixture::new();
    for root in ["a", "b"] {
        fs::create_dir_all(f.0.join(root).join("nested")).unwrap();
    }
    fs::write(f.0.join("a/nested/file"), "same").unwrap();
    fs::write(f.0.join("b/file"), "same").unwrap();
    assert_ne!(
        package::tree_digest(&f.0.join("a"), false).unwrap(),
        package::tree_digest(&f.0.join("b"), false).unwrap()
    );
    assert_eq!(
        installers::validate_name("../escape").unwrap_err().code,
        ErrorCode::InvalidInput
    );
}

#[test]
fn batch_uninstall_updates_a_shared_manifest_without_losing_other_records() {
    let mut s = Setup::new();
    let first = installers::plan_install(
        &s.repo,
        &s.context,
        &s.request(InstallMode::Copy, ConflictPolicy::Fail, &["codex-project"]),
    )
    .unwrap();
    assert_eq!(s.apply(&first.plan_id).items[0].status, "installed");
    let folder = s.fixture.0.join("source/manual/second");
    fs::create_dir(&folder).unwrap();
    fs::write(
        folder.join("SKILL.md"),
        "---\nname: fixture-second\n---\n# Second\n",
    )
    .unwrap();
    let source = s.repo.load().unwrap().sources[0].id.clone();
    s.skill_id = catalog::encode_id(&source, "manual/second/SKILL.md");
    let second = installers::plan_install(
        &s.repo,
        &s.context,
        &s.request(InstallMode::Copy, ConflictPolicy::Fail, &["codex-project"]),
    )
    .unwrap();
    assert_eq!(s.apply(&second.plan_id).items[0].status, "installed");
    let ids = installers::inventory(&s.repo, &s.context)
        .unwrap()
        .into_iter()
        .filter_map(|status| status.installation.map(|entry| entry.installation_id))
        .collect();
    let remove = installers::plan_uninstall(
        &s.repo,
        &s.context,
        &UninstallRequest {
            installation_ids: ids,
            project_path: None,
        },
    )
    .unwrap();
    let result = s.apply(&remove.plan_id);
    assert_eq!(result.items.len(), 2);
    assert!(
        result.items.iter().all(|i| i.status == "uninstalled"),
        "{result:?}"
    );
    assert!(installers::inventory(&s.repo, &s.context)
        .unwrap()
        .is_empty());
}

#[test]
fn plans_capture_target_and_reject_later_manifest_or_context_changes() {
    let s = Setup::new();
    let plan = installers::plan_install(
        &s.repo,
        &s.context,
        &s.request(InstallMode::Copy, ConflictPolicy::Fail, &["codex-project"]),
    )
    .unwrap();
    let another = s.fixture.0.join("another-project");
    fs::create_dir(&another).unwrap();
    let context = TargetContext::new(&s.fixture.0.join("home"), Some(&another)).unwrap();
    let mut operation = Operation::begin(&s.repo, "src-fixture", "install").unwrap();
    assert!(installers::apply(&s.repo, &context, &plan.plan_id, &mut operation).is_err());
    drop(operation);
    let destination = s.destination("codex", "project");
    fs::create_dir_all(destination.parent().unwrap()).unwrap();
    fs::write(
        destination
            .parent()
            .unwrap()
            .join(".skills-linker-manifest.json"),
        "user metadata",
    )
    .unwrap();
    assert_eq!(s.apply(&plan.plan_id).items[0].status, "conflict");
    assert!(!destination.exists());
}

#[test]
fn pure_file_operation_defers_application_update_and_cancel_preserves_targets() {
    let s = Setup::new();
    let plan = installers::plan_install(
        &s.repo,
        &s.context,
        &s.request(InstallMode::Copy, ConflictPolicy::Fail, &["codex-project"]),
    )
    .unwrap();
    let mut operation = Operation::begin(&s.repo, "src-fixture", "install").unwrap();
    assert!(s.repo.coordinator.try_update().is_err());
    super::operations::cancel(&s.repo, &operation.record.operation_id).unwrap();
    assert_eq!(
        installers::apply(&s.repo, &s.context, &plan.plan_id, &mut operation)
            .unwrap()
            .items[0]
            .status,
        "cancelled"
    );
    drop(operation);
    assert!(!s.destination("codex", "project").exists());
    assert!(s.repo.coordinator.try_update().is_ok());
}

#[test]
fn status_reads_wait_for_a_short_commit_and_cancel_is_not_blocked_by_writer() {
    let s = Setup::new();
    let operation = Operation::begin(&s.repo, "src-fixture", "install").unwrap();
    let lease = s.repo.coordinator.enter().unwrap();
    let writer = s.repo.coordinator.writer(&lease).unwrap();
    super::operations::cancel(&s.repo, &operation.record.operation_id).unwrap();
    assert_eq!(
        operation.check_cancelled().unwrap_err().code,
        ErrorCode::Cancelled
    );
    let repo = s.repo.clone();
    let context = s.context.clone();
    let skill = s.skill_id.clone();
    let read = std::thread::spawn(move || installers::statuses(&repo, &context, &skill));
    std::thread::sleep(std::time::Duration::from_millis(40));
    drop(writer);
    assert!(read.join().unwrap().is_ok());
}

#[test]
fn backups_are_ignored_by_git_and_not_discoverable_as_installed_skills() {
    let s = Setup::new();
    let project = s.fixture.0.join("project");
    let init = std::process::Command::new("git")
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
            "init",
            "-q",
        ])
        .current_dir(&project)
        .status()
        .unwrap();
    assert!(init.success());
    let plan = installers::plan_install(
        &s.repo,
        &s.context,
        &s.request(InstallMode::Copy, ConflictPolicy::Fail, &["codex-project"]),
    )
    .unwrap();
    s.apply(&plan.plan_id);
    let installed = installers::inventory(&s.repo, &s.context)
        .unwrap()
        .into_iter()
        .find_map(|s| s.installation)
        .unwrap();
    let remove = installers::plan_uninstall(
        &s.repo,
        &s.context,
        &UninstallRequest {
            installation_ids: vec![installed.installation_id],
            project_path: None,
        },
    )
    .unwrap();
    s.apply(&remove.plan_id);
    let output = std::process::Command::new("git")
        .args([
            "-c",
            "core.fsmonitor=false",
            "status",
            "--porcelain",
            "--untracked-files=all",
        ])
        .current_dir(&project)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).contains(".loom-skill-transactions"));
    assert!(catalog::scan(
        "src-installed",
        s.destination("codex", "project").parent().unwrap()
    )
    .unwrap()
    .skills
    .is_empty());
}
