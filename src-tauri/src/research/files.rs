//! Research-owned files: bounded reads, symlink rejection and durable publication.
use serde::{de::DeserializeOwned, Serialize};
use std::{
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

#[cfg(test)]
thread_local! { static FAIL_WRITE: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) }; }
#[cfg(test)]
pub fn fail_next_write_to(name: &str) {
    FAIL_WRITE.with(|f| *f.borrow_mut() = Some(name.into()));
}

pub fn directory(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => Ok(()),
        Ok(_) => Err("unsafe_research_path: expected an ordinary directory".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if let Some(parent) = path.parent() {
                directory(parent)?;
            }
            match fs::create_dir(path) {
                Ok(()) => {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
                            .map_err(|e| e.to_string())?;
                    }
                    Ok(())
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => directory(path),
                Err(e) => Err(e.to_string()),
            }
        }
        Err(e) => Err(e.to_string()),
    }
}

pub fn path(root: &Path, relative: &str) -> Result<PathBuf, String> {
    if relative.is_empty()
        || relative.contains(['\\', '\0'])
        || relative
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err("unsafe_research_path".into());
    }
    let mut path = root.to_path_buf();
    if fs::symlink_metadata(root).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err("unsafe_research_path".into());
    }
    for part in Path::new(relative).components() {
        match part {
            Component::Normal(p) if p != ".git" => path.push(p),
            _ => return Err("unsafe_research_path".into()),
        }
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err("unsafe_research_path: symlink".into())
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(path)
}

pub fn read_bytes(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let meta = fs::symlink_metadata(path).map_err(|e| format!("research_file_unavailable: {e}"))?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > limit {
        return Err("unsafe_or_oversized_research_file".into());
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit {
        return Err("research_file_limit".into());
    }
    Ok(bytes)
}

pub fn read_json<T: DeserializeOwned>(path: &Path, limit: u64) -> Result<T, String> {
    serde_json::from_slice(&read_bytes(path, limit)?)
        .map_err(|e| format!("invalid_research_json: {e}"))
}

pub fn write_bytes(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or("invalid_research_parent")?;
    directory(parent)?;
    if let Ok(meta) = fs::symlink_metadata(path) {
        if !meta.is_file() || meta.file_type().is_symlink() {
            return Err("unsafe_research_file".into());
        }
    }
    let temporary = parent.join(super::store::next_id("pending"));
    let result = (|| {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary).map_err(|e| e.to_string())?;
        #[cfg(test)]
        if FAIL_WRITE.with(|f| {
            let mut expected = f.borrow_mut();
            if expected.as_deref() == path.file_name().and_then(|s| s.to_str()) {
                expected.take();
                true
            } else {
                false
            }
        }) {
            return Err(std::io::Error::from_raw_os_error(28).to_string());
        }
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        fs::rename(&temporary, path).map_err(|e| e.to_string())?;
        fs::File::open(parent)
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    write_bytes(path, &bytes)
}
