//! Immutable target materials and append-only run state. All writes require a lease.
use super::{
    admission::{self, Lease},
    files,
    models::{Research, Workflow},
    storage_budget::{ArtifactKind, Ledger},
    store::{next_id, valid_id, Store},
    target_models::*,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn id(value: &str) -> Result<(), String> {
    if valid_id(value) {
        Ok(())
    } else {
        Err("invalid_research_resource_id".into())
    }
}

#[derive(Clone)]
pub struct TargetStore {
    pub(super) store: Store,
    pub(super) research_id: String,
}
pub struct Writer {
    pub(super) target: TargetStore,
    _lease: Lease,
    pub(super) run_id: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JournalEvent {
    sequence: u64,
    operation_id: String,
    state: RunState,
}

impl TargetStore {
    /// Publish the initial record and its run together. No list reader can see
    /// a target record whose initial spec/ledger has not been durably written.
    pub fn create_initial(
        store: Store,
        research: &Research,
        spec: &RunSpec,
    ) -> Result<(Self, Writer), String> {
        if research.workflow != Workflow::Target
            || spec.kind != RunKind::Initial
            || research.research_id != spec.research_id
        {
            return Err("invalid_initial_research".into());
        }
        let mut lease = admission::acquire(&store.root, &research.research_id)?;
        lease.bind(&spec.run_id, "target")?;
        let destination = store.dir(&research.research_id)?;
        if destination.exists() {
            return Err("research_already_exists".into());
        }
        let staging_root = files::path(&store.root, &format!(".creating-{}", next_id("research")))?;
        fs::create_dir(&staging_root).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&staging_root, fs::Permissions::from_mode(0o700))
                .map_err(|e| e.to_string())?;
        }
        let staged = Store {
            root: staging_root.clone(),
        };
        let result = (|| {
            staged.create(research)?;
            let target = Self::open(staged.clone(), &research.research_id)?;
            let writer = target.writer(Some(&spec.run_id))?;
            writer.create_run(spec)?;
            drop(writer);
            fs::rename(staged.dir(&research.research_id)?, &destination)
                .map_err(|e| e.to_string())?;
            fs::File::open(&store.root)
                .and_then(|f| f.sync_all())
                .map_err(|e| e.to_string())?;
            let target = Self::open(store.clone(), &research.research_id)?;
            Ok((
                target.clone(),
                Writer {
                    target,
                    _lease: lease,
                    run_id: Some(spec.run_id.clone()),
                },
            ))
        })();
        // Only our unpublished staging container is removed; published data is immutable.
        let _ = fs::remove_dir_all(staging_root);
        result
    }
    pub fn open(store: Store, research_id: &str) -> Result<Self, String> {
        let research = store.load(research_id)?;
        if research.workflow != Workflow::Target
            || research.schema_version != TARGET_SCHEMA_VERSION
            || research.target.is_none()
        {
            return Err("workflow_mismatch".into());
        }
        Ok(Self {
            store,
            research_id: research_id.into(),
        })
    }
    pub fn root(&self) -> Result<PathBuf, String> {
        self.store.dir(&self.research_id)
    }
    pub(super) fn path(&self, relative: &str) -> Result<PathBuf, String> {
        files::path(&self.root()?, relative)
    }
    pub fn writer(&self, run_id: Option<&str>) -> Result<Writer, String> {
        let mut lease = admission::acquire(&self.store.root, &self.research_id)?;
        // Revalidate after lock acquisition; deletion may have won the race.
        Self::open(self.store.clone(), &self.research_id)?;
        if let Some(run) = run_id {
            id(run)?;
            lease.bind(run, "target")?;
        }
        Ok(Writer {
            target: self.clone(),
            _lease: lease,
            run_id: run_id.map(str::to_string),
        })
    }
    pub fn run(&self, run_id: &str) -> Result<RunView, String> {
        id(run_id)?;
        let spec: RunSpec = files::read_json(
            &self.path(&format!("runs/{run_id}/spec.json"))?,
            1024 * 1024,
        )?;
        if spec.schema_version != TARGET_SCHEMA_VERSION
            || spec.research_id != self.research_id
            || spec.run_id != run_id
        {
            return Err("run_identity_mismatch".into());
        }
        let mut state = RunState::new(&spec);
        let journal = self.path(&format!("runs/{run_id}/events.jsonl"))?;
        if journal.exists() {
            let bytes = files::read_bytes(&journal, 16 * 1024 * 1024)?;
            let end = bytes.iter().rposition(|b| *b == b'\n').map_or(0, |n| n + 1);
            for line in bytes[..end]
                .split(|b| *b == b'\n')
                .filter(|l| !l.is_empty())
            {
                let event: JournalEvent =
                    serde_json::from_slice(line).map_err(|_| "corrupt_research_journal")?;
                if event.sequence != state.sequence + 1
                    || event.state.sequence != event.sequence
                    || event.state.run_id != run_id
                {
                    return Err("research_journal_sequence_mismatch".into());
                }
                state = event.state;
            }
        }
        if state.status.active()
            && admission::owner(&self.store.root, &self.research_id)?
                .as_ref()
                .is_none_or(|o| o.run_id != run_id)
        {
            state.status = RunStatus::Interrupted;
            state.stop_reason = Some("interrupted".into());
        } else if state.status.active()
            && admission::cancelled(&self.store.root, &self.research_id, run_id)
        {
            state.status = RunStatus::Cancelling;
        }
        if self.path(&format!("runs/{run_id}/disk.json"))?.exists() {
            let ledger = Ledger::load(&self.root()?, run_id, spec.budget.max_disk_bytes)?;
            let (actual, reserved) = ledger.usage(&self.root()?)?;
            state.usage.disk_bytes = actual;
            state.usage.disk_reserved_bytes = reserved;
        }
        Ok(RunView { spec, state })
    }
    pub fn list_runs(&self) -> Result<Vec<RunView>, String> {
        let research = self.store.load(&self.research_id)?;
        research
            .target
            .ok_or("workflow_mismatch")?
            .run_ids
            .iter()
            .map(|id| self.run(id))
            .collect()
    }
    pub fn cancel(&self, run_id: &str) -> Result<bool, String> {
        let run = self.run(run_id)?;
        if !run.state.status.active() {
            return Ok(false);
        }
        admission::request_cancel(&self.store.root, &self.research_id, run_id)
    }
    pub fn snapshot(&self, snapshot_id: &str) -> Result<SourceSnapshot, String> {
        id(snapshot_id)?;
        let value: SourceSnapshot = files::read_json(
            &self.path(&format!("snapshots/{snapshot_id}/manifest.json"))?,
            8 * 1024 * 1024,
        )
        .map_err(|_| "snapshot_unavailable")?;
        let expected = snapshot_identity(&value.identity, &value.files)?;
        let total = value.files.values().try_fold(0u64, |sum, file| {
            sum.checked_add(file.bytes)
                .ok_or("snapshot_integrity_mismatch")
        })?;
        if value.snapshot_id != snapshot_id
            || expected != snapshot_id
            || value.files.len() > 20_000
            || total != value.total_bytes
        {
            return Err("snapshot_integrity_mismatch".into());
        }
        Ok(value)
    }
    fn verify_snapshot(&self, snapshot: &SourceSnapshot) -> Result<(), String> {
        for (relative, entry) in &snapshot.files {
            let bytes = files::read_bytes(
                &self.path(&format!(
                    "snapshots/{}/files/{relative}",
                    snapshot.snapshot_id
                ))?,
                16 * 1024 * 1024,
            )?;
            if bytes.len() as u64 != entry.bytes || digest(&bytes) != entry.sha256 {
                return Err("snapshot_integrity_mismatch".into());
            }
        }
        Ok(())
    }
    pub fn source(&self, snapshot_id: &str, path: &str) -> Result<Vec<u8>, String> {
        let snapshot = self.snapshot(snapshot_id)?;
        let file = snapshot.files.get(path).ok_or("source_outside_snapshot")?;
        let bytes = files::read_bytes(
            &self.path(&format!("snapshots/{snapshot_id}/files/{path}"))?,
            16 * 1024 * 1024,
        )
        .map_err(|_| "snapshot_unavailable")?;
        if bytes.len() as u64 != file.bytes || digest(&bytes) != file.sha256 {
            return Err("snapshot_integrity_mismatch".into());
        }
        Ok(bytes)
    }
    pub fn report(&self, report_id: &str) -> Result<ReportContent, String> {
        id(report_id)?;
        let report: Report = files::read_json(
            &self.path(&format!("reports/{report_id}/manifest.json"))?,
            128 * 1024,
        )?;
        if report.report_id != report_id || report_identity(&report)? != report_id {
            return Err("report_identity_mismatch".into());
        }
        let run = self.run(&report.run_id)?;
        if !run.state.report_ids.iter().any(|r| r == report_id) {
            return Err("unpublished_report".into());
        }
        if report
            .snapshot_ids
            .iter()
            .any(|id| !run.state.snapshot_ids.contains(id))
            || report
                .evidence_ids
                .iter()
                .any(|id| !run.state.evidence_ids.contains(id))
            || report
                .default_source_snapshot_id
                .as_ref()
                .is_some_and(|id| !report.snapshot_ids.contains(id))
        {
            return Err("report_material_scope_mismatch".into());
        }
        let bytes = files::read_bytes(
            &self.path(&format!("reports/{report_id}/report.md"))?,
            4 * 1024 * 1024,
        )?;
        if digest(&bytes) != report.markdown_sha256 {
            return Err("report_integrity_mismatch".into());
        }
        let markdown = String::from_utf8(bytes).map_err(|_| "invalid_report_utf8")?;
        Ok(ReportContent { report, markdown })
    }
    pub fn report_source(
        &self,
        report_id: &str,
        path: &str,
        start: u32,
        end: u32,
    ) -> Result<Material, String> {
        let report = self.report(report_id)?.report;
        let snapshot = report
            .default_source_snapshot_id
            .ok_or("report_has_no_source_snapshot")?;
        if !report.snapshot_ids.contains(&snapshot) {
            return Err("report_snapshot_mismatch".into());
        }
        source_window(self.source(&snapshot, path)?, start, end)
    }
    pub fn material(&self, run_id: &str, request: &MaterialRequest) -> Result<Material, String> {
        let run = self.run(run_id)?;
        let state = run.state;
        if run.spec.kind == RunKind::EvidenceFollowup
            && (state.snapshot_ids != run.spec.source_snapshots
                || state.evidence_ids != run.spec.source_evidence)
        {
            return Err("followup_material_scope_changed".into());
        }
        match request {
            MaterialRequest::EvidenceIndex { offset, limit } => {
                if *limit == 0 || *limit > 16 || *offset > state.evidence_ids.len() {
                    return Err("invalid_material_cursor".into());
                }
                let end = offset.saturating_add(*limit).min(state.evidence_ids.len());
                let mut rows = Vec::new();
                for id in &state.evidence_ids[*offset..end] {
                    let (record, _) = self.evidence(id)?;
                    rows.push(serde_json::json!({"evidenceId":id,"tool":record.tool.chars().take(128).collect::<String>(),"coverage":record.coverage.chars().take(128).collect::<String>(),
                        "limitations":record.limitations.iter().take(4).map(|s|s.chars().take(160).collect::<String>()).collect::<Vec<_>>(),"limitationsCount":record.limitations.len()}));
                }
                Ok(Material {
                    text: serde_json::to_string(&rows).map_err(|e| e.to_string())?,
                    next_cursor: (end < state.evidence_ids.len()).then_some(end),
                    truncated: end < state.evidence_ids.len(),
                })
            }
            MaterialRequest::Source {
                snapshot_id,
                path,
                start_line,
                end_line,
            } => {
                if !state.snapshot_ids.contains(snapshot_id) {
                    return Err("material_scope_mismatch".into());
                }
                source_window(self.source(snapshot_id, path)?, *start_line, *end_line)
            }
            MaterialRequest::Tree {
                snapshot_id,
                offset,
                limit,
            } => {
                if !state.snapshot_ids.contains(snapshot_id) || *limit == 0 || *limit > 200 {
                    return Err("material_scope_mismatch".into());
                }
                let snapshot = self.snapshot(snapshot_id)?;
                let paths: Vec<_> = snapshot.files.keys().collect();
                if *offset > paths.len() {
                    return Err("invalid_material_cursor".into());
                }
                let mut end = *offset;
                let mut chars = 0usize;
                while end < offset.saturating_add(*limit).min(paths.len()) {
                    let size = paths[end].chars().count() + 1;
                    if chars + size > 12_000 {
                        break;
                    }
                    chars += size;
                    end += 1;
                }
                if end == *offset && end < paths.len() {
                    return Err("material_entry_limit".into());
                }
                Ok(Material {
                    text: paths[*offset..end]
                        .iter()
                        .map(|p| p.as_str())
                        .collect::<Vec<_>>()
                        .join("\n"),
                    next_cursor: (end < paths.len()).then_some(end),
                    truncated: end < paths.len(),
                })
            }
            MaterialRequest::Evidence {
                evidence_id,
                offset,
                limit,
            } => {
                if !state.evidence_ids.contains(evidence_id) || *limit == 0 || *limit > 48_000 {
                    return Err("material_scope_mismatch".into());
                }
                let (_, text) = self.evidence(evidence_id)?;
                let chars: Vec<_> = text.chars().collect();
                if *offset > chars.len() {
                    return Err("invalid_material_cursor".into());
                }
                let end = offset.saturating_add(*limit).min(chars.len());
                Ok(Material {
                    text: chars[*offset..end].iter().collect(),
                    next_cursor: (end < chars.len()).then_some(end),
                    truncated: end < chars.len(),
                })
            }
        }
    }
    pub fn evidence(&self, evidence_id: &str) -> Result<(EvidenceRecord, String), String> {
        id(evidence_id)?;
        let manifest: EvidenceRecord = files::read_json(
            &self.path(&format!("evidence/{evidence_id}/manifest.json"))?,
            128 * 1024,
        )?;
        if manifest.evidence_id != evidence_id
            || evidence_identity(&manifest.run_id, &manifest.raw_sha256) != evidence_id
        {
            return Err("evidence_identity_mismatch".into());
        }
        let bytes = files::read_bytes(
            &self.path(&format!("evidence/{evidence_id}/model-view.json"))?,
            16 * 1024 * 1024,
        )?;
        if digest(&bytes) != manifest.model_sha256 {
            return Err("evidence_integrity_mismatch".into());
        }
        Ok((
            manifest,
            String::from_utf8(bytes).map_err(|_| "invalid_evidence_utf8")?,
        ))
    }
}

