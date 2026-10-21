//! Private, durable metadata files. Refuse symlinks rather than following them.
use super::error::{Result, SkillsError};
use serde::{de::DeserializeOwned, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);
pub const MAX_METADATA_BYTES: u64 = 8 * 1024 * 1024;

pub fn validate_id(id: &str) -> Result<()> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(SkillsError::invalid(
            "Expected a single bounded alphanumeric identifier",
        ));
    }
    Ok(())
}

/// Canonicalize an existing ancestor and append only normal, absent components.
/// Existing final symlinks are rejected: callers must explicitly authorize a
/// canonical resource before requesting a write, not swap its link silently.
pub fn resource_path(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute() || path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(SkillsError::invalid(
            "Resource paths must be absolute without parent traversal",
        ));
    }
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => Err(SkillsError::invalid(
            "Managed resource must not be a symlink",
        )),
        Ok(_) => Ok(path.canonicalize()?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let parent = path
                .parent()
                .ok_or_else(|| SkillsError::invalid("Missing parent"))?;
            let name = path
                .file_name()
                .ok_or_else(|| SkillsError::invalid("Missing filename"))?;
            Ok(resource_path(parent)?.join(name))
        }
        Err(e) => Err(e.into()),
    }
}

pub fn private_directory(path: &Path) -> Result<PathBuf> {
    let path = resource_path(path)?;
    if !path.exists() {
        let parent = path
            .parent()
            .ok_or_else(|| SkillsError::invalid("Missing directory parent"))?;
        if !parent.is_dir() {
            private_directory(parent)?;
        }
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        match builder.create(&path) {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(e) => return Err(e.into()),
        }
    }
    let meta = fs::symlink_metadata(&path)?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(SkillsError::invalid(
            "Private metadata path must be a directory",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(path)
}

pub fn regular_file(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_file() && !meta.file_type().is_symlink() => Ok(()),
        Ok(_) => Err(SkillsError::invalid("Metadata must be a regular file")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let bytes = read_metadata_bytes(path)?;
    serde_json::from_slice(&bytes)
        .map_err(|_| SkillsError::recovery("Metadata is malformed; refusing to treat it as empty"))
}

pub fn read_metadata_bytes(path: &Path) -> Result<Vec<u8>> {
    regular_file(path)?;
    let file = File::open(path)?;
    let mut bytes = Vec::new();
    file.take(MAX_METADATA_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_METADATA_BYTES {
        return Err(SkillsError::recovery("Metadata exceeds the size limit"));
    }
    Ok(bytes)
}

pub fn atomic_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|_| SkillsError::invalid("Cannot encode metadata"))?;
    if bytes.len() as u64 > MAX_METADATA_BYTES {
        return Err(SkillsError::invalid("Metadata exceeds the size limit"));
    }
    atomic_bytes(path, &bytes)
}

pub fn atomic_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    regular_file(path)?;
    let parent = path
        .parent()
        .ok_or_else(|| SkillsError::invalid("Missing metadata parent"))?;
    let parent = resource_path(parent)?;
    if !parent.is_dir() {
        return Err(SkillsError::invalid("Metadata parent must already exist"));
    }
    let temp = parent.join(format!(
        ".write-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| -> Result<()> {
        let mut file = options.open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        regular_file(path)?;
        fs::rename(&temp, path)?;
        sync_directory(&parent)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

pub fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        File::open(path)?.sync_all()?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
