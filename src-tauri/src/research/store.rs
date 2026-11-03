//! Loom-managed research directories under `<appData>/research/<researchId>/`.
//! Nothing is ever written into the user's project.
use super::{admission, files, models::*};
use crate::storage::atomic_write_json;
use std::{
    collections::HashMap,
    fs,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
    },
};

/// Serializes read-modify-write of research.json across commands and job tasks.
fn research_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

pub fn next_id(prefix: &str) -> String {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    format!(
        "{prefix}-{}-{}-{}",
        crate::models::now_ms(),
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}

/// Single path segment of lowercase ASCII, digits and `-`.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 80
        && !id.starts_with('-')
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// In-memory registry of running jobs; a turn marked running on disk without
/// an entry here was interrupted by a restart.
#[derive(Clone)]
pub struct RunningJob {
    pub job_id: String,
    pub cancel: Arc<AtomicBool>,
    _lease: Arc<admission::Lease>,
}

fn running() -> &'static Mutex<HashMap<PathBuf, RunningJob>> {
    static RUNNING: OnceLock<Mutex<HashMap<PathBuf, RunningJob>>> = OnceLock::new();
    RUNNING.get_or_init(Default::default)
}

pub fn running_job(store: &Store, research_id: &str) -> Option<RunningJob> {
    running()
        .lock()
        .ok()?
        .get(&store.dir(research_id).ok()?)
        .cloned()
}

/// Claims the single job slot of a research.
pub fn claim_job(
    store: &Store,
    research_id: &str,
    job_id: &str,
) -> Result<Arc<AtomicBool>, String> {
    store.require_legacy(research_id)?;
    let key = store.dir(research_id)?;
    let mut map = running().lock().map_err(|_| "research registry poisoned")?;
    if map.contains_key(&key) {
        return Err("This research is already answering a question".into());
    }
    let mut lease = admission::acquire(&store.root, research_id)?;
    lease.bind(job_id, "legacy")?;
    let cancel = Arc::new(AtomicBool::new(false));
    map.insert(
        key,
        RunningJob {
            job_id: job_id.into(),
            cancel: cancel.clone(),
            _lease: Arc::new(lease),
        },
    );
    Ok(cancel)
}

pub fn release_job(store: &Store, research_id: &str, job_id: &str) {
    let Ok(key) = store.dir(research_id) else {
        return;
    };
    if let Ok(mut map) = running().lock() {
        if map.get(&key).is_some_and(|job| job.job_id == job_id) {
            map.remove(&key);
        }
    }
}

#[derive(Clone)]
pub struct Store {
    pub(super) root: PathBuf,
}

impl Store {
    pub fn new(app_data: &Path) -> Self {
        Self {
            root: app_data.join("research"),
        }
    }

    pub fn dir(&self, research_id: &str) -> Result<PathBuf, String> {
        if !valid_id(research_id) {
            return Err("Invalid research id".into());
        }
        files::path(&self.root, research_id)
    }

    /// Empty working directory for research without a project.
    pub fn workspace(&self, research_id: &str) -> Result<PathBuf, String> {
        let path = self.workspace_path(research_id)?;
        fs::create_dir_all(&path).map_err(|e| format!("failed to create workspace: {e}"))?;
        Ok(path)
    }
    pub fn workspace_path(&self, research_id: &str) -> Result<PathBuf, String> {
        files::path(&self.dir(research_id)?, "workspace")
    }

    pub fn create(&self, research: &Research) -> Result<(), String> {
        let _lease = admission::acquire(&self.root, &research.research_id)?;
        let dir = self.dir(&research.research_id)?;
        fs::create_dir_all(&self.root)
            .map_err(|e| format!("failed to create research root: {e}"))?;
        fs::create_dir(&dir).map_err(|e| format!("failed to create research: {e}"))?;
        let mut persisted = research.clone();
        persisted.active_owner = None;
        atomic_write_json(&dir.join("research.json"), &persisted)
    }