impl Writer {
    /// Export a consistent published report while holding the same lease used
    /// by runs and deletion. Destinations are generated only within app data.
    pub fn export_report(&self, input: &ExportInput) -> Result<ExportResult, String> {
        let report = self.target.report(&input.report_id)?;
        let selected = if input.evidence_ids.is_empty() {
            report.report.evidence_ids.clone()
        } else {
            input.evidence_ids.clone()
        };
        let mut seen = std::collections::BTreeSet::new();
        if selected
            .iter()
            .any(|id| !report.report.evidence_ids.contains(id) || !seen.insert(id.clone()))
        {
            return Err("export_evidence_scope_mismatch".into());
        }
        let exports = self.target.path("exports")?;
        files::directory(&exports)?;
        let staging = exports.join(next_id("pending"));
        let destination = exports.join(next_id("export"));
        files::directory(&staging)?;
        let result = (|| {
            let mut written = Vec::new();
            let mut bytes_written = 0u64;
            let mut write = |relative: &str, bytes: &[u8]| -> Result<(), String> {
                bytes_written = bytes_written
                    .checked_add(bytes.len() as u64)
                    .ok_or("export_size_limit")?;
                if bytes_written > 512 * 1024 * 1024 {
                    return Err("export_size_limit".into());
                }
                files::write_bytes(&files::path(&staging, relative)?, bytes)?;
                written.push(relative.to_string());
                Ok(())
            };
            write("report.md", report.markdown.as_bytes())?;
            write(
                "report.json",
                &serde_json::to_vec_pretty(&report.report).map_err(|e| e.to_string())?,
            )?;
            let mut records = Vec::new();
            for evidence_id in &selected {
                let (record, view) = self.target.evidence(evidence_id)?;
                write(
                    &format!("evidence/{evidence_id}/model-view.json"),
                    view.as_bytes(),
                )?;
                if input.include_raw {
                    let raw = files::read_bytes(
                        &self
                            .target
                            .path(&format!("evidence/{evidence_id}/source.bundle.json"))?,
                        64 * 1024 * 1024,
                    )?;
                    if raw.len() as u64 != record.bytes || digest(&raw) != record.raw_sha256 {
                        return Err("raw_evidence_integrity_mismatch".into());
                    }
                    write(&format!("evidence/{evidence_id}/source.bundle.json"), &raw)?;
                }
                records.push(record);
            }
            write(
                "evidence.json",
                &serde_json::to_vec_pretty(&records).map_err(|e| e.to_string())?,
            )?;
            let result = ExportResult {
                directory: destination.to_string_lossy().into_owned(),
                files: written,
                includes_raw: input.include_raw,
                warning: input.include_raw.then(|| {
                    "Raw evidence may contain target data and secrets. Review before sharing."
                        .into()
                }),
            };
            files::write_json(&staging.join("export.json"), &result)?;
            fs::rename(&staging, &destination).map_err(|e| e.to_string())?;
            fs::File::open(&exports)
                .and_then(|f| f.sync_all())
                .map_err(|e| e.to_string())?;
            Ok(result)
        })();
        let _ = fs::remove_dir_all(staging);
        result
    }
    pub fn bind(&mut self, run_id: &str) -> Result<(), String> {
        if self.run_id.is_some() {
            return Err("writer_already_bound".into());
        }
        self._lease.bind(run_id, "target")?;
        self.run_id = Some(run_id.into());
        Ok(())
    }
    pub(super) fn check_run(&self, run_id: &str) -> Result<(), String> {
        if self.run_id.as_deref().is_some_and(|id| id != run_id) {
            return Err("writer_owner_mismatch".into());
        }
        Ok(())
    }
    pub fn create_run(&self, spec: &RunSpec) -> Result<(), String> {
        self.check_run(&spec.run_id)?;
        if spec.research_id != self.target.research_id
            || spec.schema_version != TARGET_SCHEMA_VERSION
        {
            return Err("run_identity_mismatch".into());
        }
        id(&spec.run_id)?;
        spec.budget.validate(spec.input.mode)?;
        if spec.kind == RunKind::EvidenceFollowup {
            let report = self
                .target
                .report(
                    spec.parent_report_id
                        .as_deref()
                        .ok_or("missing_parent_report")?,
                )?
                .report;
            if spec.source_snapshots != report.snapshot_ids
                || spec.source_evidence != report.evidence_ids
                || spec.budget.max_collections != 0
            {
                return Err("followup_material_scope_mismatch".into());
            }
        } else if spec.parent_report_id.is_some()
            || !spec.source_snapshots.is_empty()
            || !spec.source_evidence.is_empty()
        {
            return Err("invalid_initial_material_scope".into());
        }
        let directory = self.target.path(&format!("runs/{}", spec.run_id))?;
        if directory.exists() {
            return Err("run_already_exists".into());
        }
        files::directory(&directory)?;
        let mut ledger = Ledger::new(&spec.run_id, spec.budget.max_disk_bytes)?;
        let spec_bytes = serde_json::to_vec_pretty(spec).map_err(|e| e.to_string())?;
        ledger.check_metadata(&self.target.root()?, spec_bytes.len() as u64 + 8192)?;
        files::write_json(&directory.join("spec.json"), spec)?;
        ledger.save(&self.target.root()?)?;
        ledger.reserve(
            &self.target.root()?,
            "report-headroom",
            &format!("runs/{}/staging/report-headroom", spec.run_id),
            (4 * 1024 * 1024 + 256 * 1024).min(spec.budget.max_disk_bytes / 8),
        )?;
        let path = self.target.path("research.json")?;
        let mut research: Research = files::read_json(&path, 4 * 1024 * 1024)?;
        research
            .target
            .as_mut()
            .ok_or("workflow_mismatch")?
            .run_ids
            .push(spec.run_id.clone());
        research.updated_at_ms = crate::models::now_ms();
        files::write_json(&path, &research)
    }
    pub fn save_state(&self, mut state: RunState, operation_id: &str) -> Result<RunState, String> {
        self.check_run(&state.run_id)?;
        id(operation_id)?;
        let current = self.target.run(&state.run_id)?;
        if current.state.sequence != state.sequence {
            return Err("stale_run_revision".into());
        }
        if current
            .state
            .snapshot_ids
            .iter()
            .any(|id| !state.snapshot_ids.contains(id))
            || current
                .state
                .evidence_ids
                .iter()
                .any(|id| !state.evidence_ids.contains(id))
            || current
                .state
                .report_ids
                .iter()
                .any(|id| !state.report_ids.contains(id))
        {
            return Err("published_materials_are_immutable".into());
        }
        if current.spec.kind == RunKind::EvidenceFollowup
            && (state.snapshot_ids != current.spec.source_snapshots
                || state.evidence_ids != current.spec.source_evidence)
        {
            return Err("followup_material_scope_changed".into());
        }
        let path = self
            .target
            .path(&format!("runs/{}/events.jsonl", state.run_id))?;
        // Truncate only an incomplete trailing record, under the writer lease.
        if path.exists() {
            let bytes = files::read_bytes(&path, 16 * 1024 * 1024)?;
            let end = bytes.iter().rposition(|b| *b == b'\n').map_or(0, |n| n + 1);
            if end != bytes.len() {
                let f = fs::OpenOptions::new()
                    .write(true)
                    .open(&path)
                    .map_err(|e| e.to_string())?;
                f.set_len(end as u64)
                    .and_then(|_| f.sync_all())
                    .map_err(|e| e.to_string())?;
            }
        }
        state.sequence += 1;
        state.updated_at_ms = crate::models::now_ms();
        let ledger = self.ledger(&state.run_id)?;
        let (actual, reserved) = ledger.usage(&self.target.root()?)?;
        state.usage.disk_bytes = actual;
        state.usage.disk_reserved_bytes = reserved;
        let event = JournalEvent {
            sequence: state.sequence,
            operation_id: operation_id.into(),
            state: state.clone(),
        };
        let mut bytes = serde_json::to_vec(&event).map_err(|e| e.to_string())?;
        bytes.push(b'\n');
        if fs::metadata(&path).map_or(0, |m| m.len()) + bytes.len() as u64 > 16 * 1024 * 1024 {
            return Err("journal_limit".into());
        }
        ledger.check_metadata(
            &self.target.root()?,
            bytes.len() as u64
                + serde_json::to_vec_pretty(&state)
                    .map_err(|e| e.to_string())?
                    .len() as u64
                + 1,
        )?;
        let mut options = fs::OpenOptions::new();
        options.append(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut f = options.open(&path).map_err(|e| e.to_string())?;
        f.write_all(&bytes)
            .and_then(|_| f.sync_all())
            .map_err(|e| e.to_string())?;
        files::write_json(
            &self
                .target
                .path(&format!("runs/{}/state.json", state.run_id))?,
            &state,
        )?;
        Ok(state)
    }
    pub fn recover(&self, run_id: &str) -> Result<RunState, String> {
        self.check_run(run_id)?;
        let mut state = self.target.run(run_id)?.state;
        let path = self.target.path("research.json")?;
        let mut research: Research = files::read_json(&path, 4 * 1024 * 1024)?;
        let runs = &mut research.target.as_mut().ok_or("workflow_mismatch")?.run_ids;
        if !runs.iter().any(|id| id == run_id) {
            runs.push(run_id.into());
            research.updated_at_ms = crate::models::now_ms();
            files::write_json(&path, &research)?;
        }
        let mut ledger = self.ledger(run_id)?;
        let mut publications: Vec<_> = ledger
            .allocations
            .iter()
            .filter_map(|(key, allocation)| {
                allocation
                    .publication
                    .clone()
                    .map(|publication| (key.clone(), publication))
            })
            .collect();
        publications.sort_by_key(|(_, p)| match p.kind {
            ArtifactKind::Snapshot => 0,
            ArtifactKind::Evidence => 1,
            ArtifactKind::Report => 2,
        });
        for (key, publication) in publications {
            let path = self.target.path(&publication.destination)?;
            if !path.exists() {
                if ledger.allocations.get(&key).is_some_and(|a| a.settled) {
                    return Err("published_artifact_missing".into());
                }
                continue;
            }
            match publication.kind {
                ArtifactKind::Snapshot => {
                    if self.target.run(run_id)?.spec.kind == RunKind::EvidenceFollowup {
                        return Err("followup_cannot_collect".into());
                    }
                    let snapshot = self.target.snapshot(&publication.id)?;
                    self.target.verify_snapshot(&snapshot)?;
                    if !state.snapshot_ids.contains(&publication.id) {
                        state.snapshot_ids.push(publication.id.clone());
                    }
                }
                ArtifactKind::Evidence => {
                    if self.target.run(run_id)?.spec.kind == RunKind::EvidenceFollowup {
                        return Err("followup_cannot_collect".into());
                    }
                    let (record, _) = self.target.evidence(&publication.id)?;
                    let raw = files::read_bytes(
                        &files::path(&path, "source.bundle.json")?,
                        64 * 1024 * 1024,
                    )?;
                    if record.run_id != run_id
                        || record.raw_sha256 != digest(&raw)
                        || record.bytes != raw.len() as u64
                    {
                        return Err("evidence_integrity_mismatch".into());
                    }
                    if !state.evidence_ids.contains(&publication.id) {
                        state.evidence_ids.push(publication.id.clone());
                    }
                }
                ArtifactKind::Report => {
                    let report: Report =
                        files::read_json(&files::path(&path, "manifest.json")?, 128 * 1024)?;
                    let markdown =
                        files::read_bytes(&files::path(&path, "report.md")?, 4 * 1024 * 1024)?;
                    if report.run_id != run_id
                        || report.report_id != publication.id
                        || report_identity(&report)? != publication.id
                        || report.markdown_sha256 != digest(&markdown)
                        || report
                            .snapshot_ids
                            .iter()
                            .any(|id| !state.snapshot_ids.contains(id))
                        || report
                            .evidence_ids
                            .iter()
                            .any(|id| !state.evidence_ids.contains(id))
                    {
                        return Err("report_integrity_mismatch".into());
                    }
                    if !state.report_ids.contains(&publication.id) {
                        state.report_ids.push(publication.id.clone());
                    }
                }
            }
            ledger.settle(&self.target.root()?, &key)?;
        }
        for key in ledger
            .allocations
            .iter()
            .filter(|(_, a)| !a.settled)
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>()
        {
            ledger.observe(&self.target.root()?, &key)?;
        }
        if state.status.active() || state.status == RunStatus::Interrupted {
            state.status = RunStatus::Interrupted;
            state.stop_reason = Some("interrupted".into());
            state.usage.recover();
        }
        self.save_state(state, &next_id("recovery"))
    }
    pub fn snapshot_from(
        &self,
        source: &Path,
        paths: &[String],
        identity: &str,
        max_bytes: u64,
    ) -> Result<SourceSnapshot, String> {
        let run_id = self
            .run_id
            .as_deref()
            .ok_or("snapshot_requires_bound_writer")?;
        if self.target.run(run_id)?.spec.kind == RunKind::EvidenceFollowup {
            return Err("followup_cannot_collect".into());
        }
        if paths.len() > 20_000 || identity.len() > 8000 {
            return Err("snapshot_manifest_limit".into());
        }
        let mut lengths = BTreeMap::new();
        let mut total = 0u64;
        let mut overhead = identity.len() as u64 * 6 + 4096;
        for relative in paths {
            let path = files::path(source, relative)?;
            let meta = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
            if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 16 * 1024 * 1024 {
                return Err("unsafe_or_oversized_snapshot_file".into());
            }
            if lengths.insert(relative.clone(), meta.len()).is_some() {
                return Err("duplicate_snapshot_path".into());
            }
            total = total.checked_add(meta.len()).ok_or("snapshot_limit")?;
            overhead = overhead
                .checked_add(relative.len() as u64 * 6 + 512)
                .ok_or("snapshot_limit")?;
            if total > max_bytes {
                return Err("snapshot_limit".into());
            }
        }
        let area = self.reserve_area(
            run_id,
            "staging",
            total.checked_add(overhead).ok_or("snapshot_limit")?,
        )?;
        let mut entries = BTreeMap::new();
        let result = (|| {
            let mut copied = 0u64;
            for (relative, length) in lengths {
                let bytes = files::read_bytes(&files::path(source, &relative)?, 16 * 1024 * 1024)?;
                if bytes.len() as u64 != length {
                    return Err("source_changed_during_snapshot".into());
                }
                copied = copied.checked_add(length).ok_or("snapshot_limit")?;
                if copied > total {
                    return Err("snapshot_limit".into());
                }
                // This private staging area has one writer. The precomputed bound
                // and per-file length check avoid an O(files²) directory scan.
                files::write_bytes(
                    &files::path(&area.path, &format!("files/{relative}"))?,
                    &bytes,
                )?;
                entries.insert(
                    relative,
                    SnapshotFile {
                        sha256: digest(&bytes),
                        bytes: length,
                    },
                );
            }
            let snapshot_id = snapshot_identity(identity, &entries)?;
            let snapshot = SourceSnapshot {
                snapshot_id: snapshot_id.clone(),
                identity: identity.into(),
                files: entries,
                total_bytes: total,
                created_at_ms: crate::models::now_ms(),
            };
            self.write_area(
                &area,
                "manifest.json",
                &serde_json::to_vec_pretty(&snapshot).map_err(|e| e.to_string())?,
            )?;
            if self
                .target
                .path(&format!("snapshots/{snapshot_id}"))?
                .exists()
            {
                let existing = self.target.snapshot(&snapshot_id)?;
                self.target.verify_snapshot(&existing)?;
                fs::remove_dir_all(&area.path).map_err(|e| e.to_string())?;
                self.settle_area(&area)?;
            } else {
                self.publish_area(&area, ArtifactKind::Snapshot, &snapshot_id)?;
            }
            let mut state = self.target.run(run_id)?.state;
            if !state.snapshot_ids.contains(&snapshot_id) {
                state.snapshot_ids.push(snapshot_id);
                self.save_state(state, &next_id("snapshot-published"))?;
            }
            Ok(snapshot)
        })();
        // Failed staging remains owned and charged. Recovery does not re-read a target.
        result
    }
    pub fn publish_report(&self, run_id: &str, markdown: &str) -> Result<Report, String> {
        self.check_run(run_id)?;
        if markdown.trim().is_empty() || markdown.len() > 4 * 1024 * 1024 {
            return Err("invalid_report_size".into());
        }
        let mut state = self.target.run(run_id)?.state;
        for snapshot in &state.snapshot_ids {
            self.target.snapshot(snapshot)?;
        }
        for evidence in &state.evidence_ids {
            self.target.evidence(evidence)?;
        }
        let mut report = Report {
            report_id: String::new(),
            run_id: run_id.into(),
            revision: state.report_ids.len() as u32 + 1,
            snapshot_ids: state.snapshot_ids.clone(),
            evidence_ids: state.evidence_ids.clone(),
            default_source_snapshot_id: state.snapshot_ids.last().cloned(),
            markdown_sha256: digest(markdown.as_bytes()),
            created_at_ms: crate::models::now_ms(),
        };
        report.report_id = report_identity(&report)?;
        let manifest = serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?;
        let area = self.report_area(run_id, markdown.len() as u64 + manifest.len() as u64)?;
        self.write_area(&area, "report.md", markdown.as_bytes())?;
        self.write_area(&area, "manifest.json", &manifest)?;
        self.publish_area(&area, ArtifactKind::Report, &report.report_id)?;
        state.report_ids.push(report.report_id.clone());
        self.save_state(state, &next_id("report-published"))?;
        Ok(report)
    }

    /// Publish raw evidence and its independently hashed model view. Only the
    /// host collector can supply these bytes; model material reads never see raw.
    pub fn publish_evidence(
        &self,
        run_id: &str,
        mut record: EvidenceRecord,
        raw: &[u8],
        model_view: &str,
    ) -> Result<EvidenceRecord, String> {
        self.check_run(run_id)?;
        let run = self.target.run(run_id)?;
        if run.spec.kind == RunKind::EvidenceFollowup {
            return Err("followup_cannot_collect".into());
        }
        if raw.len() > 64 * 1024 * 1024 || model_view.len() > 16 * 1024 * 1024 {
            return Err("evidence_size_limit".into());
        }
        record.run_id = run_id.into();
        record.raw_sha256 = digest(raw);
        record.model_sha256 = digest(model_view.as_bytes());
        record.evidence_id = evidence_identity(run_id, &record.raw_sha256);
        record.bytes = raw.len() as u64;
        record.created_at_ms = crate::models::now_ms();
        let published = self
            .target
            .path(&format!("evidence/{}", record.evidence_id))?;
        let mut state = run.state;
        if !published.exists() {
            let manifest = serde_json::to_vec_pretty(&record).map_err(|e| e.to_string())?;
            let area = self.reserve_area(
                run_id,
                "staging",
                raw.len() as u64 + model_view.len() as u64 + manifest.len() as u64,
            )?;
            self.write_area(&area, "source.bundle.json", raw)?;
            self.write_area(&area, "model-view.json", model_view.as_bytes())?;
            self.write_area(&area, "manifest.json", &manifest)?;
            self.publish_area(&area, ArtifactKind::Evidence, &record.evidence_id)?;
        } else {
            let existing = self.target.evidence(&record.evidence_id)?.0;
            if existing.model_sha256 != record.model_sha256 {
                return Err("evidence_view_conflict".into());
            }
            record = existing;
        }
        if !state.evidence_ids.contains(&record.evidence_id) {
            state.evidence_ids.push(record.evidence_id.clone());
            self.save_state(state, &next_id("evidence-published"))?;
        }
        Ok(record)
    }
}

fn evidence_identity(run_id: &str, raw_hash: &str) -> String {
    format!(
        "evidence-{}",
        digest(format!("{run_id}:{raw_hash}").as_bytes())
    )
}

fn report_identity(report: &Report) -> Result<String, String> {
    let mut content = report.clone();
    content.report_id.clear();
    Ok(format!(
        "report-{}",
        digest(&serde_json::to_vec(&content).map_err(|e| e.to_string())?)
    ))
}

fn snapshot_identity(
    identity: &str,
    files: &BTreeMap<String, SnapshotFile>,
) -> Result<String, String> {
    Ok(format!(
        "snapshot-{}",
        digest(&serde_json::to_vec(&(identity, files)).map_err(|e| e.to_string())?)
    ))
}
fn source_window(bytes: Vec<u8>, start: u32, end: u32) -> Result<Material, String> {
    if start == 0 || end < start || end - start >= 200 {
        return Err("invalid_source_window".into());
    }
    let text = String::from_utf8(bytes).map_err(|_| "source_not_text")?;
    let lines: Vec<_> = text.lines().collect();
    if start as usize > lines.len() && !(start == 1 && lines.is_empty()) {
        return Err("source_range_missing".into());
    }
    let end = (end as usize).min(lines.len());
    let text = lines[(start as usize - 1).min(end)..end].join("\n");
    let truncated = text.chars().count() > 48_000;
    Ok(Material {
        text: text.chars().take(48_000).collect(),
        next_cursor: None,
        truncated,
    })
}

#[cfg(test)]
mod tests {
    use super::super::store::tests::{sample_research, temp_root};
    use super::*;
    fn setup() -> (PathBuf, TargetStore, RunSpec) {
        let root = temp_root();
        let store = Store::new(&root);
        let mut research = sample_research("research-target");
        research.schema_version = 2;
        research.workflow = Workflow::Target;
        let target = Target::App {
            path: root.to_string_lossy().into_owned(),
        };
        research.target = Some(TargetConfig {
            target: target.clone(),
            run_ids: vec![],
        });
        store.create(&research).unwrap();
        let target_store = TargetStore::open(store, "research-target").unwrap();
        let budget = Budget::defaults(ResearchMode::Single);
        let spec = RunSpec {
            schema_version: 2,
            research_id: "research-target".into(),
            run_id: "run-a".into(),
            kind: RunKind::Initial,
            input: TargetInput {
                target,
                question: "Explain".into(),
                agent_id: "a".into(),
                model: None,
                language: "en".into(),
                mode: ResearchMode::Single,
                budget: None,
                tools: None,
            },
            budget,
            config_digest: "fixture".into(),
            agent_config_digest: "fixture".into(),
            target_identity: "fixture".into(),
            parent_report_id: None,
            source_snapshots: vec![],
            source_evidence: vec![],
            created_at_ms: 1,
        };
        (root, target_store, spec)
    }
    #[test]
    fn report_keeps_its_snapshot_when_same_path_changes() {
        let (root, target, spec) = setup();
        let writer = target.writer(Some("run-a")).unwrap();
        writer.create_run(&spec).unwrap();
        let source = root.join("input");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("main.js"), "old source\n").unwrap();
        let first = writer
            .snapshot_from(&source, &["main.js".into()], "commit-a", 1024)
            .unwrap();
        let state = target.run("run-a").unwrap().state;
        assert!(state.snapshot_ids.contains(&first.snapshot_id));
        writer.save_state(state, "first-snapshot").unwrap();
        let report = writer
            .publish_report("run-a", "See [main](loom-src:main.js#L1-L1)")
            .unwrap();
        fs::write(source.join("main.js"), "new source\n").unwrap();
        let second = writer
            .snapshot_from(&source, &["main.js".into()], "commit-b", 1024)
            .unwrap();
        assert_ne!(first.snapshot_id, second.snapshot_id);
        assert_eq!(
            target
                .report_source(&report.report_id, "main.js", 1, 1)
                .unwrap()
                .text,
            "old source"
        );
        let state = target.run("run-a").unwrap().state;
        assert!(state.snapshot_ids.contains(&second.snapshot_id));
        writer.save_state(state, "second-snapshot").unwrap();
        let latest = writer.publish_report("run-a", "Updated").unwrap();
        assert_eq!(
            target
                .report_source(&latest.report_id, "main.js", 1, 1)
                .unwrap()
                .text,
            "new source"
        );
        fs::write(
            target
                .path(&format!("snapshots/{}/files/main.js", first.snapshot_id))
                .unwrap(),
            "tampered",
        )
        .unwrap();
        assert!(target
            .report_source(&report.report_id, "main.js", 1, 1)
            .unwrap_err()
            .contains("integrity"));
        fs::remove_dir_all(
            target
                .path(&format!("snapshots/{}", first.snapshot_id))
                .unwrap(),
        )
        .unwrap();
        assert!(target
            .report_source(&report.report_id, "main.js", 1, 1)
            .unwrap_err()
            .contains("unavailable"));
        assert_eq!(
            target
                .report_source(&latest.report_id, "main.js", 1, 1)
                .unwrap()
                .text,
            "new source"
        );
        drop(writer);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn run_journal_recovers_tail_and_conservatively_charges_reservation() {
        let (root, target, spec) = setup();
        let writer = target.writer(Some("run-a")).unwrap();
        writer.create_run(&spec).unwrap();
        let mut state = target.run("run-a").unwrap().state;
        state.usage.reserve(&spec.budget, "agent", 1000).unwrap();
        state.status = RunStatus::Analyzing;
        writer.save_state(state, "reserved").unwrap();
        drop(writer);
        let path = target.path("runs/run-a/events.jsonl").unwrap();
        fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"{partial")
            .unwrap();
        let before = fs::read(&path).unwrap();
        let run = target.run("run-a").unwrap();
        assert_eq!(run.state.status, RunStatus::Interrupted);
        assert_eq!(fs::read(&path).unwrap(), before);
        let writer = target.writer(Some("run-a")).unwrap();
        let recovered = writer.recover("run-a").unwrap();
        assert_eq!(recovered.usage.active_ms, 1000);
        assert_eq!(recovered.usage.calls, 1);
        assert_eq!(recovered.usage.reservation_ms, 0);
        let again = writer.recover("run-a").unwrap();
        assert_eq!(again.usage.active_ms, 1000);
        assert!(!String::from_utf8(fs::read(path).unwrap())
            .unwrap()
            .contains("partial"));
        drop(writer);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn evidence_materials_are_paged_scoped_and_immutable() {
        let (root, target, spec) = setup();
        let writer = target.writer(Some("run-a")).unwrap();
        writer.create_run(&spec).unwrap();
        let record = EvidenceRecord {
            evidence_id: String::new(),
            run_id: String::new(),
            source_id: Some("provider-original".into()),
            tool: "fixture".into(),
            tool_version: "1".into(),
            raw_sha256: String::new(),
            model_sha256: String::new(),
            bytes: 0,
            coverage: "partial".into(),
            limitations: vec!["fixture".into()],
            created_at_ms: 0,
        };
        let text = format!("{}KEY_AT_END", "文".repeat(50_000));
        let published = writer
            .publish_evidence("run-a", record.clone(), b"raw-secret", &text)
            .unwrap();
        assert_ne!(published.raw_sha256, published.model_sha256);
        let request = MaterialRequest::Evidence {
            evidence_id: published.evidence_id.clone(),
            offset: 50_000,
            limit: 100,
        };
        let batch = super::super::materials::read_batch(&target, "run-a", &[request]).unwrap();
        assert!(batch.contains("KEY_AT_END"));
        assert!(!batch.contains("raw-secret"));
        let oversized = MaterialRequest::Evidence {
            evidence_id: published.evidence_id.clone(),
            offset: 0,
            limit: 48_000,
        };
        assert!(super::super::materials::read_batch(&target, "run-a", &[oversized]).is_err());
        assert!(writer
            .publish_evidence("run-a", record, b"raw-secret", "changed")
            .unwrap_err()
            .contains("conflict"));
        let report = writer.publish_report("run-a", "Evidence report").unwrap();
        let export = writer
            .export_report(&ExportInput {
                report_id: report.report_id.clone(),
                evidence_ids: vec![],
                include_raw: false,
            })
            .unwrap();
        assert!(!export.includes_raw);
        assert!(export
            .files
            .iter()
            .all(|p| !p.ends_with("source.bundle.json")));
        assert!(
            !fs::read_to_string(Path::new(&export.directory).join("evidence.json"))
                .unwrap()
                .contains("raw-secret")
        );
        let full = writer
            .export_report(&ExportInput {
                report_id: report.report_id.clone(),
                evidence_ids: vec![],
                include_raw: true,
            })
            .unwrap();
        assert!(full.includes_raw && full.warning.is_some());
        assert!(full.files.iter().any(|p| p.ends_with("source.bundle.json")));
        assert!(writer
            .export_report(&ExportInput {
                report_id: report.report_id.clone(),
                evidence_ids: vec!["evidence-foreign".into()],
                include_raw: true
            })
            .is_err());
        fs::write(
            target
                .path(&format!(
                    "evidence/{}/source.bundle.json",
                    published.evidence_id
                ))
                .unwrap(),
            "tampered",
        )
        .unwrap();
        assert!(writer
            .export_report(&ExportInput {
                report_id: report.report_id.clone(),
                evidence_ids: vec![],
                include_raw: true
            })
            .unwrap_err()
            .contains("integrity"));

        let mut narrowed = target.run("run-a").unwrap().state;
        narrowed.evidence_ids.clear();
        assert!(writer
            .save_state(narrowed, "erase")
            .unwrap_err()
            .contains("immutable"));
        drop(writer);
        let mut followup = spec.clone();
        followup.run_id = "run-followup".into();
        followup.kind = RunKind::EvidenceFollowup;
        followup.parent_report_id = Some(report.report_id.clone());
        followup.source_evidence = report.evidence_ids;
        followup.budget.max_collections = 0;
        let writer = target.writer(Some("run-followup")).unwrap();
        writer.create_run(&followup).unwrap();
        let mut expanded = target.run("run-followup").unwrap().state;
        expanded.evidence_ids.push("evidence-foreign".into());
        assert!(writer
            .save_state(expanded, "expand")
            .unwrap_err()
            .contains("scope_changed"));
        let invalid = MaterialRequest::Evidence {
            evidence_id: "evidence-foreign".into(),
            offset: 0,
            limit: 10,
        };
        assert!(target.material("run-followup", &invalid).is_err());
        drop(writer);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn data_quota_preserves_headroom_and_unknown_writes_stay_charged() {
        let (root, target, mut spec) = setup();
        spec.budget.max_disk_bytes = 64 * 1024;
        let writer = target.writer(Some("run-a")).unwrap();
        writer.create_run(&spec).unwrap();
        let area = writer
            .reserve_area("run-a", "steps", writer.available_disk("run-a").unwrap())
            .unwrap();
        assert!(writer.reserve_area("run-a", "steps", 1).is_err());
        assert!(writer
            .write_area(&area, "oversized", &vec![0; area.capacity as usize + 1])
            .is_err());
        assert!(!area.path.join("oversized").exists());
        writer
            .write_area(&area, "retained", b"partial output")
            .unwrap();
        drop(writer);
        let writer = target.writer(Some("run-a")).unwrap();
        let recovered = writer.recover("run-a").unwrap();
        assert!(recovered.usage.disk_reserved_bytes >= area.capacity - 14);
        assert!(writer.reserve_area("run-a", "steps", 1).is_err());
        let report = writer
            .publish_report("run-a", "# Partial report\nDisk budget exhausted.")
            .unwrap();
        assert!(target
            .report(&report.report_id)
            .unwrap()
            .markdown
            .contains("Partial"));
        let state = target.run("run-a").unwrap().state;
        assert!(
            state.usage.disk_bytes + state.usage.disk_reserved_bytes <= spec.budget.max_disk_bytes
        );
        drop(writer);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn recovery_links_valid_published_artifacts_without_reopening_target() {
        let (root, target, spec) = setup();
        let writer = target.writer(Some("run-a")).unwrap();
        writer.create_run(&spec).unwrap();
        writer.save_state(RunState::new(&spec), "initial").unwrap();
        let journal = target.path("runs/run-a/events.jsonl").unwrap();
        let before_snapshot = fs::read(&journal).unwrap();
        let source = root.join("source");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("index.js"), "export const fixed = 42;").unwrap();
        let snapshot = writer
            .snapshot_from(&source, &["index.js".into()], "capture", 4096)
            .unwrap();
        // Simulate loss of the completion event after durable artifact rename.
        fs::write(&journal, &before_snapshot).unwrap();
        fs::remove_dir_all(&source).unwrap();
        assert!(target.run("run-a").unwrap().state.snapshot_ids.is_empty());
        let recovered = writer.recover("run-a").unwrap();
        assert_eq!(recovered.snapshot_ids, vec![snapshot.snapshot_id]);
        let before_report = fs::read(&journal).unwrap();
        let report = writer
            .publish_report("run-a", "Recovered evidence")
            .unwrap();
        fs::write(&journal, &before_report).unwrap();
        assert!(target
            .report(&report.report_id)
            .unwrap_err()
            .contains("unpublished"));
        writer.recover("run-a").unwrap();
        assert_eq!(
            target.report(&report.report_id).unwrap().markdown,
            "Recovered evidence"
        );
        let once = target.run("run-a").unwrap().state;
        writer.recover("run-a").unwrap();
        let twice = target.run("run-a").unwrap().state;
        assert_eq!(once.report_ids, twice.report_ids);
        assert_eq!(once.usage.calls, 0);
        assert_eq!(twice.usage.collections, 0);
        drop(writer);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn recovery_rejects_corrupt_published_snapshot_before_linking_it() {
        let (root, target, spec) = setup();
        let writer = target.writer(Some("run-a")).unwrap();
        writer.create_run(&spec).unwrap();
        writer.save_state(RunState::new(&spec), "initial").unwrap();
        let journal = target.path("runs/run-a/events.jsonl").unwrap();
        let before = fs::read(&journal).unwrap();
        let source = root.join("source");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("index.js"), "observed").unwrap();
        let snapshot = writer
            .snapshot_from(&source, &["index.js".into()], "capture", 4096)
            .unwrap();
        fs::write(&journal, before).unwrap();
        fs::write(
            target
                .path(&format!(
                    "snapshots/{}/files/index.js",
                    snapshot.snapshot_id
                ))
                .unwrap(),
            "tampered",
        )
        .unwrap();
        assert!(writer.recover("run-a").unwrap_err().contains("integrity"));
        assert!(target.run("run-a").unwrap().state.snapshot_ids.is_empty());
        drop(writer);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn disk_full_during_evidence_write_retains_reservation_not_a_fake_result() {
        let (root, target, spec) = setup();
        let writer = target.writer(Some("run-a")).unwrap();
        writer.create_run(&spec).unwrap();
        let record = EvidenceRecord {
            evidence_id: String::new(),
            run_id: String::new(),
            source_id: None,
            tool: "fixture".into(),
            tool_version: "1".into(),
            raw_sha256: String::new(),
            model_sha256: String::new(),
            bytes: 0,
            coverage: "partial".into(),
            limitations: vec![],
            created_at_ms: 0,
        };
        files::fail_next_write_to("model-view.json");
        assert!(writer
            .publish_evidence("run-a", record, b"raw-private", "safe view")
            .unwrap_err()
            .contains("28"));
        assert!(target.run("run-a").unwrap().state.evidence_ids.is_empty());
        let charged = target.run("run-a").unwrap().state.usage.disk_reserved_bytes;
        drop(writer);
        let writer = target.writer(Some("run-a")).unwrap();
        let recovered = writer.recover("run-a").unwrap();
        assert!(recovered.evidence_ids.is_empty());
        assert!(recovered.usage.disk_bytes + recovered.usage.disk_reserved_bytes >= charged);
        let partial = writer
            .publish_report("run-a", "Partial: evidence publication failed.")
            .unwrap();
        assert!(target
            .report(&partial.report_id)
            .unwrap()
            .report
            .evidence_ids
            .is_empty());
        drop(writer);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn persisted_context_is_scope_bound_and_detects_corruption() {
        let (root, target, spec) = setup();
        let writer = target.writer(Some("run-a")).unwrap();
        writer.create_run(&spec).unwrap();
        let scope = super::super::materials::scope_digest(&target.run("run-a").unwrap()).unwrap();
        let mut context =
            super::super::materials::Context::new("run-a", scope, "manifest".into()).unwrap();
        context.push("first page".into()).unwrap();
        context.push("second page".into()).unwrap();
        writer.save_context("run-a", &context).unwrap();
        drop(writer);
        let restored = super::super::materials::load_context(&target, "run-a")
            .unwrap()
            .unwrap()
            .render()
            .unwrap();
        assert!(restored.contains("first page") && restored.contains("second page"));
        let reference = target.run("run-a").unwrap().state.material_context.unwrap();
        fs::write(
            target
                .path(&format!(
                    "runs/run-a/steps/{}/context.json",
                    reference.allocation_key
                ))
                .unwrap(),
            "tampered",
        )
        .unwrap();
        assert!(super::super::materials::load_context(&target, "run-a")
            .unwrap_err()
            .contains("integrity"));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn export_lease_prevents_deletion_until_complete() {
        let (root, target, spec) = setup();
        let writer = target.writer(Some("run-a")).unwrap();
        writer.create_run(&spec).unwrap();
        let report = writer.publish_report("run-a", "Consistent export").unwrap();
        drop(writer);
        let acquired = std::sync::Arc::new(std::sync::Barrier::new(2));
        let release = std::sync::Arc::new(std::sync::Barrier::new(2));
        let (other, a, r) = (target.clone(), acquired.clone(), release.clone());
        let child = std::thread::spawn(move || {
            let writer = other.writer(None).unwrap();
            a.wait();
            r.wait();
            writer
                .export_report(&ExportInput {
                    report_id: report.report_id,
                    evidence_ids: vec![],
                    include_raw: false,
                })
                .unwrap()
        });
        acquired.wait();
        assert!(target.store.delete("research-target").is_err());
        assert!(target.writer(Some("run-other")).is_err());
        release.wait();
        let exported = child.join().unwrap();
        assert_eq!(
            fs::read_to_string(Path::new(&exported.directory).join("report.md")).unwrap(),
            "Consistent export"
        );
        target.store.delete("research-target").unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn concurrent_readers_ignore_disappearing_atomic_metadata_temporaries() {
        let (root, target, spec) = setup();
        let writer = target.writer(Some("run-a")).unwrap();
        writer.create_run(&spec).unwrap();
        let reader = target.clone();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let done = stop.clone();
        let child = std::thread::spawn(move || {
            let mut reads = 0;
            while !done.load(std::sync::atomic::Ordering::Relaxed) {
                reader.run("run-a").unwrap();
                reads += 1;
            }
            reads
        });
        for _ in 0..40 {
            let state = target.run("run-a").unwrap().state;
            writer.save_state(state, &next_id("progress")).unwrap();
        }
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(child.join().unwrap() > 0);
        drop(writer);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn legacy_commands_and_material_escape_cannot_cross_target_boundary() {
        let (root, target, spec) = setup();
        assert!(target.store.require_legacy("research-target").is_err());
        assert!(target.store.cancel_legacy("research-target").is_err());
        assert!(target.store.read_turn("research-target", 1).is_err());
        assert!(target
            .store
            .read_log("research-target", "job-a", "stdout", 0, None)
            .is_err());
        let writer = target.writer(Some("run-a")).unwrap();
        writer.create_run(&spec).unwrap();
        assert!(writer.create_run(&spec).is_err());
        assert!(target
            .material(
                "run-a",
                &MaterialRequest::Source {
                    snapshot_id: "snapshot-missing".into(),
                    path: "/etc/passwd".into(),
                    start_line: 1,
                    end_line: 1
                }
            )
            .is_err());
        let source = root.join("input");
        fs::create_dir(&source).unwrap();
        fs::write(root.join("private"), "outside").unwrap();
        assert!(writer
            .snapshot_from(&source, &["../private".into()], "x", 1024)
            .is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(root.join("private"), source.join("link")).unwrap();
            assert!(writer
                .snapshot_from(&source, &["link".into()], "x", 1024)
                .is_err());
        }
        drop(writer);
        fs::remove_dir_all(root).unwrap();
    }
}
