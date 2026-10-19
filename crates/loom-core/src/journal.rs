use crate::{
    projection::checksum, CoreError, ErrorCode, EventEnvelope, Result, MAX_SAFE_SEQUENCE,
    SCHEMA_VERSION,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_RECORD_BYTES: u64 = 8 * 1024 * 1024;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);
pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(MAX_SAFE_SEQUENCE as u128) as u64
}

pub(crate) fn directory(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => Ok(()),
        Ok(_) => Err(CoreError::corrupt("data directory is not a real directory")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            match builder.create(path) {
                Ok(()) => sync_parent(path),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => directory(path),
                Err(e) => Err(e.into()),
            }
        }
        Err(error) => Err(error.into()),
    }
}
pub(crate) fn regular(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_file() && !meta.file_type().is_symlink() => Ok(()),
        Ok(_) => Err(CoreError::corrupt(
            "data file must be a regular file, not a link",
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}
pub(crate) fn options() -> OpenOptions {
    let mut options = OpenOptions::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    options
}
pub(crate) fn sync_parent(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        File::open(
            path.parent()
                .ok_or_else(|| CoreError::invalid("missing parent directory"))?,
        )?
        .sync_all()?;
    }
    Ok(())
}
pub(crate) fn atomic_write(path: &Path, value: &impl Serialize) -> Result<()> {
    regular(path)?;
    let temp = path.with_file_name(format!(
        ".tmp-{}-{}",
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let mut created = false;
    let result = (|| -> Result<()> {
        let mut file = options().write(true).create_new(true).open(&temp)?;
        created = true;
        serde_json::to_writer(&mut file, value)
            .map_err(|e| CoreError::new(ErrorCode::StorageUnavailable, e.to_string()))?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        fs::rename(&temp, path)?;
        sync_parent(path)?;
        Ok(())
    })();
    if result.is_err() && created {
        let _ = fs::remove_file(temp);
    }
    result
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Stamp {
    pub len: u64,
    modified_ns: u128,
    #[cfg(unix)]
    identity: (u64, u64, i64, i64),
}
impl Stamp {
    fn from_metadata(meta: fs::Metadata) -> Result<Self> {
        Ok(Self {
            len: meta.len(),
            modified_ns: meta
                .modified()?
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            #[cfg(unix)]
            identity: {
                use std::os::unix::fs::MetadataExt;
                (meta.dev(), meta.ino(), meta.ctime(), meta.ctime_nsec())
            },
        })
    }
}
pub(crate) fn stamp(path: &Path) -> Result<Stamp> {
    regular(path)?;
    Stamp::from_metadata(fs::metadata(path)?)
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CommitHead {
    schema_version: u32,
    stream: String,
    seq: u64,
    record_checksum: String,
    checksum: String,
}
fn write_head(directory: &Path, stream: &str, seq: u64, record_checksum: &str) -> Result<()> {
    let mut head = CommitHead {
        schema_version: SCHEMA_VERSION,
        stream: stream.to_owned(),
        seq,
        record_checksum: record_checksum.to_owned(),
        checksum: String::new(),
    };
    head.checksum = checksum(&head)?;
    atomic_write(&directory.join("commit.json"), &head)
}
pub(crate) fn read_bounded(path: &Path, max: u64) -> Result<Vec<u8>> {
    regular(path)?;
    let file = options().read(true).open(path)?;
    if file.metadata()?.len() > max {
        return Err(CoreError::corrupt("data file exceeds its size budget"));
    }
    let mut bytes = Vec::new();
    file.take(max + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max {
        return Err(CoreError::corrupt("data file grew beyond its size budget"));
    }
    Ok(bytes)
}
fn read_head(directory: &Path, stream: &str) -> Result<Option<CommitHead>> {
    let path = directory.join("commit.json");
    regular(&path)?;
    if !path.exists() {
        return Ok(None);
    }
    let mut head: CommitHead = serde_json::from_slice(&read_bounded(&path, 4096)?)
        .map_err(|e| CoreError::corrupt(e.to_string()))?;
    let saved = std::mem::take(&mut head.checksum);
    if head.schema_version != SCHEMA_VERSION || head.stream != stream || saved != checksum(&head)? {
        return Err(CoreError::corrupt("commit watermark is invalid"));
    }
    head.checksum = saved;
    Ok(Some(head))
}
pub(crate) fn committed_sequence(directory: &Path, stream: &str) -> Result<Option<u64>> {
    Ok(read_head(directory, stream)?.map(|head| head.seq))
}

struct Offset {
    start: u64,
    end: u64,
    checksum: String,
}
pub(crate) struct Journal<T> {
    directory: PathBuf,
    path: PathBuf,
    file: File,
    stream: String,
    offsets: Vec<Offset>,
    stamp: Stamp,
    poisoned: bool,
    marker: PhantomData<T>,
}
impl<T: Serialize + DeserializeOwned + Clone> Journal<T> {
    pub fn open(
        dir: &Path,
        stream: String,
        create: bool,
        mut visit: impl FnMut(&EventEnvelope<T>) -> Result<()>,
    ) -> Result<Self> {
        directory(dir)?;
        let path = dir.join("events.jsonl");
        regular(&path)?;
        let mut file = options()
            .read(true)
            .write(true)
            .create_new(create)
            .open(&path)?;
        if create {
            file.sync_all()?;
            sync_parent(&path)?;
        }
        let mut offsets = Vec::new();
        let mut previous = String::new();
        let mut position = 0;
        // A distinct descriptor keeps the append descriptor's cursor independent.
        let mut reader = BufReader::new(options().read(true).open(&path)?);
        loop {
            let mut bytes = Vec::new();
            let size = reader
                .by_ref()
                .take(MAX_RECORD_BYTES + 1)
                .read_until(b'\n', &mut bytes)?;
            if size == 0 {
                break;
            }
            if size as u64 > MAX_RECORD_BYTES {
                return Err(CoreError::corrupt("journal record exceeds 8 MiB"));
            }
            if !bytes.ends_with(b"\n") {
                let tail_name = format!("incomplete-tail-{}.bin", checksum(&bytes)?);
                let tail_path = dir.join(tail_name);
                regular(&tail_path)?;
                match options().write(true).create_new(true).open(&tail_path) {
                    Ok(mut backup) => {
                        backup.write_all(&bytes)?;
                        backup.sync_all()?;
                        sync_parent(&tail_path)?;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                        if fs::read(&tail_path)? != bytes {
                            return Err(CoreError::corrupt("tail backup content mismatch"));
                        }
                    }
                    Err(error) => return Err(error.into()),
                }
                file.set_len(position)?;
                file.sync_data()?;
                break;
            }
            let event: EventEnvelope<T> = serde_json::from_slice(&bytes).map_err(|e| {
                CoreError::corrupt(format!("invalid journal JSON at byte {position}: {e}"))
            })?;
            Self::verify(&event, &stream, offsets.len() as u64 + 1, &previous)?;
            visit(&event).map_err(|e| {
                CoreError::corrupt(format!(
                    "invalid journal transition at seq {}: {}",
                    event.seq, e.message
                ))
            })?;
            previous = event.checksum.clone();
            offsets.push(Offset {
                start: position,
                end: position + size as u64,
                checksum: previous.clone(),
            });
            position += size as u64;
        }
        if let Some(head) = read_head(dir, &stream)? {
            if head.seq > offsets.len() as u64
                || head.record_checksum
                    != if head.seq == 0 {
                        ""
                    } else {
                        &offsets[head.seq as usize - 1].checksum
                    }
            {
                return Err(CoreError::corrupt(
                    "journal lost previously committed history",
                ));
            }
        }
        write_head(dir, &stream, offsets.len() as u64, &previous)?;
        file.seek(SeekFrom::End(0))?;
        let stamp = stamp(&path)?;
        Ok(Self {
            directory: dir.to_owned(),
            path,
            file,
            stream,
            offsets,
            stamp,
            poisoned: false,
            marker: PhantomData,
        })
    }
    fn verify(event: &EventEnvelope<T>, stream: &str, seq: u64, previous: &str) -> Result<()> {
        if event.schema_version != SCHEMA_VERSION
            || event.seq != seq
            || seq > MAX_SAFE_SEQUENCE
            || event.stream != stream
            || event.previous_checksum != previous
            || event.timestamp_ms > MAX_SAFE_SEQUENCE
        {
            return Err(CoreError::corrupt(
                "journal version, sequence or scope mismatch",
            ));
        }
        let mut body = event.clone();
        body.checksum.clear();
        if checksum(&body)? != event.checksum {
            return Err(CoreError::corrupt("journal checksum mismatch"));
        }
        Ok(())
    }
    pub fn last_seq(&self) -> u64 {
        self.offsets.len() as u64
    }
    pub fn stamp(&self) -> &Stamp {
        &self.stamp
    }
    pub fn head_checksum(&self) -> &str {
        self.offsets
            .last()
            .map(|o| o.checksum.as_str())
            .unwrap_or("")
    }
    pub fn current(&self) -> Result<()> {
        if self.poisoned {
            return Err(CoreError::new(
                ErrorCode::StorageUnavailable,
                "journal write outcome is uncertain; close and reopen the store",
            ));
        }
        directory(&self.directory)?;
        if stamp(&self.path)? != self.stamp
            || Stamp::from_metadata(self.file.metadata()?)? != self.stamp
        {
            return Err(CoreError::corrupt(
                "journal changed outside its active writer",
            ));
        }
        Ok(())
    }
    pub fn prepare(&self, payload: T, timestamp_ms: u64) -> Result<EventEnvelope<T>> {
        self.current()?;
        let seq = self.last_seq() + 1;
        if seq > MAX_SAFE_SEQUENCE {
            return Err(CoreError::conflict("journal sequence limit exceeded"));
        }
        let mut event = EventEnvelope {
            schema_version: SCHEMA_VERSION,
            seq,
            stream: self.stream.clone(),
            timestamp_ms,
            previous_checksum: self.head_checksum().to_owned(),
            payload,
            checksum: String::new(),
        };
        event.checksum = checksum(&event)?;
        Ok(event)
    }
    pub fn append(&mut self, event: &EventEnvelope<T>) -> Result<()> {
        self.current()?;
        Self::verify(
            event,
            &self.stream,
            self.last_seq() + 1,
            self.head_checksum(),
        )?;
        let mut bytes = serde_json::to_vec(event).map_err(|e| CoreError::invalid(e.to_string()))?;
        bytes.push(b'\n');
        if bytes.len() as u64 > MAX_RECORD_BYTES {
            return Err(CoreError::invalid("event exceeds 8 MiB"));
        }
        let start = self.stamp.len;
        self.poisoned = true;
        self.file.seek(SeekFrom::End(0))?;
        self.file.write_all(&bytes)?;
        self.file.flush()?;
        self.file.sync_data()?;
        // An acknowledgement requires both durable bytes and a durable lower
        // bound for detecting a later truncated log. Failure poisons this writer;
        // reopening reconciles any complete but unacknowledged record.
        write_head(&self.directory, &self.stream, event.seq, &event.checksum)?;
        self.stamp = stamp(&self.path)?;
        self.offsets.push(Offset {
            start,
            end: self.stamp.len,
            checksum: event.checksum.clone(),
        });
        self.poisoned = false;
        Ok(())
    }
    pub fn page(&mut self, after: u64, limit: usize) -> Result<(Vec<EventEnvelope<T>>, bool)> {
        self.current()?;
        if after > self.last_seq() {
            return Err(CoreError::invalid("event cursor is ahead of the journal"));
        }
        let end = self.offsets.len().min(after as usize + limit.clamp(1, 500));
        let mut events = Vec::new();
        let mut total_bytes = 0;
        for index in after as usize..end {
            let offset = &self.offsets[index];
            let size = offset.end - offset.start;
            if !events.is_empty() && total_bytes + size > 2 * 1024 * 1024 {
                break;
            }
            self.file.seek(SeekFrom::Start(offset.start))?;
            let mut bytes = vec![0; size as usize];
            self.file.read_exact(&mut bytes)?;
            let event: EventEnvelope<T> =
                serde_json::from_slice(&bytes).map_err(|e| CoreError::corrupt(e.to_string()))?;
            Self::verify(
                &event,
                &self.stream,
                index as u64 + 1,
                if index == 0 {
                    ""
                } else {
                    &self.offsets[index - 1].checksum
                },
            )?;
            events.push(event);
            total_bytes += size;
        }
        let more = after + (events.len() as u64) < self.last_seq();
        Ok((events, more))
    }
}
