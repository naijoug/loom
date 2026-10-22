use super::{
    cli::{self, Options},
    files, installers, operations,
    repository::Repository,
    runtime_paths::RuntimePaths,
    targets::TargetContext,
    tests::Fixture,
};
use serde_json::Value;
use std::fs;
struct Setup {
    fixture: Fixture,
    paths: RuntimePaths,
}
impl Setup {
    fn new() -> Self {
        let fixture = Fixture::new();
        for dir in ["home", "project", "source/auto/demo"] {
            fs::create_dir_all(fixture.0.join(dir)).unwrap();
        }
        fs::write(
            fixture.0.join("source/auto/demo/SKILL.md"),
            "---\nname: demo\ndescription: Test fixture.\n---\n# Demo\n",
        )
        .unwrap();
        fs::write(
            fixture.0.join("source/auto/demo/inject.md"),
            "Original rule\n",
        )
        .unwrap();
        let paths = RuntimePaths {
            home: fixture.0.join("home"),
            app_data: fixture.0.join("app"),
            library: fixture.0.join("data"),
        };
        Self { fixture, paths }
    }
    fn args(&self, command: &str, more: &[&str], source: bool) -> Vec<String> {
        let mut args = vec![
            command.into(),
            "--json".into(),
            "--scope".into(),
            "project".into(),
            "--project-root".into(),
            self.fixture
                .0
                .join("project")
                .to_string_lossy()
                .into_owned(),
        ];
        if source {
            args.extend([
                "--skills-root".into(),
                self.fixture.0.join("source").to_string_lossy().into_owned(),
            ]);
        }
        args.extend(more.iter().map(|s| s.to_string()));
        args
    }
    fn run(&self, command: &str, more: &[&str], source: bool) -> (i32, Value) {
        let out = cli::run_with_paths(
            &Options::parse(&self.args(command, more, source)).unwrap(),
            &self.paths,
        )
        .unwrap();
        (out.exit_code, serde_json::from_str(&out.text).unwrap())
    }
    fn repo(&self) -> Repository {
        self.paths.repository().unwrap()
    }
}
#[test]
fn cli_list_preserves_legacy_json_fields_without_registering_source() {
    let s = Setup::new();
    let (code, out) = s.run("list", &[], true);
    assert_eq!(code, 0);
    assert_eq!(out["skills"], serde_json::json!(["demo"]));
    assert_eq!(out["count"], 1);
    assert_eq!(out["items"][0]["id"], "demo");
    assert_eq!(out["items"][0]["path"], "auto/demo");
    assert_eq!(out["items"][0]["category"], "auto");
    assert!(out["items"][0]["source_skill_id"]
        .as_str()
        .unwrap()
        .contains("::"));
    assert!(!s.paths.library.join("library.json").exists());
    assert!(Options::parse(&["install".into(), "--mode".into()]).is_err());
    assert!(Options::parse(&["install".into(), "--bad".into()]).is_err());
}
#[test]
fn cli_and_desktop_services_share_installs_and_live_instruction_refresh() {
    let s = Setup::new();
    let (code, result) = s.run("install", &["--tool", "chatgpt", "demo"], true);
    assert_eq!(code, 0);
    assert_eq!(result["results"][0]["status"], "installed");
    let dest = s.fixture.0.join("project/.codex/skills/demo");
    assert!(fs::symlink_metadata(&dest)
        .unwrap()
        .file_type()
        .is_symlink());
    let context = TargetContext::new(&s.paths.home, Some(&s.fixture.0.join("project"))).unwrap();
    let inventory = installers::inventory(&s.repo(), &context).unwrap();
    assert_eq!(
        inventory
            .iter()
            .filter(|e| e.installation.is_some())
            .count(),
        1
    );
    assert_eq!(
        s.run("doctor", &["demo"], true).1["results"][0]["status"],
        "current"
    );
    fs::write(
        s.fixture.0.join("source/auto/demo/inject.md"),
        "Updated rule\n",
    )
    .unwrap();
    assert_eq!(s.run("doctor", &["demo"], true).0, 1);
    assert_eq!(s.run("install", &["demo"], true).0, 0);
    assert!(fs::read_to_string(s.fixture.0.join("project/AGENTS.md"))
        .unwrap()
        .contains("Updated rule"));
    assert_eq!(
        s.run("uninstall", &["demo"], false).1["results"][0]["status"],
        "removed"
    );
    assert!(!dest.exists());
    assert!(s.fixture.0.join("source/auto/demo/SKILL.md").exists());
}
#[test]
fn cli_copy_force_and_orphan_uninstall_preserve_unmanaged_content() {
    let s = Setup::new();
    assert_eq!(s.run("install", &["--mode", "copy", "demo"], true).0, 0);
    let dest = s.fixture.0.join("project/.codex/skills/demo");
    fs::write(dest.join("user.md"), "Keep").unwrap();
    assert_eq!(
        s.run("install", &["--mode", "copy", "demo"], true).1["results"][0]["status"],
        "skip"
    );
    assert_eq!(s.run("uninstall", &["--force", "demo"], false).0, 1);
    assert!(dest.join("user.md").exists());
    assert_eq!(
        s.run("install", &["--mode", "copy", "--force", "demo"], true)
            .0,
        0
    );
    let repo = s.repo();
    repo.remove_source(&repo.load().unwrap().sources[0].id)
        .unwrap();
    fs::remove_dir_all(s.fixture.0.join("source")).unwrap();
    assert_eq!(
        s.run("status", &["demo"], false).1["results"][0]["reason"],
        "managed=true"
    );
    assert_eq!(s.run("uninstall", &["demo"], false).0, 0);
    fs::create_dir_all(&dest).unwrap();
    fs::write(dest.join("SKILL.md"), "Foreign").unwrap();
    assert!(cli::run_with_paths(
        &Options::parse(&s.args("uninstall", &["--force", "demo"], false)).unwrap(),
        &s.paths
    )
    .is_err());
    assert_eq!(
        fs::read_to_string(dest.join("SKILL.md")).unwrap(),
        "Foreign"
    );
}
#[test]
fn cli_multiple_tools_use_coordinated_updates_and_record_failures() {
    let s = Setup::new();
    assert_eq!(
        s.run(
            "install",
            &["--mode", "copy", "--tool", "codex,amp", "demo"],
            true
        )
        .0,
        0
    );
    fs::write(
        s.fixture.0.join("source/auto/demo/inject.md"),
        "Revision B\n",
    )
    .unwrap();
    assert_eq!(
        s.run(
            "install",
            &["--mode", "copy", "--force", "--tool", "codex", "demo"],
            true
        )
        .0,
        1
    );
    assert!(operations::list(&s.repo())
        .unwrap()
        .iter()
        .any(|o| o.phase == operations::Phase::Failed));
    assert_eq!(
        s.run(
            "install",
            &["--mode", "copy", "--force", "--tool", "codex,amp", "demo"],
            true
        )
        .0,
        0
    );
    assert!(fs::read_to_string(s.fixture.0.join("project/AGENTS.md"))
        .unwrap()
        .contains("Revision B"));
}
#[test]
fn custom_cli_target_requires_an_explicit_safe_directory_and_has_no_inferred_instructions() {
    let s = Setup::new();
    let path = s.fixture.0.join("custom tool/skills");
    let text = path.to_str().unwrap();
    assert_eq!(
        s.run(
            "install",
            &["--tool", "custom", "--target-dir", text, "demo"],
            true
        )
        .0,
        0
    );
    assert!(path.join("demo/SKILL.md").exists());
    assert!(!s.fixture.0.join("project/AGENTS.md").exists());
    assert_eq!(
        s.run(
            "uninstall",
            &["--tool", "custom", "--target-dir", text, "demo"],
            false
        )
        .0,
        0
    );
    assert!(cli::run_with_paths(
        &Options::parse(&s.args(
            "install",
            &[
                "--target-dir",
                s.fixture.0.join("project").to_str().unwrap(),
                "demo"
            ],
            true
        ))
        .unwrap(),
        &s.paths
    )
    .is_err());
    let normal = s.fixture.0.join("project/.codex/skills");
    assert_eq!(
        s.run(
            "install",
            &["--target-dir", normal.to_str().unwrap(), "demo"],
            true
        )
        .0,
        0
    );
}
#[test]
fn cli_update_admission_and_preview_do_not_mutate_installation_targets() {
    let s = Setup::new();
    let repo = s.repo();
    let update = repo.coordinator.try_update().unwrap();
    assert!(cli::run_with_paths(
        &Options::parse(&s.args("install", &["demo"], true)).unwrap(),
        &s.paths
    )
    .is_err());
    drop(update);
    let (_, out) = s.run("install", &["--preview", "demo"], true);
    assert_eq!(out["plans"].as_array().unwrap().len(), 1);
    assert!(!s.fixture.0.join("project/.codex/skills/demo").exists());
    let another = RuntimePaths {
        library: s.fixture.0.join("other-data"),
        ..s.paths.clone()
    };
    assert_eq!(
        repo.coordinator.root(),
        another.repository().unwrap().coordinator.root()
    );
}
#[test]
fn cli_rejects_ambiguous_short_names_before_installing() {
    let s = Setup::new();
    let second = s.fixture.0.join("source/manual/other");
    fs::create_dir_all(&second).unwrap();
    fs::write(second.join("SKILL.md"), "---\nname: demo\n---\n# Other\n").unwrap();
    let args = s.args("install", &["demo"], true);
    assert!(cli::run_with_paths(&Options::parse(&args).unwrap(), &s.paths).is_err());
    assert!(!s.fixture.0.join("project/.codex/skills/demo").exists());
    let value: Value = files::read_json(&s.paths.library.join("library.json"))
        .unwrap_or(serde_json::json!({"sources":[]}));
    assert_eq!(value["sources"].as_array().unwrap().len(), 0);
}

