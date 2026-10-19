//! The newline-committed v2 journal is authoritative; snapshot.json is a cache.
//! No fsync/power-loss guarantee is implied. A record is published only after
//! its complete JSON and newline have been written and flushed.
use super::repository::validate_file;
use super::{
    ChatEvent, ChatEventPage, ChatEventPayload, ChatSession, ChatSessionPatch, ChatStreamEvent,
    CHAT_SCHEMA_VERSION,
};
use crate::{models::now_ms, storage};
use std::{
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Read, Seek, SeekFrom, Write},
    path::Path,
};

const MAX_RECORD_BYTES: u64 = 32 * 1024 * 1024;
const MAX_SEQUENCE: u64 = 9_007_199_254_740_991;

#[derive(Clone, PartialEq, Eq)]
struct FileStamp {
    len: u64,
    modified: std::time::SystemTime,
    #[cfg(unix)]
    identity: (u64, u64, i64, i64),
}

fn file_stamp(path: &Path) -> Result<FileStamp, String> {
    validate_file(path)?;
    let meta = fs::metadata(path).map_err(|e| e.to_string())?;
    Ok(FileStamp {
        len: meta.len(),
        modified: meta.modified().map_err(|e| e.to_string())?,
        #[cfg(unix)]
        identity: {
            use std::os::unix::fs::MetadataExt;
            (meta.dev(), meta.ino(), meta.ctime(), meta.ctime_nsec())
        },
    })
}

fn snapshot_stamp(directory: &Path) -> Result<Option<FileStamp>, String> {
    let path = directory.join("snapshot.json");
    validate_file(&path)?;
    if path.exists() {
        file_stamp(&path).map(Some)
    } else {
        Ok(None)
    }
}

/// Disposable byte offsets for a live turn whose projection was validated from
/// the journal. Never persisted or trusted across a repository/application exit.
pub(super) struct EventIndex {
    offsets: Vec<u64>,
    stamp: FileStamp,
    snapshot: Option<FileStamp>,
}

impl EventIndex {
    pub(super) fn build(directory: &Path, session: &ChatSession) -> Result<Self, String> {
        let mut offsets = Vec::new();
        scan_offsets(directory, |event, offset| {
            if event.project_key != session.project_path || event.session_id != session.id {
                return Err("chat journal identity mismatch".into());
            }
            offsets.push(offset);
            Ok(())
        })?;
        if offsets.len() as u64 != session.last_seq {
            return Err("chat journal does not match the live projection".into());
        }
        Ok(Self {
            offsets,
            stamp: file_stamp(&directory.join("events.jsonl"))?,
            snapshot: snapshot_stamp(directory)?,
        })
    }

    pub(super) fn matches(&self, directory: &Path) -> bool {
        file_stamp(&directory.join("events.jsonl")).is_ok_and(|stamp| stamp == self.stamp)
            && snapshot_stamp(directory).is_ok_and(|stamp| stamp == self.snapshot)
    }

    /// Called only under the repository lock, after checking the old stamp and
    /// successfully committing this exact event. Failed writes discard the index.
    pub(super) fn appended(&mut self, directory: &Path, event: &ChatEvent) -> Result<(), String> {
        let stamp = file_stamp(&directory.join("events.jsonl"))?;
        let size = serde_json::to_vec(event).map_err(|e| e.to_string())?.len() as u64 + 1;
        if event.seq != self.offsets.len() as u64 + 1 || stamp.len != self.stamp.len + size {
            return Err("chat journal changed outside its writer".into());
        }
        self.offsets.push(self.stamp.len);
        self.stamp = stamp;
        Ok(())
    }

