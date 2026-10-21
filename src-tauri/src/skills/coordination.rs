//! One coordination domain per OS user, independent from the library data path.
//! Callers acquire a lifecycle lease before the short-lived writer lock.
use super::{
    error::{ErrorCode, Result, SkillsError},
    files,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
};

const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PendingTransaction {
    pub schema_version: u32,
    pub operation_id: String,
    pub write_set: Vec<PathBuf>,
    pub data_dir: PathBuf,
    pub journal_path: PathBuf,
    pub prepared_digest: String,
}

impl PendingTransaction {
    pub fn prepare(
        operation_id: &str,
        write_set: &[PathBuf],
        data_dir: &Path,
        journal_path: &Path,
    ) -> Result<Self> {
        files::validate_id(operation_id)?;
        let data_dir = files::resource_path(data_dir)?;
        let journal_path = files::resource_path(journal_path)?;
        if !journal_path.starts_with(&data_dir) || journal_path == data_dir {
            return Err(SkillsError::invalid(
                "Recovery journal must belong to its data directory",
            ));
        }
        files::regular_file(&journal_path)?;
        let bytes = files::read_metadata_bytes(&journal_path)?;
        let mut normalized = write_set
            .iter()
            .map(|p| files::resource_path(p))
            .collect::<Result<Vec<_>>>()?;
        normalized.sort();
        normalized.dedup();
        if normalized.is_empty() || normalized.iter().any(|p| p.parent().is_none()) {
            return Err(SkillsError::invalid(
                "Transaction needs bounded non-root resources",
            ));
        }
        // Persist the recovery description before publishing its common pointer.
        File::open(&journal_path)?.sync_all()?;
        files::sync_directory(journal_path.parent().unwrap())?;
        Ok(Self {
            schema_version: SCHEMA_VERSION,
            operation_id: operation_id.into(),
            write_set: normalized,
            data_dir,
            journal_path,
            prepared_digest: files::digest(&bytes),
        })
    }

