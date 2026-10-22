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
            with_slash_commands: false,
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

fn auto_setup() -> Setup {
    let mut s = Setup::new();
    fs::rename(
        s.fixture.0.join("source/manual"),
        s.fixture.0.join("source/auto"),
    )
    .unwrap();
    fs::write(
        s.fixture.0.join("source/auto/demo/inject.md"),
        "Shared instruction\n",
    )
    .unwrap();
    let source = s.repo.load().unwrap().sources[0].id.clone();
    s.skill_id = catalog::encode_id(&source, "auto/demo/SKILL.md");
    s
}
fn install_auto(s: &Setup, targets: &[&str], policy: ConflictPolicy) -> installers::InstallPlan {
    let plan = installers::plan_install(
        &s.repo,
        &s.context,
        &s.request(InstallMode::Copy, policy, targets),
    )
    .unwrap();
    assert!(
        plan.items.iter().all(|i| i.decision != "blocked"),
        "{plan:?}"
    );
    assert!(s
        .apply(&plan.plan_id)
        .items
        .iter()
        .all(|i| i.status == "installed"));
    plan
}
fn remove_target(s: &Setup, repo: &Repository, id: &str) -> installers::InstallReceipt {
    let inventory = installers::inventory(repo, &s.context).unwrap();
    let installed = inventory
        .iter()
        .find(|i| i.target.id == id)
        .unwrap()
        .installation
        .as_ref()
        .unwrap();
    let plan = installers::plan_uninstall(
        repo,
        &s.context,
        &UninstallRequest {
            installation_ids: vec![installed.installation_id.clone()],
            project_path: None,
        },
    )
    .unwrap();
    let mut op = Operation::begin(repo, "fixture", "uninstall").unwrap();
    let result = installers::apply(repo, &s.context, &plan.plan_id, &mut op).unwrap();
    op.finish(Ok(())).unwrap();
    result
}

#[test]
fn shared_instructions_keep_other_users_and_preserve_crlf_and_permissions() {
    let s = auto_setup();
    let file = s.fixture.0.join("project/AGENTS.md");
    let original = "User rules\r\n<!-- skills-linker:foreign:begin -->\r\nForeign rule\r\n<!-- skills-linker:foreign:end -->\r\n";
    fs::write(&file, original).unwrap();
    package::set_mode(&file, 0o640).unwrap();
    let first = install_auto(&s, &["codex-project", "amp-project"], ConflictPolicy::Fail);
    assert!(first.items.iter().all(|i| i.resource_changes.len() == 2));
    let installed = fs::read_to_string(&file).unwrap();
    assert_eq!(
        installed
            .matches("skills-linker:fixture-demo:begin")
            .count(),
        1
    );
    assert!(!installed.replace("\r\n", "").contains('\n'));
    // Replacement adds no duplicate user and does not change a peer's block.
    install_auto(&s, &["codex-project"], ConflictPolicy::Overwrite);
    assert_eq!(
        remove_target(&s, &s.repo, "codex-project").items[0].status,
        "uninstalled"
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), installed);
    assert_eq!(
        remove_target(&s, &s.repo, "amp-project").items[0].status,
        "uninstalled"
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), original);
    assert!(matches!(
        package::state(&file).unwrap(),
        package::EntryState::File { mode: 0o640, .. }
    ));
}

