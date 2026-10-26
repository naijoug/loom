use super::*;
use crate::skills::{
    cli::{self, Options},
    runtime_paths::RuntimePaths,
    tests::Fixture,
};
fn input() -> MigrationInput {
    MigrationInput {
        include_global: true,
        ..Default::default()
    }
}
fn setup() -> (Fixture, PathBuf, PathBuf, PathBuf, TargetContext) {
    let f = Fixture::new();
    let home = f.0.join("home");
    let data = f.0.join("old");
    let loom = f.0.join("new");
    fs::create_dir(&home).unwrap();
    fs::create_dir(&data).unwrap();
    let ctx = TargetContext::new(&home, None).unwrap();
    (f, home, data, loom, ctx)
}
fn write(path: &Path, value: Value) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
}
fn tree(root: &Path) -> Vec<(PathBuf, String, u64)> {
    fn walk(root: &Path, path: &Path, out: &mut Vec<(PathBuf, String, u64)>) {
        let m = fs::symlink_metadata(path).unwrap();
        #[cfg(unix)]
        let mode = {
            use std::os::unix::fs::PermissionsExt;
            m.permissions().mode() as u64
        };
        #[cfg(not(unix))]
        let mode = m.permissions().readonly() as u64;
        let state = if m.file_type().is_symlink() {
            format!("link:{}", fs::read_link(path).unwrap().display())
        } else if m.is_file() {
            files::digest(&fs::read(path).unwrap())
        } else {
            "dir".into()
        };
        out.push((path.strip_prefix(root).unwrap().into(), state, mode));
        if m.is_dir() {
            for e in fs::read_dir(path).unwrap() {
                walk(root, &e.unwrap().path(), out);
            }
        }
    }
    let mut out = vec![];
    walk(root, root, &mut out);
    out.sort();
    out
}
#[test]
fn legacy_preview_is_repeatable_secret_free_and_does_not_create_or_chmod_anything() {
    let (f, home, data, loom, ctx) = setup();
    let cache = data.join("repos/fixture--skills");
    fs::create_dir_all(&cache).unwrap();
    write(
        &data.join("library.json"),
        serde_json::json!({"version":1,"repositories":[{"id":"legacy-alias","url":"https://github.com/fixture/skills","slug":"fixture--skills","source":"server-cache"}]}),
    );
    write(
        &data.join("config.json"),
        serde_json::json!({"translation":{"openai":{"apiKey":"do-not-return-fixture-key","model":"secret-model-value"}}}),
    );
    let target = home.join(".codex/skills");
    fs::create_dir_all(target.join("demo")).unwrap();
    fs::write(target.join("demo/SKILL.md"), "User edited copy").unwrap();
    write(
        &target.join(".skills-linker-manifest.json"),
        serde_json::json!({"version":1,"skills":{"demo":{"mode":"copy","source":cache}}}),
    );
    let before = tree(&f.0);
    let input = input();
    let a = preview(&data, &loom, &ctx, &input).unwrap();
    let b = preview(&data, &loom, &ctx, &input).unwrap();
    assert_eq!(
        serde_json::to_value(&a).unwrap(),
        serde_json::to_value(&b).unwrap()
    );
    assert_eq!(before, tree(&f.0));
    assert!(!loom.exists());
    assert!(a.read_only && !a.can_apply && a.complete);
    assert_eq!(a.sources.len(), 1);
    assert!(a.sources[0].legacy_ids.contains(&"legacy-alias".into()));
    assert_eq!(a.installations[0].ownership, "legacy-unverified");
    assert_eq!(a.installations[0].next_step, "compare_copy_before_adoption");
    let json = serde_json::to_string(&a).unwrap();
    assert!(!json.contains("do-not-return-fixture-key") && !json.contains("secret-model-value"));
    assert!(a.configs[0].key_present && a.configs[0].model_present);
    fs::write(data.join("config.json"), "{}").unwrap();
    assert_ne!(
        a.preview_id,
        preview(&data, &loom, &ctx, &input).unwrap().preview_id
    );
}
#[test]
fn missing_bad_future_and_mixed_metadata_are_reported_without_fake_empty_success() {
    let (f, home, data, loom, ctx) = setup();
    write(
        &data.join("library.json"),
        serde_json::json!({"version":999,"repositories":[]}),
    );
    fs::write(data.join("config.json"), "{bad-json-private-marker").unwrap();
    let target = home.join(".codex/skills");
    write(
        &target.join(".skills-linker-manifest.json"),
        serde_json::json!({"version":99,"skills":{}}),
    );
    fs::write(
        target.join(".skills-linker-manifest.tsv"),
        format!(
            "demo\tcopy\t{}\tdate\nbad\trow\n",
            data.join("missing").display()
        ),
    )
    .unwrap();
    let before = tree(&f.0);
    let report = preview(&data, &loom, &ctx, &input()).unwrap();
    for code in [
        "unsupported_library",
        "malformed_metadata",
        "unsupported_manifest",
        "multiple_manifests",
        "invalid_tsv",
        "destination_unavailable",
    ] {
        assert!(report.issues.iter().any(|i| i.code == code), "{code}");
    }
    assert!(!report.complete && !report.can_apply);
    assert!(!serde_json::to_string(&report)
        .unwrap()
        .contains("private-marker"));
    assert_eq!(before, tree(&f.0));
    assert!(preview(
        &data,
        &loom,
        &ctx,
        &MigrationInput {
            data_dirs: vec![f.0.join("absent")],
            ..Default::default()
        }
    )
    .unwrap()
    .files
    .iter()
    .any(|i| i.state == "missing"));
}
#[cfg(unix)]
#[test]
fn links_and_bundle_sources_are_observed_without_following_metadata_or_external_targets() {
    use std::os::unix::fs::symlink;
    let (f, home, data, loom, ctx) = setup();
    let target = home.join(".codex/skills");
    fs::create_dir_all(&target).unwrap();
    let missing = data.join("repos/missing");
    symlink(&missing, target.join("broken")).unwrap();
    let bundle =
        f.0.join("Skills Manager.app/Contents/Resources/skills/demo");
    symlink(&bundle, target.join("bundle")).unwrap();
    write(
        &target.join(".skills-linker-manifest.json"),
        serde_json::json!({"version":1,"skills":{"broken":{"mode":"symlink","source":missing},"bundle":{"mode":"symlink","source":bundle}}}),
    );
    let private = f.0.join("private.json");
    fs::write(&private, "PRIVATE_CONFIG").unwrap();
    symlink(&private, data.join("config.json")).unwrap();
    let before = tree(&f.0);
    let report = preview(&data, &loom, &ctx, &input()).unwrap();
    assert_eq!(before, tree(&f.0));
    assert!(report.issues.iter().any(|i| i.code == "broken_link"));
    assert!(report
        .issues
        .iter()
        .any(|i| i.code == "unreadable_metadata"));
    let bundle = report
        .installations
        .iter()
        .find(|i| i.name == "bundle")
        .unwrap();
    assert_eq!(bundle.source_state, "outside_scope_unchecked");
    assert_eq!(bundle.next_step, "preserve_bundle_source_before_adoption");
    assert!(!serde_json::to_string(&report)
        .unwrap()
        .contains("PRIVATE_CONFIG"));
}
#[test]
fn aliases_modes_and_existing_library_conflicts_remain_visible() {
    let (_f, _home, data, loom, ctx) = setup();
    write(
        &data.join("library.json"),
        serde_json::json!({"version":1,"repositories":[
            {"id":"same","url":"https://github.com/fixture/one","source":"github-api"},
            {"id":"same","url":"https://github.com/fixture/two","source":"github-api"},
            {"id":"other","url":"https://github.com/fixture/one","source":"server-cache","path":"/not-authorized/cache"},
            {"id":"bad","url":"https://token-secret@github.com/fixture/no"}
        ]}),
    );
    let report = preview(&data, &loom, &ctx, &input()).unwrap();
    assert_eq!(report.sources.len(), 3);
    assert_ne!(report.sources[0].source_id, report.sources[1].source_id);
    assert!(report.issues.iter().any(|i| i.code == "alias_conflict"));
    assert!(report.issues.iter().any(|i| i.code == "cache_unavailable"));
    assert!(!serde_json::to_string(&report)
        .unwrap()
        .contains("token-secret"));
    write(
        &loom.join("library.json"),
        serde_json::json!({"schemaVersion":99,"sources":[]}),
    );
    assert!(preview(&data, &loom, &ctx, &input())
        .unwrap()
        .issues
        .iter()
        .any(|i| i.code == "loom_library_invalid"));
}
#[test]
fn bounded_reads_and_unsafe_names_fail_closed() {
    let (_f, home, data, loom, ctx) = setup();
    fs::write(data.join("library.json"), vec![b'x'; MAX_FILE as usize + 1]).unwrap();
    write(
        &home.join(".codex/skills/.skills-linker-manifest.json"),
        serde_json::json!({"version":1,"skills":{"../escape":{"mode":"copy","source":"/outside"}}}),
    );
    let report = preview(&data, &loom, &ctx, &input()).unwrap();
    assert!(report.installations.is_empty());
    assert!(report
        .issues
        .iter()
        .any(|i| i.code == "unreadable_metadata"));
    assert!(report
        .issues
        .iter()
        .any(|i| i.code == "invalid_installation"));
    assert!(preview(
        &data,
        &loom,
        &ctx,
        &MigrationInput {
            data_dirs: vec![data.clone(); 9],
            ..Default::default()
        }
    )
    .is_err());
}
#[test]
fn cli_preview_never_initializes_library_or_coordination_or_accepts_apply_options() {
    let (f, home, data, loom, _ctx) = setup();
    let paths = RuntimePaths {
        home,
        app_data: f.0.join("app"),
        library: loom,
    };
    let args = vec![
        "migration-preview".into(),
        "--legacy-data-dir".into(),
        data.to_string_lossy().into(),
        "--json".into(),
    ];
    let options = Options::parse(&args).unwrap();
    let before = tree(&f.0);
    let output = cli::run_with_paths(&options, &paths).unwrap();
    assert_eq!(before, tree(&f.0));
    assert!(
        serde_json::from_str::<MigrationPreview>(&output.text)
            .unwrap()
            .read_only
    );
    let mut bad = args;
    bad.push("--force".into());
    assert!(Options::parse(&bad).is_err());
}

