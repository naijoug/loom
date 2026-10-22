use super::{
    catalog, coordination::Coordinator, error::ErrorCode, files, repository::Repository, snapshots,
    tests::Fixture,
};
use std::{fs, path::Path};

fn skill(root: &Path, relative: &str, name: &str, title: &str) {
    let directory = root.join(relative);
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("SKILL.md"),format!("---\nname: {name}\ndescription: >-\n  Explain source boundaries\n  with portable examples.\n---\n# {title}\n\nBody **text**.\n")).unwrap();
}

#[test]
fn scan_reports_bad_packages_without_hiding_valid_skills_and_skips_archives() {
    let f = Fixture::new();
    let source = f.0.join("来源 with spaces");
    skill(&source, "skills/manual/tool/demo", "ng-tool-demo", "Demo");
    skill(&source, "skills/archive/old", "old", "Archived");
    fs::create_dir_all(source.join("skills/manual/bad")).unwrap();
    fs::write(
        source.join("skills/manual/bad/SKILL.md"),
        "---\nname: unterminated",
    )
    .unwrap();
    fs::write(
        source.join("skills/manual/tool/demo/skill.yaml"),
        "title: Catalog title\ntags:\n  - testing\ntriggers:\n  keywords:\n    - 精确搜索\n",
    )
    .unwrap();
    let catalog = catalog::scan("src-fixture", &source).unwrap();
    assert_eq!(catalog.skills.len(), 1);
    assert_eq!(catalog.issues.len(), 1);
    let summary = &catalog.skills[0];
    assert_eq!(summary.title, "Catalog title");
    assert_eq!(summary.name, "ng-tool-demo");
    assert_eq!(summary.category, "manual/tool");
    assert!(summary.description.contains("portable examples"));
    assert!(summary.keywords.contains("精确搜索"));
    assert_eq!(
        catalog::decode_id(&summary.id).unwrap(),
        ("src-fixture".into(), "manual/tool/demo/SKILL.md".into())
    );
}

#[test]
fn root_skill_and_related_files_are_read_without_execution_or_binary_decoding() {
    let f = Fixture::new();
    skill(&f.0, "package", "demo", "Demo");
    let root = f.0.join("package");
    fs::create_dir(root.join("scripts")).unwrap();
    fs::write(root.join("scripts/run.sh"), "touch NEVER_EXECUTE").unwrap();
    fs::write(root.join("binary.dat"), [0, 255, 1]).unwrap();
    let detail = catalog::detail("src-fixture", &root, "SKILL.md").unwrap();
    assert_eq!(detail.related_files.len(), 2);
    assert!(detail
        .related_files
        .iter()
        .any(|v| v.relative_path == "binary.dat" && v.content.is_none()));
    assert!(!root.join("NEVER_EXECUTE").exists());
    for relative in ["../SKILL.md", "/etc/passwd", "scripts/../../SKILL.md"] {
        assert!(catalog::detail("src-fixture", &root, relative).is_err());
    }
}

#[test]
fn local_library_survives_reopen_and_removing_source_never_removes_content() {
    let f = Fixture::new();
    skill(&f.0, "source", "demo", "Demo");
    let data = f.0.join("data");
    let coordination = f.0.join("coord");
    let repo = Repository::open(&data, &coordination).unwrap();
    assert!(repo.list().unwrap().sources.is_empty());
    let source = repo.add_local(&f.0.join("source")).unwrap();
    assert_eq!(repo.add_local(&f.0.join("source")).unwrap().id, source.id);
    let reopened = Repository::open(&data, &coordination).unwrap();
    let view = reopened.list().unwrap();
    assert_eq!(view.sources.len(), 1);
    assert_eq!(view.skills.len(), 1);
    assert_eq!(
        reopened.detail(&view.skills[0].id).unwrap().summary.title,
        "Demo"
    );
    reopened.remove_source(&source.id).unwrap();
    assert!(f.0.join("source/SKILL.md").is_file());
    assert!(reopened.list().unwrap().sources.is_empty());
    fs::write(data.join("library.json"), "broken").unwrap();
    assert_eq!(
        reopened.list().unwrap_err().code,
        ErrorCode::RecoveryRequired
    );
}