#[test]
fn another_data_directory_shares_ownership_and_unknown_peers_prevent_deletion() {
    let s = auto_setup();
    install_auto(&s, &["codex-project"], ConflictPolicy::Fail);
    let other =
        Repository::open(&s.fixture.0.join("other-data"), &s.fixture.0.join("coord")).unwrap();
    other.add_local(&s.fixture.0.join("source")).unwrap();
    let plan = installers::plan_install(
        &other,
        &s.context,
        &s.request(InstallMode::Copy, ConflictPolicy::Fail, &["amp-project"]),
    )
    .unwrap();
    let mut op = Operation::begin(&other, "fixture", "install").unwrap();
    assert_eq!(
        installers::apply(&other, &s.context, &plan.plan_id, &mut op)
            .unwrap()
            .items[0]
            .status,
        "installed"
    );
    op.finish(Ok(())).unwrap();
    drop(op);
    let file = s.fixture.0.join("project/AGENTS.md");
    let original = fs::read(&file).unwrap();
    let peer = s
        .fixture
        .0
        .join("project/.codex/skills/.skills-linker-manifest.json");
    let bytes = fs::read(&peer).unwrap();
    fs::remove_file(&peer).unwrap();
    assert_eq!(
        remove_target(&s, &other, "amp-project").items[0].status,
        "conflict"
    );
    assert_eq!(fs::read(&file).unwrap(), original);
    fs::write(&peer, bytes).unwrap();
    assert_eq!(
        remove_target(&s, &other, "amp-project").items[0].status,
        "uninstalled"
    );
    assert_eq!(fs::read(&file).unwrap(), original);
    assert_eq!(
        remove_target(&s, &s.repo, "codex-project").items[0].status,
        "uninstalled"
    );
    assert!(!file.exists());
}

#[test]
fn instruction_edits_unknown_markers_and_shared_version_conflicts_are_preserved() {
    let s = auto_setup();
    let file = s.fixture.0.join("project/AGENTS.md");
    fs::write(&file, "<!-- skills-linker:fixture-demo:begin -->\nUnknown owner\n<!-- skills-linker:fixture-demo:end -->\n").unwrap();
    let old = fs::read(&file).unwrap();
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
    assert_eq!(fs::read(&file).unwrap(), old);
    fs::remove_file(&file).unwrap();
    install_auto(&s, &["codex-project", "amp-project"], ConflictPolicy::Fail);
    fs::write(
        s.fixture.0.join("source/auto/demo/inject.md"),
        "Changed instruction",
    )
    .unwrap();
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
    let installed = fs::read_to_string(&file).unwrap();
    fs::write(&file, installed.replace("Shared instruction", "User edit")).unwrap();
    assert_eq!(
        remove_target(&s, &s.repo, "amp-project").items[0].status,
        "conflict"
    );
    assert!(fs::read_to_string(&file).unwrap().contains("User edit"));
}