#[test]
fn canonical_legacy_fixtures_parse_and_global_inspection_is_opt_in() {
    let (f, home, data, loom, ctx) = setup();
    fs::write(
        data.join("library.json"),
        include_str!("../../tests/fixtures/skills/legacy-library.json"),
    )
    .unwrap();
    let target = home.join(".codex/skills");
    fs::create_dir_all(&target).unwrap();
    fs::write(
        target.join(".skills-linker-manifest.tsv"),
        include_str!("../../tests/fixtures/skills/legacy-install-manifest.tsv")
            .replace("/fixture/", &format!("{}/", f.0.display())),
    )
    .unwrap();
    let report = preview(&data, &loom, &ctx, &input()).unwrap();
    assert_eq!(report.sources.len(), 2);
    assert_eq!(report.installations.len(), 2);
    assert!(report
        .sources
        .iter()
        .any(|s| s.kind == SourceKind::GithubApi));
    let report = preview(&data, &loom, &ctx, &MigrationInput::default()).unwrap();
    assert!(report.installations.is_empty());
    assert!(!report.files.iter().any(|f| f.path.starts_with(&target)));
}

#[test]
fn source_identity_conflicts_and_entry_limits_do_not_become_silent_success() {
    let (_f, _home, data, loom, ctx) = setup();
    let repos=(0..MAX_ITEMS+2).map(|_|serde_json::json!({"id":"same","url":"https://github.com/fixture/repo","source":"github-api"})).collect::<Vec<_>>();
    write(
        &data.join("library.json"),
        serde_json::json!({"version":1,"repositories":repos}),
    );
    let report = preview(&data, &loom, &ctx, &input()).unwrap();
    assert_eq!(report.sources.len(), MAX_ITEMS);
    assert!(!report.complete);
    assert!(report.issues.iter().any(|i| i.code == "item_limit"));
    assert!(report.issues.iter().any(|i| i.code == "duplicate_source"));
}