    fn validate(&self) -> Result<()> {
        files::validate_id(&self.operation_id)
            .map_err(|_| SkillsError::recovery("Invalid pending transaction identity"))?;
        if self.schema_version != SCHEMA_VERSION
            || self.write_set.is_empty()
            || self.write_set.iter().any(|p| {
                !p.is_absolute()
                    || p.parent().is_none()
                    || p.components()
                        .any(|c| matches!(c, std::path::Component::ParentDir))
            })
            || !self.data_dir.is_absolute()
            || self
                .data_dir
                .components()
                .chain(self.journal_path.components())
                .any(|c| matches!(c, std::path::Component::ParentDir))
            || !self.journal_path.starts_with(&self.data_dir)
            || self.journal_path == self.data_dir
            || self.prepared_digest.len() != 64
            || !self.prepared_digest.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(SkillsError::recovery(
                "Unsupported or invalid pending transaction; writes are blocked",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct Coordinator {
    root: PathBuf,
}

pub struct LifecycleLease {
    root: PathBuf,
    file: File,
    updating: bool,
}

impl Drop for LifecycleLease {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

pub struct WriterGuard<'a> {
    coordinator: &'a Coordinator,
    _lease: &'a LifecycleLease,
    file: File,
}

impl Drop for WriterGuard<'_> {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

impl Coordinator {
    /// The desktop/CLI path resolver supplies the same fixed user directory.
    /// Tests supply a disposable directory; a custom dataDir must not be used.
    pub fn open(coordination_root: &Path) -> Result<Self> {
        let root = files::private_directory(coordination_root)?;
        files::private_directory(&root.join("pending"))?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn lock_file(&self, name: &str) -> Result<File> {
        let path = self.root.join(name);
        files::regular_file(&path)?;
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        Ok(options.open(path)?)
    }

    pub fn enter(&self) -> Result<LifecycleLease> {
        let file = self.lock_file("lifecycle.lock")?;
        file.try_lock_shared().map_err(|_| {
            SkillsError::new(
                ErrorCode::Updating,
                "Loom update admission is active; restart this command later",
            )
        })?;
        Ok(LifecycleLease {
            root: self.root.clone(),
            file,
            updating: false,
        })
    }

    pub fn writer<'a>(&'a self, lease: &'a LifecycleLease) -> Result<WriterGuard<'a>> {
        if lease.root != self.root {
            return Err(SkillsError::invalid(
                "Lifecycle lease belongs to another coordination domain",
            ));
        }
        let file = self.lock_file("writer.lock")?;
        file.try_lock().map_err(|_| {
            SkillsError::new(
                ErrorCode::LockBusy,
                "Another Skills client is committing changes",
            )
        })?;
        Ok(WriterGuard {
            coordinator: self,
            _lease: lease,
            file,
        })
    }

    /// An update never waits holding one lock for another active operation.
    pub fn try_update(&self) -> Result<LifecycleLease> {
        let file = self.lock_file("lifecycle.lock")?;
        file.try_lock().map_err(|_| {
            SkillsError::new(
                ErrorCode::LockBusy,
                "Skills operations are active; update deferred",
            )
        })?;
        let lease = LifecycleLease {
            root: self.root.clone(),
            file,
            updating: true,
        };
        if !self.writer(&lease)?.pending()?.is_empty() {
            return Err(SkillsError::recovery(
                "Resolve unfinished Skills transactions before updating Loom",
            ));
        }
        Ok(lease)
    }
}

impl WriterGuard<'_> {
    pub fn pending(&self) -> Result<Vec<PendingTransaction>> {
        let directory = self.coordinator.root.join("pending");
        let mut entries = Vec::new();
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            // Interrupted atomic metadata writes are never transaction markers.
            if entry.file_name().to_string_lossy().starts_with(".write-") {
                continue;
            }
            let path = entry.path();
            let record: PendingTransaction = files::read_json(&path).map_err(|_| {
                SkillsError::recovery(
                    "Pending transaction index cannot be read; writes are blocked",
                )
            })?;
            record.validate()?;
            if path.file_name().and_then(|v| v.to_str())
                != Some(&format!("{}.json", record.operation_id))
            {
                return Err(SkillsError::recovery(
                    "Pending transaction filename does not match identity",
                ));
            }
            entries.push(record);
        }
        entries.sort_by(|a, b| a.operation_id.cmp(&b.operation_id));
        Ok(entries)
    }

    pub fn check_write_set(&self, paths: &[PathBuf]) -> Result<()> {
        if self._lease.updating {
            return Err(SkillsError::new(
                ErrorCode::Updating,
                "Cannot mutate skills through an update lease",
            ));
        }
        let paths = paths
            .iter()
            .map(|p| files::resource_path(p))
            .collect::<Result<Vec<_>>>()?;
        for record in self.pending()? {
            if paths.iter().any(|p| {
                record
                    .write_set
                    .iter()
                    .any(|q| p.starts_with(q) || q.starts_with(p))
            }) {
                return Err(SkillsError::recovery(format!(
                    "Operation {} must be recovered before writing this resource",
                    record.operation_id
                )));
            }
        }
        Ok(())
    }

    /// Journal and backups must be durable before calling this, and no target
    /// may be mutated until it succeeds. Recovery clearing is intentionally not
    /// exposed until the transaction engine can verify a terminal receipt.
    pub fn publish(&self, record: &PendingTransaction) -> Result<()> {
        record.validate()?;
        let prepared = files::read_metadata_bytes(&record.journal_path)?;
        if files::digest(&prepared) != record.prepared_digest {
            return Err(SkillsError::new(
                ErrorCode::Conflict,
                "Prepared journal changed before publication",
            ));
        }
        self.check_write_set(&record.write_set)?;
        let path = self
            .coordinator
            .root
            .join("pending")
            .join(format!("{}.json", record.operation_id));
        if path.try_exists()? {
            return Err(SkillsError::new(
                ErrorCode::Conflict,
                "Operation identity already exists",
            ));
        }
        files::atomic_json(&path, record)
    }
}
