//! Loom-managed research directories under `<appData>/research/<researchId>/`.
//! Nothing is ever written into the user's project.
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

/// Serializes read-modify-write of research.json across commands and job tasks.
fn research_lock() -> &'static Mutex<()> {
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

/// In-memory registry of running jobs; a turn marked running on disk without
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

pub fn running_job(research_id: &str) -> Option<RunningJob> {
    running().lock().ok()?.get(research_id).cloned()
}

/// Claims the single job slot of a research.
pub fn claim_job(research_id: &str, job_id: &str) -> Result<Arc<AtomicBool>, String> {
    let mut map = running().lock().map_err(|_| "research registry poisoned")?;
    if map.contains_key(research_id) {
        return Err("This research is already answering a question".into());
    }
    let cancel = Arc::new(AtomicBool::new(false));
    map.insert(
        research_id.into(),
        RunningJob {
            job_id: job_id.into(),
            cancel: cancel.clone(),
        },
    );
    Ok(cancel)
}

pub fn release_job(research_id: &str, job_id: &str) {
    if let Ok(mut map) = running().lock() {
        if map.get(research_id).is_some_and(|job| job.job_id == job_id) {
            map.remove(research_id);
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
            root: app_data.join("research"),
        }
    }

    pub fn dir(&self, research_id: &str) -> Result<PathBuf, String> {
        if !valid_id(research_id) {
            return Err("Invalid research id".into());
        }
        let dir = self.root.join(research_id);
        if fs::symlink_metadata(&dir).is_ok_and(|meta| meta.file_type().is_symlink()) {
            return Err("Research directory must not be a symlink".into());
        }
        Ok(dir)
    }

    /// Empty working directory for research without a project.
    pub fn workspace(&self, research_id: &str) -> Result<PathBuf, String> {
        let path = self.dir(research_id)?.join("workspace");
        fs::create_dir_all(&path).map_err(|e| format!("failed to create workspace: {e}"))?;
        Ok(path)
    }

    pub fn create(&self, research: &Research) -> Result<(), String> {
        let dir = self.dir(&research.research_id)?;
        fs::create_dir_all(&self.root)
            .map_err(|e| format!("failed to create research root: {e}"))?;
        fs::create_dir(&dir).map_err(|e| format!("failed to create research: {e}"))?;
        atomic_write_json(&dir.join("research.json"), research)
    }

    fn read(&self, research_id: &str) -> Result<Research, String> {
        let path = self.dir(research_id)?.join("research.json");
        let text = fs::read_to_string(&path).map_err(|e| format!("research unreadable: {e}"))?;
        serde_json::from_str(&text).map_err(|e| format!("research.json is invalid: {e}"))
    }

    /// Loads a research, marking turns left `running` by a previous process as
    /// interrupted. Interrupted turns are never restarted automatically.
    pub fn load(&self, research_id: &str) -> Result<Research, String> {
        let _lock = research_lock()
            .lock()
            .map_err(|_| "research lock poisoned")?;
        let mut research = self.read(research_id)?;
        let active = running_job(research_id).map(|job| job.job_id);
        let mut changed = false;
        for turn in &mut research.turns {
            if turn.status == TurnStatus::Running && Some(&turn.job_id) != active.as_ref() {
                turn.status = TurnStatus::Interrupted;
                turn.error =
                    Some("Interrupted: Loom stopped while this question was running".into());
                changed = true;
            }
        }
        if changed {
            atomic_write_json(&self.dir(research_id)?.join("research.json"), &research)?;
        }
        Ok(research)
    }

    pub fn update<T>(
        &self,
        research_id: &str,
        change: impl FnOnce(&mut Research) -> Result<T, String>,
    ) -> Result<(Research, T), String> {
        let _lock = research_lock()
            .lock()
            .map_err(|_| "research lock poisoned")?;
        let mut research = self.read(research_id)?;
        let value = change(&mut research)?;
        research.updated_at_ms = crate::models::now_ms();
        atomic_write_json(&self.dir(research_id)?.join("research.json"), &research)?;
        Ok((research, value))
    }

    pub fn list(&self) -> ResearchList {
        let mut items = Vec::new();
        let mut storage_errors = Vec::new();
        if let Ok(entries) = fs::read_dir(&self.root) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if !valid_id(&name) {
                    continue;
                }
                match self.load(&name) {
                    Ok(research) => items.push(summary(&research)),
                    Err(error) => storage_errors.push(format!("{name}: {error}")),
                }
            }
        }
        items.sort_by_key(|s| std::cmp::Reverse(s.updated_at_ms));
        ResearchList {
            items,
            storage_errors,
        }
    }

    pub fn delete(&self, research_id: &str) -> Result<(), String> {
        if running_job(research_id).is_some() {
            return Err("Stop the running question before deleting this research".into());
        }
        let dir = self.dir(research_id)?;
        if !dir.starts_with(&self.root) || !dir.join("research.json").is_file() {
            return Err("Research not found".into());
        }
        fs::remove_dir_all(&dir).map_err(|e| format!("failed to delete research: {e}"))
    }

    fn turn_path(&self, research_id: &str, index: u32) -> Result<PathBuf, String> {
        Ok(self
            .dir(research_id)?
            .join("turns")
            .join(format!("{index}.md")))
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
        if !valid_id(job_id) {
            return Err("Invalid job id".into());
        }
        Ok(self.dir(research_id)?.join("jobs").join(job_id))
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
            schema_version: RESEARCH_SCHEMA_VERSION,
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
        claim_job("research-b", "job-live").unwrap();
        assert!(claim_job("research-b", "job-other").is_err());
        assert_eq!(
            store.load("research-b").unwrap().turns[0].status,
            TurnStatus::Running
        );
        assert!(store.delete("research-b").is_err());
        release_job("research-b", "job-live");
        store.delete("research-b").unwrap();
        assert!(store.load("research-b").is_err());
        fs::remove_dir_all(root).unwrap();
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
