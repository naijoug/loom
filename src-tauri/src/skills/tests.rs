use super::{
    coordination::{Coordinator, PendingTransaction},
    error::ErrorCode,
    files,
};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

pub(super) struct Fixture(pub(super) PathBuf);
impl Fixture {
    pub(super) fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "loom-skills-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self(root.canonicalize().unwrap())
    }
    fn coordinator(&self) -> Coordinator {
        Coordinator::open(&self.0.join("coordination")).unwrap()
    }
    fn prepared(&self, id: &str) -> PendingTransaction {
        let data = files::private_directory(&self.0.join("data-a")).unwrap();
        let journal = data.join(format!("{id}.json"));
        files::atomic_json(
            &journal,
            &serde_json::json!({"operationId": id, "phase": "prepared"}),
        )
        .unwrap();
        PendingTransaction::prepare(id, &[self.0.join("project/AGENTS.md")], &data, &journal)
            .unwrap()
    }
    fn child(&self, mode: &str) -> Command {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "skills::tests::coordination_child",
                "--ignored",
                "--nocapture",
            ])
            .env("LOOM_SKILLS_TEST_ROOT", &self.0)
            .env("LOOM_SKILLS_TEST_MODE", mode);
        command
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn metadata_is_durable_private_and_corruption_is_not_empty() {
    let fixture = Fixture::new();
    let root = files::private_directory(&fixture.0.join("private")).unwrap();
    let path = root.join("state.json");
    files::atomic_json(&path, &vec!["first"]).unwrap();
    files::atomic_json(&path, &vec!["second"]).unwrap();
    assert_eq!(
        files::read_json::<Vec<String>>(&path).unwrap(),
        vec!["second"]
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&root).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    fs::write(&path, "{").unwrap();
    assert_eq!(
        files::read_json::<serde_json::Value>(&path)
            .unwrap_err()
            .code,
        ErrorCode::RecoveryRequired
    );
    assert!(fs::read_dir(root).unwrap().all(|e| !e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".write-")));
}

#[test]
fn operation_and_update_leases_are_mutually_exclusive_and_release() {
    let fixture = Fixture::new();
    let coordinator = fixture.coordinator();
    let first = coordinator.enter().unwrap();
    let second = coordinator.enter().unwrap();
    assert!(coordinator.try_update().is_err());
    let writer = coordinator.writer(&first).unwrap();
    assert!(coordinator.writer(&second).is_err());
    drop(writer);
    drop(first);
    drop(second);
    let update = coordinator.try_update().unwrap();
    assert!(coordinator.enter().is_err());
    assert!(coordinator
        .writer(&update)
        .unwrap()
        .check_write_set(&[fixture.0.join("target")])
        .is_err());
    drop(update);
    assert!(coordinator.enter().is_ok());
}

#[test]
fn pending_transactions_protect_overlapping_resources_across_data_directories() {
    let fixture = Fixture::new();
    let record = fixture.prepared("crashed-operation");
    {
        let a = fixture.coordinator();
        let lease = a.enter().unwrap();
        a.writer(&lease).unwrap().publish(&record).unwrap();
    }
    let b = fixture.coordinator();
    let lease = b.enter().unwrap();
    let writer = b.writer(&lease).unwrap();
    for target in [
        fixture.0.join("project"),
        fixture.0.join("project/AGENTS.md"),
    ] {
        assert_eq!(
            writer.check_write_set(&[target]).unwrap_err().code,
            ErrorCode::RecoveryRequired
        );
    }
    writer
        .check_write_set(&[fixture.0.join("unrelated")])
        .unwrap();
    assert_eq!(
        writer.pending().unwrap()[0].data_dir,
        fixture.0.join("data-a")
    );
    // A missing recovery journal does not erase the marker or allow a new write.
    fs::remove_file(&record.journal_path).unwrap();
    assert!(writer.check_write_set(&record.write_set).is_err());
    drop(writer);
    drop(lease);
    assert!(b.try_update().is_err());
    assert!(b.enter().is_ok(), "failed update must release admission");
}

