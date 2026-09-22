//! Serialized v2 journal persistence with read-only v1 migration and live-turn ownership.
use super::{
    journal, normalize_permission_mode, reconcile_interrupted_session, ChatEvent, ChatEventPage,
    ChatIndex, ChatSendInput, ChatSendReceipt, ChatSendResult, ChatSession, ChatStreamEvent,
    CHAT_SCHEMA_VERSION, DEFAULT_SESSION_STATUS,
};
use crate::{models::now_ms, projects::canonical_project_path, storage};
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc, Mutex, OnceLock, Weak,
    },
};

struct ActiveTurn {
    id: String,
    cancelled: Arc<AtomicU8>,
    projection: ChatSession,
    event_index: Option<journal::EventIndex>,
}

struct ProjectStore {
    project: PathBuf,
    root: PathBuf,
    // Held while a command or live turn owns this store. OS releases it on crash.
    _writer: File,
    active: Mutex<HashMap<String, ActiveTurn>>,
}

impl Drop for ProjectStore {
    fn drop(&mut self) {
        // A concurrent spawn may briefly inherit the descriptor before exec
        // closes it. Closing only our descriptor would leave that shared lock
        // alive; explicitly unlock when the last repository owner goes away.
        let _ = self._writer.unlock();
    }
}

#[derive(Clone)]
pub(super) struct ChatRepository(Arc<ProjectStore>);

pub(super) type SessionReadEntry = (String, Result<ChatSession, String>);

pub(super) enum BeginTurn<T> {
    Started(ChatSession, T, TurnLease),
    Existing(ChatSendResult),
}

pub(super) struct TurnLease {
    repository: ChatRepository,
    pub(super) session_id: String,
    pub(super) turn_id: String,
    cancelled: Arc<AtomicU8>,
}

impl TurnLease {
    pub(super) fn stop_reason(&self) -> Option<&'static str> {
        match self.cancelled.load(Ordering::Acquire) {
            1 => Some(super::STOP_REASON_ABORT),
            2 => Some(super::STOP_REASON_SHUTDOWN),
            _ => None,
        }
    }
    #[cfg(test)]
    pub(super) fn is_cancelled(&self) -> bool {
        self.stop_reason().is_some()
    }
}

impl Drop for TurnLease {
    fn drop(&mut self) {
        if let Ok(mut active) = self.repository.0.active.lock() {
            if active
                .get(&self.session_id)
                .is_some_and(|turn| turn.id == self.turn_id)
            {
                active.remove(&self.session_id);
            }
        }
    }
}

fn stores() -> &'static Mutex<HashMap<PathBuf, Weak<ProjectStore>>> {
    static STORES: OnceLock<Mutex<HashMap<PathBuf, Weak<ProjectStore>>>> = OnceLock::new();
    STORES.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(super) fn validate_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > 160
        || !id
            .bytes()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == b'-' || ch == b'_')
    {
        return Err("invalid chat id: expected a single alphanumeric path segment".into());
    }
    Ok(())
}

fn ensure_directory(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => Ok(()),
        Ok(_) => Err(format!(
            "chat directory must not be a symlink: {}",
            path.display()
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => match fs::create_dir(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                ensure_directory(path)
            }
            Err(error) => Err(format!("failed to create chat directory: {error}")),
        },
        Err(error) => Err(format!("failed to inspect chat directory: {error}")),
    }
}

pub(super) fn validate_file(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_file() && !meta.file_type().is_symlink() => Ok(()),
        Ok(_) => Err(format!(
            "chat file must be a regular file: {}",
            path.display()
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("failed to inspect chat file: {error}")),
    }
}

impl ChatRepository {
    pub(super) fn live_repositories() -> Result<Vec<Self>, String> {
        Ok(stores()
            .lock()
            .map_err(|_| "chat store registry unavailable")?
            .values()
            .filter_map(Weak::upgrade)
            .map(Self)
            .collect())
    }
    pub(super) fn active_session_ids(&self) -> Result<Vec<String>, String> {
        Ok(self
            .0
            .active
            .lock()
            .map_err(|_| "chat store lock unavailable")?
            .keys()
            .cloned()
            .collect())
    }
    pub(super) fn export_session(
        &self,
        id: &str,
        export_id: &str,
    ) -> Result<super::ChatExportResult, String> {
        validate_id(export_id)?;
        let session = self.load(id)?;
        let exports = {
            let _guard = self
                .0
                .active
                .lock()
                .map_err(|_| "chat store lock unavailable")?;
            self.validate_layout()?;
            let path = self.0.root.join("exports");
            ensure_directory(&path)?;
            path
        };
        super::export::write(self, session, &exports, export_id)
    }

    pub(super) fn run_directory(
        &self,
        session_id: &str,
        turn_id: &str,
        create: bool,
    ) -> Result<PathBuf, String> {
        validate_id(turn_id)?;
        let runs = self.session_directory(session_id, false)?.join("runs");
        let directory = runs.join(turn_id);
        for path in [&runs, &directory] {
            if create {
                ensure_directory(path)?;
            }
            let meta = fs::symlink_metadata(path)
                .map_err(|e| format!("chat run directory unavailable: {e}"))?;
            if !meta.is_dir() || meta.file_type().is_symlink() {
                return Err("chat run directory must not be a symlink".into());
            }
        }
        Ok(directory)
    }

    pub(super) fn open_run_logs(&self, lease: &TurnLease) -> Result<super::logs::RunLogs, String> {
        let session = self.load(&lease.session_id)?;
        if session.active_turn_id.as_deref() != Some(&lease.turn_id)
            || !session.turns.iter().any(|turn| turn.id == lease.turn_id)
        {
            return Err("chat run does not belong to the active turn".into());
        }
        super::logs::RunLogs::open(&self.run_directory(&lease.session_id, &lease.turn_id, true)?)
    }

    pub(super) fn read_run_logs(
        &self,
        id: &str,
        turn_id: &str,
        stream: super::ChatLogStream,
        offset: u64,
        limit: usize,
    ) -> Result<super::ChatLogPage, String> {
        let session = self.load(id)?;
        if !session.turns.iter().any(|turn| turn.id == turn_id) {
            return Err("chat run does not belong to this session".into());
        }
        super::logs::read(
            &self.run_directory(id, turn_id, false)?,
            stream,
            offset,
            limit,
        )
    }

