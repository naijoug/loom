//! Immutable prepare journal and common pending marker precede every mutation.
//! Recovery only removes exact postimages and preserves later user changes.
use super::{
    coordination::{PendingTransaction, WriterGuard},
    error::{ErrorCode, Result, SkillsError},
    files,
    package::{self, EntryState},
    repository::Repository,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Change {
    pub target: PathBuf,
    pub before: EntryState,
    pub after: EntryState,
    pub staged: PathBuf,
    pub backup: PathBuf,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Prepared {
    pub schema_version: u32,
    pub operation_id: String,
    pub changes: Vec<Change>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Committed,
    RolledBack,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Receipt {
    pub schema_version: u32,
    pub operation_id: String,
    pub prepared_digest: String,
    pub outcome: Outcome,
}

pub fn artifact(target: &Path, id: &str, index: usize, kind: &str) -> Result<PathBuf> {
    files::validate_id(id)?;
    let parent = target
        .parent()
        .ok_or_else(|| SkillsError::invalid("Missing target parent"))?;
    // Keep staged and backed-up SKILL.md files outside the directory scanned
    // by Agent CLIs, including after a successful uninstall.
    let sibling = if parent
        .file_name()
        .is_some_and(|name| name == "skills" || name == "commands" || name == "prompts")
    {
        parent
            .parent()
            .ok_or_else(|| SkillsError::invalid("Target needs a transaction storage parent"))?
    } else {
        parent
    };
    Ok(sibling
        .join(".loom-skill-transactions")
        .join(id)
        .join(format!("{kind}-{index}")))
}

pub fn prepare_storage(target: &Path, id: &str) -> Result<()> {
    let staged = artifact(target, id, 0, "staged")?;
    let directory = staged.parent().unwrap();
    let root = directory.parent().unwrap();
    files::private_directory(root)?;
    let ignore = root.join(".gitignore");
    files::regular_file(&ignore)?;
    if ignore.try_exists()? {
        if files::read_metadata_bytes(&ignore)?.as_slice() != b"*\n" {
            return Err(SkillsError::new(
                ErrorCode::Conflict,
                "Transaction storage has an unexpected ignore policy",
            ));
        }
    } else {
        files::atomic_bytes(&ignore, b"*\n")?;
    }
    files::private_directory(directory)?;
    Ok(())
}
fn journal_path(data: &Path, id: &str) -> Result<PathBuf> {
    files::validate_id(id)?;
    Ok(data.join("transactions").join(id).join("prepare.json"))
}
fn receipt_path(data: &Path, id: &str) -> Result<PathBuf> {
    Ok(journal_path(data, id)?.with_file_name("receipt.json"))
}

pub fn receipt(data: &Path, id: &str) -> Result<Option<Receipt>> {
    let path = receipt_path(data, id)?;
    files::regular_file(&path)?;
    if !path.try_exists()? {
        return Ok(None);
    }
    let receipt: Receipt = files::read_json(&path)?;
    if receipt.schema_version != 1 || receipt.operation_id != id {
        return Err(SkillsError::recovery("Invalid transaction receipt"));
    }
    Ok(Some(receipt))
}

/// Used when several items in one batch share a manifest. Carry forward this
/// transaction's verified postimage, never trust a fresh external edit as ours.
pub fn committed_state(data: &Path, id: &str, target: &Path) -> Result<EntryState> {
    let receipt =
        receipt(data, id)?.ok_or_else(|| SkillsError::recovery("Missing committed receipt"))?;
    if receipt.outcome != Outcome::Committed {
        return Err(SkillsError::recovery("Transaction was not committed"));
    }
    let bytes = files::read_metadata_bytes(&journal_path(data, id)?)?;
    if files::digest(&bytes) != receipt.prepared_digest {
        return Err(SkillsError::recovery("Committed journal digest mismatch"));
    }
    let journal: Prepared = serde_json::from_slice(&bytes)
        .map_err(|_| SkillsError::recovery("Malformed committed journal"))?;
    journal
        .changes
        .into_iter()
        .find(|c| c.target == target)
        .map(|c| c.after)
        .ok_or_else(|| SkillsError::recovery("Committed resource not found"))
}

pub fn prepare(
    repo: &Repository,
    writer: &WriterGuard<'_>,
    id: &str,
    changes: Vec<Change>,
) -> Result<PendingTransaction> {
    files::validate_id(id)?;
    if changes.is_empty() {
        return Err(SkillsError::invalid("Empty transaction"));
    }
    let path = journal_path(&repo.root, id)?;
    if path.try_exists()? {
        return Err(SkillsError::new(
            ErrorCode::Conflict,
            "Transaction already prepared; inspect its receipt",
        ));
    }
    let roots = changes
        .iter()
        .map(|c| {
            c.target
                .parent()
                .map(Path::to_path_buf)
                .ok_or_else(|| SkillsError::invalid("Missing target parent"))
        })
        .collect::<Result<Vec<_>>>()?;
    let prepared = Prepared {
        schema_version: 1,
        operation_id: id.into(),
        changes,
    };
    validate(&prepared, &roots)?;
    writer.check_write_set(&roots)?;
    for c in &prepared.changes {
        if package::state(&c.target)? != c.before
            || package::state(&c.backup)? != EntryState::Missing
            || package::state(&c.staged)? != c.after
        {
            return Err(SkillsError::new(
                ErrorCode::Conflict,
                "Target or staging changed before prepare",
            ));
        }
    }
    files::private_directory(path.parent().unwrap())?;
    files::atomic_json(&path, &prepared)?;
    let pending = PendingTransaction::prepare(id, &roots, &repo.root, &path)?;
    writer.publish(&pending)?;
    Ok(pending)
}

fn validate(prepared: &Prepared, allowed_roots: &[PathBuf]) -> Result<()> {
    files::validate_id(&prepared.operation_id)?;
    if prepared.schema_version != 1 || prepared.changes.is_empty() || prepared.changes.len() > 32 {
        return Err(SkillsError::recovery("Unsupported transaction journal"));
    }
    let mut targets = std::collections::BTreeSet::new();
    for (i, c) in prepared.changes.iter().enumerate() {
        if package::entry_path(&c.target)? != c.target
            || !allowed_roots
                .iter()
                .any(|root| c.target.parent() == Some(root.as_path()))
            || !targets.insert(&c.target)
            || c.staged != artifact(&c.target, &prepared.operation_id, i, "staged")?
            || c.backup != artifact(&c.target, &prepared.operation_id, i, "backup")?
        {
            return Err(SkillsError::recovery(
                "Transaction resource is outside its authorized target",
            ));
        }
    }
    Ok(())
}

fn load(pending: &PendingTransaction, allowed_roots: &[PathBuf]) -> Result<Prepared> {
    if pending.journal_path != journal_path(&pending.data_dir, &pending.operation_id)? {
        return Err(SkillsError::recovery("Unexpected recovery journal path"));
    }
    let bytes = files::read_metadata_bytes(&pending.journal_path)?;
    if files::digest(&bytes) != pending.prepared_digest {
        return Err(SkillsError::recovery(
            "Prepared journal changed; recovery refused",
        ));
    }
    let prepared: Prepared = serde_json::from_slice(&bytes)
        .map_err(|_| SkillsError::recovery("Malformed recovery journal"))?;
    if prepared.operation_id != pending.operation_id {
        return Err(SkillsError::recovery("Recovery identity mismatch"));
    }
    validate(&prepared, allowed_roots)?;
    let mut expected = prepared
        .changes
        .iter()
        .map(|c| c.target.parent().unwrap().to_path_buf())
        .collect::<Vec<_>>();
    expected.sort();
    expected.dedup();
    if pending.write_set != expected {
        return Err(SkillsError::recovery("Recovery write set mismatch"));
    }
    Ok(prepared)
}

pub fn resource_targets(
    pending: &PendingTransaction,
    allowed_roots: &[PathBuf],
) -> Result<Vec<PathBuf>> {
    Ok(load(pending, allowed_roots)?
        .changes
        .into_iter()
        .map(|c| c.target)
        .collect())
}

fn durable_rename(source: &Path, destination: &Path) -> Result<()> {
    fs::rename(source, destination)?;
    // Renames between the skill directory and its sibling backup directory
    // change both directory entries. Persist both before advancing the journal.
    files::sync_directory(
        destination
            .parent()
            .ok_or_else(|| SkillsError::invalid("Missing rename destination parent"))?,
    )?;
    files::sync_directory(
        source
            .parent()
            .ok_or_else(|| SkillsError::invalid("Missing rename source parent"))?,
    )
}

pub fn commit(
    writer: &WriterGuard<'_>,
    pending: &PendingTransaction,
    allowed_roots: &[PathBuf],
) -> Result<()> {
    commit_observed(writer, pending, allowed_roots, |_| Ok(()))
}

pub(super) fn commit_observed(
    writer: &WriterGuard<'_>,
    pending: &PendingTransaction,
    allowed_roots: &[PathBuf],
    mut observe: impl FnMut(usize) -> Result<()>,
) -> Result<()> {
    let prepared = load(pending, allowed_roots)?;
    for (i, c) in prepared.changes.iter().enumerate() {
        if package::state(&c.target)? != c.before
            || package::state(&c.backup)? != EntryState::Missing
            || package::state(&c.staged)? != c.after
        {
            return Err(SkillsError::new(
                ErrorCode::Conflict,
                "Resource changed before commit",
            ));
        }
        if c.before != EntryState::Missing {
            durable_rename(&c.target, &c.backup)?;
        }
        observe(i * 2)?;
        if c.after != EntryState::Missing {
            durable_rename(&c.staged, &c.target)?;
        }
        observe(i * 2 + 1)?;
    }
    finish(writer, pending, &prepared, Outcome::Committed)
}

fn finish(
    writer: &WriterGuard<'_>,
    pending: &PendingTransaction,
    prepared: &Prepared,
    outcome: Outcome,
) -> Result<()> {
    for change in &prepared.changes {
        let expected = if outcome == Outcome::Committed {
            &change.after
        } else {
            &change.before
        };
        if &package::state(&change.target)? != expected {
            return Err(SkillsError::recovery(
                "Resource does not match the terminal transaction state",
            ));
        }
    }
    let receipt = Receipt {
        schema_version: 1,
        operation_id: pending.operation_id.clone(),
        prepared_digest: pending.prepared_digest.clone(),
        outcome,
    };
    files::atomic_json(
        &receipt_path(&pending.data_dir, &pending.operation_id)?,
        &receipt,
    )?;
    writer.clear_verified(pending)
}

pub fn recover(
    writer: &WriterGuard<'_>,
    pending: &PendingTransaction,
    allowed_roots: &[PathBuf],
) -> Result<Outcome> {
    let prepared = load(pending, allowed_roots)?;
    if let Some(receipt) = receipt(&pending.data_dir, &pending.operation_id)? {
        if receipt.prepared_digest != pending.prepared_digest {
            return Err(SkillsError::recovery(
                "Transaction receipt does not match journal",
            ));
        }
        finish(writer, pending, &prepared, receipt.outcome.clone())?;
        return Ok(receipt.outcome);
    }
    for c in prepared.changes.iter().rev() {
        let current = package::state(&c.target)?;
        let backup = package::state(&c.backup)?;
        if backup == c.before && backup != EntryState::Missing {
            if current == c.after && current != EntryState::Missing {
                package::remove_entry(&c.target)?;
            } else if current != EntryState::Missing {
                return Err(SkillsError::recovery(
                    "Target was changed after interruption; preserving it",
                ));
            }
            durable_rename(&c.backup, &c.target)?;
        } else if backup == EntryState::Missing {
            if c.before == EntryState::Missing {
                if current == c.after && current != EntryState::Missing {
                    package::remove_entry(&c.target)?;
                } else if current != EntryState::Missing {
                    return Err(SkillsError::recovery(
                        "New target was changed after interruption; preserving it",
                    ));
                }
            } else if current != c.before {
                return Err(SkillsError::recovery("Original resource backup is missing"));
            }
        } else {
            return Err(SkillsError::recovery(
                "Backup changed after interruption; preserving it",
            ));
        }
    }
    finish(writer, pending, &prepared, Outcome::RolledBack)?;
    Ok(Outcome::RolledBack)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::tests::Fixture;
    fn setup(f: &Fixture, id: &str) -> (Repository, Vec<Change>, PathBuf) {
        let repo = Repository::open(&f.0.join("data-a"), &f.0.join("coord")).unwrap();
        let target = files::private_directory(&f.0.join("target")).unwrap();
        let mut changes = Vec::new();
        for (i, name) in ["skill", "manifest.json"].iter().enumerate() {
            let path = target.join(name);
            fs::write(&path, format!("before-{name}")).unwrap();
            let staged = artifact(&path, id, i, "staged").unwrap();
            files::private_directory(staged.parent().unwrap()).unwrap();
            fs::write(&staged, format!("after-{name}")).unwrap();
            changes.push(Change {
                before: package::state(&path).unwrap(),
                after: package::state(&staged).unwrap(),
                target: path.clone(),
                staged,
                backup: artifact(&path, id, i, "backup").unwrap(),
            });
        }
        (repo, changes, target)
    }
    #[test]
    fn interruption_at_each_rename_recovers_from_another_data_directory() {
        for stop in 0..4 {
            let f = Fixture::new();
            let (repo, changes, target) = setup(&f, "test-transaction");
            let lease = repo.coordinator.enter().unwrap();
            let writer = repo.coordinator.writer(&lease).unwrap();
            let pending = prepare(&repo, &writer, "test-transaction", changes.clone()).unwrap();
            assert!(
                commit_observed(&writer, &pending, std::slice::from_ref(&target), |step| {
                    if step == stop {
                        Err(SkillsError::invalid("injected crash"))
                    } else {
                        Ok(())
                    }
                })
                .is_err()
            );
            drop(writer);
            drop(lease);
            let b = Repository::open(&f.0.join("data-b"), &f.0.join("coord")).unwrap();
            let lease = b.coordinator.enter().unwrap();
            let writer = b.coordinator.writer(&lease).unwrap();
            assert!(writer
                .check_write_set(std::slice::from_ref(&target))
                .is_err());
            assert_eq!(
                recover(
                    &writer,
                    &writer.pending().unwrap()[0],
                    std::slice::from_ref(&target)
                )
                .unwrap(),
                Outcome::RolledBack
            );
            for change in changes {
                assert_eq!(package::state(&change.target).unwrap(), change.before);
            }
            assert!(writer.pending().unwrap().is_empty());
        }
    }
    #[test]
    fn post_crash_user_changes_are_preserved_and_marker_remains() {
        let f = Fixture::new();
        let (repo, changes, target) = setup(&f, "edited");
        let lease = repo.coordinator.enter().unwrap();
        let writer = repo.coordinator.writer(&lease).unwrap();
        let pending = prepare(&repo, &writer, "edited", changes).unwrap();
        assert!(
            commit_observed(&writer, &pending, std::slice::from_ref(&target), |_| Err(
                SkillsError::invalid("stop")
            ))
            .is_err()
        );
        fs::write(target.join("skill"), "user edit").unwrap();
        assert!(recover(&writer, &pending, std::slice::from_ref(&target)).is_err());
        assert_eq!(
            fs::read_to_string(target.join("skill")).unwrap(),
            "user edit"
        );
        assert_eq!(writer.pending().unwrap().len(), 1);
    }
    #[test]
    fn committed_receipt_clears_stale_marker_without_replaying() {
        let f = Fixture::new();
        let (repo, changes, target) = setup(&f, "finished");
        let lease = repo.coordinator.enter().unwrap();
        let writer = repo.coordinator.writer(&lease).unwrap();
        let pending = prepare(&repo, &writer, "finished", changes.clone()).unwrap();
        commit(&writer, &pending, std::slice::from_ref(&target)).unwrap();
        files::atomic_json(
            &repo.coordinator.root().join("pending/finished.json"),
            &pending,
        )
        .unwrap();
        assert_eq!(
            recover(&writer, &pending, std::slice::from_ref(&target)).unwrap(),
            Outcome::Committed
        );
        for c in changes {
            assert_eq!(package::state(&c.target).unwrap(), c.after);
        }
        assert!(writer.pending().unwrap().is_empty());
    }

    #[test]
    fn real_process_exit_is_recovered_by_a_new_client() {
        for stop in [0, 3] {
            let f = Fixture::new();
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "skills::transactions::tests::transaction_crash_child",
                    "--ignored",
                    "--nocapture",
                ])
                .env("LOOM_TX_TEST_ROOT", &f.0)
                .env("LOOM_TX_TEST_STOP", stop.to_string())
                .status()
                .unwrap();
            assert_eq!(status.code(), Some(27));
            let repo = Repository::open(&f.0.join("data-b"), &f.0.join("coord")).unwrap();
            let lease = repo.coordinator.enter().unwrap();
            let writer = repo.coordinator.writer(&lease).unwrap();
            let pending = writer.pending().unwrap().remove(0);
            assert_eq!(
                recover(&writer, &pending, &[f.0.join("target")]).unwrap(),
                Outcome::RolledBack
            );
            assert_eq!(
                fs::read_to_string(f.0.join("target/skill")).unwrap(),
                "before-skill"
            );
            assert!(writer.pending().unwrap().is_empty());
        }
    }

    #[test]
    #[ignore = "isolated subprocess used by recovery test"]
    fn transaction_crash_child() {
        let f = Fixture(PathBuf::from(
            std::env::var_os("LOOM_TX_TEST_ROOT").expect("test root"),
        ));
        let stop: usize = std::env::var("LOOM_TX_TEST_STOP").unwrap().parse().unwrap();
        let (repo, changes, target) = setup(&f, "process-crash");
        let lease = repo.coordinator.enter().unwrap();
        let writer = repo.coordinator.writer(&lease).unwrap();
        let pending = prepare(&repo, &writer, "process-crash", changes).unwrap();
        let _ = commit_observed(&writer, &pending, &[target], |step| {
            if step == stop {
                std::process::exit(27);
            }
            Ok(())
        });
        panic!("fault point was not reached");
    }
}
