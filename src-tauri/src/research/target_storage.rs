//! Disk-backed staging and publication helpers; every mutation holds Writer's lease.
use super::{
    files,
    storage_budget::{ArtifactKind, Ledger, Publication},
    store::next_id,
    target_store::Writer,
};
use std::{fs, path::PathBuf};

pub struct DiskArea {
    pub key: String,
    pub path: PathBuf,
    pub capacity: u64,
    pub run_id: String,
}
impl Writer {
    pub fn save_context(
        &self,
        run_id: &str,
        context: &super::materials::Context,
    ) -> Result<(), String> {
        let run = self.target.run(run_id)?;
        if context.run_id != run_id || context.scope_digest != super::materials::scope_digest(&run)?
        {
            return Err("material_context_scope_mismatch".into());
        }
        context.render()?;
        let bytes = serde_json::to_vec(context).map_err(|e| e.to_string())?;
        let area = self.reserve_area(run_id, "steps", bytes.len() as u64)?;
        self.write_area(&area, "context.json", &bytes)?;
        self.settle_area(&area)?;
        let mut state = self.target.run(run_id)?.state;
        state.material_context = Some(super::materials::ContextRef {
            allocation_key: area.key,
            sha256: super::target_store::digest(&bytes),
        });
        self.save_state(state, &next_id("context-saved"))?;
        Ok(())
    }
    pub(super) fn report_area(&self, run_id: &str, required: u64) -> Result<DiskArea, String> {
        let ledger = self.ledger(run_id)?;
        if let Some(allocation) = ledger
            .allocations
            .get("report-headroom")
            .filter(|a| !a.settled && a.publication.is_none())
        {
            if required > allocation.capacity {
                return Err("report_disk_budget_exhausted".into());
            }
            let path = files::path(&self.target.root()?, &allocation.path)?;
            files::directory(&path)?;
            return Ok(DiskArea {
                key: "report-headroom".into(),
                path,
                capacity: allocation.capacity,
                run_id: run_id.into(),
            });
        }
        self.reserve_area(run_id, "staging", required)
    }
    pub(super) fn ledger(&self, run_id: &str) -> Result<Ledger, String> {
        self.check_run(run_id)?;
        let spec = self.target.run(run_id)?.spec;
        Ledger::load(&self.target.root()?, run_id, spec.budget.max_disk_bytes)
    }
    pub fn available_disk(&self, run_id: &str) -> Result<u64, String> {
        let run = self.target.run(run_id)?;
        Ok(run
            .spec
            .budget
            .max_disk_bytes
            .saturating_sub(run.state.usage.disk_bytes)
            .saturating_sub(run.state.usage.disk_reserved_bytes))
    }
    pub fn reserve_area(
        &self,
        run_id: &str,
        group: &str,
        capacity: u64,
    ) -> Result<DiskArea, String> {
        self.check_run(run_id)?;
        if !matches!(group, "staging" | "steps" | "workspace") {
            return Err("invalid_disk_area_group".into());
        }
        let key = next_id("disk");
        let relative = format!("runs/{run_id}/{group}/{key}");
        let root = self.target.root()?;
        let mut ledger = self.ledger(run_id)?;
        ledger.reserve(&root, &key, &relative, capacity)?;
        let path = files::path(&root, &relative)?;
        files::directory(&path)?;
        Ok(DiskArea {
            key,
            path,
            capacity,
            run_id: run_id.into(),
        })
    }
    pub fn write_area(&self, area: &DiskArea, relative: &str, bytes: &[u8]) -> Result<(), String> {
        self.check_run(&area.run_id)?;
        let ledger = self.ledger(&area.run_id)?;
        let allocation = ledger
            .allocations
            .get(&area.key)
            .ok_or("unknown_disk_allocation")?;
        if allocation.settled || files::path(&self.target.root()?, &allocation.path)? != area.path {
            return Err("closed_disk_area".into());
        }
        let current = crate::tool_process::directory_bytes(&area.path, area.capacity)?;
        if current.saturating_add(bytes.len() as u64) > area.capacity {
            return Err("disk_budget_exhausted".into());
        }
        files::write_bytes(&files::path(&area.path, relative)?, bytes)?;
        crate::tool_process::directory_bytes(&area.path, area.capacity)?;
        Ok(())
    }
    pub fn settle_area(&self, area: &DiskArea) -> Result<(), String> {
        let mut ledger = self.ledger(&area.run_id)?;
        ledger.settle(&self.target.root()?, &area.key)
    }
    pub fn append_area(&self, area: &DiskArea, relative: &str, bytes: &[u8]) -> Result<(), String> {
        use std::io::Write;
        self.check_run(&area.run_id)?;
        let ledger = self.ledger(&area.run_id)?;
        let allocation = ledger
            .allocations
            .get(&area.key)
            .ok_or("unknown_disk_allocation")?;
        if allocation.settled || files::path(&self.target.root()?, &allocation.path)? != area.path {
            return Err("closed_disk_area".into());
        }

        let current = crate::tool_process::directory_bytes(&area.path, area.capacity)?;
        if current.saturating_add(bytes.len() as u64) > area.capacity {
            return Err("disk_budget_exhausted".into());
        }
        let path = files::path(&area.path, relative)?;
        let mut options = fs::OpenOptions::new();
        options.append(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(path).map_err(|e| e.to_string())?;
        file.write_all(bytes)
            .and_then(|_| file.sync_data())
            .map_err(|e| e.to_string())
    }
    pub(super) fn publish_area(
        &self,
        area: &DiskArea,
        kind: ArtifactKind,
        id: &str,
    ) -> Result<(), String> {
        let root = self.target.root()?;
        let mut ledger = self.ledger(&area.run_id)?;
        let group = match kind {
            ArtifactKind::Snapshot => "snapshots",
            ArtifactKind::Evidence => "evidence",
            ArtifactKind::Report => "reports",
        };
        let destination = format!("{group}/{id}");
        let path = files::path(&root, &destination)?;
        if path.exists() {
            return Err("publication_already_exists".into());
        }
        files::directory(path.parent().ok_or("invalid_publication_parent")?)?;
        crate::tool_process::directory_bytes(&area.path, area.capacity)?;
        ledger.plan_publication(
            &root,
            &area.key,
            Publication {
                kind,
                id: id.into(),
                destination,
            },
        )?;
        fs::rename(&area.path, &path).map_err(|e| format!("publication_rename_failed: {e}"))?;
        fs::File::open(path.parent().ok_or("invalid_publication_parent")?)
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())?;
        ledger.settle(&root, &area.key)
    }
}
