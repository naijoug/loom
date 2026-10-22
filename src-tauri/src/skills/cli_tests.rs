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