#[cfg(unix)]
#[test]
fn selected_root_aliases_are_normalized_but_internal_links_never_expand_scope() {
    use std::os::unix::fs::symlink;
    let (f, home, data, loom, ctx) = setup();
    let alias = f.0.join("alias");
    symlink(&f.0, &alias).unwrap();
    let declared = alias.join("old");
    fs::create_dir_all(data.join("repos/demo")).unwrap();
    write(
        &data.join("library.json"),
        serde_json::json!({"version":1,"repositories":[{"url":"https://github.com/fixture/demo","path":declared.join("repos/demo")}]}),
    );
    let target = home.join(".codex/skills");
    fs::create_dir_all(target.join("demo")).unwrap();
    write(
        &target.join(".skills-linker-manifest.json"),
        serde_json::json!({"version":1,"skills":{"demo":{"mode":"copy","source":declared.join("repos/demo")}}}),
    );
    let input = MigrationInput {
        data_dirs: vec![declared.clone()],
        include_global: true,
        ..Default::default()
    };
    let report = preview(&data, &loom, &ctx, &input).unwrap();
    assert!(report.complete);
    assert_eq!(report.sources[0].cache_state, "directory_unverified");
    assert_eq!(report.installations[0].source_state, "directory_unverified");
    let outside = f.0.join("outside");
    fs::create_dir(&outside).unwrap();
    symlink(&outside, data.join("repos/link")).unwrap();
    write(
        &data.join("library.json"),
        serde_json::json!({"version":1,"repositories":[{"url":"https://github.com/fixture/demo","path":declared.join("repos/link")}]}),
    );
    let report = preview(&data, &loom, &ctx, &input).unwrap();
    assert_eq!(report.sources[0].cache_state, "linked_or_unreadable");
}