#[test]
fn cli_missing_uninstall_and_plain_preview_remain_reviewable() {
    let s = Setup::new();
    let (code, value) = s.run("uninstall", &["demo"], false);
    assert_eq!(code, 0);
    assert_eq!(value["results"][0]["status"], "missing");
    assert_eq!(
        value["target_dir"],
        s.fixture
            .0
            .join("project/.codex/skills")
            .to_string_lossy()
            .as_ref()
    );
    let mut args = s.args("install", &["--preview", "demo"], true);
    args.retain(|arg| arg != "--json");
    let output = cli::run_with_paths(&Options::parse(&args).unwrap(), &s.paths).unwrap();
    assert!(output.text.contains("destination"));
    assert!(!s.fixture.0.join("project/.codex/skills/demo").exists());
}

#[test]
fn doctor_without_an_injection_file_is_not_applicable() {
    let s = Setup::new();
    fs::remove_file(s.fixture.0.join("source/auto/demo/inject.md")).unwrap();
    let (code, result) = s.run("doctor", &["demo"], true);
    assert_eq!(code, 0);
    assert_eq!(result["results"][0]["status"], "not-applicable");
    assert!(!s.fixture.0.join("project/AGENTS.md").exists());
}

#[test]
#[ignore = "explicit acceptance: requires LOOM_SKILLS_ACCEPTANCE_BINARY and briefly uses the real user coordination domain"]
fn actual_cli_recovers_a_fixture_from_another_data_directory() {
    use super::{package, transactions};
    let binary = std::path::PathBuf::from(
        std::env::var_os("LOOM_SKILLS_ACCEPTANCE_BINARY")
            .expect("Pass the verified loom-skills binary path"),
    );
    assert!(binary.is_absolute() && binary.is_file());
    let fixture = Fixture::new();
    let project = fixture.0.join("project");
    fs::create_dir(&project).unwrap();
    let paths = RuntimePaths::system(Some(&fixture.0.join("data-a"))).unwrap();
    let repo = paths.repository().unwrap();
    let context = TargetContext::new(&paths.home, Some(&project)).unwrap();
    let target = context
        .selected(&["codex-project".into()])
        .unwrap()
        .remove(0);
    context.ensure(&target).unwrap();
    let destination = target.skills_dir.join("cli-recovery-fixture");
    fs::create_dir(&destination).unwrap();
    fs::write(destination.join("SKILL.md"), "Original fixture").unwrap();
    let id = format!(
        "cli-acceptance-{}-{}",
        std::process::id(),
        crate::models::now_ms()
    );
    transactions::prepare_storage(&destination, &id).unwrap();
    let staged = transactions::artifact(&destination, &id, 0, "staged").unwrap();
    fs::create_dir(&staged).unwrap();
    fs::write(staged.join("SKILL.md"), "Interrupted replacement").unwrap();
    let lease = repo.coordinator.enter().unwrap();
    let writer = repo.coordinator.writer_wait(&lease).unwrap();
    let pending = transactions::prepare(
        &repo,
        &writer,
        &id,
        vec![transactions::Change {
            target: destination.clone(),
            before: package::state(&destination).unwrap(),
            after: package::state(&staged).unwrap(),
            staged,
            backup: transactions::artifact(&destination, &id, 0, "backup").unwrap(),
        }],
    )
    .unwrap();
    let interrupted = transactions::commit_observed(
        &writer,
        &pending,
        std::slice::from_ref(&target.skills_dir),
        |step| {
            if step == 1 {
                Err(super::error::SkillsError::invalid(
                    "acceptance interruption",
                ))
            } else {
                Ok(())
            }
        },
    );
    drop(writer);
    drop(lease);
    let data_b = fixture.0.join("data-b");
    let result = (|| -> std::result::Result<(), String> {
        if interrupted.is_ok() {
            return Err("Failure injection did not interrupt the fixture".into());
        }
        let run = |action: &str, operation: Option<&str>| -> std::result::Result<Value, String> {
            let mut command = std::process::Command::new(&binary);
            command
                .args([action, "--scope", "project", "--project-root"])
                .arg(&project)
                .arg("--data-dir")
                .arg(&data_b)
                .arg("--json");
            if let Some(id) = operation {
                command.arg(id);
            }
            let output = command.output().map_err(|e| e.to_string())?;
            if !output.status.success() {
                return Err(String::from_utf8_lossy(&output.stderr).into_owned());
            }
            serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())
        };
        let list = run("pending", None)?;
        if !list.as_array().is_some_and(|items| {
            items
                .iter()
                .any(|p| p["operationId"] == id && p["canRecover"] == true)
        }) {
            return Err("CLI did not discover its authorized fixture transaction".into());
        }
        if run("recover", Some(&id))?["outcome"] != "rolled_back" {
            return Err("CLI did not report rollback".into());
        }
        if fs::read_to_string(destination.join("SKILL.md")).map_err(|e| e.to_string())?
            != "Original fixture"
        {
            return Err("Fixture contents were not restored".into());
        }
        Ok(())
    })();
    // Never leave this opt-in fixture's pending marker behind after a failed
    // executable or assertion. Preserve its files if safe cleanup itself fails.
    let cleanup = (|| -> super::error::Result<()> {
        if installers::recovery_items(&repo, &context)?
            .iter()
            .any(|p| p.operation_id == id)
        {
            installers::recover_one(&repo, &context, &id)?;
        }
        Ok(())
    })();
    if let Err(error) = cleanup {
        let root = fixture.0.clone();
        std::mem::forget(fixture);
        panic!(
            "Fixture recovery needs attention at {}: {}",
            root.display(),
            error.message
        );
    }
    assert!(result.is_ok(), "{}", result.unwrap_err());
    println!("M2C_CLI_RECOVERY_OK: real CLI discovered and recovered the isolated transaction from data-b");
}