    pub(super) fn events(
        &self,
        directory: &Path,
        project: &str,
        session: &str,
        after: u64,
        limit: usize,
    ) -> Result<ChatEventPage, String> {
        let last_seq = self.offsets.len() as u64;
        if after > last_seq {
            return Err("chat event cursor is ahead of the journal".into());
        }
        let mut events = Vec::new();
        let mut bytes = 0;
        let mut file = fs::File::open(directory.join("events.jsonl")).map_err(|e| e.to_string())?;
        for index in (after as usize..self.offsets.len()).take(limit.clamp(1, 500)) {
            let start = self.offsets[index];
            let end = self
                .offsets
                .get(index + 1)
                .copied()
                .unwrap_or(self.stamp.len);
            let size = end - start;
            if !events.is_empty() && bytes + size > 2 * 1024 * 1024 {
                break;
            }
            if size > MAX_RECORD_BYTES {
                return Err("chat journal contains an oversized record".into());
            }
            file.seek(SeekFrom::Start(start))
                .map_err(|e| e.to_string())?;
            let mut record = vec![0; size as usize];
            file.read_exact(&mut record)
                .map_err(|e| format!("failed to read chat record: {e}"))?;
            #[cfg(test)]
            RECORD_READS.with(|reads| reads.set(reads.get() + 1));
            if record.last() != Some(&b'\n') {
                return Err("chat journal lost a committed record".into());
            }
            let event: ChatEvent = serde_json::from_slice(&record).map_err(|e| e.to_string())?;
            if event.seq != index as u64 + 1
                || event.schema_version != CHAT_SCHEMA_VERSION
                || event.project_key != project
                || event.session_id != session
            {
                return Err("chat journal version, sequence or identity is invalid".into());
            }
            events.push(event);
            bytes += size;
        }
        Ok(ChatEventPage {
            has_more: after + (events.len() as u64) < last_seq,
            events,
            last_seq,
        })
    }
}

#[cfg(test)]
thread_local! { pub(super) static RECORD_READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }

const MUTABLE_FIELDS: &[&str] = &[
    "title",
    "agentId",
    "permissionMode",
    "updatedAtMs",
    "resumeCommand",
    "resumeHandle",
    "activeTurnId",
    "turnStatus",
    "promotedTaskId",
    "status",
    "flagged",
    "sendReceipts",
    "turns",
];

fn projection_json(session: &ChatSession) -> Result<serde_json::Value, String> {
    serde_json::to_value(session)
        .map_err(|error| format!("failed to encode chat projection: {error}"))
}

pub(super) fn patch(before: &ChatSession, after: &ChatSession) -> Result<ChatSessionPatch, String> {
    if before.id != after.id
        || before.project_path != after.project_path
        || before.schema_version != after.schema_version
        || before.created_at_ms != after.created_at_ms
    {
        return Err("immutable chat identity was modified".into());
    }
    if after.messages.len() < before.messages.len() {
        return Err("chat history cannot be removed by a metadata update".into());
    }
    validate_receipts(before, after)?;
    super::turns::validate(before, after)?;
    let old = projection_json(before)?;
    let new = projection_json(after)?;
    let mut patch = ChatSessionPatch::default();
    for field in MUTABLE_FIELDS {
        if old.get(field) != new.get(field) {
            patch.fields.insert(
                field.to_string(),
                new.get(field).cloned().unwrap_or(serde_json::Value::Null),
            );
        }
    }
    for (previous, next) in before.messages.iter().zip(&after.messages) {
        if previous.id != next.id {
            return Err("chat history cannot be reordered".into());
        }
        if serde_json::to_value(previous).map_err(|e| e.to_string())?
            != serde_json::to_value(next).map_err(|e| e.to_string())?
        {
            patch.changed_messages.push(next.clone());
        }
    }
    patch.appended_messages = after.messages[before.messages.len()..].to_vec();
    Ok(patch)
}