#[cfg(unix)]
#[test]
fn scoped_instruction_symlink_is_preserved_but_escape_and_stale_preview_are_blocked() {
    let s = auto_setup();
    let file = s.fixture.0.join("project/AGENTS.md");
    let real = s.fixture.0.join("project/rules.md");
    fs::write(&real, "User\n").unwrap();
    std::os::unix::fs::symlink(&real, &file).unwrap();
    install_auto(&s, &["codex-project"], ConflictPolicy::Fail);
    assert!(fs::symlink_metadata(&file)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(
        remove_target(&s, &s.repo, "codex-project").items[0].status,
        "uninstalled"
    );
    assert_eq!(fs::read_to_string(&real).unwrap(), "User\n");
    let plan = installers::plan_install(
        &s.repo,
        &s.context,
        &s.request(InstallMode::Copy, ConflictPolicy::Fail, &["codex-project"]),
    )
    .unwrap();
    fs::write(&real, "Edited after preview\n").unwrap();
    assert_eq!(s.apply(&plan.plan_id).items[0].status, "conflict");
    fs::remove_file(&file).unwrap();
    let outside = s.fixture.0.join("outside.md");
    fs::write(&outside, "Outside").unwrap();
    std::os::unix::fs::symlink(&outside, &file).unwrap();
    let plan = installers::plan_install(
        &s.repo,
        &s.context,
        &s.request(InstallMode::Copy, ConflictPolicy::Fail, &["codex-project"]),
    )
    .unwrap();
    assert_eq!(plan.items[0].decision, "blocked");
    assert_eq!(fs::read_to_string(&outside).unwrap(), "Outside");
}

#[test]
fn wrappers_are_opt_in_point_to_installed_packages_and_preserve_user_files() {
    let s = Setup::new();
    let mut request = s.request(
        InstallMode::Copy,
        ConflictPolicy::Fail,
        &["claude-code-project"],
    );
    request.with_slash_commands = true;
    let plan = installers::plan_install(&s.repo, &s.context, &request).unwrap();
    assert_eq!(s.apply(&plan.plan_id).items[0].status, "installed");
    let wrapper = s.fixture.0.join("project/.claude/commands/fixture-demo.md");
    let text = fs::read_to_string(&wrapper).unwrap();
    assert!(text.contains("$ARGUMENTS"));
    assert!(text.contains(".claude/skills/fixture-demo/SKILL.md"));
    assert!(text.contains("<!-- skills-linker:slash:fixture-demo -->"));
    s.repo
        .remove_source(&s.repo.load().unwrap().sources[0].id)
        .unwrap();
    assert_eq!(
        remove_target(&s, &s.repo, "claude-code-project").items[0].status,
        "uninstalled"
    );
    assert!(!wrapper.exists());
    s.repo.add_local(&s.fixture.0.join("source")).unwrap();
    request.conflict_policy = ConflictPolicy::Overwrite;
    for existing in ["My manual command", ""] {
        fs::write(&wrapper, existing).unwrap();
        assert_eq!(
            installers::plan_install(&s.repo, &s.context, &request)
                .unwrap()
                .items[0]
                .decision,
            "blocked"
        );
        assert_eq!(fs::read_to_string(&wrapper).unwrap(), existing);
    }
}

#[test]
fn cron_inject_file_never_creates_an_instruction_or_wrapper() {
    let mut s = auto_setup();
    fs::rename(
        s.fixture.0.join("source/auto"),
        s.fixture.0.join("source/cron"),
    )
    .unwrap();
    s.skill_id = catalog::encode_id(&s.repo.load().unwrap().sources[0].id, "cron/demo/SKILL.md");
    let mut request = s.request(InstallMode::Copy, ConflictPolicy::Fail, &["codex-project"]);
    request.with_slash_commands = true;
    let plan = installers::plan_install(&s.repo, &s.context, &request).unwrap();
    assert!(plan.items[0].resource_changes.is_empty());
    assert_eq!(s.apply(&plan.plan_id).items[0].status, "installed");
    assert!(!s.fixture.0.join("project/AGENTS.md").exists());
    assert!(!s
        .fixture
        .0
        .join("project/.codex/prompts/fixture-demo.md")
        .exists());
}

#[test]
fn doctor_detects_local_live_injection_changes_without_writing() {
    let s = auto_setup();
    let plan = installers::plan_install(
        &s.repo,
        &s.context,
        &s.request(
            InstallMode::Symlink,
            ConflictPolicy::Fail,
            &["codex-project"],
        ),
    )
    .unwrap();
    assert_eq!(s.apply(&plan.plan_id).items[0].status, "installed");
    let file = s.fixture.0.join("project/AGENTS.md");
    let before = fs::read(&file).unwrap();
    let checks = installers::inventory(&s.repo, &s.context).unwrap();
    assert_eq!(
        checks
            .iter()
            .find(|s| s.installation.is_some())
            .unwrap()
            .instruction_checks[0]
            .state,
        "current"
    );
    fs::write(
        s.fixture.0.join("source/auto/demo/inject.md"),
        "New live rule\n",
    )
    .unwrap();
    let checks = installers::inventory(&s.repo, &s.context).unwrap();
    assert_eq!(
        checks
            .iter()
            .find(|s| s.installation.is_some())
            .unwrap()
            .instruction_checks[0]
            .state,
        "stale"
    );
    assert_eq!(fs::read(&file).unwrap(), before);
    let plan = installers::plan_install(
        &s.repo,
        &s.context,
        &s.request(
            InstallMode::Symlink,
            ConflictPolicy::Overwrite,
            &["codex-project"],
        ),
    )
    .unwrap();
    assert_eq!(s.apply(&plan.plan_id).items[0].status, "installed");
    assert!(fs::read_to_string(file).unwrap().contains("New live rule"));
}

#[test]
fn batch_uninstall_of_shared_instruction_users_removes_block_once() {
    let s = auto_setup();
    install_auto(&s, &["codex-project", "amp-project"], ConflictPolicy::Fail);
    let ids = installers::inventory(&s.repo, &s.context)
        .unwrap()
        .iter()
        .filter_map(|i| i.installation.as_ref().map(|v| v.installation_id.clone()))
        .collect();
    let plan = installers::plan_uninstall(
        &s.repo,
        &s.context,
        &UninstallRequest {
            installation_ids: ids,
            project_path: None,
        },
    )
    .unwrap();
    let result = s.apply(&plan.plan_id);
    assert_eq!(result.items.len(), 2);
    assert!(
        result.items.iter().all(|i| i.status == "uninstalled"),
        "{result:?}"
    );
    assert!(!s.fixture.0.join("project/AGENTS.md").exists());
}

#[test]
fn coordinated_shared_update_requires_all_users_and_changes_the_whole_group() {
    let s = auto_setup();
    install_auto(&s, &["codex-project", "amp-project"], ConflictPolicy::Fail);
    fs::write(
        s.fixture.0.join("source/auto/demo/inject.md"),
        "Shared revision B\n",
    )
    .unwrap();
    fs::write(
        s.fixture.0.join("source/auto/demo/references/guide.md"),
        "package B",
    )
    .unwrap();
    let partial = installers::plan_install(
        &s.repo,
        &s.context,
        &s.request(
            InstallMode::Copy,
            ConflictPolicy::Overwrite,
            &["codex-project"],
        ),
    )
    .unwrap();
    assert_eq!(partial.items[0].decision, "blocked");
    let full = installers::plan_install(
        &s.repo,
        &s.context,
        &s.request(
            InstallMode::Copy,
            ConflictPolicy::Overwrite,
            &["codex-project", "amp-project"],
        ),
    )
    .unwrap();
    assert!(full.coordinated);
    assert!(
        full.items.iter().all(|i| i.decision == "replace"),
        "{full:?}"
    );
    let receipt = s.apply(&full.plan_id);
    assert!(
        receipt.items.iter().all(|i| i.status == "installed"),
        "{receipt:?}"
    );
    assert_eq!(s.apply(&full.plan_id).operation_id, receipt.operation_id);
    let entries = installers::inventory(&s.repo, &s.context).unwrap();
    let mut digests = Vec::new();
    for entry in entries.iter().filter(|e| e.installation.is_some()) {
        assert_eq!(
            fs::read_to_string(entry.destination.join("references/guide.md")).unwrap(),
            "package B"
        );
        assert_eq!(entry.instruction_checks[0].state, "current");
        digests.push(
            entry.installation.as_ref().unwrap().managed_resources[0]
                .content_digest
                .clone(),
        );
    }
    assert_eq!(digests.len(), 2);
    assert_eq!(digests[0], digests[1]);
    let file = s.fixture.0.join("project/AGENTS.md");
    assert!(fs::read_to_string(&file)
        .unwrap()
        .contains("Shared revision B"));
    assert_eq!(
        remove_target(&s, &s.repo, "codex-project").items[0].status,
        "uninstalled"
    );
    assert!(fs::read_to_string(&file)
        .unwrap()
        .contains("Shared revision B"));
    assert_eq!(
        remove_target(&s, &s.repo, "amp-project").items[0].status,
        "uninstalled"
    );
    assert!(!file.exists());
}

#[test]
fn a_stale_group_preview_cannot_update_the_other_members() {
    let s = auto_setup();
    install_auto(&s, &["codex-project", "amp-project"], ConflictPolicy::Fail);
    fs::write(s.fixture.0.join("source/auto/demo/inject.md"), "B\n").unwrap();
    let plan = installers::plan_install(
        &s.repo,
        &s.context,
        &s.request(
            InstallMode::Copy,
            ConflictPolicy::Overwrite,
            &["codex-project", "amp-project"],
        ),
    )
    .unwrap();
    assert!(plan.coordinated);
    let codex = s.destination("codex", "project");
    let amp = s.destination("amp", "project");
    let original = package::state(&codex).unwrap();
    let instructions = fs::read(s.fixture.0.join("project/AGENTS.md")).unwrap();
    fs::write(amp.join("user-note.md"), "User edit after preview").unwrap();
    let result = s.apply(&plan.plan_id);
    assert!(
        result.items.iter().all(|i| i.status == "conflict"),
        "{result:?}"
    );
    assert_eq!(package::state(&codex).unwrap(), original);
    assert_eq!(
        fs::read(s.fixture.0.join("project/AGENTS.md")).unwrap(),
        instructions
    );
    assert_eq!(
        fs::read_to_string(amp.join("user-note.md")).unwrap(),
        "User edit after preview"
    );
}

#[test]
fn coordinated_update_recovers_both_packages_manifests_and_shared_resources() {
    use super::transactions;
    for stop in 0..12 {
        let s = auto_setup();
        install_auto(&s, &["codex-project", "amp-project"], ConflictPolicy::Fail);
        fs::write(s.fixture.0.join("source/auto/demo/inject.md"), "B\n").unwrap();
        let plan = installers::plan_install(
            &s.repo,
            &s.context,
            &s.request(
                InstallMode::Copy,
                ConflictPolicy::Overwrite,
                &["codex-project", "amp-project"],
            ),
        )
        .unwrap();
        let operation = Operation::begin(&s.repo, "fixture", "install").unwrap();
        let lease = s.repo.coordinator.enter().unwrap();
        let writer = s.repo.coordinator.writer(&lease).unwrap();
        let id = format!("{}-group", plan.plan_id);
        let (pending, roots) = installers::prepare_items(
            &s.repo,
            &s.context,
            &writer,
            &plan,
            &plan.items.iter().collect::<Vec<_>>(),
            &id,
            &operation,
        )
        .unwrap();
        let prepared: transactions::Prepared = files::read_json(&pending.journal_path).unwrap();
        assert_eq!(prepared.changes.len(), 6);
        assert!(
            transactions::commit_observed(&writer, &pending, &roots, |step| if step == stop {
                Err(super::error::SkillsError::invalid(
                    "interrupted group commit",
                ))
            } else {
                Ok(())
            })
            .is_err()
        );
        drop(writer);
        drop(lease);
        drop(operation);
        let other = Repository::open(
            &s.fixture.0.join("data-recovery"),
            &s.fixture.0.join("coord"),
        )
        .unwrap();
        assert_eq!(
            installers::recover_one(&other, &s.context, &id).unwrap(),
            transactions::Outcome::RolledBack
        );
        for change in prepared.changes {
            assert_eq!(
                package::state(&change.target).unwrap(),
                change.before,
                "failure at {stop}"
            );
        }
        assert!(installers::recovery_items(&other, &s.context)
            .unwrap()
            .is_empty());
        let entries = installers::inventory(&other, &s.context).unwrap();
        assert_eq!(
            entries.iter().filter(|s| s.installation.is_some()).count(),
            2
        );
        assert!(entries
            .iter()
            .filter(|s| s.installation.is_some())
            .all(|s| s.instruction_checks[0].state == "current"));
    }
}

#[test]
fn global_auto_instructions_use_tool_specific_paths_and_restore_existing_rules() {
    let s = auto_setup();
    let paths = [
        s.fixture.0.join("home/.codex/AGENTS.md"),
        s.fixture.0.join("home/.claude/CLAUDE.md"),
        s.fixture.0.join("home/.config/amp/AGENTS.md"),
    ];
    for path in &paths {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "Existing global rules\n").unwrap();
    }
    let plan = install_auto(
        &s,
        &["codex-global", "claude-code-global", "amp-global"],
        ConflictPolicy::Fail,
    );
    assert!(
        !plan.coordinated,
        "independent instruction files retain per-target commits"
    );
    for path in &paths {
        let text = fs::read_to_string(path).unwrap();
        assert!(text.starts_with("Existing global rules\n"));
        assert_eq!(text.matches("skills-linker:fixture-demo:begin").count(), 1);
    }
    assert!(!s.fixture.0.join("home/AGENTS.md").exists());
    for target in ["amp-global", "claude-code-global", "codex-global"] {
        assert_eq!(
            remove_target(&s, &s.repo, target).items[0].status,
            "uninstalled"
        );
    }
    for path in paths {
        assert_eq!(fs::read_to_string(path).unwrap(), "Existing global rules\n");
    }
}

