//! Private, durable metadata files. Refuse symlinks rather than following them.
use super::error::{Result, SkillsError};
use crate::storage::private as storage;
use serde::{de::DeserializeOwned, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};
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

pub fn resource_path(path: &Path) -> Result<PathBuf> {
    storage::resource_path(path).map_err(skills_error)
}

pub fn private_directory(path: &Path) -> Result<PathBuf> {
    storage::private_directory(path).map_err(skills_error)
}

pub fn regular_file(path: &Path) -> Result<()> {
    storage::regular_file(path).map_err(skills_error)
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
    storage::atomic_bytes(path, bytes).map_err(skills_error)
}

pub fn sync_directory(path: &Path) -> Result<()> {
    storage::sync_directory(path).map_err(skills_error)
}

/// Shared refusals keep the invalid-input code; OS failures map as before.
fn skills_error(error: std::io::Error) -> SkillsError {
    match storage::refusal(&error) {
        Some(message) => SkillsError::invalid(message),
        None => error.into(),
    }
}

pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