    fn read(&self, research_id: &str) -> Result<Research, String> {
        let path = files::path(&self.dir(research_id)?, "research.json")?;
        let research: Research = files::read_json(&path, 4 * 1024 * 1024)?;
        if !matches!(research.schema_version, 1 | 2) || research.research_id != research_id {
            return Err("unsupported_or_mismatched_research_schema".into());
        }
        if research.schema_version == 1 && research.workflow != Workflow::Legacy {
            return Err("invalid_research_workflow".into());
        }
        if (research.workflow == Workflow::Target) != research.target.is_some() {
            return Err("invalid_research_workflow".into());
        }
        Ok(research)
    }

    /// Read-only projection. Readers never overwrite another process's state.
    pub fn load(&self, research_id: &str) -> Result<Research, String> {
        let _lock = research_lock()
            .lock()
            .map_err(|_| "research lock poisoned")?;
        let mut research = self.read(research_id)?;
        research.active_owner = admission::owner(&self.root, research_id)?;
        if !admission::is_active(&self.root, research_id)? {
            interrupt_stale(&mut research, None);
        }
        Ok(research)
    }

    pub fn require_legacy(&self, id: &str) -> Result<Research, String> {
        let research = self.load(id)?;
        if research.workflow != Workflow::Legacy {
            return Err("workflow_mismatch".into());
        }
        Ok(research)
    }

    pub fn cancel_legacy(&self, id: &str) -> Result<bool, String> {
        self.require_legacy(id)?;
        let Some(owner) = admission::owner(&self.root, id)? else {
            return Ok(false);
        };
        if owner.workflow != "legacy" {
            return Err("workflow_mismatch".into());
        }
        let requested = admission::request_cancel(&self.root, id, &owner.run_id)?;
        if let Some(job) = running_job(self, id) {
            if job.job_id == owner.run_id {
                job.cancel.store(true, Ordering::Relaxed);
            }
        }
        Ok(requested)
    }

    pub fn is_cancelled(&self, id: &str, job_id: &str) -> bool {
        admission::cancelled(&self.root, id, job_id)
    }

    pub fn update<T>(
        &self,
        research_id: &str,
        change: impl FnOnce(&mut Research) -> Result<T, String>,
    ) -> Result<(Research, T), String> {
        let _lock = research_lock()
            .lock()
            .map_err(|_| "research lock poisoned")?;
        let owner = running_job(self, research_id);
        let _lease = if owner.is_none() {
            Some(admission::acquire(&self.root, research_id)?)
        } else {
            None
        };
        let mut research = self.read(research_id)?;
        if research.workflow != Workflow::Legacy {
            return Err("workflow_mismatch".into());
        }
        if research.schema_version == 1 {
            let backup = self.dir(research_id)?.join("research.v1.backup.json");
            if !backup.exists() {
                let original = files::read_bytes(
                    &self.dir(research_id)?.join("research.json"),
                    4 * 1024 * 1024,
                )?;
                files::write_bytes(&backup, &original)?;
            } else {
                let saved: Research = files::read_json(&backup, 4 * 1024 * 1024)?;
                if saved.schema_version != 1
                    || saved.research_id != research_id
                    || saved.workflow != Workflow::Legacy
                {
                    return Err("invalid_legacy_backup".into());
                }
            }
            research.schema_version = 2;
        }
        interrupt_stale(&mut research, owner.as_ref().map(|v| v.job_id.as_str()));
        let value = change(&mut research)?;
        if research.workflow != Workflow::Legacy || research.schema_version != 2 {
            return Err("research_workflow_is_immutable".into());
        }
        research.updated_at_ms = crate::models::now_ms();
        research.active_owner = None;
        files::write_json(&self.dir(research_id)?.join("research.json"), &research)?;
        Ok((research, value))
    }

