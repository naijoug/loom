//! Durable per-run disk reservations. A crash never frees an outstanding write.
//! Metadata has its own reserved headroom so exhausted data quota can still end a run.
use super::{files, store::valid_id};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

const LEDGER_LIMIT: u64 = 512 * 1024;
const MAX_ALLOCATIONS: usize = 512;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Snapshot,
    Evidence,
    Report,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Publication {
    pub kind: ArtifactKind,
    pub id: String,
    pub destination: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Allocation {
    pub path: String,
    pub capacity: u64,
    pub actual: u64,
    pub settled: bool,
    pub publication: Option<Publication>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ledger {
    version: u32,
    pub run_id: String,
    max_bytes: u64,
    pub metadata_capacity: u64,
    pub allocations: BTreeMap<String, Allocation>,
}
impl Ledger {
    pub fn new(run_id: &str, max_bytes: u64) -> Result<Self, String> {
        if !valid_id(run_id) || max_bytes < 64 * 1024 {
            return Err("disk_budget_too_small".into());
        }
        Ok(Self {
            version: 1,
            run_id: run_id.into(),
            max_bytes,
            metadata_capacity: (max_bytes / 4).min(20 * 1024 * 1024),
            allocations: BTreeMap::new(),
        })
    }
    fn run_dir(&self, root: &Path) -> Result<PathBuf, String> {
        files::path(root, &format!("runs/{}", self.run_id))
    }
    pub fn load(root: &Path, run_id: &str, max_bytes: u64) -> Result<Self, String> {
        if !valid_id(run_id) {
            return Err("invalid_run_id".into());
        }
        let value: Self = files::read_json(
            &files::path(root, &format!("runs/{run_id}/disk.json"))?,
            LEDGER_LIMIT,
        )?;
        if value.version != 1
            || value.run_id != run_id
            || value.max_bytes != max_bytes
            || value.metadata_capacity != Self::new(run_id, max_bytes)?.metadata_capacity
        {
            return Err("disk_ledger_identity_mismatch".into());
        }
        value.validate(root)?;
        Ok(value)
    }
    fn charged(&self) -> Result<u64, String> {
        self.allocations
            .values()
            .try_fold(self.metadata_capacity, |sum, a| {
                sum.checked_add(if a.settled { a.actual } else { a.capacity })
                    .ok_or("disk_budget_overflow".into())
            })
    }
    fn validate(&self, root: &Path) -> Result<(), String> {
        if self.allocations.len() > MAX_ALLOCATIONS || self.charged()? > self.max_bytes {
            return Err("disk_budget_exhausted".into());
        }
        for (key, allocation) in &self.allocations {
            if !valid_id(key)
                || allocation.actual > allocation.capacity
                || !allocation
                    .path
                    .starts_with(&format!("runs/{}/", self.run_id))
            {
                return Err("invalid_disk_allocation".into());
            }
            files::path(root, &allocation.path)?;
            if let Some(publication) = &allocation.publication {
                let folder = match publication.kind {
                    ArtifactKind::Snapshot => "snapshots",
                    ArtifactKind::Evidence => "evidence",
                    ArtifactKind::Report => "reports",
                };
                if !valid_id(&publication.id)
                    || publication.destination != format!("{folder}/{}", publication.id)
                {
                    return Err("invalid_publication_path".into());
                }
                files::path(root, &publication.destination)?;
            }
        }
        Ok(())
    }
    pub fn metadata_bytes(&self, root: &Path) -> Result<u64, String> {
        let dir = self.run_dir(root)?;
        if !dir.exists() {
            return Ok(0);
        }
        let mut sum = 0u64;
        for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let meta = match fs::symlink_metadata(entry.path()) {
                Ok(meta) => meta,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.to_string()),
            };
            if meta.file_type().is_symlink() {
                return Err("unsafe_budget_path".into());
            }
            if meta.is_file() {
                sum = sum.checked_add(meta.len()).ok_or("disk_budget_overflow")?;
            }
        }
        Ok(sum)
    }
    pub fn check_metadata(&self, root: &Path, extra: u64) -> Result<(), String> {
        if self.metadata_bytes(root)?.saturating_add(extra) > self.metadata_capacity {
            Err("metadata_disk_budget_exhausted".into())
        } else {
            Ok(())
        }
    }
    pub fn save(&self, root: &Path) -> Result<(), String> {
        self.validate(root)?;
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        if bytes.len() as u64 > LEDGER_LIMIT {
            return Err("disk_ledger_limit".into());
        }
        self.check_metadata(root, bytes.len() as u64)?;
        files::write_bytes(&self.run_dir(root)?.join("disk.json"), &bytes)
    }
    /// Persist before opening a temporary file or starting a producer.
    pub fn reserve(
        &mut self,
        root: &Path,
        key: &str,
        path: &str,
        capacity: u64,
    ) -> Result<(), String> {
        if !valid_id(key)
            || capacity == 0
            || self.allocations.contains_key(key)
            || self.allocations.values().any(|a| a.path == path)
        {
            return Err("duplicate_or_invalid_disk_reservation".into());
        }
        let mut next = self.clone();
        next.allocations.insert(
            key.into(),
            Allocation {
                path: path.into(),
                capacity,
                actual: 0,
                settled: false,
                publication: None,
            },
        );
        next.save(root)?;
        *self = next;
        Ok(())
    }
    /// Publish intent is durable before rename. Recovery must validate content before linking it.
    pub fn plan_publication(
        &mut self,
        root: &Path,
        key: &str,
        publication: Publication,
    ) -> Result<(), String> {
        let mut next = self.clone();
        let allocation = next
            .allocations
            .get_mut(key)
            .ok_or("unknown_disk_allocation")?;
        if allocation.settled || allocation.publication.is_some() {
            return Err("publication_already_planned".into());
        }
        allocation.publication = Some(publication);
        next.save(root)?;
        *self = next;
        Ok(())
    }
    pub fn settle(&mut self, root: &Path, key: &str) -> Result<(), String> {
        let mut next = self.clone();
        let allocation = next
            .allocations
            .get_mut(key)
            .ok_or("unknown_disk_allocation")?;
        let staging = files::path(root, &allocation.path)?;
        let published = allocation
            .publication
            .as_ref()
            .map(|p| files::path(root, &p.destination))
            .transpose()?;
        if published.as_ref().is_some_and(|p| p.exists()) && staging.exists() {
            return Err("ambiguous_publication_storage".into());
        }
        let location = published
            .as_ref()
            .filter(|p| p.exists())
            .unwrap_or(&staging);
        let actual = super::super::tool_process::directory_bytes(location, allocation.capacity)?;
        allocation.actual = actual;
        allocation.settled = true;
        next.save(root)?;
        *self = next;
        Ok(())
    }
    pub fn observe(&mut self, root: &Path, key: &str) -> Result<(), String> {
        let mut next = self.clone();
        let allocation = next
            .allocations
            .get_mut(key)
            .ok_or("unknown_disk_allocation")?;
        allocation.actual = crate::tool_process::directory_bytes(
            &files::path(root, &allocation.path)?,
            allocation.capacity,
        )?;
        // Unknown producers cannot reclaim their remaining capacity just by restarting.
        next.save(root)?;
        *self = next;
        Ok(())
    }
    pub fn usage(&self, root: &Path) -> Result<(u64, u64), String> {
        let metadata = self.metadata_bytes(root)?;
        let actual = self.allocations.values().try_fold(metadata, |sum, a| {
            sum.checked_add(a.actual).ok_or("disk_budget_overflow")
        })?;
        let reserved = self
            .metadata_capacity
            .saturating_sub(metadata)
            .saturating_add(
                self.allocations
                    .values()
                    .filter(|a| !a.settled)
                    .map(|a| a.capacity.saturating_sub(a.actual))
                    .sum::<u64>(),
            );
        Ok((actual, reserved))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reservations_survive_reopen_and_rejected_growth_is_atomic() {
        let root = super::super::store::tests::temp_root();
        let mut ledger = Ledger::new("run-a", 64 * 1024).unwrap();
        ledger.save(&root).unwrap();
        ledger
            .reserve(&root, "write-a", "runs/run-a/data/a", 32 * 1024)
            .unwrap();
        let before = fs::read(root.join("runs/run-a/disk.json")).unwrap();
        assert!(ledger
            .reserve(&root, "write-b", "runs/run-a/data/b", 32 * 1024)
            .is_err());
        assert_eq!(fs::read(root.join("runs/run-a/disk.json")).unwrap(), before);
        let mut reopened = Ledger::load(&root, "run-a", 64 * 1024).unwrap();
        assert!(reopened.usage(&root).unwrap().1 >= 32 * 1024);
        files::write_bytes(&root.join("runs/run-a/data/a"), b"retained").unwrap();
        reopened.settle(&root, "write-a").unwrap();
        assert_eq!(reopened.allocations["write-a"].actual, 8);
        assert!(reopened.reserve(&root, "outside", "../private", 1).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