fn validate_receipts(before: &ChatSession, after: &ChatSession) -> Result<(), String> {
    if !after.send_receipts.starts_with(&before.send_receipts) {
        return Err("accepted chat request receipts are immutable".into());
    }
    let mut requests = std::collections::HashSet::new();
    let mut turns = std::collections::HashSet::new();
    for receipt in &after.send_receipts {
        if receipt.client_request_id.is_empty()
            || !requests.insert(&receipt.client_request_id)
            || !turns.insert(&receipt.turn_id)
            || !after
                .messages
                .iter()
                .any(|m| m.id == receipt.user_message_id && m.role == "user")
            || !after
                .messages
                .iter()
                .any(|m| m.id == receipt.assistant_message_id && m.role == "assistant")
        {
            return Err("chat request receipt is invalid or duplicated".into());
        }
        if let Some(old) = before
            .messages
            .iter()
            .find(|m| m.id == receipt.user_message_id)
        {
            if after
                .messages
                .iter()
                .find(|m| m.id == old.id)
                .is_none_or(|m| m.content != old.content)
            {
                return Err("accepted chat request content is immutable".into());
            }
        }
    }
    Ok(())
}

fn validate_stream(session: &ChatSession, stream: &ChatStreamEvent) -> Result<usize, String> {
    if stream.session_id != session.id || session.active_turn_id.as_deref() != Some(&stream.turn_id)
    {
        return Err("stream event does not belong to the active chat turn".into());
    }
    session
        .messages
        .iter()
        .rposition(|message| message.id == stream.message_id && message.role == "assistant")
        .ok_or_else(|| "stream event assistant message was not found".into())
}

pub(super) fn apply(session: &mut ChatSession, event: &ChatEvent) -> Result<(), String> {
    if event.schema_version != CHAT_SCHEMA_VERSION
        || event.seq != session.last_seq + 1
        || event.seq > MAX_SEQUENCE
    {
        return Err("chat journal version or sequence is invalid".into());
    }
    if event.session_id != session.id || event.project_key != session.project_path {
        return Err("chat journal event belongs to another project/session".into());
    }
    match &event.body {
        ChatEventPayload::SessionCreated(_) => return Err("duplicate chat creation record".into()),
        ChatEventPayload::Stream(stream) => {
            let index = validate_stream(session, stream)?;
            let message = &mut session.messages[index];
            if !stream.done {
                message.content.push_str(&stream.delta);
            }
            if let Some(part) = &stream.part {
                message.parts.push(part.clone());
            }
            session.updated_at_ms = event.timestamp_ms;
        }
        ChatEventPayload::SessionPatch(patch) => {
            let mut value = projection_json(session)?;
            let fields = value
                .as_object_mut()
                .ok_or("chat projection is not an object")?;
            for (key, value) in &patch.fields {
                if !MUTABLE_FIELDS.contains(&key.as_str()) {
                    return Err(format!("unsupported chat journal field: {key}"));
                }
                fields.insert(key.clone(), value.clone());
            }
            let mut next: ChatSession = serde_json::from_value(value)
                .map_err(|error| format!("invalid chat patch: {error}"))?;
            for changed in &patch.changed_messages {
                let message = next
                    .messages
                    .iter_mut()
                    .find(|message| message.id == changed.id)
                    .ok_or("chat patch message is missing")?;
                *message = changed.clone();
            }
            for appended in &patch.appended_messages {
                if next
                    .messages
                    .iter()
                    .any(|message| message.id == appended.id)
                {
                    return Err("duplicate chat message id".into());
                }
                next.messages.push(appended.clone());
            }
            validate_receipts(session, &next)?;
            super::turns::validate(session, &next)?;
            *session = next;
        }
    }
    session.last_seq = event.seq;
    session.revision = event.seq;
    Ok(())
}

fn append(directory: &Path, event: &ChatEvent) -> Result<(), String> {
    let path = directory.join("events.jsonl");
    validate_file(&path)?;
    let mut bytes = serde_json::to_vec(event)
        .map_err(|error| format!("failed to encode chat event: {error}"))?;
    if bytes.len() as u64 >= MAX_RECORD_BYTES {
        return Err("chat record exceeds the 32 MiB journal record budget".into());
    }
    bytes.push(b'\n');
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| format!("failed to open chat journal: {error}"))?;
    file.write_all(&bytes)
        .and_then(|_| file.flush())
        .map_err(|error| format!("failed to commit chat journal: {error}"))
}

