//! Immutable remote contents: publishing a new revision never changes a path
//! already used by an installation. Source staging is supplied by the runner.
use super::{
    catalog,
    coordination::WriterGuard,
    error::{ErrorCode, Result, SkillsError},
    files,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

const MAX_PACKAGE_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Snapshot {
    pub schema_version: u32,
    pub source_id: String,
    pub revision: String,
    pub digest: String,
    pub file_count: usize,
}

pub fn snapshot_path(root: &Path, source_id: &str, revision: &str) -> Result<PathBuf> {
    files::validate_id(source_id)?;
    files::validate_id(revision)?;
    Ok(root
        .join("sources")
        .join(source_id)
        .join("snapshots")
        .join(revision))
}

pub fn prepare(source: &Path, staging: &Path, source_id: &str, revision: &str) -> Result<Snapshot> {
    if source.join(".gitmodules").exists() {
        return Err(SkillsError::invalid(
            "Submodule sources require materialization before import",
        ));
    }
    files::validate_id(source_id)?;
    files::validate_id(revision)?;
    if staging.exists() {
        return Err(SkillsError::new(
            ErrorCode::Conflict,
            "Snapshot staging already exists",
        ));
    }
    let staging = files::private_directory(staging)?;
    let result = (|| {
        let contents = files::private_directory(&staging.join("contents"))?;
        let mut total = 0;
        let mut entries = 0;
        copy_package(source, &contents, 0, &mut total, &mut entries)?;
        let scanned = catalog::scan(source_id, &contents)?;
        if scanned.skills.is_empty() || !scanned.issues.is_empty() {
            return Err(SkillsError::invalid(
                "Repository snapshot must contain valid skills without unreadable packages",
            ));
        }
        let (digest, file_count) = fingerprint(&contents)?;
        let snapshot = Snapshot {
            schema_version: 1,
            source_id: source_id.into(),
            revision: revision.into(),
            digest,
            file_count,
        };
        files::atomic_json(&staging.join("snapshot.json"), &snapshot)?;
        files::sync_directory(&staging)?;
        Ok(snapshot)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

pub fn publish(
    root: &Path,
    staging: &Path,
    snapshot: &Snapshot,
    writer: &WriterGuard<'_>,
) -> Result<PathBuf> {
    let destination = snapshot_path(root, &snapshot.source_id, &snapshot.revision)?;
    writer.check_write_set(std::slice::from_ref(&destination))?;
    if destination.exists() {
        let existing = verify(&destination)?;
        if existing.digest != snapshot.digest {
            return Err(SkillsError::new(
                ErrorCode::Conflict,
                "Existing revision has different contents",
            ));
        }
        fs::remove_dir_all(staging)?;
        return Ok(destination.join("contents"));
    }
    files::private_directory(destination.parent().unwrap())?;
    verify(staging)?;
    fs::rename(staging, &destination)?;
    files::sync_directory(destination.parent().unwrap())?;
    Ok(destination.join("contents"))
}

pub fn verify(path: &Path) -> Result<Snapshot> {
    let expected: Snapshot = files::read_json(&path.join("snapshot.json"))?;
    if expected.schema_version != 1 {
        return Err(SkillsError::recovery("Unsupported snapshot version"));
    }
    files::validate_id(&expected.source_id)?;
    files::validate_id(&expected.revision)?;
    let (actual, count) = fingerprint(&path.join("contents"))?;
    if actual != expected.digest || count != expected.file_count {
        return Err(SkillsError::recovery(
            "Snapshot contents changed; repair required",
        ));
    }
    Ok(expected)
}

fn copy_package(
    source: &Path,
    dest: &Path,
    depth: usize,
    bytes: &mut u64,
    entries: &mut usize,
) -> Result<()> {
    if depth > 32 {
        return Err(SkillsError::invalid(
            "Snapshot exceeds directory depth limit",
        ));
    }
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let name = entry.file_name();
        // Git internals and generated dependencies are not skill package files.
        if matches!(
            name.to_str(),
            Some(".git" | "node_modules" | "__pycache__" | ".DS_Store")
        ) {
            continue;
        }
        *entries += 1;
        if *entries > catalog::MAX_ENTRIES {
            return Err(SkillsError::invalid("Snapshot exceeds entry limit"));
        }
        let meta = fs::symlink_metadata(entry.path())?;
        if meta.file_type().is_symlink() || (!meta.is_dir() && !meta.is_file()) {
            return Err(SkillsError::invalid(
                "Remote snapshots cannot contain symlinks or special files",
            ));
        }
        let target = dest.join(&name);
        if meta.is_dir() {
            files::private_directory(&target)?;
            copy_package(&entry.path(), &target, depth + 1, bytes, entries)?;
            files::sync_directory(&target)?;
        } else {
            *bytes = bytes.saturating_add(meta.len());
            if *bytes > MAX_PACKAGE_BYTES {
                return Err(SkillsError::invalid("Snapshot exceeds 128 MiB"));
            }
            let mut prefix = [0u8; 128];
            let length = fs::File::open(entry.path())?.read(&mut prefix)?;
            if prefix[..length].starts_with(b"version https://git-lfs.github.com/spec/v1") {
                return Err(SkillsError::invalid(
                    "Git LFS objects must be materialized before import",
                ));
            }
            fs::copy(entry.path(), &target)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(
                    &target,
                    fs::Permissions::from_mode(0o600 | (meta.permissions().mode() & 0o100)),
                )?;
            }
            fs::File::open(&target)?.sync_all()?;
        }
    }
    Ok(())
}

pub fn fingerprint(root: &Path) -> Result<(String, usize)> {
    fn walk(
        root: &Path,
        path: &Path,
        hash: &mut Sha256,
        count: &mut usize,
        bytes: &mut u64,
        depth: usize,
    ) -> Result<()> {
        if depth > 32 {
            return Err(SkillsError::invalid(
                "Snapshot exceeds directory depth limit",
            ));
        }
        let mut entries = fs::read_dir(path)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let meta = fs::symlink_metadata(entry.path())?;
            *count += 1;
            if *count > catalog::MAX_ENTRIES {
                return Err(SkillsError::invalid("Snapshot exceeds entry limit"));
            }
            if meta.file_type().is_symlink() {
                return Err(SkillsError::recovery("Snapshot contains a symbolic link"));
            }
            let relative = entry
                .path()
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            hash.update((relative.len() as u64).to_le_bytes());
            hash.update(relative.as_bytes());
            if meta.is_dir() {
                hash.update(b"directory");
                walk(root, &entry.path(), hash, count, bytes, depth + 1)?;
            } else if meta.is_file() {
                hash.update(b"file");
                hash.update(meta.len().to_le_bytes());
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    hash.update((meta.permissions().mode() & 0o111).to_le_bytes());
                }
                let mut file = fs::File::open(entry.path())?;
                let mut buffer = [0; 16 * 1024];
                loop {
                    let n = file.read(&mut buffer)?;
                    if n == 0 {
                        break;
                    }
                    *bytes += n as u64;
                    if *bytes > MAX_PACKAGE_BYTES {
                        return Err(SkillsError::invalid("Snapshot exceeds 128 MiB"));
                    }
                    hash.update(&buffer[..n]);
                }
            } else {
                return Err(SkillsError::invalid("Unsupported snapshot file"));
            }
        }
        Ok(())
    }
    if fs::symlink_metadata(root)?.file_type().is_symlink() {
        return Err(SkillsError::recovery(
            "Snapshot root must not be a symbolic link",
        ));
    }
    let mut hash = Sha256::new();
    let mut count = 0;
    let mut bytes = 0;
    walk(root, root, &mut hash, &mut count, &mut bytes, 0)?;
    Ok((format!("{:x}", hash.finalize()), count))
}
