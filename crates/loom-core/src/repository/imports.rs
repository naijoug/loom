use super::*;
use crate::legacy::{capture_project, CapturedProject};
use sha2::{Digest, Sha256};
use std::io::Write;

const PLAN_LIMIT: u64 = 8 * 1024 * 1024;
const HISTORY_LIMIT: u64 = 64 * 1024 * 1024;
const RESOURCE_LIMIT: u64 = 32 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PlanRecord {
    schema_version: u32,
    plan: LegacyImportPlan,
    checksum: String,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReportRecord {
    schema_version: u32,
    report: LegacyImportReport,
    checksum: String,
}
fn save_report(path: &Path, report: &LegacyImportReport) -> Result<()> {
    let record = ReportRecord {
        schema_version: 1,
        report: report.clone(),
        checksum: projection::checksum(report)?,
    };
    if serde_json::to_vec(&record)
        .map_err(|_| CoreError::invalid("cannot encode import report"))?
        .len() as u64
        + 1
        > PLAN_LIMIT
    {
        return Err(CoreError::invalid("import report exceeds 8 MiB"));
    }
    journal::atomic_write(path, &record)
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Manifest {
    schema_version: u32,
    origin: ImportOrigin,
    history_digest: String,
    resources: Vec<ImportedResource>,
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn decode<T: serde::de::DeserializeOwned>(path: &Path, limit: u64) -> Result<T> {
    serde_json::from_slice(&journal::read_bounded(path, limit)?)
        .map_err(|_| CoreError::corrupt("invalid import record JSON"))
}
fn write_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = journal::options().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    journal::sync_parent(path)
}
fn item_key(item: &LegacyItem) -> String {
    format!("{}/{}", item.kind, item.source_id)
}

impl Core {
    fn imports_directory(&self, workspace: &Id) -> Result<PathBuf> {
        let state = self.lock()?;
        Self::require_workspace(&state, workspace)?;
        let directory = self.workspace_directory(workspace)?.join("imports");
        journal::directory(&directory)?;
        Ok(directory)
    }
    fn import_project(&self, workspace: &Id, project: &Id) -> Result<Project> {
        let project = self
            .list_projects(workspace)?
            .into_iter()
            .find(|p| &p.id == project)
            .ok_or_else(|| CoreError::missing("import project not registered in workspace"))?;
        if self
            .0
            .root
            .starts_with(Path::new(&project.directory).join(".loom"))
        {
            return Err(CoreError::invalid(
                "new workbench data must not be stored inside legacy .loom",
            ));
        }
        Ok(project)
    }
    fn read_import_plan(&self, workspace: &Id, plan_id: &Id) -> Result<LegacyImportPlan> {
        let base = self.imports_directory(workspace)?;
        let plans = base.join("plans");
        journal::directory(&plans)?;
        let stored: PlanRecord = decode(&plans.join(format!("{plan_id}.json")), PLAN_LIMIT)?;
        if stored.schema_version != 1
            || &stored.plan.id != plan_id
            || &stored.plan.workspace_id != workspace
            || projection::checksum(&stored.plan)? != stored.checksum
        {
            return Err(CoreError::corrupt(
                "import plan checksum, schema or ownership mismatch",
            ));
        }
        Ok(stored.plan)
    }
    pub fn prepare_legacy_import(
        &self,
        workspace_id: &Id,
        project_id: &Id,
        expected_fingerprint: &str,
    ) -> Result<LegacyImportPlan> {
        let _import = self.0.importing.try_lock().map_err(|_| {
            CoreError::new(ErrorCode::Busy, "another import operation is in progress")
        })?;
        let project = self.import_project(workspace_id, project_id)?;
        let preview = inspect_legacy_project(Path::new(&project.directory))?;
        if preview.fingerprint != expected_fingerprint {
            return Err(CoreError::conflict(
                "legacy source changed; run preview again",
            ));
        }
        if !preview.items.iter().any(|i| i.content_hash.is_some()) {
            return Err(CoreError::invalid("no readable legacy items to import"));
        }
        let id = Id::new(format!(
            "import-{}",
            projection::checksum(&(workspace_id, project_id, &preview.fingerprint))?
        ))?;
        let base = self.imports_directory(workspace_id)?.join("plans");
        journal::directory(&base)?;
        let path = base.join(format!("{id}.json"));
        journal::regular(&path)?;
        if path.exists() {
            return self.read_import_plan(workspace_id, &id);
        }
        let plan = LegacyImportPlan {
            id,
            workspace_id: workspace_id.clone(),
            project_id: project_id.clone(),
            created_at_ms: journal::now_ms(),
            preview,
        };
        let record = PlanRecord {
            schema_version: 1,
            checksum: projection::checksum(&plan)?,
            plan: plan.clone(),
        };
        if serde_json::to_vec(&record)
            .map_err(|_| CoreError::invalid("cannot encode import plan"))?
            .len() as u64
            + 1
            > PLAN_LIMIT
        {
            return Err(CoreError::invalid("import plan exceeds 8 MiB"));
        }
        journal::atomic_write(&path, &record)?;
        Ok(plan)
    }
    pub fn execute_legacy_import(
        &self,
        workspace_id: &Id,
        plan_id: &Id,
    ) -> Result<LegacyImportReport> {
        let _import = self.0.importing.try_lock().map_err(|_| {
            CoreError::new(ErrorCode::Busy, "another import operation is in progress")
        })?;
        let plan = self.read_import_plan(workspace_id, plan_id)?;
        let project = self.import_project(workspace_id, &plan.project_id)?;
        if project.directory != plan.preview.project_directory {
            return Err(CoreError::conflict("import project directory changed"));
        }
        let captured = capture_project(Path::new(&project.directory), true)?;
        if captured.preview != plan.preview {
            return Err(CoreError::conflict(
                "legacy source changed after plan preparation; preview again",
            ));
        }
        let imports = self.imports_directory(workspace_id)?;
        let reports = imports.join("reports");
        journal::directory(&reports)?;
        let staging = imports.join("staging");
        journal::directory(&staging)?;
        let stage = staging.join(plan_id.as_str());
        if fs::symlink_metadata(&stage).is_ok() {
            // This is private disposable staging, never a source or published session.
            journal::directory(&stage)?;
            fs::remove_dir_all(&stage)?;
            journal::sync_parent(&stage)?;
        }
        journal::directory(&stage)?;
        let mut report = LegacyImportReport {
            plan_id: plan.id.clone(),
            workspace_id: workspace_id.clone(),
            completed_at_ms: 0,
            results: Vec::new(),
        };
        let report_path = reports.join(format!("{plan_id}.json"));
        for item in &plan.preview.items {
            let mut result = LegacyImportResult {
                kind: item.kind.clone(),
                source_id: item.source_id.clone(),
                session_key: None,
                status: "skipped".into(),
                error: None,
                issues: item.issues.clone(),
            };
            if item.content_hash.is_some() {
                match self.stage_import_item(&plan, &captured, item, &stage) {
                    Ok((key, _)) => {
                        result.session_key = Some(key);
                        result.status = "prepared".into();
                    }
                    Err(error) => {
                        result.status = "failed".into();
                        result.error = Some(error.to_string());
                    }
                }
            }
            report.results.push(result);
        }
        // The captured bytes are immutable; recheck before publishing any item.
        if inspect_legacy_project(Path::new(&project.directory))? != plan.preview {
            return Err(CoreError::conflict(
                "legacy source changed while staging; preview again",
            ));
        }
        save_report(&report_path, &report)?;
        for index in 0..report.results.len() {
            let result = &mut report.results[index];
            if result.status != "prepared" {
                continue;
            }
            let key = result.session_key.clone().expect("prepared item key");
            let staged = stage.join(key.session_id.as_str());
            match self.publish_import(&key, &staged) {
                Ok(already) => {
                    result.status = if already {
                        "already_imported"
                    } else {
                        "imported"
                    }
                    .into()
                }
                Err(error) => {
                    result.status = "failed".into();
                    result.error = Some(error.to_string());
                }
            }
            save_report(&report_path, &report)?;
        }
        report.completed_at_ms = journal::now_ms();
        save_report(&report_path, &report)?;
        // Published directories have moved. Failed/duplicate copies are bounded
        // disposable data; a crash before cleanup is also handled on the next try.
        fs::remove_dir_all(&stage)?;
        journal::sync_parent(&stage)?;
        Ok(report)
    }
    fn stage_import_item(
        &self,
        plan: &LegacyImportPlan,
        captured: &CapturedProject,
        item: &LegacyItem,
        stage: &Path,
    ) -> Result<(SessionKey, ImportedHistoryRef)> {
        let source_key = item_key(item);
        let document = captured
            .documents
            .get(&source_key)
            .ok_or_else(|| CoreError::corrupt("captured document missing"))?;
        let file_paths = captured
            .item_files
            .get(&source_key)
            .ok_or_else(|| CoreError::corrupt("captured resource map missing"))?;
        let records: Vec<_> = captured
            .preview
            .files
            .iter()
            .filter(|f| file_paths.contains(&f.path))
            .collect();
        let content_digest = projection::checksum(&(item, &records))?;
        let key = SessionKey {
            workspace_id: plan.workspace_id.clone(),
            session_id: Id::new(format!(
                "legacy-{}",
                projection::checksum(&(
                    &plan.preview.project_directory,
                    &item.kind,
                    &item.source_id
                ))?
            ))?,
        };
        let origin = ImportOrigin {
            project_directory: plan.preview.project_directory.clone(),
            kind: item.kind.clone(),
            source_id: Id::new(&item.source_id)?,
            source_version: item.source_version,
            content_digest,
            imported_at_ms: plan.created_at_ms,
        };
        let directory = stage.join(key.session_id.as_str());
        journal::directory(&directory)?;
        let bundle = directory.join("import");
        journal::directory(&bundle)?;
        let resources_dir = bundle.join("resources");
        journal::directory(&resources_dir)?;
        let mut resources = Vec::new();
        for record in records {
            let Some(expected) = &record.sha256 else {
                continue;
            };
            let bytes = captured
                .files
                .get(&record.path)
                .ok_or_else(|| CoreError::corrupt("captured file missing"))?;
            if digest(bytes) != *expected || bytes.len() as u64 != record.bytes {
                return Err(CoreError::corrupt("captured file checksum mismatch"));
            }
            let id = Id::new(format!("file-{}", digest(record.path.as_bytes())))?;
            write_bytes(&resources_dir.join(id.as_str()), bytes)?;
            resources.push(ImportedResource {
                id,
                source_path: record.path.clone(),
                bytes: record.bytes,
                sha256: expected.clone(),
            });
        }
        let mut messages: Vec<HistoricalMessage> = if item.kind == "chat" {
            serde_json::from_value(document["messages"].clone())
                .map_err(|_| CoreError::corrupt("invalid historical message schema"))?
        } else {
            Vec::new()
        };
        for message in &mut messages {
            if matches!(
                message.status.as_str(),
                "starting" | "running" | "streaming" | "pending" | "cancelling"
            ) {
                message.status = "interrupted".into();
            }
            for part in &mut message.parts {
                if let HistoricalPart::Tool { status, .. } = part {
                    if status
                        .as_deref()
                        .is_some_and(|s| matches!(s, "running" | "pending" | "streaming"))
                    {
                        *status = Some("interrupted".into());
                    }
                }
            }
        }
        let history_bytes = serde_json::to_vec(&messages)
            .map_err(|_| CoreError::invalid("cannot encode historical messages"))?;
        if history_bytes.len() as u64 > HISTORY_LIMIT {
            return Err(CoreError::invalid("historical transcript exceeds 64 MiB"));
        }
        write_bytes(&bundle.join("history.json"), &history_bytes)?;
        // Save the full reconstructed record too: Task plans/reviews/events and
        // Chat turn audit stay historical data, never runtime configuration.
        let document_bytes = serde_json::to_vec(document)
            .map_err(|_| CoreError::invalid("cannot encode historical record"))?;
        if document_bytes.len() as u64 > RESOURCE_LIMIT {
            return Err(CoreError::invalid("historical record exceeds 32 MiB"));
        }
        let document_id = Id::new("original-record")?;
        write_bytes(&resources_dir.join(document_id.as_str()), &document_bytes)?;
        resources.push(ImportedResource {
            id: document_id,
            source_path: "reconstructed-record.json".into(),
            bytes: document_bytes.len() as u64,
            sha256: digest(&document_bytes),
        });
        let manifest = Manifest {
            schema_version: 1,
            origin: origin.clone(),
            history_digest: digest(&history_bytes),
            resources,
        };
        let history = ImportedHistoryRef {
            origin,
            manifest_digest: projection::checksum(&manifest)?,
            message_count: messages.len(),
        };
        journal::atomic_write(&bundle.join("manifest.json"), &manifest)?;
        let mut title = item
            .title
            .clone()
            .unwrap_or_else(|| item.source_id.clone())
            .chars()
            .filter(|c| !c.is_control())
            .collect::<String>();
        while title.len() > 1024 {
            title.pop();
        }
        if title.trim().is_empty() {
            title = format!("历史记录 {}", item.source_id);
        }
        let now = journal::now_ms().max(plan.created_at_ms);
        let metadata = SessionMetadata {
            key: key.clone(),
            title: Some(title),
            project_id: Some(plan.project_id.clone()),
            status_id: Id::new(if item.kind == "task" { "done" } else { "todo" })?,
            archived: item.kind == "task"
                || document.get("status").and_then(serde_json::Value::as_str) == Some("archived"),
            flagged: document
                .get("flagged")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            labels: Vec::new(),
            permission_mode: PermissionMode::Explore,
            connection: Default::default(),
            enabled_sources: Vec::new(),
            created_at_ms: now,
            updated_at_ms: now,
            last_read_message_id: None,
        };
        let mut view = None;
        let mut log = Journal::open(&directory, projection::stream_name(&key), true, |_| Ok(()))?;
        for payload in [
            SessionEvent::Created { metadata },
            SessionEvent::HistoryImported {
                history: history.clone(),
            },
        ] {
            let event = log.prepare(payload, now)?;
            projection::validate(view.as_ref(), &event)?;
            log.append(&event)?;
            projection::apply(&mut view, &event)?;
        }
        drop(log);
        verify_bundle(&directory, &history, true)?;
        Ok((key, history))
    }
    fn publish_import(&self, key: &SessionKey, staged: &Path) -> Result<bool> {
        let mut state = self.lock()?;
        Self::require_workspace(&state, &key.workspace_id)?;
        let mut staged_view = None;
        let staged_log = Journal::open(staged, projection::stream_name(key), false, |event| {
            projection::apply(&mut staged_view, event)
        })?;
        drop(staged_log);
        let expected = staged_view
            .and_then(|v| v.imported_history)
            .ok_or_else(|| CoreError::corrupt("staged import has no historical reference"))?;
        verify_bundle(staged, &expected, true)?;
        let destination = self.session_directory(key)?;
        if destination.exists() {
            let current = self
                .session(&mut state, key)?
                .view()
                .imported_history
                .clone()
                .ok_or_else(|| CoreError::conflict("target session is not an import"))?;
            if current.origin.project_directory != expected.origin.project_directory
                || current.origin.kind != expected.origin.kind
                || current.origin.source_id != expected.origin.source_id
                || current.origin.content_digest != expected.origin.content_digest
            {
                return Err(CoreError::conflict("legacy record was already imported with different content; existing session preserved"));
            }
            verify_bundle(&destination, &current, true)?;
            journal::sync_parent(&destination)?;
            return Ok(true);
        }
        fs::rename(staged, &destination)?;
        journal::sync_parent(&destination)?;
        Self::evict_for_insert(&mut state);
        self.session(&mut state, key)?;
        self.save_caches(&mut state, key, true);
        Self::notify_workspace(
            &mut state,
            &key.workspace_id,
            Some(key.session_id.clone()),
            2,
        );
        Ok(false)
    }
    pub fn legacy_import_report(&self, workspace: &Id, plan_id: &Id) -> Result<LegacyImportReport> {
        self.read_import_plan(workspace, plan_id)?;
        let directory = self.imports_directory(workspace)?.join("reports");
        journal::directory(&directory)?;
        let stored: ReportRecord = decode(&directory.join(format!("{plan_id}.json")), PLAN_LIMIT)?;
        if stored.schema_version != 1 || stored.checksum != projection::checksum(&stored.report)? {
            return Err(CoreError::corrupt("import report checksum/schema mismatch"));
        }
        let report = stored.report;
        if &report.plan_id != plan_id || &report.workspace_id != workspace {
            return Err(CoreError::corrupt("import report ownership mismatch"));
        }
        Ok(report)
    }
    pub fn read_import_history(
        &self,
        key: &SessionKey,
        offset: usize,
        limit: usize,
    ) -> Result<ImportHistoryPage> {
        let history = self
            .get_session(key)?
            .imported_history
            .ok_or_else(|| CoreError::missing("session has no imported history"))?;
        let directory = self.session_directory(key)?;
        let manifest = verify_bundle(&directory, &history, false)?;
        let bytes = journal::read_bounded(&directory.join("import/history.json"), HISTORY_LIMIT)?;
        if digest(&bytes) != manifest.history_digest {
            return Err(CoreError::corrupt(
                "historical transcript checksum mismatch",
            ));
        }
        let messages: Vec<HistoricalMessage> = serde_json::from_slice(&bytes)
            .map_err(|_| CoreError::corrupt("invalid historical transcript"))?;
        if messages.len() != history.message_count || offset > messages.len() {
            return Err(CoreError::invalid(
                "historical message count/cursor mismatch",
            ));
        }
        let count = messages.len();
        let messages = messages
            .into_iter()
            .skip(offset)
            .take(limit.clamp(1, 100))
            .collect::<Vec<_>>();
        Ok(ImportHistoryPage {
            history,
            has_more: offset + messages.len() < count,
            messages,
            offset,
            resources: manifest.resources,
        })
    }
    pub fn read_import_resource(
        &self,
        key: &SessionKey,
        resource_id: &Id,
        offset: u64,
    ) -> Result<ImportResourceChunk> {
        let history = self
            .get_session(key)?
            .imported_history
            .ok_or_else(|| CoreError::missing("session has no imported history"))?;
        let directory = self.session_directory(key)?;
        let manifest = verify_bundle(&directory, &history, false)?;
        let resource = manifest
            .resources
            .into_iter()
            .find(|r| &r.id == resource_id)
            .ok_or_else(|| CoreError::missing("resource does not belong to imported session"))?;
        let bytes = read_resource(&directory, &resource)?;
        if offset > bytes.len() as u64 {
            return Err(CoreError::invalid("resource cursor exceeds file size"));
        }
        let end = bytes.len().min(offset as usize + 64 * 1024);
        Ok(ImportResourceChunk {
            resource,
            offset,
            has_more: end < bytes.len(),
            bytes: bytes[offset as usize..end].to_vec(),
        })
    }
}
fn verify_bundle(directory: &Path, history: &ImportedHistoryRef, all: bool) -> Result<Manifest> {
    let bundle = directory.join("import");
    journal::directory(&bundle)?;
    let resources = bundle.join("resources");
    journal::directory(&resources)?;
    let manifest: Manifest = decode(&bundle.join("manifest.json"), PLAN_LIMIT)?;
    if manifest.schema_version != 1
        || manifest.origin != history.origin
        || projection::checksum(&manifest)? != history.manifest_digest
    {
        return Err(CoreError::corrupt(
            "import manifest checksum or identity mismatch",
        ));
    }
    if all {
        let bytes = journal::read_bounded(&bundle.join("history.json"), HISTORY_LIMIT)?;
        if digest(&bytes) != manifest.history_digest {
            return Err(CoreError::corrupt("imported transcript checksum mismatch"));
        }
        for resource in &manifest.resources {
            read_resource(directory, resource)?;
        }
    }
    Ok(manifest)
}
fn read_resource(directory: &Path, resource: &ImportedResource) -> Result<Vec<u8>> {
    let bytes = journal::read_bounded(
        &directory
            .join("import/resources")
            .join(resource.id.as_str()),
        RESOURCE_LIMIT,
    )?;
    if bytes.len() as u64 != resource.bytes || digest(&bytes) != resource.sha256 {
        return Err(CoreError::corrupt("import resource checksum mismatch"));
    }
    Ok(bytes)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn import_crash_writer() {
        let Some(path) = std::env::var_os("LOOM_IMPORT_CRASH_FIXTURE") else {
            return;
        };
        let root = PathBuf::from(path);
        let core = Core::open(root.join("data")).unwrap();
        let w = Id::new("w").unwrap();
        let p = Id::new("p").unwrap();
        core.create_workspace(w.clone(), "test".into()).unwrap();
        core.register_project(w.clone(), p.clone(), "source".into(), &root.join("source"))
            .unwrap();
        let preview = inspect_legacy_project(&root.join("source")).unwrap();
        let plan = core
            .prepare_legacy_import(&w, &p, &preview.fingerprint)
            .unwrap();
        fs::write(root.join("plan-id"), plan.id.as_str()).unwrap();
        let captured = capture_project(&root.join("source"), true).unwrap();
        let base = core.imports_directory(&w).unwrap().join("staging");
        journal::directory(&base).unwrap();
        let stage = base.join(plan.id.as_str());
        journal::directory(&stage).unwrap();
        let mut keys = Vec::new();
        for item in &plan.preview.items {
            keys.push(
                core.stage_import_item(&plan, &captured, item, &stage)
                    .unwrap()
                    .0,
            );
        }
        core.publish_import(&keys[0], &stage.join(keys[0].session_id.as_str()))
            .unwrap();
        // Exit without destructors or an import result. One atomic directory is
        // published, another remains staged, exactly the uncertain-reply boundary.
        std::process::exit(23);
    }
    #[test]
    fn process_exit_after_first_publication_retries_without_duplicates() {
        let root = std::env::temp_dir().join(format!(
            "loom-import-crash-{}-{}",
            std::process::id(),
            journal::now_ms()
        ));
        fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(root.clone());
        let source = root.join("source");
        let records = source.join(".loom/tasks");
        fs::create_dir_all(&records).unwrap();
        for id in ["a", "b"] {
            fs::write(records.join(format!("{id}.json")),serde_json::to_vec(&json!({"id":id,"projectPath":source,"title":id,"rawRequirement":"old task","status":"done","events":[],"commandRuns":[],"feedback":[],"createdAtMs":1,"updatedAtMs":2})).unwrap()).unwrap();
        }
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "repository::imports::tests::import_crash_writer",
                "--nocapture",
            ])
            .env("LOOM_IMPORT_CRASH_FIXTURE", &root)
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(23));
        let core = Core::open(root.join("data")).unwrap();
        let w = Id::new("w").unwrap();
        let plan_id = Id::new(fs::read_to_string(root.join("plan-id")).unwrap()).unwrap();
        assert_eq!(core.list_sessions(&w).unwrap().len(), 1);
        let report = core.execute_legacy_import(&w, &plan_id).unwrap();
        assert_eq!(
            report
                .results
                .iter()
                .filter(|r| r.status == "already_imported")
                .count(),
            1
        );
        assert_eq!(
            report
                .results
                .iter()
                .filter(|r| r.status == "imported")
                .count(),
            1
        );
        assert_eq!(core.list_sessions(&w).unwrap().len(), 2);
        assert!(core
            .execute_legacy_import(&w, &plan_id)
            .unwrap()
            .results
            .iter()
            .all(|r| r.status == "already_imported"));
    }
}
