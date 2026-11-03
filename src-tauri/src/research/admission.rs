//! One OS writer lock per research. Control requests name an owner, never a PID.
use super::{files, store::valid_id};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Owner {
    pub run_id: String,
    pub workflow: String,
    #[serde(default)]
    pub lease_token: String,
}

pub struct Lease {
    file: File,
    directory: PathBuf,
    owner: Option<Owner>,
}

fn control_directory(root: &Path, id: &str) -> Result<PathBuf, String> {
    if !valid_id(id) {
        return Err("invalid_research_id".into());
    }
    files::path(root, &format!(".control/{id}"))
}
fn open_lock(dir: &Path, create: bool) -> Result<Option<File>, String> {
    if !dir.exists() && !create {
        return Ok(None);
    }
    if create {
        files::directory(dir)?;
    }
    let path = files::path(dir, "writer.lock")?;
    if !path.exists() && !create {
        return Ok(None);
    }
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(true)
        .create(create)
        .truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).map(Some).map_err(|e| e.to_string())
}
pub fn acquire(root: &Path, id: &str) -> Result<Lease, String> {
    let dir = control_directory(root, id)?;
    let file = open_lock(&dir, true)?.ok_or("missing_writer_lock")?;
    file.try_lock().map_err(|e| format!("research_busy: {e}"))?;
    // A successfully acquired OS lock proves the prior owner is gone. Clear
    // its projection before any reader can mistake this unbound lease for it.
    let stale_owner = files::path(&dir, "owner.json")?;
    match fs::remove_file(stale_owner) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("stale_owner_cleanup_failed: {e}")),
    }
    Ok(Lease {
        file,
        directory: dir,
        owner: None,
    })
}
pub fn is_active(root: &Path, id: &str) -> Result<bool, String> {
    let dir = control_directory(root, id)?;
    let Some(file) = open_lock(&dir, false)? else {
        return Ok(false);
    };
    match file.try_lock() {
        Ok(()) => {
            let _ = file.unlock();
            Ok(false)
        }
        Err(std::fs::TryLockError::WouldBlock) => Ok(true),
        Err(e) => Err(format!("research_lock_unavailable: {e}")),
    }
}
pub fn owner(root: &Path, id: &str) -> Result<Option<Owner>, String> {
    if !is_active(root, id)? {
        return Ok(None);
    }
    let path = files::path(&control_directory(root, id)?, "owner.json")?;
    if !path.exists() {
        return Ok(None);
    }
    match files::read_json(&path, 4096) {
        Ok(owner) => Ok(Some(owner)),
        Err(_) if !path.exists() => Ok(None),
        Err(error) => Err(error),
    }
}
pub fn request_cancel(root: &Path, id: &str, run_id: &str) -> Result<bool, String> {
    if !valid_id(run_id) {
        return Err("invalid_run_id".into());
    }
    let Some(owner) = owner(root, id)?.filter(|v| v.run_id == run_id) else {
        return Ok(false);
    };
    // Atomic publication remains safe if ownership changes: each run has its own path.
    let path = files::path(
        &control_directory(root, id)?,
        &format!("cancel-{}-{}.json", run_id, owner.lease_token),
    )?;
    files::write_json(&path, &serde_json::json!({"runId":run_id}))?;
    Ok(true)
}
pub fn cancelled(root: &Path, id: &str, run_id: &str) -> bool {
    let Ok(Some(owner)) = owner(root, id) else {
        return false;
    };
    if owner.run_id != run_id {
        return false;
    }
    control_directory(root, id)
        .and_then(|dir| {
            files::path(
                &dir,
                &format!("cancel-{}-{}.json", run_id, owner.lease_token),
            )
        })
        .is_ok_and(|path| path.exists())
}
impl Lease {
    pub fn bind(&mut self, run_id: &str, workflow: &str) -> Result<(), String> {
        if !valid_id(run_id) || !matches!(workflow, "legacy" | "target") || self.owner.is_some() {
            return Err("invalid_owner_binding".into());
        }
        let owner = Owner {
            run_id: run_id.into(),
            workflow: workflow.into(),
            lease_token: super::store::next_id("lease"),
        };
        files::write_json(&files::path(&self.directory, "owner.json")?, &owner)?;
        self.owner = Some(owner);
        Ok(())
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        // Delete only while holding this lock; no new owner can have published yet.
        if let Some(owner) = &self.owner {
            let _ = fs::remove_file(self.directory.join("owner.json"));
            let _ = fs::remove_file(self.directory.join(format!(
                "cancel-{}-{}.json",
                owner.run_id, owner.lease_token
            )));
        }
        let _ = self.file.unlock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn locks_and_cancel_are_owner_scoped() {
        let root = super::super::store::tests::temp_root();
        let mut lease = acquire(&root, "research-a").unwrap();
        lease.bind("run-a", "target").unwrap();
        assert!(acquire(&root, "research-a").is_err());
        assert!(is_active(&root, "research-a").unwrap());
        assert!(!request_cancel(&root, "research-a", "run-old").unwrap());
        assert!(request_cancel(&root, "research-a", "run-a").unwrap());
        assert!(cancelled(&root, "research-a", "run-a"));
        drop(lease);
        assert!(!is_active(&root, "research-a").unwrap());
        let mut next = acquire(&root, "research-a").unwrap();
        next.bind("run-b", "target").unwrap();
        assert!(!request_cancel(&root, "research-a", "run-a").unwrap());
        assert!(!cancelled(&root, "research-a", "run-b"));
        drop(next);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn delayed_cancel_for_a_previous_attempt_does_not_stop_retried_run() {
        let root = super::super::store::tests::temp_root();
        let mut first = acquire(&root, "research-a").unwrap();
        first.bind("run-a", "target").unwrap();
        let old = owner(&root, "research-a").unwrap().unwrap();
        drop(first);
        let mut retry = acquire(&root, "research-a").unwrap();
        retry.bind("run-a", "target").unwrap();
        let stale_path = control_directory(&root, "research-a")
            .unwrap()
            .join(format!("cancel-run-a-{}.json", old.lease_token));
        files::write_json(&stale_path, &serde_json::json!({"runId":"run-a"})).unwrap();
        assert!(!cancelled(&root, "research-a", "run-a"));
        assert!(request_cancel(&root, "research-a", "run-a").unwrap());
        assert!(cancelled(&root, "research-a", "run-a"));
        drop(retry);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unbound_recovery_lease_does_not_revive_a_stale_owner() {
        let root = super::super::store::tests::temp_root();
        let lease = acquire(&root, "research-stale").unwrap();
        let directory = control_directory(&root, "research-stale").unwrap();
        // Crash residue: the OS lock is gone, but its owner file remains.
        files::write_json(
            &directory.join("owner.json"),
            &Owner {
                run_id: "run-old".into(),
                workflow: "target".into(),
                lease_token: "old".into(),
            },
        )
        .unwrap();
        drop(lease);
        let recovered = acquire(&root, "research-stale").unwrap();
        assert!(owner(&root, "research-stale").unwrap().is_none());
        assert!(!request_cancel(&root, "research-stale", "run-old").unwrap());
        drop(recovered);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn child_owner() {
        let Ok(root) = std::env::var("LOOM_RESEARCH_LOCK_TEST") else {
            return;
        };
        let root = PathBuf::from(root);
        let mut lease = acquire(&root.join("research"), "research-cross").unwrap();
        lease.bind("job-child", "legacy").unwrap();
        fs::write(root.join("ready"), b"ready").unwrap();
        let start = std::time::Instant::now();
        while !cancelled(&root.join("research"), "research-cross", "job-child") {
            assert!(start.elapsed() < std::time::Duration::from_secs(5));
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    #[test]
    fn another_process_reader_is_inert_and_can_cancel_the_owner() {
        use super::super::{
            models::TurnStatus,
            store::{
                tests::{sample_research, temp_root, turn},
                Store,
            },
        };
        let root = temp_root();
        let store = Store::new(&root);
        let mut research = sample_research("research-cross");
        research
            .turns
            .push(turn(1, TurnStatus::Running, "job-child"));
        store.create(&research).unwrap();
        let path = store.dir("research-cross").unwrap().join("research.json");
        let before = fs::read(&path).unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "research::admission::tests::child_owner",
                "--nocapture",
            ])
            .env("LOOM_RESEARCH_LOCK_TEST", &root)
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let start = std::time::Instant::now();
        while !root.join("ready").exists() {
            if start.elapsed() > std::time::Duration::from_secs(3) {
                let _ = child.kill();
                panic!("child did not acquire lease");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(
            store.load("research-cross").unwrap().turns[0].status,
            TurnStatus::Running
        );
        assert_eq!(fs::read(&path).unwrap(), before);
        assert!(store.delete("research-cross").is_err());
        assert!(store.update("research-cross", |_| Ok(())).is_err());
        assert!(store.cancel_legacy("research-cross").unwrap());
        assert!(child.wait().unwrap().success());
        assert_eq!(
            store.load("research-cross").unwrap().turns[0].status,
            TurnStatus::Interrupted
        );
        assert_eq!(
            fs::read(&path).unwrap(),
            before,
            "read must not migrate/recover on disk"
        );
        store.update("research-cross", |_| Ok(())).unwrap();
        assert_eq!(
            fs::read(
                store
                    .dir("research-cross")
                    .unwrap()
                    .join("research.v1.backup.json")
            )
            .unwrap(),
            before
        );
        fs::remove_dir_all(root).unwrap();
    }
}
