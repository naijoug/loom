//! Package copying and fingerprints never follow an installed symlink.
use super::{
    catalog,
    error::{Result, SkillsError},
    files,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

const MAX_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EntryState {
    Missing,
    File { digest: String, mode: u32 },
    Directory { digest: String },
    Symlink { target: PathBuf },
}

pub fn entry_path(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(SkillsError::invalid(
            "Expected absolute entry without parent traversal",
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| SkillsError::invalid("Cannot mutate filesystem root"))?;
    let name = path
        .file_name()
        .ok_or_else(|| SkillsError::invalid("Missing entry name"))?;
    Ok(files::resource_path(parent)?.join(name))
}

fn mode(meta: &fs::Metadata) -> u32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o777
    }
    #[cfg(not(unix))]
    {
        if meta.is_dir() {
            0o700
        } else if meta.permissions().readonly() {
            0o400
        } else {
            0o600
        }
    }
}

pub fn state(path: &Path) -> Result<EntryState> {
    let meta = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(EntryState::Missing),
        Err(e) => return Err(e.into()),
    };
    if meta.file_type().is_symlink() {
        return Ok(EntryState::Symlink {
            target: fs::read_link(path)?,
        });
    }
    if meta.is_file() {
        let mut hash = Sha256::new();
        let mut bytes = 0;
        hash_file(path, &mut hash, &mut bytes)?;
        return Ok(EntryState::File {
            digest: format!("{:x}", hash.finalize()),
            mode: mode(&meta),
        });
    }
    if meta.is_dir() {
        return Ok(EntryState::Directory {
            digest: tree_digest(path, false)?,
        });
    }
    Err(SkillsError::invalid(
        "Special filesystem entries cannot be managed as skills",
    ))
}

fn excluded(name: &std::ffi::OsStr) -> bool {
    matches!(
        name.to_str(),
        Some(".git" | "node_modules" | "__pycache__" | ".DS_Store")
    )
}

fn hash_file(path: &Path, hash: &mut Sha256, bytes: &mut u64) -> Result<()> {
    let mut file = fs::File::open(path)?;
    let mut buffer = [0; 16384];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        *bytes += n as u64;
        if *bytes > MAX_BYTES {
            return Err(SkillsError::invalid("Skill package exceeds 128 MiB"));
        }
        hash.update(&buffer[..n]);
    }
    Ok(())
}

/// Source normalization matches copy(). Installed copies have no exclusions so
/// user-added files and permission changes are protected during uninstall.
pub fn tree_digest(root: &Path, source: bool) -> Result<String> {
    fn walk(
        path: &Path,
        hash: &mut Sha256,
        source: bool,
        depth: usize,
        count: &mut usize,
        bytes: &mut u64,
    ) -> Result<()> {
        if depth > 32 {
            return Err(SkillsError::invalid(
                "Skill package exceeds directory depth limit",
            ));
        }
        let meta = fs::symlink_metadata(path)?;
        if meta.file_type().is_symlink() {
            if source {
                return Err(SkillsError::invalid(
                    "Source package contains a symbolic link",
                ));
            }
            hash.update(b"link");
            let target = fs::read_link(path)?;
            let target = target
                .to_str()
                .ok_or_else(|| SkillsError::invalid("Non-UTF-8 symbolic link target"))?;
            hash.update((target.len() as u64).to_le_bytes());
            hash.update(target.as_bytes());
            return Ok(());
        }
        let permissions = if source {
            if meta.is_dir() {
                0o700
            } else {
                0o600 | (mode(&meta) & 0o100)
            }
        } else {
            mode(&meta)
        };
        hash.update(permissions.to_le_bytes());
        if meta.is_dir() {
            hash.update(b"dir");
            let mut entries = fs::read_dir(path)?.collect::<std::io::Result<Vec<_>>>()?;
            entries.sort_by_key(|e| e.file_name());
            for entry in entries {
                if source && excluded(&entry.file_name()) {
                    continue;
                }
                *count += 1;
                if *count > catalog::MAX_ENTRIES {
                    return Err(SkillsError::invalid("Skill package exceeds entry limit"));
                }
                let name = entry
                    .file_name()
                    .into_string()
                    .map_err(|_| SkillsError::invalid("Non-UTF-8 package filename"))?;
                hash.update((name.len() as u64).to_le_bytes());
                hash.update(name.as_bytes());
                walk(&entry.path(), hash, source, depth + 1, count, bytes)?;
            }
            hash.update(b"end-directory");
        } else if meta.is_file() {
            hash.update(b"file");
            hash.update(meta.len().to_le_bytes());
            hash_file(path, hash, bytes)?;
        } else {
            return Err(SkillsError::invalid(
                "Skill package contains a special file",
            ));
        }
        Ok(())
    }
    let mut hash = Sha256::new();
    walk(root, &mut hash, source, 0, &mut 0, &mut 0)?;
    Ok(format!("{:x}", hash.finalize()))
}