    pub fn list(&self) -> ResearchList {
        let mut items = Vec::new();
        let mut storage_errors = Vec::new();
        let entries = match fs::symlink_metadata(&self.root) {
            Ok(meta) if !meta.is_dir() || meta.file_type().is_symlink() => Err(
                std::io::Error::other("research root is not an ordinary directory"),
            ),
            Ok(_) => fs::read_dir(&self.root),
            Err(error) => Err(error),
        };
        if let Ok(entries) = entries {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if !valid_id(&name) {
                    continue;
                }
                match self.load(&name) {
                    Ok(research) => {
                        let mut item = summary(&research);
                        if research.workflow == Workflow::Target {
                            match super::target_store::TargetStore::open(self.clone(), &name)
                                .and_then(|target| target.list_runs())
                            {
                                Ok(runs) => {
                                    if let Some(run) = runs.last() {
                                        item.status = run.state.status.as_str().into();
                                    }
                                    item.turns = runs.len();
                                }
                                Err(error) => storage_errors.push(format!("{name}: {error}")),
                            }
                        }
                        items.push(item);
                    }
                    Err(error) => storage_errors.push(format!("{name}: {error}")),
                }
            }
        } else if let Err(error) = entries {
            if error.kind() != std::io::ErrorKind::NotFound {
                storage_errors.push(format!("research root unavailable: {error}"));
            }
        }
        items.sort_by_key(|s| std::cmp::Reverse(s.updated_at_ms));
        ResearchList {
            items,
            storage_errors,
        }
    }

    pub fn delete(&self, research_id: &str) -> Result<(), String> {
        let _lease = admission::acquire(&self.root, research_id)?;
        let dir = self.dir(research_id)?;
        if !dir.starts_with(&self.root) || !dir.join("research.json").is_file() {
            return Err("Research not found".into());
        }
        fs::remove_dir_all(&dir).map_err(|e| format!("failed to delete research: {e}"))
    }

    fn turn_path(&self, research_id: &str, index: u32) -> Result<PathBuf, String> {
        self.require_legacy(research_id)?;
        files::path(&self.dir(research_id)?, &format!("turns/{index}.md"))
    }

    pub fn write_turn(&self, research_id: &str, index: u32, markdown: &str) -> Result<(), String> {
        write_text(&self.turn_path(research_id, index)?, markdown)
    }

    pub fn read_turn(&self, research_id: &str, index: u32) -> Result<Option<String>, String> {
        let path = self.turn_path(research_id, index)?;
        if !path.is_file() {
            return Ok(None);
        }
        fs::read_to_string(&path)
            .map(Some)
            .map_err(|e| format!("report unreadable: {e}"))
    }

    pub fn job_dir(&self, research_id: &str, job_id: &str) -> Result<PathBuf, String> {
        self.require_legacy(research_id)?;
        if !valid_id(job_id) {
            return Err("Invalid job id".into());
        }
        files::path(&self.dir(research_id)?, &format!("jobs/{job_id}"))
    }

    pub fn save_job(&self, research_id: &str, job: &JobRecord) -> Result<(), String> {
        atomic_write_json(
            &self.job_dir(research_id, &job.job_id)?.join("job.json"),
            job,
        )
    }

    pub fn append_log(&self, research_id: &str, job_id: &str, stderr: bool, line: &str) {
        let Ok(dir) = self.job_dir(research_id, job_id) else {
            return;
        };
        let _ = fs::create_dir_all(&dir);
        let name = if stderr { "stderr.log" } else { "stdout.log" };
        if let Ok(mut file) = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join(name))
        {
            let _ = writeln!(file, "{line}");
        }
    }

    pub fn append_prompt(&self, research_id: &str, job_id: &str, text: &str) {
        let Ok(dir) = self.job_dir(research_id, job_id) else {
            return;
        };
        let _ = fs::create_dir_all(&dir);
        if let Ok(mut file) = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("prompt.md"))
        {
            let _ = file.write_all(text.as_bytes());
        }
    }

    pub fn read_log(
        &self,
        research_id: &str,
        job_id: &str,
        stream: &str,
        offset: u64,
        limit: Option<u64>,
    ) -> Result<JobLogChunk, String> {
        let name = match stream {
            "stdout" => "stdout.log",
            "stderr" => "stderr.log",
            "prompt" => "prompt.md",
            _ => return Err("Unknown log stream".into()),
        };
        let path = self.job_dir(research_id, job_id)?.join(name);
        let Ok(mut file) = fs::File::open(&path) else {
            return Ok(JobLogChunk {
                text: String::new(),
                next_offset: 0,
                has_more: false,
            });
        };
        let size = file.metadata().map(|m| m.len()).unwrap_or(0);
        let limit = limit.unwrap_or(256 * 1024).min(1024 * 1024);
        let offset = offset.min(size);
        file.seek(SeekFrom::Start(offset))
            .map_err(|e| e.to_string())?;
        let mut buffer = vec![0; (size - offset).min(limit) as usize];
        file.read_exact(&mut buffer).map_err(|e| e.to_string())?;
        // Do not split a UTF-8 sequence across chunks.
        let mut end = buffer.len();
        while end > 0 && std::str::from_utf8(&buffer[..end]).is_err() && buffer.len() - end < 4 {
            end -= 1;
        }
        buffer.truncate(end);
        let next_offset = offset + buffer.len() as u64;
        Ok(JobLogChunk {
            text: String::from_utf8_lossy(&buffer).into_owned(),
            next_offset,
            has_more: next_offset < size,
        })
    }
}