#[test]
fn unreadable_source_is_an_issue_not_a_successful_empty_source() {
    let f = Fixture::new();
    skill(&f.0, "source", "demo", "Demo");
    let repo = Repository::open(&f.0.join("data"), &f.0.join("coord")).unwrap();
    let source = repo.add_local(&f.0.join("source")).unwrap();
    fs::remove_dir_all(f.0.join("source")).unwrap();
    let view = repo.list().unwrap();
    assert_eq!(view.sources.len(), 1);
    assert_eq!(view.issues[0].source_id, source.id);
}

#[test]
fn fixed_snapshot_refresh_preserves_old_revision_and_detects_tampering() {
    let f = Fixture::new();
    skill(&f.0, "source", "demo", "Version A");
    let source = f.0.join("source");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::write(source.join("run.sh"), "#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(source.join("run.sh"), fs::Permissions::from_mode(0o755)).unwrap();
    }
    let root = files::private_directory(&f.0.join("data")).unwrap();
    let coordinator = Coordinator::open(&f.0.join("coord")).unwrap();
    let lease = coordinator.enter().unwrap();
    let a = snapshots::prepare(&source, &f.0.join("stage-a"), "src-fixture", "revision-a").unwrap();
    let a_path = snapshots::publish(
        &root,
        &f.0.join("stage-a"),
        &a,
        &coordinator.writer(&lease).unwrap(),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(a_path.join("run.sh"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(&a_path, f.0.join("installed")).unwrap();
    skill(&f.0, "source", "demo", "Version B");
    let b = snapshots::prepare(&source, &f.0.join("stage-b"), "src-fixture", "revision-b").unwrap();
    snapshots::publish(
        &root,
        &f.0.join("stage-b"),
        &b,
        &coordinator.writer(&lease).unwrap(),
    )
    .unwrap();
    assert!(fs::read_to_string(a_path.join("SKILL.md"))
        .unwrap()
        .contains("Version A"));
    #[cfg(unix)]
    assert!(fs::read_to_string(f.0.join("installed/SKILL.md"))
        .unwrap()
        .contains("Version A"));
    fs::write(a_path.join("SKILL.md"), "tampered").unwrap();
    assert!(snapshots::verify(a_path.parent().unwrap()).is_err());
}

#[cfg(unix)]
#[test]
fn symlink_source_escape_is_never_previewed_or_published() {
    let f = Fixture::new();
    skill(&f.0, "source", "demo", "Demo");
    fs::write(f.0.join("private.txt"), "private outside source").unwrap();
    std::os::unix::fs::symlink(f.0.join("private.txt"), f.0.join("source/escape.txt")).unwrap();
    let detail = catalog::detail("src-fixture", &f.0.join("source"), "SKILL.md").unwrap();
    assert!(detail.related_files.is_empty());
    assert!(catalog::resolve_file(&f.0.join("source"), "escape.txt").is_err());
    assert!(snapshots::prepare(
        &f.0.join("source"),
        &f.0.join("stage"),
        "src-fixture",
        "revision"
    )
    .is_err());
    assert!(!f.0.join("stage").exists());
}

#[test]
fn old_percent_encoded_id_roundtrips_and_invalid_ids_fail() {
    let source = "gitlab:group/subgroup/repo";
    let path = "manual/中文 folder/SKILL.md";
    assert_eq!(
        catalog::decode_id(&catalog::encode_id(source, path)).unwrap(),
        (source.into(), path.into())
    );
    for id in ["invalid", "src::%XX", "src::%FF", "src::%"] {
        assert!(catalog::decode_id(id).is_err());
    }
}

#[test]
fn empty_conventional_skills_directory_does_not_hide_other_packages() {
    let f = Fixture::new();
    fs::create_dir(f.0.join("skills")).unwrap();
    skill(&f.0, "manual/demo", "demo", "Visible outside empty folder");
    let result = catalog::scan("src-fixture", &f.0).unwrap();
    assert_eq!(result.skills.len(), 1);
    assert_eq!(result.skills[0].relative_path, "manual/demo/SKILL.md");
}
