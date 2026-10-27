//! Loom-managed study directories under `<appData>/learn/studies/<studyId>/`.
use super::models::*;
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

/// Serializes read-modify-write of study.json across commands and job tasks.
fn study_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

pub fn next_id(prefix: &str) -> String {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    format!(
        "{prefix}-{}-{}",
        crate::models::now_ms(),
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

/// In-memory registry of running jobs; anything marked running on disk without
/// an entry here was interrupted by a restart.
#[derive(Clone)]
pub struct RunningJob {
    pub job_id: String,
    pub cancel: Arc<AtomicBool>,
}

fn running() -> &'static Mutex<HashMap<String, RunningJob>> {
    static RUNNING: OnceLock<Mutex<HashMap<String, RunningJob>>> = OnceLock::new();
    RUNNING.get_or_init(Default::default)
}

pub fn running_job(study_id: &str) -> Option<RunningJob> {
    running().lock().ok()?.get(study_id).cloned()
}

/// Claims the single job slot of a study.
pub fn claim_job(study_id: &str, job_id: &str) -> Result<Arc<AtomicBool>, String> {
    let mut map = running().lock().map_err(|_| "learn registry poisoned")?;
    if map.contains_key(study_id) {
        return Err("A generation job is already running for this study".into());
    }
    let cancel = Arc::new(AtomicBool::new(false));
    map.insert(
        study_id.into(),
        RunningJob {
            job_id: job_id.into(),
            cancel: cancel.clone(),
        },
    );
    Ok(cancel)
}

pub fn release_job(study_id: &str, job_id: &str) {
    if let Ok(mut map) = running().lock() {
        if map.get(study_id).is_some_and(|job| job.job_id == job_id) {
            map.remove(study_id);
        }
    }
}

#[derive(Clone)]
pub struct Store {
    root: PathBuf,
}

impl Store {
    pub fn new(app_data: &Path) -> Self {
        Self {
            root: app_data.join("learn").join("studies"),
        }
    }

    pub fn dir(&self, study_id: &str) -> Result<PathBuf, String> {
        if !valid_id(study_id) {
            return Err("Invalid study id".into());
        }
        let dir = self.root.join(study_id);
        if fs::symlink_metadata(&dir).is_ok_and(|meta| meta.file_type().is_symlink()) {
            return Err("Study directory must not be a symlink".into());
        }
        Ok(dir)
    }

    pub fn checkout(&self, study_id: &str) -> Result<PathBuf, String> {
        Ok(self.dir(study_id)?.join("checkout"))
    }

    pub fn create(&self, study: &Study) -> Result<(), String> {
        let dir = self.dir(&study.study_id)?;
        fs::create_dir_all(&self.root).map_err(|e| format!("failed to create study root: {e}"))?;
        fs::create_dir(&dir).map_err(|e| format!("failed to create study: {e}"))?;
        atomic_write_json(&dir.join("study.json"), study)
    }

    fn read(&self, study_id: &str) -> Result<Study, String> {
        let path = self.dir(study_id)?.join("study.json");
        let text = fs::read_to_string(&path).map_err(|e| format!("study unreadable: {e}"))?;
        serde_json::from_str(&text).map_err(|e| format!("study.json is invalid: {e}"))
    }

    /// Loads a study, marking stages left `running` by a previous process as
    /// interrupted. Interrupted work is never restarted automatically.
    pub fn load(&self, study_id: &str) -> Result<Study, String> {
        let _lock = study_lock().lock().map_err(|_| "study lock poisoned")?;
        let mut study = self.read(study_id)?;
        let active = running_job(study_id).map(|job| job.job_id);
        let mut changed = false;
        for stage in &mut study.stages {
            if stage.status == StageStatus::Running && stage.job_id != active {
                stage.status = StageStatus::Interrupted;
                stage.error = Some("Interrupted: Loom stopped while this stage was running".into());
                changed = true;
            }
        }
        if changed {
            atomic_write_json(&self.dir(study_id)?.join("study.json"), &study)?;
        }
        Ok(study)
    }

    pub fn update<T>(
        &self,
        study_id: &str,
        change: impl FnOnce(&mut Study) -> Result<T, String>,
    ) -> Result<(Study, T), String> {
        let _lock = study_lock().lock().map_err(|_| "study lock poisoned")?;
        let mut study = self.read(study_id)?;
        let value = change(&mut study)?;
        study.updated_at_ms = crate::models::now_ms();
        atomic_write_json(&self.dir(study_id)?.join("study.json"), &study)?;
        Ok((study, value))
    }

    pub fn list(&self) -> StudyList {
        let mut studies = Vec::new();
        let mut storage_errors = Vec::new();
        if let Ok(entries) = fs::read_dir(&self.root) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if !valid_id(&name) {
                    continue;
                }
                match self.load(&name) {
                    Ok(study) => studies.push(summary(&study)),
                    Err(error) => storage_errors.push(format!("{name}: {error}")),
                }
            }
        }
        studies.sort_by_key(|s| std::cmp::Reverse(s.updated_at_ms));
        StudyList {
            studies,
            storage_errors,
        }
    }

    pub fn delete(&self, study_id: &str) -> Result<(), String> {
        if running_job(study_id).is_some() {
            return Err("Cancel the running job before deleting this study".into());
        }
        let dir = self.dir(study_id)?;
        if !dir.starts_with(&self.root) || !dir.join("study.json").is_file() {
            return Err("Study not found".into());
        }
        fs::remove_dir_all(&dir).map_err(|e| format!("failed to delete study: {e}"))
    }

    pub fn write_architecture(&self, study_id: &str, value: &Architecture) -> Result<(), String> {
        atomic_write_json(&self.dir(study_id)?.join("architecture.json"), value)
    }

    pub fn architecture(&self, study_id: &str) -> Result<Option<Architecture>, String> {
        let path = self.dir(study_id)?.join("architecture.json");
        if !path.is_file() {
            return Ok(None);
        }
        let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        serde_json::from_str(&text)
            .map(Some)
            .map_err(|e| format!("architecture.json is invalid: {e}"))
    }

    pub fn write_json<T: serde::Serialize>(
        &self,
        study_id: &str,
        name: &str,
        value: &T,
    ) -> Result<(), String> {
        atomic_write_json(&self.dir(study_id)?.join(name), value)
    }

    pub fn read_json<T: serde::de::DeserializeOwned>(
        &self,
        study_id: &str,
        name: &str,
    ) -> Result<Option<T>, String> {
        let path = self.dir(study_id)?.join(name);
        if !path.is_file() {
            return Ok(None);
        }
        let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        serde_json::from_str(&text)
            .map(Some)
            .map_err(|e| format!("{name} is invalid: {e}"))
    }

    fn page_path(&self, study_id: &str, page_id: &str) -> Result<PathBuf, String> {
        let wiki = self.dir(study_id)?.join("wiki");
        let path = match page_id {
            "overview" | "architecture" | "glossary" => wiki.join(format!("{page_id}.md")),
            _ => {
                let id = page_id
                    .strip_prefix("module:")
                    .filter(|id| valid_id(id))
                    .ok_or("Invalid page id")?;
                wiki.join("modules").join(format!("{id}.md"))
            }
        };
        Ok(path)
    }

    pub fn write_page(&self, study_id: &str, page_id: &str, markdown: &str) -> Result<(), String> {
        let path = self.page_path(study_id, page_id)?;
        write_text(&path, markdown)
    }

    pub fn read_page(&self, study_id: &str, page_id: &str) -> Result<Option<String>, String> {
        let path = self.page_path(study_id, page_id)?;
        if !path.is_file() {
            return Ok(None);
        }
        fs::read_to_string(&path)
            .map(Some)
            .map_err(|e| format!("page unreadable: {e}"))
    }

    pub fn job_dir(&self, study_id: &str, job_id: &str) -> Result<PathBuf, String> {
        if !valid_id(job_id) {
            return Err("Invalid job id".into());
        }
        Ok(self.dir(study_id)?.join("jobs").join(job_id))
    }

    pub fn save_job(&self, study_id: &str, job: &JobRecord) -> Result<(), String> {
        atomic_write_json(&self.job_dir(study_id, &job.job_id)?.join("job.json"), job)
    }

    pub fn append_log(&self, study_id: &str, job_id: &str, stderr: bool, line: &str) {
        let Ok(dir) = self.job_dir(study_id, job_id) else {
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

    pub fn read_log(
        &self,
        study_id: &str,
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
        let path = self.job_dir(study_id, job_id)?.join(name);
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

pub fn write_text(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
    }
    let temp = path.with_extension(format!("tmp-{}", next_id("w")));
    fs::write(&temp, text).map_err(|e| format!("failed to write page: {e}"))?;
    fs::rename(&temp, path).map_err(|e| format!("failed to replace page: {e}"))
}

pub fn summary(study: &Study) -> StudySummary {
    let status = if study.active_job().is_some() {
        "running"
    } else if study.stages.iter().any(|s| {
        matches!(s.status, StageStatus::Failed | StageStatus::Interrupted)
            && (s.stage == "fetch" || s.stage == "inventory" || s.stage == "architecture")
    }) {
        "failed"
    } else if study.stage_done("architecture") {
        "ready"
    } else {
        "idle"
    };
    StudySummary {
        study_id: study.study_id.clone(),
        owner: study.owner.clone(),
        repo: study.repo.clone(),
        commit: study.commit.clone(),
        agent_name: study.agent_name.clone(),
        model: study.model.clone(),
        status: status.into(),
        pages_ready: study.pages.iter().filter(|p| p.ready).count(),
        pages_total: study.pages.len(),
        updated_at_ms: study.updated_at_ms,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn temp_root() -> PathBuf {
        let root = std::env::temp_dir().join(next_id("loom-learn-test"));
        fs::create_dir_all(&root).unwrap();
        root
    }

    pub(crate) fn sample_study(id: &str) -> Study {
        Study {
            schema_version: STUDY_SCHEMA_VERSION,
            study_id: id.into(),
            url: "https://github.com/o/r".into(),
            owner: "o".into(),
            repo: "r".into(),
            git_ref: None,
            commit: None,
            agent_id: "agent".into(),
            agent_name: "Agent".into(),
            adapter_type: "codex_cli".into(),
            model: None,
            language: "en".into(),
            depth: "standard".into(),
            prompt_version: 1,
            created_at_ms: 1,
            updated_at_ms: 1,
            stages: vec![StageState::pending("fetch")],
            pages: Vec::new(),
            coverage: None,
            checkout_fingerprint: None,
        }
    }

    #[test]
    fn ids_are_single_lowercase_segments() {
        assert!(valid_id("study-1-2"));
        for bad in ["", "-x", "../x", "a/b", "A", "a b", &"a".repeat(81)] {
            assert!(!valid_id(bad), "accepted {bad}");
        }
    }

    #[test]
    fn stale_running_stage_is_marked_interrupted_and_not_rerun() {
        let root = temp_root();
        let store = Store::new(&root);
        let mut study = sample_study("study-a");
        study.stages[0].status = StageStatus::Running;
        study.stages[0].job_id = Some("job-old".into());
        store.create(&study).unwrap();
        let loaded = store.load("study-a").unwrap();
        assert_eq!(loaded.stages[0].status, StageStatus::Interrupted);
        assert_eq!(store.list().studies[0].status, "failed");
        // A live job keeps its running state.
        let mut live = sample_study("study-b");
        live.stages[0].status = StageStatus::Running;
        live.stages[0].job_id = Some("job-live".into());
        store.create(&live).unwrap();
        claim_job("study-b", "job-live").unwrap();
        assert!(claim_job("study-b", "job-other").is_err());
        assert_eq!(
            store.load("study-b").unwrap().stages[0].status,
            StageStatus::Running
        );
        assert!(store.delete("study-b").is_err());
        release_job("study-b", "job-live");
        store.delete("study-b").unwrap();
        assert!(store.load("study-b").is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn pages_and_logs_stay_inside_the_study() {
        let root = temp_root();
        let store = Store::new(&root);
        store.create(&sample_study("study-c")).unwrap();
        assert!(store.write_page("study-c", "module:../x", "x").is_err());
        assert!(store.write_page("study-c", "notes", "x").is_err());
        store
            .write_page("study-c", "module:core", "# Core")
            .unwrap();
        assert_eq!(
            store.read_page("study-c", "module:core").unwrap().unwrap(),
            "# Core"
        );
        assert!(store.dir("../etc").is_err());
        store.append_log("study-c", "job-1", false, "line one");
        store.append_log("study-c", "job-1", false, "line two");
        let first = store
            .read_log("study-c", "job-1", "stdout", 0, Some(9))
            .unwrap();
        assert_eq!(first.text, "line one\n");
        assert!(first.has_more);
        let rest = store
            .read_log("study-c", "job-1", "stdout", first.next_offset, None)
            .unwrap();
        assert_eq!(rest.text, "line two\n");
        assert!(store.read_log("study-c", "job-1", "../x", 0, None).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