fn interrupt_stale(research: &mut Research, active: Option<&str>) {
    for turn in &mut research.turns {
        if turn.status == TurnStatus::Running && active != Some(turn.job_id.as_str()) {
            turn.status = TurnStatus::Interrupted;
            turn.error = Some("Interrupted: Loom stopped while this question was running".into());
        }
    }
}

fn write_text(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
    }
    let temp = path.with_extension(format!("tmp-{}", next_id("w")));
    fs::write(&temp, text).map_err(|e| format!("failed to write report: {e}"))?;
    fs::rename(&temp, path).map_err(|e| format!("failed to replace report: {e}"))
}

pub fn summary(research: &Research) -> ResearchSummary {
    let status = match research.turns.last().map(|t| &t.status) {
        Some(TurnStatus::Running) => "running",
        Some(TurnStatus::Done) => "ready",
        _ => "failed",
    };
    ResearchSummary {
        active_owner: research.active_owner.clone(),
        workflow: research.workflow,
        research_id: research.research_id.clone(),
        title: research.title.clone(),
        agent_name: research.agent_name.clone(),
        model: research.model.clone(),
        project_name: research.project_name.clone(),
        status: status.into(),
        turns: research.turns.len(),
        updated_at_ms: research.updated_at_ms,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn temp_root() -> PathBuf {
        let root = std::env::temp_dir().join(next_id("loom-research-test"));
        fs::create_dir_all(&root).unwrap();
        root
    }

    pub(crate) fn sample_research(id: &str) -> Research {
        Research {
            active_owner: None,
            workflow: Workflow::Legacy,
            target: None,
            schema_version: 1, // Historical legacy fixture exercises migration.
            research_id: id.into(),
            title: "How does it work?".into(),
            agent_id: "agent".into(),
            agent_name: "Agent".into(),
            adapter_type: "codex_cli".into(),
            model: None,
            project_path: None,
            project_name: None,
            language: "en".into(),
            prompt_version: 1,
            created_at_ms: 1,
            updated_at_ms: 1,
            turns: Vec::new(),
            resume_command: None,
            resume_agent_command: None,
        }
    }

    pub(crate) fn turn(index: u32, status: TurnStatus, job_id: &str) -> Turn {
        Turn {
            index,
            question: format!("question {index}"),
            status,
            job_id: job_id.into(),
            started_at_ms: 1,
            finished_at_ms: None,
            error: None,
            warnings: Vec::new(),
            refs: None,
            context: None,
        }
    }

    #[test]
    fn ids_are_single_lowercase_segments() {
        assert!(valid_id("research-1-2"));
        for bad in ["", "-x", "../x", "a/b", "A", "a b", &"a".repeat(81)] {
            assert!(!valid_id(bad), "accepted {bad}");
        }
    }

    #[test]
    fn stale_running_turn_is_interrupted_and_live_one_is_kept() {
        let root = temp_root();
        let store = Store::new(&root);
        let mut research = sample_research("research-a");
        research.turns.push(turn(1, TurnStatus::Running, "job-old"));
        store.create(&research).unwrap();
        let loaded = store.load("research-a").unwrap();
        assert_eq!(loaded.turns[0].status, TurnStatus::Interrupted);
        assert_eq!(store.list().items[0].status, "failed");

        let mut live = sample_research("research-b");
        live.turns.push(turn(1, TurnStatus::Running, "job-live"));
        store.create(&live).unwrap();
        claim_job(&store, "research-b", "job-live").unwrap();
        assert!(claim_job(&store, "research-b", "job-other").is_err());
        assert_eq!(
            store.load("research-b").unwrap().turns[0].status,
            TurnStatus::Running
        );
        assert!(store.delete("research-b").is_err());
        release_job(&store, "research-b", "job-live");
        store.delete("research-b").unwrap();
        assert!(store.load("research-b").is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn future_schema_and_failed_backup_never_replace_original_data() {
        let root = temp_root();
        let store = Store::new(&root);
        store.create(&sample_research("research-schema")).unwrap();
        let path = store.dir("research-schema").unwrap().join("research.json");
        let original = fs::read(&path).unwrap();
        let backup = store
            .dir("research-schema")
            .unwrap()
            .join("research.v1.backup.json");
        fs::create_dir(&backup).unwrap();
        // A corrupt pre-existing backup must not be treated as a valid backup.
        assert!(store.update("research-schema", |_| Ok(())).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
        fs::remove_dir(&backup).unwrap();
        let mut future = sample_research("research-schema");
        future.schema_version = 99;
        files::write_json(&path, &future).unwrap();
        let before = fs::read(&path).unwrap();
        assert!(store.load("research-schema").is_err());
        assert!(store.update("research-schema", |_| Ok(())).is_err());
        assert_eq!(fs::read(path).unwrap(), before);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn broken_root_and_inconsistent_workflow_are_visible_without_repair() {
        let root = temp_root();
        let store = Store::new(&root);
        fs::write(&store.root, "not a directory").unwrap();
        assert!(!store.list().storage_errors.is_empty());
        assert_eq!(fs::read_to_string(&store.root).unwrap(), "not a directory");
        fs::remove_file(&store.root).unwrap();
        let mut malformed = sample_research("research-shape");
        malformed.target = Some(super::super::target_models::TargetConfig {
            target: super::super::target_models::Target::App {
                path: "/not-read".into(),
            },
            run_ids: vec![],
        });
        store.create(&malformed).unwrap();
        assert!(store.load("research-shape").is_err());
        assert!(!store.list().storage_errors.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn legacy_create_rejects_target_payload_fields() {
        let input = serde_json::json!({"question":"Explain","agentId":"a","language":"en","target":{"kind":"app","path":"/app"}});
        assert!(serde_json::from_value::<CreateResearchInput>(input).is_err());
    }

    #[test]
    fn reports_and_logs_stay_inside_the_research() {
        let root = temp_root();
        let store = Store::new(&root);
        store.create(&sample_research("research-c")).unwrap();
        store.write_turn("research-c", 1, "# Report").unwrap();
        assert_eq!(
            store.read_turn("research-c", 1).unwrap().unwrap(),
            "# Report"
        );
        assert!(store.read_turn("research-c", 2).unwrap().is_none());
        assert!(store.dir("../etc").is_err());
        assert!(store.job_dir("research-c", "../x").is_err());
        let workspace = store.workspace("research-c").unwrap();
        assert!(workspace.starts_with(store.dir("research-c").unwrap()));
        store.append_log("research-c", "job-1", false, "line one");
        store.append_log("research-c", "job-1", false, "line two");
        let first = store
            .read_log("research-c", "job-1", "stdout", 0, Some(9))
            .unwrap();
        assert_eq!(first.text, "line one\n");
        assert!(first.has_more);
        let rest = store
            .read_log("research-c", "job-1", "stdout", first.next_offset, None)
            .unwrap();
        assert_eq!(rest.text, "line two\n");
        assert!(store
            .read_log("research-c", "job-1", "../x", 0, None)
            .is_err());
        // Survives a reload: everything lives in research.json and turns/.
        let reopened = Store::new(&root);
        assert_eq!(reopened.list().items.len(), 1);
        fs::remove_dir_all(root).unwrap();
    }
}
