//! Discussion persistence and the one-active-orchestrator lease. Never writes to a project.
use super::models::*;
use crate::{models::now_ms, storage::atomic_write_json};
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
fn lock() -> &'static Mutex<()> {
    static L: OnceLock<Mutex<()>> = OnceLock::new();
    L.get_or_init(Default::default)
}
type RunningDiscussions = HashMap<String, (String, Arc<AtomicBool>)>;
fn runs() -> &'static Mutex<RunningDiscussions> {
    static R: OnceLock<Mutex<RunningDiscussions>> = OnceLock::new();
    R.get_or_init(Default::default)
}
pub fn next_id(prefix: &str) -> String {
    static N: AtomicU64 = AtomicU64::new(0);
    format!(
        "{prefix}-{}-{}-{}",
        now_ms(),
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    )
}
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 100
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
pub fn cancel(id: &str) -> bool {
    let Ok(r) = runs().lock() else {
        return false;
    };
    if let Some((_, c)) = r.get(id) {
        c.store(true, Ordering::Relaxed);
        true
    } else {
        false
    }
}

pub struct Lease {
    pub run_id: String,
    pub cancel: Arc<AtomicBool>,
    id: String,
}
impl Drop for Lease {
    fn drop(&mut self) {
        if let Ok(mut r) = runs().lock() {
            if r.get(&self.id).is_some_and(|(run, _)| run == &self.run_id) {
                r.remove(&self.id);
            }
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
            root: app_data.join("discuss"),
        }
    }
    pub fn path(&self, id: &str, parts: &[&str]) -> Result<PathBuf, String> {
        if !valid_id(id) {
            return Err("Invalid discussion id".into());
        }
        let mut path = self.root.clone();
        for part in std::iter::once("")
            .chain(std::iter::once(id))
            .chain(parts.iter().copied())
        {
            if !part.is_empty() {
                if part == "." || part == ".." || part.contains(['/', '\\']) {
                    return Err("Invalid discussion path".into());
                }
                path.push(part);
            }
            if fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
                return Err("Discussion paths must not be symlinks".into());
            }
        }
        Ok(path)
    }
    pub fn create(&self, d: &Discussion) -> Result<(), String> {
        let _l = lock().lock().map_err(|_| "Discussion lock poisoned")?;
        let path = self.path(&d.discussion_id, &[])?;
        fs::create_dir_all(&self.root).map_err(|e| e.to_string())?;
        fs::create_dir(path).map_err(|e| e.to_string())?;
        self.save(d)
    }
    fn save(&self, d: &Discussion) -> Result<(), String> {
        atomic_write_json(&self.path(&d.discussion_id, &["discussion.json"])?, d)
    }
    fn read(&self, id: &str) -> Result<Discussion, String> {
        let text = fs::read_to_string(self.path(id, &["discussion.json"])?)
            .map_err(|e| format!("Discussion unreadable: {e}"))?;
        let d: Discussion =
            serde_json::from_str(&text).map_err(|e| format!("Invalid discussion: {e}"))?;
        if d.schema_version != SCHEMA_VERSION
            || d.discussion_id != id
            || !(2..=4).contains(&d.participants.len())
        {
            return Err("Unsupported discussion record".into());
        }
        for (i, p) in d.participants.iter().enumerate() {
            if p.id != format!("p{}", i + 1) {
                return Err("Invalid participant id".into());
            }
        }
        let expected = tasks(&d.participants);
        if d.tasks.len() != expected.len()
            || d.tasks.iter().zip(expected.iter()).any(|(a, b)| {
                a.id != b.id || a.round != b.round || a.participant_id != b.participant_id
            })
        {
            return Err("Invalid discussion tasks".into());
        }
        Ok(d)
    }
    pub fn load(&self, id: &str) -> Result<Discussion, String> {
        let _l = lock().lock().map_err(|_| "Discussion lock poisoned")?;
        let mut d = self.read(id)?;
        let live = runs()
            .lock()
            .map_err(|_| "Discussion registry poisoned")?
            .get(id)
            .map(|(r, _)| r.clone());
        if d.active_run_id.is_some() && d.active_run_id != live {
            for t in &mut d.tasks {
                if matches!(t.status, Status::Running | Status::Pending) {
                    t.status = Status::Interrupted;
                    t.error = Some("Loom stopped before this call completed".into());
                }
            }
            d.status = "interrupted".into();
            d.active_run_id = None;
            d.updated_at_ms = now_ms();
            self.save(&d)?;
        }
        Ok(d)
    }
    pub fn update<T>(
        &self,
        id: &str,
        change: impl FnOnce(&mut Discussion) -> Result<T, String>,
    ) -> Result<(Discussion, T), String> {
        let _l = lock().lock().map_err(|_| "Discussion lock poisoned")?;
        let mut d = self.read(id)?;
        let value = change(&mut d)?;
        d.updated_at_ms = now_ms();
        self.save(&d)?;
        Ok((d, value))
    }
    /// Starts the initial pipeline or exactly one explicit retry. Admission and deletion share the lock.
    pub fn begin(&self, id: &str, retry: Option<&str>) -> Result<(Discussion, Lease), String> {
        let _l = lock().lock().map_err(|_| "Discussion lock poisoned")?;
        let mut registry = runs().lock().map_err(|_| "Discussion registry poisoned")?;
        if registry.contains_key(id) {
            return Err("This discussion is already running".into());
        }
        let mut d = self.read(id)?;
        if let Some(task_id) = retry {
            let task = d.task(task_id)?;
            if !task.retryable() {
                return Err(
                    "Only failed, stopped, interrupted or outdated results can be retried".into(),
                );
            }
            let round = task.round;
            for other in &mut d.tasks {
                if other.round > round {
                    other.stale = true;
                }
            }
            let task = d.task_mut(task_id)?;
            task.status = Status::Pending;
            task.error = None;
        } else if d
            .tasks
            .iter()
            .any(|t| t.status != Status::Pending || !t.attempts.is_empty())
        {
            return Err("This discussion has already started".into());
        }
        let run_id = next_id("run");
        let cancel = Arc::new(AtomicBool::new(false));
        d.active_run_id = Some(run_id.clone());
        d.status = "running".into();
        d.updated_at_ms = now_ms();
        self.save(&d)?;
        registry.insert(id.into(), (run_id.clone(), cancel.clone()));
        Ok((
            d,
            Lease {
                id: id.into(),
                run_id,
                cancel,
            },
        ))
    }
    pub fn list(&self) -> DiscussionList {
        let mut result = DiscussionList {
            items: Vec::new(),
            storage_errors: Vec::new(),
        };
        match fs::read_dir(&self.root) {
            Ok(entries) => {
                for e in entries {
                    match e {
                        Ok(e) => {
                            let id = e.file_name().to_string_lossy().into_owned();
                            if !valid_id(&id) {
                                continue;
                            }
                            match self.load(&id) {
                                Ok(d) => result.items.push(DiscussionSummary {
                                    discussion_id: id,
                                    title: d.title,
                                    status: d.status,
                                    participants: d.participants.len(),
                                    project_name: d.project_name,
                                    updated_at_ms: d.updated_at_ms,
                                }),
                                Err(e) => result.storage_errors.push(format!("{id}: {e}")),
                            }
                        }
                        Err(e) => result.storage_errors.push(e.to_string()),
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => result.storage_errors.push(e.to_string()),
        }
        result
            .items
            .sort_by_key(|s| std::cmp::Reverse(s.updated_at_ms));
        result
    }
    pub fn delete(&self, id: &str) -> Result<(), String> {
        let _l = lock().lock().map_err(|_| "Discussion lock poisoned")?;
        if runs()
            .lock()
            .map_err(|_| "Discussion registry poisoned")?
            .contains_key(id)
        {
            return Err("Stop the discussion before deleting it".into());
        }
        self.read(id)?;
        fs::remove_dir_all(self.path(id, &[])?).map_err(|e| e.to_string())
    }
    pub fn workspace(&self, id: &str) -> Result<PathBuf, String> {
        let p = self.path(id, &["workspace"])?;
        fs::create_dir_all(&p).map_err(|e| e.to_string())?;
        Ok(p)
    }
    pub fn report_path(&self, id: &str, t: &Task) -> Result<PathBuf, String> {
        if !valid_id(&t.participant_id) {
            return Err("Invalid participant id".into());
        }
        if t.round == 3 {
            self.path(id, &["summary.md"])
        } else if (1..=2).contains(&t.round) {
            self.path(
                id,
                &[
                    "rounds",
                    &t.round.to_string(),
                    &format!("{}.md", t.participant_id),
                ],
            )
        } else {
            Err("Invalid round".into())
        }
    }
    pub fn read_report(&self, id: &str, t: &Task) -> Result<Option<String>, String> {
        let p = self.report_path(id, t)?;
        match fs::read_to_string(p) {
            Ok(s) => Ok(Some(s)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }
    pub fn write_report(&self, id: &str, t: &Task, text: &str) -> Result<(), String> {
        write_text(&self.report_path(id, t)?, text)
    }
    fn job_path(&self, id: &str, job: &str, name: &str) -> Result<PathBuf, String> {
        if !valid_id(job) {
            return Err("Invalid job id".into());
        }
        self.path(id, &["jobs", job, name])
    }
    pub fn save_job(&self, id: &str, job: &JobRecord) -> Result<(), String> {
        atomic_write_json(&self.job_path(id, &job.job_id, "job.json")?, job)
    }
    pub fn write_job_text(
        &self,
        id: &str,
        job: &str,
        name: &str,
        text: &str,
    ) -> Result<(), String> {
        write_text(&self.job_path(id, job, name)?, text)
    }
    pub fn append_log(&self, id: &str, job: &str, stderr: bool, line: &str) -> Result<(), String> {
        let p = self.job_path(id, job, if stderr { "stderr.log" } else { "stdout.log" })?;
        fs::create_dir_all(p.parent().unwrap()).map_err(|e| e.to_string())?;
        writeln!(
            fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(p)
                .map_err(|e| e.to_string())?,
            "{line}"
        )
        .map_err(|e| e.to_string())
    }
    pub fn read_log(
        &self,
        id: &str,
        job: &str,
        stream: &str,
        offset: u64,
    ) -> Result<LogChunk, String> {
        let name = match stream {
            "stdout" => "stdout.log",
            "stderr" => "stderr.log",
            "prompt" => "prompt.md",
            "record" => "job.json",
            _ => return Err("Unknown log stream".into()),
        };
        let d = self.load(id)?;
        if !d.tasks.iter().any(|t| t.attempts.iter().any(|j| j == job)) {
            return Err("Attempt not found".into());
        }
        let mut f = fs::File::open(self.job_path(id, job, name)?)
            .map_err(|e| format!("Log unavailable: {e}"))?;
        let size = f.metadata().map_err(|e| e.to_string())?.len();
        let offset = offset.min(size);
        f.seek(SeekFrom::Start(offset)).map_err(|e| e.to_string())?;
        let mut buf = vec![0; (size - offset).min(256 * 1024) as usize];
        f.read_exact(&mut buf).map_err(|e| e.to_string())?;
        let mut end = buf.len();
        while end > 0 && std::str::from_utf8(&buf[..end]).is_err() && buf.len() - end < 4 {
            end -= 1;
        }
        buf.truncate(end);
        let next = offset + buf.len() as u64;
        Ok(LogChunk {
            text: String::from_utf8_lossy(&buf).into_owned(),
            next_offset: next,
            has_more: next < size,
        })
    }
}
fn write_text(path: &Path, text: &str) -> Result<(), String> {
    fs::create_dir_all(path.parent().ok_or("Invalid path")?).map_err(|e| e.to_string())?;
    let temp = path.with_extension(next_id("tmp"));
    fs::write(&temp, text).map_err(|e| e.to_string())?;
    fs::rename(temp, path).map_err(|e| e.to_string())
}