fn checkpoint(directory: &Path, session: &ChatSession) -> Result<(), String> {
    let path = directory.join("snapshot.json");
    validate_file(&path)?;
    storage::atomic_write_json(&path, session)
}

fn event(session: &ChatSession, body: ChatEventPayload) -> Result<ChatEvent, String> {
    if session.last_seq >= MAX_SEQUENCE {
        return Err("chat event sequence exhausted".into());
    }
    Ok(ChatEvent {
        schema_version: CHAT_SCHEMA_VERSION,
        project_key: session.project_path.clone(),
        session_id: session.id.clone(),
        seq: session.last_seq + 1,
        timestamp_ms: now_ms(),
        body,
    })
}

pub(super) fn initialize(directory: &Path, session: &mut ChatSession) -> Result<ChatEvent, String> {
    validate_file(&directory.join("snapshot.json"))?;
    let path = directory.join("events.jsonl");
    validate_file(&path)?;
    if path.exists() && fs::metadata(&path).map_err(|e| e.to_string())?.len() > 0 {
        return Err("chat journal already exists".into());
    }
    if directory.join("snapshot.json").exists() {
        return Err(
            "chat snapshot exists without a committed journal; refusing to replace history".into(),
        );
    }
    session.schema_version = CHAT_SCHEMA_VERSION;
    session.last_seq = 0;
    session.revision = 0;
    let mut event = event(
        session,
        ChatEventPayload::SessionCreated(Box::new(session.clone())),
    )?;
    session.last_seq = event.seq;
    session.revision = event.seq;
    event.body = ChatEventPayload::SessionCreated(Box::new(session.clone()));
    append(directory, &event)?;
    // The committed journal remains readable if a disposable checkpoint fails.
    let _ = checkpoint(directory, session);
    Ok(event)
}

pub(super) fn commit(
    directory: &Path,
    before: &ChatSession,
    after: &mut ChatSession,
) -> Result<ChatEvent, String> {
    let patch = patch(before, after)?;
    let event = event(before, ChatEventPayload::SessionPatch(patch))?;
    // Validate before touching the journal, including message id uniqueness.
    let mut projected = before.clone();
    apply(&mut projected, &event)?;
    append(directory, &event)?;
    *after = projected;
    let _ = checkpoint(directory, after);
    Ok(event)
}

pub(super) fn stream(
    directory: &Path,
    session: &mut ChatSession,
    stream: ChatStreamEvent,
) -> Result<ChatEvent, String> {
    validate_stream(session, &stream)?;
    let event = event(session, ChatEventPayload::Stream(stream))?;
    append(directory, &event)?;
    apply(session, &event)?;
    Ok(event)
}

/// Scan without loading the entire log into memory. Only a non-newline tail is
/// an uncommitted write; malformed complete records must not be skipped.
fn scan(
    directory: &Path,
    mut visit: impl FnMut(ChatEvent) -> Result<(), String>,
) -> Result<(), String> {
    scan_offsets(directory, |event, _| visit(event))
}

fn scan_offsets(
    directory: &Path,
    mut visit: impl FnMut(ChatEvent, u64) -> Result<(), String>,
) -> Result<(), String> {
    let path = directory.join("events.jsonl");
    validate_file(&path)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .map_err(|error| format!("failed to read chat journal: {error}"))?;
    let mut reader = BufReader::new(file);
    let mut committed_offset = 0u64;
    let mut expected_seq = 1u64;
    loop {
        let mut bytes = Vec::new();
        let count = reader
            .by_ref()
            .take(MAX_RECORD_BYTES + 1)
            .read_until(b'\n', &mut bytes)
            .map_err(|error| format!("failed to read chat record: {error}"))?;
        if count == 0 {
            break;
        }
        if count as u64 > MAX_RECORD_BYTES {
            return Err("chat journal contains an oversized record".into());
        }
        if bytes.last() != Some(&b'\n') {
            reader
                .get_ref()
                .set_len(committed_offset)
                .map_err(|error| {
                    format!("failed to repair incomplete chat journal tail: {error}")
                })?;
            break;
        }
        #[cfg(test)]
        RECORD_READS.with(|reads| reads.set(reads.get() + 1));
        let event: ChatEvent = serde_json::from_slice(&bytes).map_err(|error| {
            format!("corrupt committed chat record at byte {committed_offset}: {error}")
        })?;
        if event.schema_version != CHAT_SCHEMA_VERSION
            || event.seq != expected_seq
            || event.seq > MAX_SEQUENCE
        {
            return Err("chat journal version or sequence is invalid".into());
        }
        visit(event, committed_offset)?;
        expected_seq += 1;
        committed_offset += count as u64;
    }
    Ok(())
}