#[test]
fn missing_and_malformed_instruction_diagnoses_do_not_write_or_repair() {
    let s = auto_setup();
    install_auto(&s, &["codex-project"], ConflictPolicy::Fail);
    let file = s.fixture.0.join("project/AGENTS.md");
    fs::remove_file(&file).unwrap();
    let entries = installers::inventory(&s.repo, &s.context).unwrap();
    assert_eq!(
        entries
            .iter()
            .find(|s| s.installation.is_some())
            .unwrap()
            .instruction_checks[0]
            .state,
        "missing"
    );
    assert!(!file.exists());
    let broken = "User rules\n<!-- skills-linker:fixture-demo:begin -->\nMissing end\n";
    fs::write(&file, broken).unwrap();
    let entries = installers::statuses(&s.repo, &s.context, &s.skill_id).unwrap();
    assert_eq!(
        entries
            .iter()
            .find(|s| s.installation.is_some())
            .unwrap()
            .instruction_checks[0]
            .state,
        "malformed"
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), broken);
}

#[test]
fn reviewed_multi_skill_plans_carry_only_their_verified_metadata_changes() {
    let s = auto_setup();
    let other = s.fixture.0.join("source/auto/other");
    fs::create_dir(&other).unwrap();
    fs::write(other.join("SKILL.md"), "---\nname: other\n---\n# Other\n").unwrap();
    fs::write(other.join("inject.md"), "Other rule\n").unwrap();
    let source = s.repo.load().unwrap().sources[0].id.clone();
    let mut request = s.request(InstallMode::Copy, ConflictPolicy::Fail, &["codex-project"]);
    let first = installers::plan_install(&s.repo, &s.context, &request).unwrap();
    request.skill_id = catalog::encode_id(&source, "auto/other/SKILL.md");
    let second = installers::plan_install(&s.repo, &s.context, &request).unwrap();
    let results =
        installers::apply_reviewed_batch(&s.repo, &s.context, &[first.plan_id, second.plan_id]);
    assert_eq!(results.len(), 2);
    for result in results {
        assert!(result
            .result
            .unwrap()
            .items
            .iter()
            .all(|i| i.status == "installed"));
    }
    let rules = fs::read_to_string(s.fixture.0.join("project/AGENTS.md")).unwrap();
    assert!(rules.contains("Shared instruction"));
    assert!(rules.contains("Other rule"));
    let entries = installers::inventory(&s.repo, &s.context).unwrap();
    assert_eq!(
        entries.iter().filter(|e| e.installation.is_some()).count(),
        2
    );
}

