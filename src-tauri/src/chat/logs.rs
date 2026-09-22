//! Independent, bounded, redacted stdout/stderr. Paths are supplied only by the repository.
use super::{repository::validate_file, ChatLogPage, ChatLogStream};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};

const MAX_LOG_BYTES: usize = 8 * 1024 * 1024;

pub(crate) fn redact(value: &str) -> String {
    value
        .lines()
        .map(|line| {
            let lower = line.to_ascii_lowercase();
            if lower.contains("api-key") || lower.contains("authorization") {
                "[REDACTED sensitive line]".into()
            } else {
                crate::agents::redact_sensitive_text(line)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}
pub(super) struct RunLogs {
    stdout: File,
    stderr: File,
    bytes: usize,
}

impl RunLogs {
    pub(super) fn open(directory: &Path) -> Result<Self, String> {
        fn create(path: &Path) -> Result<File, String> {
            validate_file(path)?;
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            options
                .open(path)
                .map_err(|e| format!("cannot create chat run log: {e}"))
        }
        Ok(Self {
            stdout: create(&directory.join("stdout.log"))?,
            stderr: create(&directory.join("stderr.log"))?,
            bytes: 0,
        })
    }
    pub(super) fn append(&mut self, stderr: bool, line: &str) -> Result<(), String> {
        let line = redact(line);
        let size = line.len() + 1;
        if self.bytes.saturating_add(size) > MAX_LOG_BYTES {
            return Err("log_limit: chat run logs exceeded 8 MiB".into());
        }
        let file = if stderr {
            &mut self.stderr
        } else {
            &mut self.stdout
        };
        file.write_all(line.as_bytes())
            .and_then(|_| file.write_all(b"\n"))
            .and_then(|_| file.flush())
            .map_err(|e| format!("chat run log write failed: {e}"))?;
        self.bytes += size;
        Ok(())
    }
}

pub(super) fn read(
    directory: &Path,
    stream: ChatLogStream,
    offset: u64,
    limit: usize,
) -> Result<ChatLogPage, String> {
    if !(4..=65_536).contains(&limit) {
        return Err("log page limit must be 4–65536 bytes".into());
    }
    let path = directory.join(match stream {
        ChatLogStream::Stdout => "stdout.log",
        ChatLogStream::Stderr => "stderr.log",
    });
    validate_file(&path)?;
    let mut file = File::open(path).map_err(|e| format!("chat run log unavailable: {e}"))?;
    let len = file.metadata().map_err(|e| e.to_string())?.len();
    if offset > len {
        return Err("log cursor is beyond the end of the file".into());
    }
    file.seek(SeekFrom::Start(offset))
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take(limit as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    let text = match std::str::from_utf8(&bytes) {
        Ok(text) => text,
        Err(e) if e.error_len().is_none() => {
            std::str::from_utf8(&bytes[..e.valid_up_to()]).map_err(|e| e.to_string())?
        }
        Err(_) => return Err("invalid UTF-8 log or cursor is inside a character".into()),
    };
    let next_offset = offset + text.len() as u64;
    Ok(ChatLogPage {
        text: text.into(),
        next_offset,
        has_more: next_offset < len,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn log_limits_utf8_cursors_and_no_overwrite_are_enforced() {
        let path =
            std::env::temp_dir().join(crate::models::IdGenerator::default().next("loom-log-pages"));
        std::fs::create_dir_all(&path).unwrap();
        let mut writer = RunLogs::open(&path).unwrap();
        writer.append(false, "中abc").unwrap();
        writer
            .append(true, "X-API-Key: private-header-value")
            .unwrap();
        assert!(read(&path, ChatLogStream::Stdout, 1, 4).is_err());
        assert!(read(&path, ChatLogStream::Stdout, 999, 4).is_err());
        let page = read(&path, ChatLogStream::Stdout, 0, 4).unwrap();
        assert_eq!(page.text, "中a");
        assert_eq!(page.next_offset, 4);
        assert!(page.has_more);
        assert!(RunLogs::open(&path).is_err());
        assert!(!read(&path, ChatLogStream::Stderr, 0, 65536)
            .unwrap()
            .text
            .contains("private-header-value"));
        assert!(writer.append(false, &"x".repeat(MAX_LOG_BYTES)).is_err());
        assert_eq!(
            std::fs::read_to_string(path.join("stdout.log")).unwrap(),
            "中abc\n"
        );
        drop(writer);
        std::fs::remove_dir_all(path).unwrap();
    }
}