    pub(super) fn mark_running(&self, lease: &TurnLease, pid: u32) -> Result<ChatSession, String> {
        self.update(&lease.session_id, |session| {
            if session.active_turn_id.as_deref() != Some(&lease.turn_id) {
                return Err("chat turn is not active".into());
            }
            let turn = session
                .turns
                .iter_mut()
                .find(|turn| turn.id == lease.turn_id)
                .ok_or("chat turn record is missing")?;
            if turn.status.is_terminal() || turn.started_at_ms.is_some() {
                return Err("chat turn already started or ended".into());
            }
            turn.started_at_ms = Some(now_ms() as u64);
            turn.process_id = Some(pid);
            if turn.status != super::ChatTurnStatus::Cancelling {
                turn.status = super::ChatTurnStatus::Running;
            }
            Ok(())
        })
    }
    pub(super) fn owns_active_project(project: &Path) -> bool {
        let store = stores()
            .lock()
            .ok()
            .and_then(|stores| stores.get(project).and_then(Weak::upgrade));
        store.is_some_and(|store| store.active.lock().is_ok_and(|active| !active.is_empty()))
    }
    pub(super) fn open(project: &Path) -> Result<Self, String> {
        let project = canonical_project_path(&project.to_string_lossy())?;
        let mut stores = stores()
            .lock()
            .map_err(|_| "chat store registry unavailable")?;
        stores.retain(|_, store| store.strong_count() > 0);
        if let Some(store) = stores.get(&project).and_then(Weak::upgrade) {
            let repository = Self(store);
            repository.validate_layout()?;
            return Ok(repository);
        }
        let root = project.join(".loom").join("chat");
        for path in [
            project.join(".loom"),
            root.clone(),
            root.join("sessions"),
            root.join("v2"),
            root.join("v2/sessions"),
        ] {
            ensure_directory(&path)?;
        }
        let lock_path = root.join("writer.lock");
        validate_file(&lock_path)?;
        let writer = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|error| format!("failed to open chat writer lock: {error}"))?;
        writer
            .try_lock()
            .map_err(|error| format!("Chat is in use by another Loom process: {error}"))?;
        let store = Arc::new(ProjectStore {
            project: project.clone(),
            root,
            _writer: writer,
            active: Mutex::new(HashMap::new()),
        });
        stores.insert(project, Arc::downgrade(&store));
        Ok(Self(store))
    }

    pub(super) fn project_path(&self) -> &Path {
        &self.0.project
    }

    fn validate_layout(&self) -> Result<(), String> {
        for path in [
            self.0.project.join(".loom"),
            self.0.root.clone(),
            self.0.root.join("sessions"),
            self.0.root.join("v2"),
            self.0.root.join("v2/sessions"),
        ] {
            let meta = fs::symlink_metadata(&path)
                .map_err(|error| format!("chat directory unavailable: {error}"))?;
            if meta.file_type().is_symlink() || !meta.is_dir() {
                return Err(format!(
                    "chat directory must not be a symlink: {}",
                    path.display()
                ));
            }
        }
        Ok(())
    }

    fn session_directory(&self, id: &str, create: bool) -> Result<PathBuf, String> {
        validate_id(id)?;
        self.validate_layout()?;
        let directory = self.0.root.join("v2/sessions").join(id);
        if create {
            ensure_directory(&directory)?;
        } else if let Ok(meta) = fs::symlink_metadata(&directory) {
            if meta.file_type().is_symlink() || !meta.is_dir() {
                return Err("chat session directory must not be a symlink".into());
            }
        }
        Ok(directory)
    }

    #[cfg(test)]
    fn session_path(&self, id: &str) -> Result<PathBuf, String> {
        let path = self.session_directory(id, false)?.join("snapshot.json");
        validate_file(&path)?;
        Ok(path)
    }

    fn read_projection(&self, id: &str) -> Result<ChatSession, String> {
        let directory = self.session_directory(id, false)?;
        let journal_path = directory.join("events.jsonl");
        validate_file(&journal_path)?;
        if journal_path.exists()
            && fs::metadata(&journal_path)
                .map_err(|e| e.to_string())?
                .len()
                > 0
        {
            match journal::load(&directory, &self.0.project.to_string_lossy(), id) {
                Ok(session) => return Ok(session),
                Err(error) => {
                    if fs::metadata(&journal_path)
                        .map_err(|e| e.to_string())?
                        .len()
                        > 0
                        || directory.join("snapshot.json").exists()
                    {
                        return Err(error);
                    }
                }
            }
        }
        // A directory or cache alone is not a migration-complete marker. Only a
        // committed creation record is authoritative; retry partial migration.
        let legacy_path = self.0.root.join("sessions").join(format!("{id}.json"));
        validate_file(&legacy_path)?;
        let raw = fs::read_to_string(&legacy_path)
            .map_err(|error| format!("failed to read chat session: {error}"))?;
        let mut session: ChatSession = serde_json::from_str(&raw)
            .map_err(|error| format!("invalid legacy chat session: {error}"))?;
        if session.schema_version != 1 {
            return Err(format!(
                "unsupported legacy chat schema {}",
                session.schema_version
            ));
        }
        session.schema_version = CHAT_SCHEMA_VERSION;
        self.validate_session(&session, id)?;
        session.project_path = self.0.project.to_string_lossy().into_owned();
        session.permission_mode = normalize_permission_mode(&session.permission_mode);
        if session.status != "active" && session.status != "archived" {
            session.status = DEFAULT_SESSION_STATUS.to_string();
        }
        let directory = self.session_directory(id, true)?;
        journal::initialize(&directory, &mut session)?;
        let marker = directory.join("migration.json");
        validate_file(&marker)?;
        storage::atomic_write_json(
            &marker,
            &serde_json::json!({"schemaVersion": 2, "sourceSchemaVersion": 1, "sessionId": id, "completedAtMs": now_ms()}),
        )?;
        Ok(session)
    }

    fn validate_session(&self, session: &ChatSession, expected_id: &str) -> Result<(), String> {
        if session.id != expected_id {
            return Err("chat session id does not match its file".into());
        }
        if session.schema_version != CHAT_SCHEMA_VERSION {
            return Err(format!(
                "unsupported chat schema version {}",
                session.schema_version
            ));
        }
        let cwd = canonical_project_path(&session.project_path)?;
        if cwd != self.0.project {
            return Err("chat session project does not match the requested project".into());
        }
        Ok(())
    }

    fn load_locked(
        &self,
        id: &str,
        active: &HashMap<String, ActiveTurn>,
    ) -> Result<ChatSession, String> {
        let mut session = self.read_projection(id)?;
        self.validate_session(&session, id)?;
        let before = session.clone();
        if let Some(turn) = active.get(id) {
            if session.last_seq < turn.projection.last_seq {
                return Err("active chat journal lost committed events".into());
            }
            if session.active_turn_id.as_deref() != Some(&turn.id) {
                return Err("persisted chat turn conflicts with the live turn".into());
            }
        } else if reconcile_interrupted_session(&mut session) {
            self.save_locked(&before, &mut session)?;
        }
        Ok(session)
    }

    fn save_locked(
        &self,
        before: &ChatSession,
        session: &mut ChatSession,
    ) -> Result<ChatEvent, String> {
        self.validate_session(session, &session.id)?;
        journal::commit(
            &self.session_directory(&session.id, false)?,
            before,
            session,
        )
    }

    pub(super) fn load(&self, id: &str) -> Result<ChatSession, String> {
        let active = self
            .0
            .active
            .lock()
            .map_err(|_| "chat store lock unavailable")?;
        self.load_locked(id, &active)
    }

    pub(super) fn update(
        &self,
        id: &str,
        change: impl FnOnce(&mut ChatSession) -> Result<(), String>,
    ) -> Result<ChatSession, String> {
        let mut active = self
            .0
            .active
            .lock()
            .map_err(|_| "chat store lock unavailable")?;
        let mut session = self.load_locked(id, &active)?;
        let before = session.clone();
        change(&mut session)?;
        session.updated_at_ms = now_ms();
        self.save_locked(&before, &mut session)?;
        if let Some(turn) = active.get_mut(id) {
            turn.projection = session.clone();
            turn.event_index = None;
        }
        Ok(session)
    }

    pub(super) fn create(&self, session: &ChatSession) -> Result<(), String> {
        let _guard = self
            .0
            .active
            .lock()
            .map_err(|_| "chat store lock unavailable")?;
        self.validate_session(session, &session.id)?;
        let directory = self.session_directory(&session.id, true)?;
        let legacy = self
            .0
            .root
            .join("sessions")
            .join(format!("{}.json", session.id));
        if legacy.exists() {
            return Err("chat session already exists".into());
        }
        let mut session = session.clone();
        journal::initialize(&directory, &mut session)?;
        let path = self.0.root.join("v2/index.json");
        validate_file(&path)?;
        storage::atomic_write_json(
            &path,
            &ChatIndex {
                schema_version: CHAT_SCHEMA_VERSION,
                session_ids: self.session_ids()?,
            },
        )
    }

    fn session_ids(&self) -> Result<Vec<String>, String> {
        self.validate_layout()?;
        let mut ids = std::collections::BTreeSet::new();
        for (directory, legacy) in [
            (self.0.root.join("sessions"), true),
            (self.0.root.join("v2/sessions"), false),
        ] {
            for entry in fs::read_dir(directory).map_err(|e| e.to_string())? {
                let path = entry.map_err(|e| e.to_string())?.path();
                let id = if legacy && path.extension().is_some_and(|ext| ext == "json") {
                    path.file_stem()
                } else if !legacy {
                    path.file_name()
                } else {
                    None
                };
                if let Some(id) = id
                    .and_then(|value| value.to_str())
                    .filter(|id| validate_id(id).is_ok())
                {
                    ids.insert(id.to_string());
                }
            }
        }
        Ok(ids.into_iter().collect())
    }

    pub(super) fn append_stream(
        &self,
        lease: &TurnLease,
        stream: ChatStreamEvent,
    ) -> Result<ChatEvent, String> {
        let mut active = self
            .0
            .active
            .lock()
            .map_err(|_| "chat store lock unavailable")?;
        let turn = active
            .get_mut(&lease.session_id)
            .filter(|turn| turn.id == lease.turn_id)
            .ok_or("chat turn is no longer active")?;
        let directory = self.session_directory(&lease.session_id, false)?;
        if turn
            .event_index
            .as_ref()
            .is_some_and(|index| !index.matches(&directory))
        {
            turn.event_index = None;
            // Revalidate external changes before appending to the journal.
            let session = self.read_projection(&lease.session_id)?;
            if session.last_seq != turn.projection.last_seq {
                return Err("active chat journal changed outside its writer".into());
            }
        }
        match journal::stream(&directory, &mut turn.projection, stream) {
            Ok(event) => {
                if turn
                    .event_index
                    .as_mut()
                    .is_some_and(|index| index.appended(&directory, &event).is_err())
                {
                    turn.event_index = None;
                }
                Ok(event)
            }
            Err(error) => {
                turn.event_index = None;
                Err(error)
            }
        }
    }

    pub(super) fn read_events(
        &self,
        id: &str,
        after_seq: u64,
        limit: usize,
    ) -> Result<ChatEventPage, String> {
        let mut active = self
            .0
            .active
            .lock()
            .map_err(|_| "chat store lock unavailable")?;
        let directory = self.session_directory(id, false)?;
        if active.contains_key(id) {
            let valid_index = active[id]
                .event_index
                .as_ref()
                .is_some_and(|index| index.matches(&directory));
            if !valid_index {
                let session = self.load_locked(id, &active)?;
                active
                    .get_mut(id)
                    .expect("active turn held under lock")
                    .event_index = Some(journal::EventIndex::build(&directory, &session)?);
            }
            return active[id]
                .event_index
                .as_ref()
                .expect("validated index")
                .events(
                    &directory,
                    &self.0.project.to_string_lossy(),
                    id,
                    after_seq,
                    limit,
                );
        }
        self.load_locked(id, &active)?;
        journal::events(
            &directory,
            &self.0.project.to_string_lossy(),
            id,
            after_seq,
            limit,
        )
    }

    pub(super) fn event_at(&self, session: &ChatSession) -> Result<ChatEvent, String> {
        self.read_events(&session.id, session.last_seq.saturating_sub(1), 1)?
            .events
            .into_iter()
            .next()
            .ok_or_else(|| "committed chat event missing".into())
    }

    pub(super) fn list_entries(&self) -> Result<Vec<SessionReadEntry>, String> {
        let active = self
            .0
            .active
            .lock()
            .map_err(|_| "chat store lock unavailable")?;
        Ok(self
            .session_ids()?
            .into_iter()
            .map(|id| {
                let result = self.load_locked(&id, &active);
                (id, result)
            })
            .collect())
    }

    #[cfg(test)]
    fn list(&self) -> Result<Vec<ChatSession>, String> {
        Ok(self
            .list_entries()?
            .into_iter()
            .filter_map(|(_, result)| result.ok())
            .collect())
    }

    #[cfg(test)]
    pub(super) fn begin<T>(
        &self,
        id: &str,
        turn_id: &str,
        prepare: impl FnOnce(&mut ChatSession) -> Result<T, String>,
    ) -> Result<(ChatSession, T, TurnLease), String> {
        match self.begin_locked(id, turn_id, None, prepare)? {
            BeginTurn::Started(session, prepared, lease) => Ok((session, prepared, lease)),
            BeginTurn::Existing(_) => unreachable!("no request key supplied"),
        }
    }

    pub(super) fn begin_once<T>(
        &self,
        input: &ChatSendInput,
        turn_id: &str,
        prepare: impl FnOnce(&mut ChatSession) -> Result<T, String>,
    ) -> Result<BeginTurn<T>, String> {
        validate_id(&input.client_request_id).map_err(|_| {
            "invalid clientRequestId: use 1–160 letters, digits, hyphens or underscores"
        })?;
        if input.text.trim().is_empty() {
            return Err("message text is required".into());
        }
        self.begin_locked(&input.session_id, turn_id, Some(input), prepare)
    }

    fn begin_locked<T>(
        &self,
        id: &str,
        turn_id: &str,
        request: Option<&ChatSendInput>,
        prepare: impl FnOnce(&mut ChatSession) -> Result<T, String>,
    ) -> Result<BeginTurn<T>, String> {
        validate_id(turn_id)?;
        let mut active = self
            .0
            .active
            .lock()
            .map_err(|_| "chat store lock unavailable")?;
        let mut session = self.load_locked(id, &active)?;
        // Check the durable receipt before any availability/configuration check
        // or preparation. A retry must never start another side-effecting run.
        if let Some(request) = request {
            if let Some(receipt) = session
                .send_receipts
                .iter()
                .find(|receipt| receipt.client_request_id == request.client_request_id)
            {
                let user = session
                    .messages
                    .iter()
                    .find(|message| message.id == receipt.user_message_id && message.role == "user")
                    .ok_or("chat receipt user message is missing")?;
                if user.content != request.text
                    || receipt.requested_permission_mode != request.permission_mode
                {
                    return Err("clientRequestId conflict: this request was already accepted with different content or permissions".into());
                }
                return Ok(BeginTurn::Existing(ChatSendResult {
                    turn_id: receipt.turn_id.clone(),
                    session,
                }));
            }
        }
        if active.contains_key(id) {
            return Err("a chat turn is already running for this session".into());
        }
        if crate::process_supervisor::supervisor().is_shutting_down() {
            return Err("Loom 正在退出，不能开始新回合。".into());
        }
        if session.status == "archived" {
            return Err("restore the archived chat before sending".into());
        }
        let before = session.clone();
        let prepared = prepare(&mut session)?;
        if let Some(request) = request {
            let added = &session.messages[before.messages.len()..];
            if added.len() != 2
                || added[0].role != "user"
                || added[1].role != "assistant"
                || added[0].content != request.text
            {
                return Err("chat acceptance must append the requested user message and assistant placeholder".into());
            }
            session.send_receipts.push(ChatSendReceipt {
                client_request_id: request.client_request_id.clone(),
                turn_id: turn_id.into(),
                user_message_id: added[0].id.clone(),
                assistant_message_id: added[1].id.clone(),
                requested_permission_mode: request.permission_mode.clone(),
            });
        }
        session.turn_status = "streaming".into();
        session.active_turn_id = Some(turn_id.into());
        session.updated_at_ms = now_ms();
        self.save_locked(&before, &mut session)?;
        let cancelled = Arc::new(AtomicU8::new(0));
        active.insert(
            id.into(),
            ActiveTurn {
                id: turn_id.into(),
                cancelled: cancelled.clone(),
                projection: session.clone(),
                event_index: None,
            },
        );
        let lease = TurnLease {
            repository: self.clone(),
            session_id: id.into(),
            turn_id: turn_id.into(),
            cancelled,
        };
        Ok(BeginTurn::Started(session, prepared, lease))
    }

    pub(super) fn finish(
        &self,
        lease: &TurnLease,
        change: impl FnOnce(&mut ChatSession) -> Result<(), String>,
    ) -> Result<ChatSession, String> {
        let mut active = self
            .0
            .active
            .lock()
            .map_err(|_| "chat store lock unavailable")?;
        if active
            .get(&lease.session_id)
            .is_none_or(|turn| turn.id != lease.turn_id)
        {
            return Err("chat turn is no longer active".into());
        }
        let mut session = self.load_locked(&lease.session_id, &active)?;
        let before = session.clone();
        change(&mut session)?;
        session.active_turn_id = None;
        session.turn_status = "idle".into();
        session.updated_at_ms = now_ms();
        self.save_locked(&before, &mut session)?;
        active.remove(&lease.session_id);
        Ok(session)
    }

    pub(super) fn cancel(
        &self,
        id: &str,
        expected_turn: Option<&str>,
    ) -> Result<Option<String>, String> {
        self.cancel_with_reason(id, expected_turn, 1)
    }
    pub(super) fn cancel_for_shutdown(&self, id: &str) -> Result<Option<String>, String> {
        self.cancel_with_reason(id, None, 2)
    }
    fn cancel_with_reason(
        &self,
        id: &str,
        expected_turn: Option<&str>,
        reason: u8,
    ) -> Result<Option<String>, String> {
        validate_id(id)?;
        if let Some(expected) = expected_turn {
            validate_id(expected)?;
        }
        let mut active = self
            .0
            .active
            .lock()
            .map_err(|_| "chat store lock unavailable")?;
        if let Some(turn) = active.get(id) {
            if expected_turn.is_some_and(|expected| expected != turn.id) {
                return Err("chat abort turn does not match the active turn".into());
            }
            if turn
                .cancelled
                .compare_exchange(0, reason, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
            {
                return Ok(Some(turn.id.clone()));
            }
            let turn_id = turn.id.clone();
            // Signal first even if the disk became unavailable. Failure to save
            // the cancellation state must not prevent stopping the child.
            let mut session = self.load_locked(id, &active)?;
            let before = session.clone();
            if let Some(record) = session.turns.iter_mut().find(|turn| turn.id == turn_id) {
                if !record.status.is_terminal() {
                    record.status = super::ChatTurnStatus::Cancelling;
                }
                session.updated_at_ms = now_ms();
                self.save_locked(&before, &mut session)?;
                active
                    .get_mut(id)
                    .expect("active turn held under lock")
                    .projection = session;
                active
                    .get_mut(id)
                    .expect("active turn held under lock")
                    .event_index = None;
            }
            return Ok(Some(turn_id));
        }
        self.load_locked(id, &active)?;
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{chat::ChatMessage, models::IdGenerator};

    struct Fixture {
        path: PathBuf,
        repository: Option<ChatRepository>,
    }

    impl Fixture {
        fn new() -> Self {
            static IDS: OnceLock<IdGenerator> = OnceLock::new();
            let path = std::env::temp_dir().join(format!(
                "loom-chat-repository-{}-{}",
                std::process::id(),
                IDS.get_or_init(IdGenerator::default).next("test")
            ));
            fs::create_dir_all(&path).unwrap();
            let repository = ChatRepository::open(&path).unwrap();
            Self {
                path,
                repository: Some(repository),
            }
        }
        fn repo(&self) -> &ChatRepository {
            self.repository.as_ref().unwrap()
        }
        fn session(&self, id: &str) -> ChatSession {
            ChatSession {
                id: id.into(),
                project_path: self.repo().project_path().to_string_lossy().into_owned(),
                agent_id: "agent-test".into(),
                title: "新对话".into(),
                permission_mode: "explore".into(),
                messages: Vec::new(),
                send_receipts: Vec::new(),
                turns: Vec::new(),
                created_at_ms: now_ms(),
                updated_at_ms: now_ms(),
                resume_command: None,
                resume_handle: None,
                active_turn_id: None,
                turn_status: "idle".into(),
                promoted_task_id: None,
                status: "active".into(),
                flagged: false,
                schema_version: 2,
                last_seq: 0,
                revision: 0,
            }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            self.repository.take();
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn start(repo: &ChatRepository, id: &str, turn: &str) -> TurnLease {
        repo.begin(id, turn, |session| {
            session.messages.push(ChatMessage {
                id: format!("message-{turn}"),
                role: "assistant".into(),
                content: "partial".into(),
                status: "streaming".into(),
                created_at_ms: now_ms(),
                error_summary: None,
                parts: Vec::new(),
            });
            Ok(())
        })
        .unwrap()
        .2
    }

    fn request(f: &Fixture, id: &str) -> ChatSendInput {
        ChatSendInput {
            project_path: f.path.to_string_lossy().into_owned(),
            session_id: id.into(),
            client_request_id: "request-1".into(),
            text: "  keep this text exactly\n中文  ".into(),
            permission_mode: Some("explore".into()),
        }
    }

    fn accept(repo: &ChatRepository, input: &ChatSendInput, turn: &str) -> BeginTurn<()> {
        repo.begin_once(input, turn, |session| {
            for role in ["user", "assistant"] {
                session.messages.push(ChatMessage {
                    id: format!("{turn}-{role}"),
                    role: role.into(),
                    content: if role == "user" {
                        input.text.clone()
                    } else {
                        String::new()
                    },
                    status: if role == "user" {
                        "complete"
                    } else {
                        "streaming"
                    }
                    .into(),
                    created_at_ms: now_ms(),
                    error_summary: None,
                    parts: Vec::new(),
                });
            }
            Ok(())
        })
        .unwrap()
    }

    fn assert_receipt(repo: &ChatRepository, input: &ChatSendInput, turn: &str) -> ChatSession {
        match repo
            .begin_once::<()>(input, "unused-turn", |_| {
                panic!("retry prepared another invocation")
            })
            .unwrap()
        {
            BeginTurn::Existing(result) => {
                assert_eq!(result.turn_id, turn);
                assert_eq!(result.session.send_receipts.len(), 1);
                assert_eq!(result.session.messages.len(), 2);
                result.session
            }
            BeginTurn::Started(..) => panic!("retry started another run"),
        }
    }

    #[test]
    fn send_receipt_survives_completion_archive_config_change_and_repository_reopen() {
        let mut f = Fixture::new();
        f.repo().create(&f.session("session-a")).unwrap();
        let input = request(&f, "session-a");
        let BeginTurn::Started(accepted, (), lease) = accept(f.repo(), &input, "turn-once") else {
            panic!()
        };
        assert_eq!(accepted.messages[0].content, input.text);
        assert_eq!(accepted.last_seq, 2);
        let retry = assert_receipt(f.repo(), &input, "turn-once");
        assert_eq!(retry.last_seq, accepted.last_seq);
        f.repo()
            .finish(&lease, |s| {
                s.messages[1].status = "complete".into();
                Ok(())
            })
            .unwrap();
        drop(lease);
        f.repo()
            .update("session-a", |s| {
                s.title = "renamed".into();
                s.status = "archived".into();
                s.agent_id = "other".into();
                s.permission_mode = "auto".into();
                Ok(())
            })
            .unwrap();
        f.repository.take();
        f.repository = Some(ChatRepository::open(&f.path).unwrap());
        let retry = assert_receipt(f.repo(), &input, "turn-once");
        assert_eq!(retry.title, "renamed");
        assert_eq!(retry.status, "archived");
        assert_eq!(retry.messages[1].status, "complete");
        let events = f.repo().read_events("session-a", 0, 100).unwrap();
        assert_eq!(
            events.events.len(),
            4,
            "retry must not append a journal event"
        );
    }

    #[test]
    fn concurrent_duplicate_sends_only_prepare_once() {
        let f = Fixture::new();
        f.repo().create(&f.session("session-a")).unwrap();
        let barrier = std::sync::Barrier::new(8);
        let results = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..8)
                .map(|i| {
                    let input = request(&f, "session-a");
                    let repo = f.repo();
                    let barrier = &barrier;
                    scope.spawn(move || {
                        barrier.wait();
                        accept(repo, &input, &format!("turn-{i}"))
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|h| h.join().unwrap())
                .collect::<Vec<_>>()
        });
        assert_eq!(
            results
                .iter()
                .filter(|r| matches!(r, BeginTurn::Started(..)))
                .count(),
            1
        );
        let ids: std::collections::HashSet<_> = results
            .iter()
            .map(|result| match result {
                BeginTurn::Started(_, _, lease) => lease.turn_id.as_str(),
                BeginTurn::Existing(result) => result.turn_id.as_str(),
            })
            .collect();
        assert_eq!(ids.len(), 1);
        assert_eq!(f.repo().load("session-a").unwrap().last_seq, 2);
    }

    #[test]
    fn interrupted_accepted_send_is_returned_and_never_rerun() {
        let mut f = Fixture::new();
        f.repo().create(&f.session("session-a")).unwrap();
        let input = request(&f, "session-a");
        drop(accept(f.repo(), &input, "turn-interrupted"));
        f.repository.take();
        f.repository = Some(ChatRepository::open(&f.path).unwrap());
        let retry = assert_receipt(f.repo(), &input, "turn-interrupted");
        assert_eq!(retry.turn_status, "idle");
        assert_eq!(retry.messages[1].status, "aborted");
        assert_eq!(
            retry.messages[1].error_summary.as_deref(),
            Some("interrupted_by_restart")
        );
    }

    #[test]
    fn send_request_conflicts_and_new_request_ids_do_not_bypass_active_turn_lock() {
        let f = Fixture::new();
        f.repo().create(&f.session("session-a")).unwrap();
        let input = request(&f, "session-a");
        let _running = accept(f.repo(), &input, "turn-a");
        for changed in [
            ChatSendInput {
                text: "different".into(),
                ..input.clone()
            },
            ChatSendInput {
                permission_mode: Some("auto".into()),
                ..input.clone()
            },
        ] {
            let error = f
                .repo()
                .begin_once::<()>(&changed, "turn-b", |_| panic!())
                .err()
                .unwrap();
            assert!(error.contains("clientRequestId conflict"), "{error}");
        }
        let another = ChatSendInput {
            client_request_id: "request-2".into(),
            ..input.clone()
        };
        assert!(f
            .repo()
            .begin_once::<()>(&another, "turn-b", |_| panic!())
            .is_err());
        f.repo().create(&f.session("session-b")).unwrap();
        let other_session = ChatSendInput {
            session_id: "session-b".into(),
            ..input
        };
        assert!(matches!(
            accept(f.repo(), &other_session, "turn-b"),
            BeginTurn::Started(..)
        ));
    }

    #[test]
    fn failed_acceptance_does_not_leave_a_receipt_or_lease() {
        let f = Fixture::new();
        f.repo().create(&f.session("session-a")).unwrap();
        let input = request(&f, "session-a");
        assert!(f
            .repo()
            .begin_once::<()>(&input, "turn-fail", |_| Err("configuration invalid".into()))
            .is_err());
        assert!(f.repo().load("session-a").unwrap().send_receipts.is_empty());
        let accepted = accept(f.repo(), &input, "turn-success");
        assert!(matches!(accepted, BeginTurn::Started(..)));
        assert!(f
            .repo()
            .update("session-a", |s| {
                s.send_receipts.clear();
                Ok(())
            })
            .is_err());
        assert!(f
            .repo()
            .update("session-a", |s| {
                s.messages[0].content = "altered".into();
                Ok(())
            })
            .is_err());
        assert_eq!(f.repo().load("session-a").unwrap().last_seq, 2);
    }

    #[test]
    fn journal_write_failure_cannot_acknowledge_or_reserve_a_request() {
        let f = Fixture::new();
        f.repo().create(&f.session("session-a")).unwrap();
        let input = request(&f, "session-a");
        let log = f
            .repo()
            .session_directory("session-a", false)
            .unwrap()
            .join("events.jsonl");
        let backup = log.with_extension("backup");
        let error = f
            .repo()
            .begin_once(&input, "turn-fail", |session| {
                for role in ["user", "assistant"] {
                    session.messages.push(ChatMessage {
                        id: role.into(),
                        role: role.into(),
                        content: input.text.clone(),
                        status: "streaming".into(),
                        created_at_ms: now_ms(),
                        error_summary: None,
                        parts: vec![],
                    });
                }
                // Make the authoritative append fail after preparation.
                fs::rename(&log, &backup).unwrap();
                fs::create_dir(&log).unwrap();
                Ok(())
            })
            .err()
            .unwrap();
        assert!(error.contains("regular file"), "{error}");
        fs::remove_dir(&log).unwrap();
        fs::rename(&backup, &log).unwrap();
        let session = f.repo().load("session-a").unwrap();
        assert!(session.send_receipts.is_empty());
        assert!(session.messages.is_empty());
        assert_eq!(session.last_seq, 1);
        assert!(matches!(
            accept(f.repo(), &input, "turn-retry"),
            BeginTurn::Started(..)
        ));
    }

    #[test]
    fn receipt_is_preserved_after_process_exit_without_destructors() {
        const ENV: &str = "LOOM_RECEIPT_CRASH_TEST_PROJECT";
        if let Ok(path) = std::env::var(ENV) {
            let repo = ChatRepository::open(Path::new(&path)).unwrap();
            let input = ChatSendInput {
                project_path: path,
                session_id: "session-a".into(),
                client_request_id: "request-crash".into(),
                text: "once".into(),
                permission_mode: None,
            };
            let _accepted = accept(&repo, &input, "turn-crash");
            std::process::exit(0);
        }
        let mut f = Fixture::new();
        f.repo().create(&f.session("session-a")).unwrap();
        f.repository.take();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "chat::repository::tests::receipt_is_preserved_after_process_exit_without_destructors", "--nocapture"])
            .env(ENV, &f.path).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        f.repository = Some(ChatRepository::open(&f.path).unwrap());
        let input = ChatSendInput {
            client_request_id: "request-crash".into(),
            text: "once".into(),
            permission_mode: None,
            ..request(&f, "session-a")
        };
        let session = assert_receipt(f.repo(), &input, "turn-crash");
        assert_eq!(session.messages[1].status, "aborted");
    }

    #[test]
    fn request_ids_are_bounded_and_scoped_to_each_project() {
        let a = Fixture::new();
        let b = Fixture::new();
        for f in [&a, &b] {
            f.repo().create(&f.session("session-a")).unwrap();
            let mut input = request(f, "session-a");
            for invalid in [String::new(), "../bad".into(), "x".repeat(161)] {
                input.client_request_id = invalid;
                assert!(f
                    .repo()
                    .begin_once::<()>(&input, "turn-invalid", |_| panic!())
                    .is_err());
            }
        }
        let _a = accept(a.repo(), &request(&a, "session-a"), "turn-a");
        let _b = accept(b.repo(), &request(&b, "session-a"), "turn-b");
        assert_receipt(a.repo(), &request(&a, "session-a"), "turn-a");
        assert_receipt(b.repo(), &request(&b, "session-a"), "turn-b");
    }

    #[test]
    fn reading_and_listing_a_live_turn_do_not_reconcile_it_as_a_restart() {
        let f = Fixture::new();
        f.repo().create(&f.session("session-a")).unwrap();
        let lease = start(f.repo(), "session-a", "turn-a");
        let reopened = ChatRepository::open(&f.path).unwrap();
        for session in [
            reopened.load("session-a").unwrap(),
            reopened.list().unwrap().remove(0),
        ] {
            assert_eq!(session.turn_status, "streaming");
            assert_eq!(session.active_turn_id.as_deref(), Some("turn-a"));
            assert_eq!(session.messages[0].status, "streaming");
            assert!(!session.flagged);
        }
        assert!(reopened.begin("session-a", "turn-b", |_| Ok(())).is_err());
        assert!(!lease.is_cancelled());
    }

    #[test]
    fn abandoned_turn_recovers_partial_output_once_and_can_run_again() {
        let f = Fixture::new();
        f.repo().create(&f.session("session-a")).unwrap();
        drop(start(f.repo(), "session-a", "turn-a"));
        let recovered = f.repo().load("session-a").unwrap();
        assert_eq!(recovered.turn_status, "idle");
        assert!(recovered.active_turn_id.is_none());
        assert_eq!(recovered.messages[0].content, "partial");
        assert_eq!(recovered.messages[0].status, "aborted");
        assert_eq!(
            recovered.messages[0].error_summary.as_deref(),
            Some("interrupted_by_restart")
        );
        let again = f.repo().load("session-a").unwrap();
        assert_eq!(again.updated_at_ms, recovered.updated_at_ms);
        let _lease = start(f.repo(), "session-a", "turn-b");
    }

    #[test]
    fn cancel_is_scoped_to_project_session_and_turn_even_before_spawn() {
        let a = Fixture::new();
        let b = Fixture::new();
        for f in [&a, &b] {
            f.repo().create(&f.session("same-id")).unwrap();
        }
        let lease_a = start(a.repo(), "same-id", "turn-a");
        let lease_b = start(b.repo(), "same-id", "turn-b");
        assert!(a.repo().cancel("same-id", Some("turn-b")).is_err());
        assert!(!lease_a.is_cancelled());
        assert!(!lease_b.is_cancelled());
        assert_eq!(
            a.repo()
                .cancel("same-id", Some("turn-a"))
                .unwrap()
                .as_deref(),
            Some("turn-a")
        );
        assert!(lease_a.is_cancelled());
        assert!(!lease_b.is_cancelled());
    }

    #[test]
    fn finish_preserves_concurrent_title_edits_and_stale_finish_cannot_end_next_turn() {
        let f = Fixture::new();
        f.repo().create(&f.session("session-a")).unwrap();
        let old = start(f.repo(), "session-a", "turn-a");
        f.repo()
            .update("session-a", |session| {
                session.title = "用户更新标题".into();
                Ok(())
            })
            .unwrap();
        f.repo()
            .finish(&old, |session| {
                session.messages[0].content = "finished".into();
                session.messages[0].status = "complete".into();
                Ok(())
            })
            .unwrap();
        let finished = f.repo().load("session-a").unwrap();
        assert_eq!(finished.title, "用户更新标题");
        assert_eq!(finished.turn_status, "idle");
        let current = start(f.repo(), "session-a", "turn-b");
        assert!(f.repo().finish(&old, |_| Ok(())).is_err());
        drop(old);
        assert_eq!(
            f.repo()
                .load("session-a")
                .unwrap()
                .active_turn_id
                .as_deref(),
            Some(current.turn_id.as_str())
        );
    }

    #[test]
    fn active_config_changes_are_rejected_using_the_actual_command_helpers() {
        let f = Fixture::new();
        f.repo().create(&f.session("session-a")).unwrap();
        let _lease = start(f.repo(), "session-a", "turn-a");
        assert!(f
            .repo()
            .update("session-a", |session| {
                crate::chat_context::update_execution_config(session, Some("another"), None)
            })
            .is_err());
        assert!(f
            .repo()
            .update("session-a", |session| {
                crate::chat_context::update_execution_config(session, None, Some("auto"))
            })
            .is_err());
        assert!(f
            .repo()
            .update("session-a", |session| super::super::require_idle(session))
            .is_err());
        assert_eq!(f.repo().load("session-a").unwrap().agent_id, "agent-test");
    }

    #[test]
    fn concurrent_create_and_updates_do_not_lose_sessions_or_mutations() {
        let f = Fixture::new();
        let mut workers = Vec::new();
        for n in 0..12 {
            let repo = f.repo().clone();
            let session = f.session(&format!("session-{n}"));
            workers.push(std::thread::spawn(move || repo.create(&session).unwrap()));
        }
        for worker in workers {
            worker.join().unwrap();
        }
        let index: ChatIndex = serde_json::from_str(
            &fs::read_to_string(f.repo().0.root.join("v2/index.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(index.session_ids.len(), 12);
        assert_eq!(f.repo().list().unwrap().len(), 12);
        let initial = f.repo().load("session-0").unwrap().title.len();
        let mut workers = Vec::new();
        for _ in 0..12 {
            let repo = f.repo().clone();
            workers.push(std::thread::spawn(move || {
                repo.update("session-0", |session| {
                    session.title.push('x');
                    Ok(())
                })
                .unwrap()
            }));
        }
        for worker in workers {
            worker.join().unwrap();
        }
        assert_eq!(
            f.repo().load("session-0").unwrap().title.len(),
            initial + 12
        );
    }

    #[test]
    fn missing_or_corrupt_index_does_not_hide_authoritative_sessions() {
        let f = Fixture::new();
        f.repo().create(&f.session("session-a")).unwrap();
        fs::write(f.repo().0.root.join("v2/index.json"), "broken").unwrap();
        assert_eq!(f.repo().list().unwrap().len(), 1);
        f.repo().create(&f.session("session-b")).unwrap();
        let index: ChatIndex = serde_json::from_str(
            &fs::read_to_string(f.repo().0.root.join("v2/index.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(index.session_ids, ["session-a", "session-b"]);
    }

    #[test]
    fn invalid_ids_forged_cwd_and_future_schema_are_rejected_without_overwriting() {
        let f = Fixture::new();
        let other = Fixture::new();
        for id in ["../escape", "/absolute", "a/b", "a\\b", "", ".", ".."] {
            assert!(f.repo().create(&f.session(id)).is_err());
            assert!(f.repo().load(id).is_err());
        }
        f.repo().create(&f.session("session-a")).unwrap();
        let path = f.repo().session_path("session-a").unwrap();
        for change in [0, 1, 2] {
            let mut session = f.session("session-a");
            match change {
                0 => session.project_path = other.path.to_string_lossy().into_owned(),
                1 => session.schema_version = 99,
                _ => session.id = "different-id".into(),
            }
            let raw = serde_json::to_string(&session).unwrap();
            fs::write(&path, &raw).unwrap();
            assert!(f.repo().load("session-a").is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), raw);
        }
    }

    #[test]
    fn archived_sessions_require_restore_and_failed_prepare_has_no_side_effects() {
        let f = Fixture::new();
        let mut session = f.session("session-a");
        session.status = "archived".into();
        f.repo().create(&session).unwrap();
        assert!(f.repo().begin("session-a", "turn-a", |_| Ok(())).is_err());
        f.repo()
            .update("session-a", |session| {
                session.status = "active".into();
                Ok(())
            })
            .unwrap();
        assert!(f
            .repo()
            .begin::<()>("session-a", "turn-a", |session| {
                session.title = "not saved".into();
                Err("unavailable CLI".into())
            })
            .is_err());
        let loaded = f.repo().load("session-a").unwrap();
        assert_eq!(loaded.title, "新对话");
        assert_eq!(loaded.turn_status, "idle");
        let _lease = start(f.repo(), "session-a", "turn-a");
    }

    #[cfg(unix)]
    #[test]
    fn symlink_directories_and_files_are_rejected_without_touching_target() {
        use std::os::unix::fs::symlink;
        let f = Fixture::new();
        let other = Fixture::new();
        other.repo().create(&other.session("original")).unwrap();
        let target = other.repo().session_path("original").unwrap();
        let before = fs::read(&target).unwrap();
        symlink(
            &target,
            f.repo()
                .session_directory("session-a", true)
                .unwrap()
                .join("snapshot.json"),
        )
        .unwrap();
        assert!(f.repo().load("session-a").is_err());
        assert!(f.repo().create(&f.session("session-a")).is_err());
        assert_eq!(fs::read(&target).unwrap(), before);
        let outside = f.path.join("nested");
        fs::create_dir(&outside).unwrap();
        symlink(other.path.join(".loom"), outside.join(".loom")).unwrap();
        assert!(ChatRepository::open(&outside).is_err());
    }

    #[test]
    fn operating_system_writer_lock_releases_when_last_owner_exits() {
        let mut f = Fixture::new();
        let inherited_descriptor = f.repo().0._writer.try_clone().unwrap();
        let probe = OpenOptions::new()
            .read(true)
            .write(true)
            .open(f.repo().0.root.join("writer.lock"))
            .unwrap();
        assert!(probe.try_lock().is_err());
        f.repository.take();
        probe.try_lock().unwrap();
        assert!(ChatRepository::open(&f.path).is_err());
        probe.unlock().unwrap();
        f.repository = Some(ChatRepository::open(&f.path).unwrap());
        drop(inherited_descriptor);
    }

    #[test]
    fn terminal_event_is_built_after_final_snapshot_is_readable() {
        let f = Fixture::new();
        let mut initial = f.session("session-a");
        initial.resume_command = Some("old-resume".into());
        f.repo().create(&initial).unwrap();
        let lease = start(f.repo(), "session-a", "turn-a");
        let event = super::super::finish_chat_turn(
            f.repo(),
            &lease,
            "message-turn-a",
            None,
            Ok(super::super::ChatTurnOutcome {
                content: "final output".into(),
                parts: Vec::new(),
                resume_command: Some("new-resume".into()),
                status: "complete".into(),
                error_summary: None,
                ..Default::default()
            }),
        );
        assert_eq!(event.status, "complete");
        let snapshot = f.repo().load("session-a").unwrap();
        assert_eq!(snapshot.messages[0].content, "final output");
        assert_eq!(snapshot.turn_status, "idle");
        assert_eq!(snapshot.resume_command.as_deref(), Some("new-resume"));
    }

    #[test]
    fn failed_completion_preserves_partial_text_and_prior_resume() {
        let f = Fixture::new();
        let mut initial = f.session("session-a");
        initial.resume_command = Some("old-resume".into());
        f.repo().create(&initial).unwrap();
        let lease = start(f.repo(), "session-a", "turn-a");
        let event = super::super::finish_chat_turn(
            f.repo(),
            &lease,
            "message-turn-a",
            None,
            Err("broken CLI".into()),
        );
        assert_eq!(event.status, "error");
        let snapshot = f.repo().load("session-a").unwrap();
        assert_eq!(snapshot.messages[0].content, "partial");
        assert_eq!(
            snapshot.messages[0].error_summary.as_deref(),
            Some("broken CLI")
        );
        assert_eq!(snapshot.resume_command.as_deref(), Some("old-resume"));
        assert!(snapshot.flagged);
    }

    #[test]
    fn storage_failure_never_emits_a_successful_completion() {
        let f = Fixture::new();
        f.repo().create(&f.session("session-a")).unwrap();
        let lease = start(f.repo(), "session-a", "turn-a");
        let path = f.repo().session_path("session-a").unwrap();
        // A future-version replacement simulates a concurrent external change
        // that the repository must refuse to overwrite, even at turn completion.
        let mut snapshot = f.repo().load("session-a").unwrap();
        snapshot.schema_version = 99;
        let raw = serde_json::to_string(&snapshot).unwrap();
        fs::write(&path, &raw).unwrap();
        let event = super::super::finish_chat_turn(
            f.repo(),
            &lease,
            "message-turn-a",
            None,
            Ok(super::super::ChatTurnOutcome {
                content: "must not overwrite".into(),
                parts: Vec::new(),
                resume_command: None,
                status: "complete".into(),
                error_summary: None,
                ..Default::default()
            }),
        );
        assert_eq!(event.status, "error");
        assert!(event.error_summary.unwrap().starts_with("storage_error:"));
        assert_eq!(fs::read_to_string(path).unwrap(), raw);
    }

    fn legacy_file(f: &Fixture, id: &str) -> (PathBuf, String) {
        let mut value = serde_json::to_value(f.session(id)).unwrap();
        value["schemaVersion"] = serde_json::json!(1);
        value["permissionMode"] = serde_json::json!("read_only");
        for key in ["lastSeq", "revision", "status", "flagged"] {
            value.as_object_mut().unwrap().remove(key);
        }
        let raw = serde_json::to_string_pretty(&value).unwrap();
        let path = f.repo().0.root.join("sessions").join(format!("{id}.json"));
        fs::write(&path, &raw).unwrap();
        (path, raw)
    }

    fn delta(lease: &TurnLease, text: &str) -> ChatStreamEvent {
        ChatStreamEvent {
            session_id: lease.session_id.clone(),
            turn_id: lease.turn_id.clone(),
            message_id: format!("message-{}", lease.turn_id),
            delta: text.into(),
            done: false,
            part: None,
        }
    }

    #[test]
    fn v1_migration_is_read_only_and_retries_an_incomplete_first_append() {
        let f = Fixture::new();
        let (source, raw) = legacy_file(&f, "legacy-a");
        let directory = f.repo().session_directory("legacy-a", true).unwrap();
        fs::write(directory.join("events.jsonl"), "{\"schemaVersion\":2").unwrap();
        let migrated = f.repo().load("legacy-a").unwrap();
        assert_eq!(migrated.schema_version, 2);
        assert_eq!(migrated.permission_mode, "explore");
        assert_eq!(migrated.last_seq, 1);
        assert_eq!(fs::read_to_string(&source).unwrap(), raw);
        assert!(directory.join("migration.json").is_file());
        f.repo()
            .update("legacy-a", |session| {
                session.title = "v2 title".into();
                Ok(())
            })
            .unwrap();
        let loaded = f.repo().load("legacy-a").unwrap();
        assert_eq!(loaded.title, "v2 title");
        assert_eq!(loaded.last_seq, 2);
        assert_eq!(fs::read_to_string(source).unwrap(), raw);
        assert_eq!(f.repo().list().unwrap().len(), 1);
    }

    #[test]
    fn journal_replays_streamed_text_and_tools_when_snapshot_is_missing() {
        let f = Fixture::new();
        f.repo().create(&f.session("session-a")).unwrap();
        let lease = start(f.repo(), "session-a", "turn-a");
        let event = f
            .repo()
            .append_stream(&lease, delta(&lease, " + durable 中"))
            .unwrap();
        assert_eq!(event.seq, 3);
        let mut tool = delta(&lease, "");
        tool.part = Some(super::super::ChatMessagePart::Tool {
            name: "read".into(),
            input_summary: Some("README".into()),
            output_summary: None,
            status: Some("done".into()),
        });
        f.repo().append_stream(&lease, tool).unwrap();
        fs::remove_file(f.repo().session_path("session-a").unwrap()).unwrap();
        let recovered = f.repo().load("session-a").unwrap();
        assert_eq!(recovered.messages[0].content, "partial + durable 中");
        assert_eq!(recovered.messages[0].parts.len(), 1);
        assert_eq!(recovered.last_seq, 4);
        assert_eq!(recovered.turn_status, "streaming");
        drop(lease);
        let interrupted = f.repo().load("session-a").unwrap();
        assert_eq!(
            interrupted.messages[0].content,
            recovered.messages[0].content
        );
        assert_eq!(interrupted.messages[0].status, "aborted");
        assert_eq!(interrupted.last_seq, 5);
        assert_eq!(f.repo().load("session-a").unwrap().last_seq, 5);
    }

    #[test]
    fn incomplete_tail_is_repaired_but_bad_committed_records_are_preserved_and_rejected() {
        use std::io::Write;
        let f = Fixture::new();
        f.repo().create(&f.session("session-a")).unwrap();
        let log = f
            .repo()
            .session_directory("session-a", false)
            .unwrap()
            .join("events.jsonl");
        let committed = fs::read(&log).unwrap();
        OpenOptions::new()
            .append(true)
            .open(&log)
            .unwrap()
            .write_all(b"{\"seq\":2")
            .unwrap();
        assert_eq!(f.repo().load("session-a").unwrap().last_seq, 1);
        assert_eq!(fs::read(&log).unwrap(), committed);
        OpenOptions::new()
            .append(true)
            .open(&log)
            .unwrap()
            .write_all(b"broken committed record\n{}\n")
            .unwrap();
        let damaged = fs::read(&log).unwrap();
        assert!(f
            .repo()
            .load("session-a")
            .unwrap_err()
            .contains("corrupt committed"));
        assert_eq!(fs::read(log).unwrap(), damaged);
    }

    #[test]
    fn live_event_pages_read_only_requested_records_after_indexing_long_history() {
        let f = Fixture::new();
        f.repo().create(&f.session("session-a")).unwrap();
        let lease = start(f.repo(), "session-a", "turn-a");
        for _ in 0..10_000 {
            f.repo().append_stream(&lease, delta(&lease, "中")).unwrap();
        }
        let warm = f.repo().read_events("session-a", 10_002, 100).unwrap();
        assert!(warm.events.is_empty());
        journal::RECORD_READS.with(|reads| reads.set(0));
        let started = std::time::Instant::now();
        for _ in 0..100 {
            assert!(f
                .repo()
                .read_events("session-a", 10_002, 100)
                .unwrap()
                .events
                .is_empty());
        }
        assert_eq!(
            journal::RECORD_READS.with(|reads| reads.get()),
            0,
            "idle polling must not reparse history"
        );
        f.repo()
            .append_stream(&lease, delta(&lease, "新增"))
            .unwrap();
        let next = f.repo().read_events("session-a", 10_002, 100).unwrap();
        assert_eq!(next.events.len(), 1);
        assert_eq!(next.events[0].seq, 10_003);
        assert_eq!(
            journal::RECORD_READS.with(|reads| reads.get()),
            1,
            "new output must not replay earlier records"
        );
        let page = f.repo().read_events("session-a", 7_000, 3).unwrap();
        assert_eq!(
            page.events.iter().map(|e| e.seq).collect::<Vec<_>>(),
            vec![7001, 7002, 7003]
        );
        assert!(page.has_more);
        assert_eq!(journal::RECORD_READS.with(|reads| reads.get()), 4);
        println!("CHAT_INDEX history=10002 empty_polls=100 parsed_records=0 delta_records=1 page_records=3 elapsed_ms={}", started.elapsed().as_millis());
        f.repo()
            .update("session-a", |s| {
                s.title = "renamed".into();
                Ok(())
            })
            .unwrap();
        let meta = f.repo().read_events("session-a", 10_003, 5).unwrap();
        assert_eq!(meta.events.len(), 1);
        assert_eq!(meta.last_seq, 10_004);
        drop(lease);
        let final_page = f.repo().read_events("session-a", 10_004, 5).unwrap();
        assert_eq!(final_page.events.len(), 1);
        assert_eq!(
            f.repo().load("session-a").unwrap().messages[0].status,
            "aborted"
        );
    }

    #[test]
    fn live_event_index_does_not_hide_replaced_corrupt_or_truncated_journals() {
        let f = Fixture::new();
        f.repo().create(&f.session("session-a")).unwrap();
        let lease = start(f.repo(), "session-a", "turn-a");
        f.repo()
            .append_stream(&lease, delta(&lease, "saved"))
            .unwrap();
        f.repo().read_events("session-a", 3, 10).unwrap();
        let directory = f.repo().session_directory("session-a", false).unwrap();
        let log = directory.join("events.jsonl");
        let original = fs::read(&log).unwrap();
        let mut corrupt = original.clone();
        corrupt[0] = b'!';
        // Same length, replaced inode: a byte-index cache must not hide corruption.
        let replacement = directory.join("replacement");
        fs::write(&replacement, &corrupt).unwrap();
        fs::rename(&replacement, &log).unwrap();
        assert!(f
            .repo()
            .read_events("session-a", 3, 10)
            .unwrap_err()
            .contains("corrupt committed"));
        fs::write(&log, &original).unwrap();
        f.repo().read_events("session-a", 3, 10).unwrap();
        let first_line = original.iter().position(|b| *b == b'\n').unwrap() + 1;
        fs::write(&log, &original[..first_line]).unwrap();
        assert!(f.repo().read_events("session-a", 3, 10).is_err());
        fs::write(&log, &original).unwrap();
        f.repo().read_events("session-a", 3, 10).unwrap();
        fs::write(directory.join("snapshot.json"), r#"{"schemaVersion":99}"#).unwrap();
        assert!(f
            .repo()
            .read_events("session-a", 3, 10)
            .unwrap_err()
            .contains("unsupported"));
    }

    #[test]
    fn cursor_pagination_and_stale_streams_do_not_duplicate_or_reorder_content() {
        let f = Fixture::new();
        f.repo().create(&f.session("session-a")).unwrap();
        let lease = start(f.repo(), "session-a", "turn-a");
        for chunk in ["a", "b", "c"] {
            f.repo()
                .append_stream(&lease, delta(&lease, chunk))
                .unwrap();
        }
        let page = f.repo().read_events("session-a", 2, 2).unwrap();
        assert!(page.has_more);
        assert_eq!(
            page.events.iter().map(|e| e.seq).collect::<Vec<_>>(),
            [3, 4]
        );
        let next = f.repo().read_events("session-a", 4, 2).unwrap();
        assert!(!next.has_more);
        assert_eq!(next.events[0].seq, 5);
        assert_eq!(next.last_seq, 5);
        assert!(f.repo().read_events("session-a", 100, 2).is_err());
        let mut wrong = delta(&lease, "wrong");
        wrong.turn_id = "another-turn".into();
        assert!(f.repo().append_stream(&lease, wrong).is_err());
        assert_eq!(
            f.repo().load("session-a").unwrap().messages[0].content,
            "partialabc"
        );
        assert_eq!(f.repo().load("session-a").unwrap().last_seq, 5);
    }

    #[test]
    fn streaming_uses_incremental_records_and_pages_have_a_byte_budget() {
        let f = Fixture::new();
        f.repo().create(&f.session("session-a")).unwrap();
        let lease = start(f.repo(), "session-a", "turn-a");
        f.repo()
            .append_stream(&lease, delta(&lease, &"x".repeat(3 * 1024 * 1024)))
            .unwrap();
        f.repo()
            .append_stream(&lease, delta(&lease, "tail"))
            .unwrap();
        let page = f.repo().read_events("session-a", 2, 500).unwrap();
        assert_eq!(page.events.len(), 1);
        assert!(page.has_more);
        let tail = f.repo().read_events("session-a", 3, 500).unwrap();
        assert_eq!(tail.events.len(), 1);
        assert!(!tail.has_more);
        let path = f
            .repo()
            .session_directory("session-a", false)
            .unwrap()
            .join("events.jsonl");
        let raw = fs::read_to_string(path).unwrap();
        assert!(
            raw.lines().last().unwrap().len() < 1_000,
            "a small delta must not rewrite the full transcript"
        );
    }

    #[test]
    fn newer_snapshot_or_missing_committed_history_is_not_overwritten_during_migration() {
        let f = Fixture::new();
        let (legacy, raw) = legacy_file(&f, "legacy-a");
        let directory = f.repo().session_directory("legacy-a", true).unwrap();
        let future = "{\"schemaVersion\":99}";
        fs::write(directory.join("snapshot.json"), future).unwrap();
        assert!(f.repo().load("legacy-a").is_err());
        assert_eq!(
            fs::read_to_string(directory.join("snapshot.json")).unwrap(),
            future
        );
        assert_eq!(fs::read_to_string(legacy).unwrap(), raw);
        f.repo().create(&f.session("session-a")).unwrap();
        let log = f
            .repo()
            .session_directory("session-a", false)
            .unwrap()
            .join("events.jsonl");
        let first = fs::read(&log).unwrap();
        f.repo()
            .update("session-a", |session| {
                session.title = "committed".into();
                Ok(())
            })
            .unwrap();
        fs::write(&log, first).unwrap();
        assert!(f.repo().load("session-a").unwrap_err().contains("shorter"));
    }

    #[test]
    fn corrupt_sessions_remain_visible_as_errors_while_healthy_sessions_load() {
        let f = Fixture::new();
        f.repo().create(&f.session("good")).unwrap();
        f.repo().create(&f.session("bad")).unwrap();
        let path = f
            .repo()
            .session_directory("bad", false)
            .unwrap()
            .join("events.jsonl");
        fs::write(path, "corrupt committed line\n").unwrap();
        let entries = f.repo().list_entries().unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries
            .iter()
            .any(|(id, value)| id == "bad" && value.is_err()));
        assert!(entries
            .iter()
            .any(|(id, value)| id == "good" && value.is_ok()));
    }

    #[test]
    fn journal_recovers_after_writer_process_exits_without_running_destructors() {
        const KEY: &str = "LOOM_TEST_CRASH_CHAT_PROJECT";
        if let Ok(path) = std::env::var(KEY) {
            let path = PathBuf::from(path);
            assert!(path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("loom-chat-repository-"));
            let repo = ChatRepository::open(&path).unwrap();
            let lease = start(&repo, "session-crash", "turn-crash");
            repo.append_stream(&lease, delta(&lease, " persisted before exit"))
                .unwrap();
            std::process::exit(0);
        }
        let mut f = Fixture::new();
        f.repo().create(&f.session("session-crash")).unwrap();
        f.repository.take();
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "chat::repository::tests::journal_recovers_after_writer_process_exits_without_running_destructors", "--nocapture"])
            .env(KEY, &f.path).output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        f.repository = Some(ChatRepository::open(&f.path).unwrap());
        let recovered = f.repo().load("session-crash").unwrap();
        assert_eq!(
            recovered.messages[0].content,
            "partial persisted before exit"
        );
        assert_eq!(recovered.messages[0].status, "aborted");
        assert_eq!(recovered.last_seq, 4);
        assert_eq!(f.repo().load("session-crash").unwrap().last_seq, 4);
    }
}