#[test]
fn copy_source_updates_are_visible_without_marking_installed_instructions_broken() {
    let s = Setup::new();
    assert_eq!(s.run("install", &["--mode", "copy", "demo"], true).0, 0);
    let rules = s.fixture.0.join("project/AGENTS.md");
    let original = fs::read(&rules).unwrap();
    fs::write(
        s.fixture.0.join("source/auto/demo/inject.md"),
        "New source rule\n",
    )
    .unwrap();
    let (code, diagnosis) = s.run("doctor", &["demo"], true);
    assert_eq!(code, 0);
    assert_eq!(diagnosis["results"][0]["status"], "current");
    assert_eq!(diagnosis["results"][0]["update_available"], true);
    assert_eq!(fs::read(&rules).unwrap(), original);
    assert_eq!(
        s.run("status", &["demo"], true).1["results"][0]["update_available"],
        true
    );
    assert_eq!(
        s.run("install", &["--mode", "copy", "--force", "demo"], true)
            .0,
        0
    );
    assert!(fs::read_to_string(rules)
        .unwrap()
        .contains("New source rule"));
    assert_eq!(
        s.run("doctor", &["demo"], true).1["results"][0]["update_available"],
        false
    );
}
#[test]
fn doctor_missing_target_does_not_create_instruction_or_install_directories() {
    let s = Setup::new();
    let (code, diagnosis) = s.run("doctor", &["demo"], true);
    assert_eq!(code, 1);
    assert_eq!(diagnosis["results"][0]["status"], "missing");
    assert!(!s.fixture.0.join("project/AGENTS.md").exists());
    assert!(!s.fixture.0.join("project/.codex").exists());
    assert!(!s.paths.library.join("library.json").exists());
}