pub(super) fn load(
    directory: &Path,
    project_key: &str,
    session_id: &str,
) -> Result<ChatSession, String> {
    let path = directory.join("snapshot.json");
    validate_file(&path)?;
    let cached = match fs::read_to_string(&path) {
        Ok(raw) => match serde_json::from_str::<serde_json::Value>(&raw) {
            Ok(value) => {
                if value
                    .get("schemaVersion")
                    .and_then(|v| v.as_u64())
                    .is_some_and(|v| v != CHAT_SCHEMA_VERSION as u64)
                {
                    return Err("unsupported chat snapshot schema version".into());
                }
                if value
                    .get("id")
                    .and_then(|v| v.as_str())
                    .is_some_and(|id| id != session_id)
                    || value
                        .get("projectPath")
                        .and_then(|v| v.as_str())
                        .is_some_and(|path| path != project_key)
                {
                    return Err("chat snapshot identity does not match its project/session".into());
                }
                serde_json::from_value::<ChatSession>(value).ok()
            }
            Err(_) => None,
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(format!("failed to inspect chat snapshot: {error}")),
    };
    let mut projection: Option<ChatSession> = None;
    scan(directory, |event| {
        if event.project_key != project_key || event.session_id != session_id {
            return Err("chat journal identity mismatch".into());
        }
        if let Some(session) = &mut projection {
            apply(session, &event)?;
        } else if let ChatEventPayload::SessionCreated(session) = event.body {
            if session.id != session_id
                || session.project_path != project_key
                || session.schema_version != CHAT_SCHEMA_VERSION
                || session.last_seq != 1
                || session.revision != 1
            {
                return Err("invalid chat creation snapshot".into());
            }
            validate_receipts(&session, &session)?;
            projection = Some(*session);
        } else {
            return Err("chat journal is missing its creation record".into());
        }
        Ok(())
    })?;
    let session = projection.ok_or("chat journal has no committed creation record")?;
    if cached
        .as_ref()
        .is_some_and(|cached| cached.last_seq > session.last_seq)
    {
        return Err(
            "chat journal is shorter than the saved snapshot; refusing to discard history".into(),
        );
    }
    if cached.as_ref().map(projection_json).transpose()? != Some(projection_json(&session)?) {
        let _ = checkpoint(directory, &session);
    }
    Ok(session)
}

pub(super) fn events(
    directory: &Path,
    project_key: &str,
    session_id: &str,
    after: u64,
    limit: usize,
) -> Result<ChatEventPage, String> {
    let limit = limit.clamp(1, 500);
    let mut events = Vec::new();
    let mut last_seq = 0;
    let mut has_more = false;
    let mut bytes = 0usize;
    scan(directory, |event| {
        if event.project_key != project_key || event.session_id != session_id {
            return Err("chat journal identity mismatch".into());
        }
        last_seq = event.seq;
        if event.seq > after {
            let size = serde_json::to_vec(&event)
                .map_err(|error| error.to_string())?
                .len();
            if !has_more
                && events.len() < limit
                && (events.is_empty() || bytes + size <= 2 * 1024 * 1024)
            {
                bytes += size;
                events.push(event);
            } else {
                has_more = true;
            }
        }
        Ok(())
    })?;
    if after > last_seq {
        return Err("chat event cursor is ahead of the journal".into());
    }
    Ok(ChatEventPage {
        events,
        has_more,
        last_seq,
    })
}