pub fn copy(source: &Path, destination: &Path) -> Result<()> {
    fn walk(
        source: &Path,
        dest: &Path,
        depth: usize,
        count: &mut usize,
        bytes: &mut u64,
    ) -> Result<()> {
        if depth > 32 {
            return Err(SkillsError::invalid(
                "Skill package exceeds directory depth limit",
            ));
        }
        let meta = fs::symlink_metadata(source)?;
        if meta.file_type().is_symlink() {
            return Err(SkillsError::invalid("Cannot copy a source symbolic link"));
        }
        if meta.is_dir() {
            fs::create_dir(dest)?;
            set_mode(dest, 0o700)?;
            for entry in fs::read_dir(source)? {
                let entry = entry?;
                if excluded(&entry.file_name()) {
                    continue;
                }
                *count += 1;
                if *count > catalog::MAX_ENTRIES {
                    return Err(SkillsError::invalid("Skill package exceeds entry limit"));
                }
                walk(
                    &entry.path(),
                    &dest.join(entry.file_name()),
                    depth + 1,
                    count,
                    bytes,
                )?;
            }
            files::sync_directory(dest)?;
        } else if meta.is_file() {
            let mut input = fs::File::open(source)?;
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(dest)?;
            set_mode(dest, 0o600 | (mode(&meta) & 0o100))?;
            let mut buffer = [0; 16384];
            loop {
                let n = input.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                *bytes += n as u64;
                if *bytes > MAX_BYTES {
                    return Err(SkillsError::invalid("Skill package exceeds 128 MiB"));
                }
                output.write_all(&buffer[..n])?;
            }
            output.sync_all()?;
        } else {
            return Err(SkillsError::invalid(
                "Cannot copy a special filesystem entry",
            ));
        }
        Ok(())
    }
    if source == destination || destination.starts_with(source) {
        return Err(SkillsError::invalid("Installation overlaps its source"));
    }
    if fs::symlink_metadata(destination).is_ok() {
        return Err(SkillsError::invalid("Staging destination already exists"));
    }
    walk(source, destination, 0, &mut 0, &mut 0)
}

pub fn set_mode(path: &Path, value: u32) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(value))?;
    }
    #[cfg(not(unix))]
    {
        let _ = (path, value);
    }
    Ok(())
}

pub fn remove_entry(path: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(path)?;
    if meta.is_dir() && !meta.file_type().is_symlink() {
        fs::remove_dir_all(path)?;
    } else {
        fs::remove_file(path)?;
    }
    files::sync_directory(
        path.parent()
            .ok_or_else(|| SkillsError::invalid("Missing parent"))?,
    )
}

pub fn symlink(source: &Path, dest: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(source, dest)?;
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_dir(source, dest)?;
    }
    #[cfg(not(any(unix, windows)))]
    {
        return Err(SkillsError::invalid(
            "Directory links are unsupported on this platform",
        ));
    }
    Ok(())
}
