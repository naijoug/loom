use crate::{
    journal::{self, Journal, Stamp},
    projection, *,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{sync_channel, Receiver, SyncSender},
    Arc, Mutex, MutexGuard, Weak,
};
mod approvals;
mod connections;
mod imports;
mod permissions;
mod remembered;
mod runtime_context;
pub use approvals::ToolPermit;
pub use runtime_context::{ContextMessage, RuntimeContext};

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum CatalogEvent {
    WorkspaceCreated { workspace: Workspace },
    ProjectRegistered { project: Project },
    ConnectionSaved { connection: ModelConnection },
    PermissionSaved { document: PermissionDocument },
}
#[derive(Default)]
struct Catalog {
    workspaces: BTreeMap<Id, Workspace>,
    projects: BTreeMap<Id, BTreeMap<Id, Project>>,
    connections: BTreeMap<Id, ModelConnection>,
    permissions: BTreeMap<PermissionScope, PermissionDocument>,
}
impl Catalog {
    fn apply(&mut self, event: &EventEnvelope<CatalogEvent>) -> Result<()> {
        match &event.payload {
            CatalogEvent::PermissionSaved { document } => {
                crate::permissions::validate_permission_document(&document.config)?;
                let previous = self.permissions.get(&document.scope);
                if document
                    .scope
                    .workspace_id()
                    .is_some_and(|w| !self.workspaces.contains_key(w))
                    || document.revision != previous.map_or(1, |p| p.revision + 1)
                    || document.revision > MAX_SAFE_SEQUENCE
                    || document.updated_at_ms != event.timestamp_ms
                {
                    return Err(CoreError::corrupt(
                        "invalid permission configuration scope or revision",
                    ));
                }
                self.permissions
                    .insert(document.scope.clone(), document.clone());
            }
            CatalogEvent::ConnectionSaved { connection } => {
                connection.config.validate()?;
                let previous = self.connections.get(&connection.id);
                if connection.revision != previous.map(|p| p.revision + 1).unwrap_or(1)
                    || connection.created_at_ms > connection.updated_at_ms
                    || connection.updated_at_ms != event.timestamp_ms
                    || previous.is_some_and(|p| p.created_at_ms != connection.created_at_ms)
                    || (connection.config.auth == ConnectionAuth::None)
                        != connection.credential_ref.is_none()
                {
                    return Err(CoreError::corrupt(
                        "model connection revision, timestamps or credential reference invalid",
                    ));
                }
                self.connections
                    .insert(connection.id.clone(), connection.clone());
            }
            CatalogEvent::WorkspaceCreated { workspace } => {
                projection::validate_name(&workspace.name)?;
                if self.workspaces.contains_key(&workspace.id) {
                    return Err(CoreError::corrupt("duplicate workspace ID"));
                }
                self.workspaces
                    .insert(workspace.id.clone(), workspace.clone());
            }
            CatalogEvent::ProjectRegistered { project } => {
                projection::validate_name(&project.name)?;
                if !self.workspaces.contains_key(&project.workspace_id)
                    || !Path::new(&project.directory).is_absolute()
                {
                    return Err(CoreError::corrupt("project workspace/path is invalid"));
                }
                let projects = self
                    .projects
                    .entry(project.workspace_id.clone())
                    .or_default();
                if projects
                    .insert(project.id.clone(), project.clone())
                    .is_some()
                {
                    return Err(CoreError::corrupt("duplicate project ID"));
                }
            }
        }
        Ok(())
    }
}
struct SessionStore {
    journal: Journal<SessionEvent>,
    view: Option<SessionView>,
    touched: u64,
}
impl SessionStore {
    fn view(&self) -> &SessionView {
        self.view.as_ref().expect("session creation was validated")
    }
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SummaryCache {
    stamp: Stamp,
    summary: SessionSummary,
    checksum: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub key: Option<SessionKey>,
    pub message: String,
}
type SessionListener = (u64, SyncSender<EventEnvelope<SessionEvent>>);
type WorkspaceListener = (u64, SyncSender<WorkspaceNotice>);
struct State {
    catalog: Catalog,
    catalog_journal: Journal<CatalogEvent>,
    sessions: HashMap<SessionKey, SessionStore>,
    active: HashMap<SessionKey, u64>,
    live_grants: HashSet<(SessionKey, Id)>,
    next_lease: u64,
    tick: u64,
    next_listener: u64,
    listeners: HashMap<SessionKey, Vec<SessionListener>>,
    workspace_listeners: HashMap<Id, Vec<WorkspaceListener>>,
    diagnostics: Vec<Diagnostic>,
}
struct Inner {
    root: PathBuf,
    writer: File,
    state: Mutex<State>,
    importing: Mutex<()>,
}
impl Drop for Inner {
    fn drop(&mut self) {
        let _ = self.writer.unlock();
    }
}
#[derive(Clone)]
pub struct Core(Arc<Inner>);

/// A runtime-only capability. It is intentionally not deserializable or clonable.
/// Dropping an unfinished owner interrupts the turn; it never launches work.
pub struct TurnLease {
    core: Core,
    key: SessionKey,
    turn_id: Id,
    nonce: u64,
    closed: AtomicBool,
}
impl TurnLease {
    pub fn key(&self) -> &SessionKey {
        &self.key
    }
    pub fn turn_id(&self) -> &Id {
        &self.turn_id
    }
    pub fn cancel_requested(&self) -> Result<bool> {
        if self.closed.load(Ordering::Acquire) {
            return Ok(true);
        }
        let mut state = self.core.lock()?;
        self.core.check_lease(&state, self)?;
        let store = self.core.session(&mut state, &self.key)?;
        Ok(store
            .view()
            .turns
            .iter()
            .find(|t| t.id == self.turn_id)
            .is_some_and(|t| t.status == TurnStatus::Cancelling))
    }
}
impl Drop for TurnLease {
    fn drop(&mut self) {
        if !self.closed.load(Ordering::Acquire) {
            if let Err(error) = self.core.finish_turn(
                self,
                FinishStatus::Interrupted,
                Some("runtime owner dropped before completion".into()),
            ) {
                if let Ok(mut state) = self.core.0.state.lock() {
                    state.active.remove(&self.key);
                    self.core.diagnostic(
                        &mut state,
                        Some(self.key.clone()),
                        format!("unable to persist interrupted owner: {error}"),
                    );
                }
            }
        }
    }
}

/// Subscribe under the same mutation lock as the snapshot. A slow receiver is
/// disconnected instead of blocking persistence; reconnect/read_events repairs gaps.
pub struct Subscription {
    pub snapshot: SessionView,
    pub events: Receiver<EventEnvelope<SessionEvent>>,
    owner: Weak<Inner>,
    key: SessionKey,
    listener_id: u64,
}
impl Drop for Subscription {
    fn drop(&mut self) {
        if let Some(owner) = self.owner.upgrade() {
            if let Ok(mut state) = owner.state.lock() {
                if let Some(listeners) = state.listeners.get_mut(&self.key) {
                    listeners.retain(|(id, _)| *id != self.listener_id);
                }
                if state.listeners.get(&self.key).is_some_and(Vec::is_empty) {
                    state.listeners.remove(&self.key);
                }
            }
        }
    }
}

pub struct WorkspaceSubscription {
    pub events: Receiver<WorkspaceNotice>,
    owner: Weak<Inner>,
    workspace_id: Id,
    listener_id: u64,
}
impl Drop for WorkspaceSubscription {
    fn drop(&mut self) {
        if let Some(owner) = self.owner.upgrade() {
            if let Ok(mut state) = owner.state.lock() {
                if let Some(listeners) = state.workspace_listeners.get_mut(&self.workspace_id) {
                    listeners.retain(|(id, _)| *id != self.listener_id);
                }
                if state
                    .workspace_listeners
                    .get(&self.workspace_id)
                    .is_some_and(Vec::is_empty)
                {
                    state.workspace_listeners.remove(&self.workspace_id);
                }
            }
        }
    }
}
impl Core {
    fn notify_workspace(state: &mut State, workspace_id: &Id, session_id: Option<Id>, seq: u64) {
        if let Some(listeners) = state.workspace_listeners.get_mut(workspace_id) {
            let notice = WorkspaceNotice {
                workspace_id: workspace_id.clone(),
                session_id,
                seq,
            };
            listeners.retain(|(_, sender)| sender.try_send(notice.clone()).is_ok());
        }
    }
    pub fn subscribe_workspace(&self, workspace_id: &Id) -> Result<WorkspaceSubscription> {
        let mut state = self.lock()?;
        Self::require_workspace(&state, workspace_id)?;
        let listener_id = state.next_listener;
        state.next_listener = state
            .next_listener
            .checked_add(1)
            .ok_or_else(|| CoreError::conflict("subscription limit"))?;
        let (sender, events) = sync_channel(64);
        state
            .workspace_listeners
            .entry(workspace_id.clone())
            .or_default()
            .push((listener_id, sender));
        Ok(WorkspaceSubscription {
            events,
            owner: Arc::downgrade(&self.0),
            workspace_id: workspace_id.clone(),
            listener_id,
        })
    }
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref();
        if !root.exists() {
            fs::create_dir_all(root)?;
        }
        journal::directory(root)?;
        let root = root.canonicalize()?;
        let lock_path = root.join("writer.lock");
        journal::regular(&lock_path)?;
        let writer = journal::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)?;
        writer
            .try_lock()
            .map_err(|_| CoreError::new(ErrorCode::Busy, "data directory already has a writer"))?;
        let result = (|| -> Result<State> {
            let manifest = root.join("manifest.json");
            journal::regular(&manifest)?;
            let expected =
                serde_json::json!({"schemaVersion":SCHEMA_VERSION,"format":"loom-workspaces"});
            if manifest.exists() {
                let stored: serde_json::Value =
                    serde_json::from_slice(&journal::read_bounded(&manifest, 4096)?)
                        .map_err(|e| CoreError::corrupt(e.to_string()))?;
                if stored != expected {
                    return Err(CoreError::corrupt("unsupported data directory schema"));
                }
            } else {
                journal::atomic_write(&manifest, &expected)?;
            }
            let workspaces = root.join("workspaces");
            journal::directory(&workspaces)?;
            let directory = root.join("catalog");
            journal::directory(&directory)?;
            let create = !directory.join("events.jsonl").exists();
            if create && fs::read_dir(&workspaces)?.next().is_some() {
                return Err(CoreError::corrupt(
                    "catalog is missing while workspace data exists",
                ));
            }
            let mut catalog = Catalog::default();
            let catalog_journal = Journal::open(&directory, "catalog".into(), create, |event| {
                catalog.apply(event)
            })?;
            Ok(State {
                catalog,
                catalog_journal,
                sessions: HashMap::new(),
                active: HashMap::new(),
                live_grants: HashSet::new(),
                next_lease: 1,
                tick: 0,
                next_listener: 1,
                listeners: HashMap::new(),
                workspace_listeners: HashMap::new(),
                diagnostics: Vec::new(),
            })
        })();
        match result {
            Ok(state) => Ok(Self(Arc::new(Inner {
                root,
                writer,
                state: Mutex::new(state),
                importing: Mutex::new(()),
            }))),
            Err(error) => {
                let _ = writer.unlock();
                Err(error)
            }
        }
    }
    fn lock(&self) -> Result<MutexGuard<'_, State>> {
        let state = self.0.state.lock().map_err(|_| {
            CoreError::new(ErrorCode::StorageUnavailable, "core state lock is poisoned")
        })?;
        state.catalog_journal.current()?;
        Ok(state)
    }
    fn workspace_directory(&self, id: &Id) -> Result<PathBuf> {
        journal::directory(&self.0.root)?;
        let base = self.0.root.join("workspaces");
        journal::directory(&base)?;
        let directory = base.join(id.as_str());
        journal::directory(&directory)?;
        Ok(directory)
    }
    fn session_directory(&self, key: &SessionKey) -> Result<PathBuf> {
        let directory = self
            .workspace_directory(&key.workspace_id)?
            .join("sessions");
        journal::directory(&directory)?;
        let session = directory.join(key.session_id.as_str());
        if let Ok(meta) = fs::symlink_metadata(&session) {
            if !meta.is_dir() || meta.file_type().is_symlink() {
                return Err(CoreError::corrupt(
                    "session directory is not a real directory",
                ));
            }
        }
        Ok(session)
    }
    fn require_workspace(state: &State, id: &Id) -> Result<()> {
        if state.catalog.workspaces.contains_key(id) {
            Ok(())
        } else {
            Err(CoreError::missing("workspace not found"))
        }
    }
    fn diagnostic(&self, state: &mut State, key: Option<SessionKey>, message: String) {
        if state.diagnostics.len() >= 100 {
            state.diagnostics.remove(0);
        }
        state.diagnostics.push(Diagnostic { key, message });
    }
    pub fn diagnostics(&self) -> Result<Vec<Diagnostic>> {
        Ok(self.lock()?.diagnostics.clone())
    }
    fn evict_for_insert(state: &mut State) {
        while state.sessions.len() >= 32 {
            let oldest = state
                .sessions
                .iter()
                .filter(|(id, _)| !state.active.contains_key(*id))
                .min_by_key(|(_, store)| store.touched)
                .map(|(id, _)| id.clone());
            let Some(id) = oldest else {
                break;
            };
            state.sessions.remove(&id);
        }
    }
    fn session<'a>(&self, state: &'a mut State, key: &SessionKey) -> Result<&'a mut SessionStore> {
        Self::require_workspace(state, &key.workspace_id)?;
        let directory = self.session_directory(key)?;
        state.tick += 1;
        if !state.sessions.contains_key(key) {
            if !directory.join("events.jsonl").exists() {
                return Err(CoreError::missing("session not found"));
            }
            Self::evict_for_insert(state);
            let mut view = None;
            let journal =
                Journal::open(&directory, projection::stream_name(key), false, |event| {
                    projection::apply(&mut view, event)
                })?;
            if view.is_none() {
                return Err(CoreError::corrupt(
                    "session has no committed creation event",
                ));
            }
            state.sessions.insert(
                key.clone(),
                SessionStore {
                    journal,
                    view,
                    touched: state.tick,
                },
            );
        }
        state.sessions.get(key).unwrap().journal.current()?;
        let stale_grants: Vec<_> = state.sessions[key]
            .view()
            .remembered_grants
            .values()
            .filter(|g| {
                g.revoked_at_ms.is_none()
                    && !state.live_grants.contains(&(key.clone(), g.id.clone()))
            })
            .map(|g| g.id.clone())
            .collect();
        if !stale_grants.is_empty() {
            self.commit(
                state,
                key,
                SessionEvent::RememberedGrantsRevoked {
                    grant_ids: stale_grants,
                    reason: GrantRevocation::RuntimeEnded,
                },
            )?;
        }
        // A durable Running turn with no live Rust owner is interrupted, never replayed.
        if !state.active.contains_key(key) {
            if let Some(turn_id) = state.sessions[key].view().active_turn_id.clone() {
                self.commit(
                    state,
                    key,
                    SessionEvent::TurnFinished {
                        turn_id,
                        status: FinishStatus::Interrupted,
                        error: Some("application restarted or runtime owner disappeared".into()),
                    },
                )?;
            }
        }
        let store = state.sessions.get_mut(key).unwrap();
        store.touched = state.tick;
        let expired: Vec<_> = store
            .view()
            .approvals
            .values()
            .filter(|approval| {
                matches!(
                    approval.status,
                    ApprovalStatus::Pending | ApprovalStatus::Allowed
                ) && approval.expires_at_ms <= journal::now_ms()
            })
            .map(|a| a.id.clone())
            .collect();
        for request_id in expired {
            self.commit(
                state,
                key,
                SessionEvent::ToolApprovalResolved {
                    request_id,
                    resolution: ApprovalResolution::Expire,
                    remembered_grant: None,
                },
            )?;
        }
        Ok(state.sessions.get_mut(key).unwrap())
    }
    fn save_caches(&self, state: &mut State, key: &SessionKey, checkpoint: bool) {
        let result = (|| -> Result<()> {
            let directory = self.session_directory(key)?;
            let store = &state.sessions[key];
            let summary = projection::summary(store.view());
            let mut cache = SummaryCache {
                stamp: store.journal.stamp().clone(),
                summary,
                checksum: String::new(),
            };
            cache.checksum = projection::checksum(&cache)?;
            journal::atomic_write(&directory.join("summary.json"), &cache)?;
            if checkpoint {
                journal::atomic_write(&directory.join("snapshot.json"), store.view())?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            self.diagnostic(
                state,
                Some(key.clone()),
                format!("derived cache write failed: {error}"),
            );
        }
    }
    fn commit(
        &self,
        state: &mut State,
        key: &SessionKey,
        payload: SessionEvent,
    ) -> Result<EventEnvelope<SessionEvent>> {
        let store = state
            .sessions
            .get_mut(key)
            .ok_or_else(|| CoreError::missing("session not loaded"))?;
        let timestamp = journal::now_ms().max(
            store
                .view
                .as_ref()
                .map(|v| v.metadata.updated_at_ms)
                .unwrap_or(0),
        );
        let event = store.journal.prepare(payload, timestamp)?;
        projection::validate(store.view.as_ref(), &event)?;
        store.journal.append(&event)?;
        projection::apply(&mut store.view, &event)?;
        let checkpoint = matches!(
            event.payload,
            SessionEvent::Created { .. }
                | SessionEvent::TurnFinished { .. }
                | SessionEvent::TurnPreparationFailed { .. }
        ) || event.seq % 128 == 0;
        if !matches!(event.payload, SessionEvent::RuntimeApplied { .. }) || checkpoint {
            self.save_caches(state, key, checkpoint);
        }
        if let Some(listeners) = state.listeners.get_mut(key) {
            listeners.retain(|(_, sender)| sender.try_send(event.clone()).is_ok());
        }
        if !matches!(event.payload, SessionEvent::RuntimeApplied { .. }) {
            Self::notify_workspace(
                state,
                &key.workspace_id,
                Some(key.session_id.clone()),
                event.seq,
            );
        }
        Ok(event)
    }
    pub fn list_workspaces(&self) -> Result<Vec<Workspace>> {
        let state = self.lock()?;
        state.catalog_journal.current()?;
        Ok(state.catalog.workspaces.values().cloned().collect())
    }
    pub fn create_workspace(&self, id: Id, name: String) -> Result<Workspace> {
        projection::validate_name(&name)?;
        let mut state = self.lock()?;
        if let Some(workspace) = state.catalog.workspaces.get(&id) {
            return if workspace.name == name {
                Ok(workspace.clone())
            } else {
                Err(CoreError::conflict("workspace ID already exists"))
            };
        }
        self.workspace_directory(&id)?;
        let workspace = Workspace {
            id,
            name,
            created_at_ms: journal::now_ms(),
        };
        let event = state.catalog_journal.prepare(
            CatalogEvent::WorkspaceCreated {
                workspace: workspace.clone(),
            },
            workspace.created_at_ms,
        )?;
        state.catalog_journal.append(&event)?;
        state.catalog.apply(&event)?;
        Ok(workspace)
    }
    pub fn register_project(
        &self,
        workspace_id: Id,
        id: Id,
        name: String,
        directory: &Path,
    ) -> Result<Project> {
        projection::validate_name(&name)?;
        let directory = directory.canonicalize()?;
        if !directory.is_dir() {
            return Err(CoreError::invalid("project path is not a directory"));
        }
        let directory = directory
            .to_str()
            .ok_or_else(|| CoreError::invalid("project path is not UTF-8"))?
            .to_owned();
        let mut state = self.lock()?;
        Self::require_workspace(&state, &workspace_id)?;
        if let Some(project) = state
            .catalog
            .projects
            .get(&workspace_id)
            .and_then(|p| p.get(&id))
        {
            return if project.name == name && project.directory == directory {
                Ok(project.clone())
            } else {
                Err(CoreError::conflict("project ID already exists"))
            };
        }
        let project = Project {
            id,
            workspace_id,
            name,
            directory,
            created_at_ms: journal::now_ms(),
        };
        let event = state.catalog_journal.prepare(
            CatalogEvent::ProjectRegistered {
                project: project.clone(),
            },
            project.created_at_ms,
        )?;
        state.catalog_journal.append(&event)?;
        state.catalog.apply(&event)?;
        Self::notify_workspace(&mut state, &project.workspace_id, None, event.seq);
        Ok(project)
    }
    pub fn list_projects(&self, workspace_id: &Id) -> Result<Vec<Project>> {
        let state = self.lock()?;
        Self::require_workspace(&state, workspace_id)?;
        Ok(state
            .catalog
            .projects
            .get(workspace_id)
            .map(|p| p.values().cloned().collect())
            .unwrap_or_default())
    }
    pub fn create_session(&self, input: CreateSession) -> Result<SessionView> {
        projection::validate_selection(&input.connection)?;
        if let Some(title) = &input.title {
            projection::validate_name(title)?;
        }
        let mut state = self.lock()?;
        Self::require_workspace(&state, &input.key.workspace_id)?;
        if let Some(project) = &input.project_id {
            if !state
                .catalog
                .projects
                .get(&input.key.workspace_id)
                .is_some_and(|p| p.contains_key(project))
            {
                return Err(CoreError::missing(
                    "project does not belong to this workspace",
                ));
            }
        }
        let directory = self.session_directory(&input.key)?;
        if directory.join("events.jsonl").exists() {
            let store = self.session(&mut state, &input.key)?;
            let (first, _) = store.journal.page(0, 1)?;
            if let SessionEvent::Created { metadata } = &first[0].payload {
                if metadata.title == input.title
                    && metadata.project_id == input.project_id
                    && metadata.connection == input.connection
                    && metadata.permission_mode == input.permission_mode
                {
                    return Ok(store.view().clone());
                }
            }
            return Err(CoreError::conflict(
                "session ID already exists with different creation input",
            ));
        }
        journal::directory(&directory)?;
        let journal = Journal::open(
            &directory,
            projection::stream_name(&input.key),
            true,
            |_| Ok(()),
        )?;
        let now = journal::now_ms();
        let metadata = SessionMetadata {
            key: input.key.clone(),
            title: input.title,
            project_id: input.project_id,
            status_id: Id::new("todo")?,
            archived: false,
            flagged: false,
            labels: Vec::new(),
            permission_mode: input.permission_mode,
            connection: input.connection,
            enabled_sources: Vec::new(),
            created_at_ms: now,
            updated_at_ms: now,
            last_read_message_id: None,
        };
        Self::evict_for_insert(&mut state);
        state.tick += 1;
        let tick = state.tick;
        state.sessions.insert(
            input.key.clone(),
            SessionStore {
                journal,
                view: None,
                touched: tick,
            },
        );
        if let Err(error) = self.commit(&mut state, &input.key, SessionEvent::Created { metadata })
        {
            state.sessions.remove(&input.key);
            return Err(error);
        }
        Ok(state.sessions[&input.key].view().clone())
    }
    pub fn get_session(&self, key: &SessionKey) -> Result<SessionView> {
        let mut state = self.lock()?;
        Ok(self.session(&mut state, key)?.view().clone())
    }
    pub fn list_sessions(&self, workspace_id: &Id) -> Result<Vec<SessionListEntry>> {
        let mut state = self.lock()?;
        Self::require_workspace(&state, workspace_id)?;
        let base = self.workspace_directory(workspace_id)?.join("sessions");
        journal::directory(&base)?;
        let mut entries = Vec::new();
        for entry in fs::read_dir(base)? {
            let entry = entry?;
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let Ok(session_id) = Id::new(name) else {
                continue;
            };
            let key = SessionKey {
                workspace_id: workspace_id.clone(),
                session_id: session_id.clone(),
            };
            let result = (|| -> Result<SessionSummary> {
                let directory = self.session_directory(&key)?;
                if !state.sessions.contains_key(&key) {
                    let cache_path = directory.join("summary.json");
                    if journal::regular(&cache_path).is_ok() {
                        if let Ok(bytes) =
                            journal::read_bounded(&cache_path, MAX_MESSAGE_BYTES as u64)
                        {
                            if bytes.len() <= MAX_MESSAGE_BYTES {
                                if let Ok(mut cache) =
                                    serde_json::from_slice::<SummaryCache>(&bytes)
                                {
                                    let saved = std::mem::take(&mut cache.checksum);
                                    if cache.summary.metadata.key == key
                                        && cache.summary.active_turn_id.is_none()
                                        && projection::checksum(&cache)? == saved
                                        && journal::stamp(&directory.join("events.jsonl"))?
                                            == cache.stamp
                                        && journal::committed_sequence(
                                            &directory,
                                            &projection::stream_name(&key),
                                        )? == Some(cache.summary.last_seq)
                                    {
                                        return Ok(cache.summary);
                                    }
                                }
                            }
                        }
                    }
                }
                let summary = projection::summary(self.session(&mut state, &key)?.view());
                self.save_caches(&mut state, &key, false);
                Ok(summary)
            })();
            entries.push(match result {
                Ok(summary) => SessionListEntry {
                    session_id,
                    summary: Some(summary),
                    error: None,
                },
                Err(error) => SessionListEntry {
                    session_id,
                    summary: None,
                    error: Some(error.message),
                },
            });
        }
        entries.sort_by(|a, b| {
            b.summary
                .as_ref()
                .map(|s| s.metadata.updated_at_ms)
                .cmp(&a.summary.as_ref().map(|s| s.metadata.updated_at_ms))
                .then_with(|| a.session_id.cmp(&b.session_id))
        });
        Ok(entries)
    }
    pub fn patch_session(
        &self,
        key: &SessionKey,
        expected_seq: u64,
        patch: SessionPatch,
    ) -> Result<SessionView> {
        let mut state = self.lock()?;
        let view = self.session(&mut state, key)?.view();
        if expected_seq < view.metadata_revision || expected_seq > view.last_seq {
            return Err(CoreError::conflict(
                "session changed; reload before updating",
            ));
        }
        if let Some(connection) = &patch.connection {
            if !view.turns.is_empty()
                && connection.connection_id != view.metadata.connection.connection_id
            {
                return Err(CoreError::conflict(
                    "connection identity is locked after the first message; create a new session",
                ));
            }
        }
        self.commit(&mut state, key, SessionEvent::MetadataChanged { patch })?;
        Ok(state.sessions[key].view().clone())
    }
    pub fn send_message(&self, input: SendMessage) -> Result<SendResult> {
        if input.text.len() > MAX_MESSAGE_BYTES {
            return Err(CoreError::invalid("message exceeds 1 MiB"));
        }
        let digest = projection::checksum(&input)?;
        let mut state = self.lock()?;
        let view = self.session(&mut state, &input.key)?.view();
        if let Some(turn) = view
            .turns
            .iter()
            .find(|t| t.receipt.client_request_id == input.client_request_id)
        {
            if turn.receipt.request_digest != digest {
                return Err(CoreError::conflict(
                    "request ID was accepted with different content",
                ));
            }
            return Ok(SendResult {
                receipt: turn.receipt.clone(),
                already_accepted: true,
                last_seq: view.last_seq,
            });
        }
        // Attachments gain an authoritative resource store in the migration/content slice.
        if !input.attachment_ids.is_empty() {
            return Err(CoreError::invalid(
                "attachment storage is not connected yet",
            ));
        }
        let receipt = SendReceipt {
            client_request_id: input.client_request_id,
            turn_id: Id::new(format!("turn-{digest}"))?,
            user_message_id: Id::new(format!("user-{digest}"))?,
            assistant_message_id: Id::new(format!("assistant-{digest}"))?,
            accepted_seq: view.last_seq + 1,
            request_digest: digest,
        };
        let mut configuration = TurnConfiguration {
            permission_mode: view.metadata.permission_mode,
            connection: view.metadata.connection.clone(),
            resolved_connection: None,
        };
        if let Some(id) = &configuration.connection.connection_id {
            let connection = state
                .catalog
                .connections
                .get(id)
                .ok_or_else(|| CoreError::missing("selected model connection no longer exists"))?
                .clone();
            if configuration
                .connection
                .model
                .as_ref()
                .is_some_and(|model| !connection.config.models.contains(model))
            {
                return Err(CoreError::invalid(
                    "selected model is not in this connection",
                ));
            }
            configuration.resolved_connection = Some(Box::new(connection));
        }
        let event = self.commit(
            &mut state,
            &input.key,
            SessionEvent::MessageAccepted {
                receipt: receipt.clone(),
                text: input.text,
                attachment_ids: input.attachment_ids,
                configuration,
            },
        )?;
        Ok(SendResult {
            receipt,
            already_accepted: false,
            last_seq: event.seq,
        })
    }
    pub fn claim_next_turn(&self, key: &SessionKey, binding: RuntimeBinding) -> Result<TurnLease> {
        self.claim_turn_inner(key, None, binding)
    }
    pub fn claim_prepared_turn(
        &self,
        key: &SessionKey,
        expected: &Id,
        binding: RuntimeBinding,
    ) -> Result<TurnLease> {
        self.claim_turn_inner(key, Some(expected), binding)
    }
    fn claim_turn_inner(
        &self,
        key: &SessionKey,
        expected: Option<&Id>,
        binding: RuntimeBinding,
    ) -> Result<TurnLease> {
        let mut state = self.lock()?;
        let view = self.session(&mut state, key)?.view();
        let project_id = view.metadata.project_id.clone();
        let turn_id = view
            .turns
            .iter()
            .find(|t| t.status == TurnStatus::Queued)
            .map(|t| t.id.clone())
            .ok_or_else(|| CoreError::missing("no queued turn"))?;
        if expected.is_some_and(|expected| expected != &turn_id) {
            return Err(CoreError::conflict(
                "prepared turn is no longer first in the queue",
            ));
        }
        let mut expected_directory = project_id
            .as_ref()
            .and_then(|id| state.catalog.projects.get(&key.workspace_id)?.get(id))
            .map(|p| p.directory.clone());
        if project_id.is_none() && binding.working_directory.is_some() {
            let directory = self.session_directory(key)?.join("work");
            journal::directory(&directory)?;
            expected_directory = Some(
                directory
                    .to_str()
                    .ok_or_else(|| CoreError::invalid("runtime path is not UTF-8"))?
                    .into(),
            );
        }
        if binding.working_directory != expected_directory {
            return Err(CoreError::conflict(
                "working directory differs from the session project",
            ));
        }
        if let Some(directory) = &expected_directory {
            let actual = Path::new(directory).canonicalize()?;
            if !actual.is_dir() || actual != Path::new(directory) {
                return Err(CoreError::conflict(
                    "registered project path changed; register its new location before running",
                ));
            }
        }
        let nonce = state.next_lease;
        state.next_lease = state
            .next_lease
            .checked_add(1)
            .ok_or_else(|| CoreError::conflict("runtime lease limit"))?;
        self.commit(
            &mut state,
            key,
            SessionEvent::TurnStarted {
                turn_id: turn_id.clone(),
                binding,
            },
        )?;
        state.active.insert(key.clone(), nonce);
        Ok(TurnLease {
            core: self.clone(),
            key: key.clone(),
            turn_id,
            nonce,
            closed: AtomicBool::new(false),
        })
    }
    fn check_lease(&self, state: &State, lease: &TurnLease) -> Result<()> {
        if !Arc::ptr_eq(&self.0, &lease.core.0)
            || lease.closed.load(Ordering::Acquire)
            || state.active.get(&lease.key) != Some(&lease.nonce)
        {
            return Err(CoreError::new(
                ErrorCode::StaleLease,
                "runtime lease is no longer active",
            ));
        }
        Ok(())
    }
    pub fn apply_runtime(
        &self,
        lease: &TurnLease,
        input_id: Id,
        update: RuntimeUpdate,
    ) -> Result<u64> {
        let mut state = self.lock()?;
        self.apply_runtime_locked(&mut state, lease, input_id, update)
    }
    fn apply_runtime_locked(
        &self,
        state: &mut State,
        lease: &TurnLease,
        input_id: Id,
        update: RuntimeUpdate,
    ) -> Result<u64> {
        self.check_lease(state, lease)?;
        let store = self.session(state, &lease.key)?;
        let turn = store
            .view()
            .turns
            .iter()
            .find(|t| t.id == lease.turn_id)
            .ok_or_else(|| CoreError::corrupt("leased turn missing"))?;
        let digest = projection::checksum(&update)?;
        if let Some(receipt) = turn.runtime_receipts.get(&input_id) {
            return if receipt.digest == digest {
                Ok(receipt.seq)
            } else {
                Err(CoreError::conflict("runtime event ID content mismatch"))
            };
        }
        Ok(self
            .commit(
                state,
                &lease.key,
                SessionEvent::RuntimeApplied {
                    turn_id: lease.turn_id.clone(),
                    input_id,
                    update,
                },
            )?
            .seq)
    }
    pub fn cancel_turn(&self, key: &SessionKey, turn_id: &Id) -> Result<SessionView> {
        let mut state = self.lock()?;
        let view = self.session(&mut state, key)?.view();
        let turn = view
            .turns
            .iter()
            .find(|t| &t.id == turn_id)
            .ok_or_else(|| CoreError::missing("turn not found"))?;
        if turn.status.is_terminal() || turn.status == TurnStatus::Cancelling {
            return Ok(view.clone());
        }
        self.commit(
            &mut state,
            key,
            SessionEvent::TurnCancelRequested {
                turn_id: turn_id.clone(),
            },
        )?;
        Ok(state.sessions[key].view().clone())
    }
    pub fn finish_turn(
        &self,
        lease: &TurnLease,
        status: FinishStatus,
        error: Option<String>,
    ) -> Result<SessionView> {
        self.finish_owned_turn(lease, status, error, false)
    }
    /// Trusted host-only failure before a runtime can acquire an execution lease.
    pub fn fail_queued_turn(
        &self,
        key: &SessionKey,
        turn_id: &Id,
        error: String,
    ) -> Result<SessionView> {
        let mut state = self.lock()?;
        let view = self.session(&mut state, key)?.view();
        if view.turns.iter().any(|t| {
            &t.id == turn_id
                && t.status == TurnStatus::Failed
                && t.started_at_ms.is_none()
                && t.error.as_ref() == Some(&error)
        }) {
            return Ok(view.clone());
        }
        self.commit(
            &mut state,
            key,
            SessionEvent::TurnPreparationFailed {
                turn_id: turn_id.clone(),
                error,
            },
        )?;
        Ok(state.sessions[key].view().clone())
    }
    /// Settle a supervised runtime after cleanup. Cancellation already accepted
    /// by the core wins atomically over a racing worker success or failure.
    pub fn settle_runtime_turn(
        &self,
        lease: &TurnLease,
        status: FinishStatus,
        error: Option<String>,
    ) -> Result<SessionView> {
        self.finish_owned_turn(lease, status, error, true)
    }
    fn finish_owned_turn(
        &self,
        lease: &TurnLease,
        mut status: FinishStatus,
        mut error: Option<String>,
        cancellation_wins: bool,
    ) -> Result<SessionView> {
        let mut state = self.lock()?;
        self.check_lease(&state, lease)?;
        let view = self.session(&mut state, &lease.key)?.view();
        if cancellation_wins
            && view
                .turns
                .iter()
                .any(|t| t.id == lease.turn_id && t.status == TurnStatus::Cancelling)
        {
            status = FinishStatus::Cancelled;
            error = None;
        }
        self.commit(
            &mut state,
            &lease.key,
            SessionEvent::TurnFinished {
                turn_id: lease.turn_id.clone(),
                status,
                error,
            },
        )?;
        lease.closed.store(true, Ordering::Release);
        state.active.remove(&lease.key);
        Ok(state.sessions[&lease.key].view().clone())
    }
    pub fn mark_read(&self, key: &SessionKey, message_id: Id) -> Result<SessionView> {
        let mut state = self.lock()?;
        let view = self.session(&mut state, key)?.view();
        if let (Some(current), Some(next)) = (
            view.metadata
                .last_read_message_id
                .as_ref()
                .and_then(|id| view.messages.iter().position(|m| &m.id == id)),
            view.messages.iter().position(|m| {
                m.id == message_id
                    && m.role == Role::Assistant
                    && view
                        .turns
                        .iter()
                        .any(|t| t.id == m.turn_id && t.status.is_terminal())
            }),
        ) {
            if next <= current {
                return Ok(view.clone());
            }
        }
        self.commit(&mut state, key, SessionEvent::ReadThrough { message_id })?;
        Ok(state.sessions[key].view().clone())
    }
    pub fn read_events(&self, key: &SessionKey, after: u64, limit: usize) -> Result<EventPage> {
        let mut state = self.lock()?;
        let store = self.session(&mut state, key)?;
        let last_seq = store.journal.last_seq();
        let (events, has_more) = store.journal.page(after, limit)?;
        Ok(EventPage {
            events,
            last_seq,
            has_more,
        })
    }
    pub fn subscribe(&self, key: &SessionKey) -> Result<Subscription> {
        let mut state = self.lock()?;
        let snapshot = self.session(&mut state, key)?.view().clone();
        let (sender, events) = sync_channel(64);
        let listener_id = state.next_listener;
        state.next_listener = state
            .next_listener
            .checked_add(1)
            .ok_or_else(|| CoreError::conflict("subscription limit"))?;
        state
            .listeners
            .entry(key.clone())
            .or_default()
            .push((listener_id, sender));
        Ok(Subscription {
            snapshot,
            events,
            owner: Arc::downgrade(&self.0),
            key: key.clone(),
            listener_id,
        })
    }
}