#[test]
fn reviewed_batch_does_not_treat_external_instruction_edits_as_its_own_commit() {
    let s = auto_setup();
    let other = s.fixture.0.join("source/auto/other");
    fs::create_dir(&other).unwrap();
    fs::write(other.join("SKILL.md"), "---\nname: other\n---\n# Other\n").unwrap();
    fs::write(other.join("inject.md"), "Other rule\n").unwrap();
    let source = s.repo.load().unwrap().sources[0].id.clone();
    let mut request = s.request(InstallMode::Copy, ConflictPolicy::Fail, &["codex-project"]);
    let first = installers::plan_install(&s.repo, &s.context, &request).unwrap();
    request.skill_id = catalog::encode_id(&source, "auto/other/SKILL.md");
    let second = installers::plan_install(&s.repo, &s.context, &request).unwrap();
    assert_eq!(s.apply(&first.plan_id).items[0].status, "installed");
    let file = s.fixture.0.join("project/AGENTS.md");
    let edited = format!(
        "{}User changed rules after first commit\n",
        fs::read_to_string(&file).unwrap()
    );
    fs::write(&file, &edited).unwrap();
    let result =
        installers::apply_reviewed_batch(&s.repo, &s.context, &[first.plan_id, second.plan_id]);
    assert_eq!(
        result[1].result.as_ref().unwrap().items[0].status,
        "conflict"
    );
    assert_eq!(fs::read_to_string(file).unwrap(), edited);
    assert!(!s.fixture.0.join("project/.codex/skills/other").exists());
}