#[test]
fn malformed_or_future_pending_index_blocks_even_unrelated_writes() {
    let fixture = Fixture::new();
    let coordinator = fixture.coordinator();
    let path = coordinator.root().join("pending/broken.json");
    let lease = coordinator.enter().unwrap();
    let writer = coordinator.writer(&lease).unwrap();
    fs::write(&path, "{").unwrap();
    assert_eq!(
        writer
            .check_write_set(&[fixture.0.join("unrelated")])
            .unwrap_err()
            .code,
        ErrorCode::RecoveryRequired
    );
    let mut record = fixture.prepared("broken");
    record.schema_version = 999;
    files::atomic_json(&path, &record).unwrap();
    assert!(writer.pending().is_err());
}

#[test]
fn recovery_paths_reject_traversal_and_journals_outside_data_root() {
    let fixture = Fixture::new();
    assert!(files::resource_path(&fixture.0.join("../outside")).is_err());
    assert!(PendingTransaction::prepare(
        "../escape",
        &[fixture.0.join("target")],
        &fixture.0,
        &fixture.0.join("journal")
    )
    .is_err());
    let record = fixture.prepared("valid");
    assert!(PendingTransaction::prepare(
        "other",
        &record.write_set,
        &fixture.0.join("data-b"),
        &record.journal_path
    )
    .is_err());
}

#[cfg(unix)]
#[test]
fn metadata_symlinks_cannot_redirect_writes() {
    let fixture = Fixture::new();
    let outside = fixture.0.join("outside.json");
    fs::write(&outside, "keep").unwrap();
    let link = fixture.0.join("link.json");
    std::os::unix::fs::symlink(&outside, &link).unwrap();
    assert!(files::atomic_json(&link, &true).is_err());
    assert_eq!(fs::read_to_string(outside).unwrap(), "keep");
}

#[test]
fn os_releases_lock_after_child_is_killed() {
    let fixture = Fixture::new();
    let coordinator = fixture.coordinator();
    let mut child = fixture
        .child("hold")
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let mut ready = false;
    for line in (&mut output).lines() {
        if line.unwrap() == "skills-lock-ready" {
            ready = true;
            break;
        }
    }
    let blocked = coordinator.try_update().is_err();
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(ready && blocked);
    let update = coordinator.try_update().unwrap();
    drop(update);
    let lease = coordinator.enter().unwrap();
    assert!(coordinator.writer(&lease).is_ok());
}

#[test]
fn abrupt_child_exit_leaves_recovery_marker_for_another_client() {
    let fixture = Fixture::new();
    let status = fixture.child("publish-and-exit").status().unwrap();
    assert_eq!(status.code(), Some(27));
    let b = fixture.coordinator();
    let lease = b.enter().unwrap();
    assert_eq!(
        b.writer(&lease)
            .unwrap()
            .check_write_set(&[fixture.0.join("project/AGENTS.md")])
            .unwrap_err()
            .code,
        ErrorCode::RecoveryRequired
    );
}

#[test]
#[ignore = "spawned by coordination tests with an isolated explicit root"]
fn coordination_child() {
    let root = PathBuf::from(std::env::var_os("LOOM_SKILLS_TEST_ROOT").expect("test root"));
    let fixture = Fixture(root);
    let coordinator = fixture.coordinator();
    let lease = coordinator.enter().unwrap();
    let writer = coordinator.writer(&lease).unwrap();
    if std::env::var("LOOM_SKILLS_TEST_MODE").unwrap() == "publish-and-exit" {
        writer.publish(&fixture.prepared("interrupted")).unwrap();
        // Deliberately bypass Rust destructors to exercise OS crash release.
        std::process::exit(27);
    }
    println!("skills-lock-ready");
    std::io::stdout().flush().unwrap();
    std::thread::sleep(std::time::Duration::from_secs(30));
    std::process::exit(28);
}
