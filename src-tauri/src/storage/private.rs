//! Private, durable files shared by Skills and the agent layer. Refuse
//! symlinks rather than following them.
use std::{
    fmt,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// A path or file this module refuses to touch, as opposed to an OS failure.
/// Callers that classify errors find it with `refusal`.
#[derive(Debug)]
pub struct Refusal(pub &'static str);
impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for Refusal {}

fn refused(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, Refusal(message))
}

pub fn refusal(error: &io::Error) -> Option<&'static str> {
    error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<Refusal>())
        .map(|refusal| refusal.0)
}

/// Canonicalize an existing ancestor and append only normal, absent components.
/// Existing final symlinks are rejected: callers must explicitly authorize a
/// canonical resource before requesting a write, not swap its link silently.
pub fn resource_path(path: &Path) -> io::Result<PathBuf> {
    if !path.is_absolute() || path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(refused(
            "Resource paths must be absolute without parent traversal",
        ));
    }
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            Err(refused("Managed resource must not be a symlink"))
        }
        Ok(_) => path.canonicalize(),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            let parent = path.parent().ok_or_else(|| refused("Missing parent"))?;
            let name = path
                .file_name()
                .ok_or_else(|| refused("Missing filename"))?;
            Ok(resource_path(parent)?.join(name))
        }
        Err(e) => Err(e),
    }
}

pub fn private_directory(path: &Path) -> io::Result<PathBuf> {
    let path = resource_path(path)?;
    if !path.exists() {
        let parent = path
            .parent()
            .ok_or_else(|| refused("Missing directory parent"))?;
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
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (),
            Err(e) => return Err(e),
        }
    }
    let meta = fs::symlink_metadata(&path)?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(refused("Private metadata path must be a directory"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(path)
}

pub fn regular_file(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_file() && !meta.file_type().is_symlink() => Ok(()),
        Ok(_) => Err(refused("Metadata must be a regular file")),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// Replace `path` with a 0600 file through a synced temporary in the same directory.
pub fn atomic_bytes(path: &Path, bytes: &[u8]) -> io::Result<()> {
    regular_file(path)?;
    let parent = path
        .parent()
        .ok_or_else(|| refused("Missing metadata parent"))?;
    let parent = resource_path(parent)?;
    if !parent.is_dir() {
        return Err(refused("Metadata parent must already exist"));
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
    let result = (|| -> io::Result<()> {
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

pub fn sync_directory(path: &Path) -> io::Result<()> {
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
