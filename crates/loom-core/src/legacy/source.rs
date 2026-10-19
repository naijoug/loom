//! Read-only, descriptor-relative source access. No old storage/migration code.
use crate::{CoreError, Result};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::Read,
    path::Path,
};

pub(super) const MAX_FILE: usize = 32 * 1024 * 1024;
const MAX_TOTAL: usize = 128 * 1024 * 1024;
const MAX_ENTRIES: usize = 10_000;

pub(super) struct Source {
    root: File,
    pub project: String,
    pub files: BTreeMap<String, super::LegacyFile>,
    pub directories: BTreeMap<String, Vec<String>>,
    pub captured: BTreeMap<String, Vec<u8>>,
    pub touched: BTreeSet<String>,
    pub capture: bool,
    bytes: usize,
    entries: usize,
}

impl Source {
    pub fn open(path: &Path) -> Result<Self> {
        if !path.is_absolute() {
            return Err(CoreError::invalid(
                "legacy source must be an absolute project directory",
            ));
        }
        let canonical = path.canonicalize()?;
        if canonical != path {
            return Err(CoreError::conflict(
                "legacy project root is no longer canonical; re-register the project",
            ));
        }
        let path = canonical;
        let project = path
            .to_str()
            .ok_or_else(|| CoreError::invalid("project path is not UTF-8"))?
            .to_owned();
        Ok(Self {
            root: platform::root(&path)?,
            project,
            files: BTreeMap::new(),
            directories: BTreeMap::new(),
            captured: BTreeMap::new(),
            touched: BTreeSet::new(),
            capture: false,
            bytes: 0,
            entries: 0,
        })
    }
    pub fn list(&mut self, path: &str) -> Result<Vec<String>> {
        validate_path(path)?;
        let names = match platform::open(&self.root, path, true) {
            Ok(file) => platform::list(file, MAX_ENTRIES.saturating_sub(self.entries))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => vec![],
            Err(error) => return Err(error.into()),
        };
        self.entries += names.len();
        self.directories.insert(path.into(), names.clone());
        Ok(names)
    }
    pub fn read(&mut self, path: &str) -> Result<Option<Vec<u8>>> {
        validate_path(path)?;
        self.touched.insert(path.into());
        let mut file = match platform::open(&self.root, path, false) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                self.files.insert(
                    path.into(),
                    super::LegacyFile {
                        path: path.into(),
                        bytes: 0,
                        sha256: None,
                    },
                );
                return Ok(None);
            }
            Err(error) => return Err(error.into()),
        };
        let before = file.metadata()?;
        if !before.is_file() {
            return Err(CoreError::invalid("legacy resource is not a regular file"));
        }
        let limit = MAX_FILE.min(MAX_TOTAL.saturating_sub(self.bytes));
        if before.len() > limit as u64 {
            return Err(CoreError::invalid(
                "legacy read budget exceeded (32 MiB per file, 128 MiB per scan)",
            ));
        }
        let mut bytes = Vec::new();
        (&mut file).take(limit as u64 + 1).read_to_end(&mut bytes)?;
        self.bytes += bytes.len();
        if bytes.len() > limit {
            return Err(CoreError::invalid("legacy read budget exceeded"));
        }
        if !platform::unchanged(&before, &file.metadata()?) || bytes.len() as u64 != before.len() {
            return Err(CoreError::conflict(
                "legacy source changed while reading; retry preview",
            ));
        }
        let record = super::LegacyFile {
            path: path.into(),
            bytes: bytes.len() as u64,
            sha256: Some(format!("{:x}", Sha256::digest(&bytes))),
        };
        if self.files.get(path).is_some_and(|old| old != &record) {
            return Err(CoreError::conflict(
                "legacy source changed during scan; retry preview",
            ));
        }
        self.files.insert(path.into(), record);
        if self.capture {
            self.captured.insert(path.into(), bytes.clone());
        }
        Ok(Some(bytes))
    }
    pub fn required(&mut self, path: &str) -> Result<Vec<u8>> {
        self.read(path)?
            .ok_or_else(|| CoreError::missing("legacy source file is missing"))
    }
}

pub(super) fn validate_path(path: &str) -> Result<()> {
    if path.is_empty()
        || path.contains('\\')
        || path.contains('\0')
        || path
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == "..")
    {
        return Err(CoreError::invalid(
            "legacy path must remain inside the project without traversal",
        ));
    }
    Ok(())
}

#[cfg(unix)]
mod platform {
    use super::*;
    use std::{
        ffi::{CStr, CString},
        fs::{Metadata, OpenOptions},
        io,
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::fs::{MetadataExt, OpenOptionsExt},
        },
    };
    pub fn root(path: &Path) -> io::Result<File> {
        let root = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open("/")?;
        let relative = path
            .strip_prefix("/")
            .map_err(|_| io::Error::other("source root must be absolute"))?;
        if relative.as_os_str().is_empty() {
            return Ok(root);
        }
        open(
            &root,
            relative
                .to_str()
                .ok_or_else(|| io::Error::other("source root must be UTF-8"))?,
            true,
        )
    }
    pub fn open(root: &File, path: &str, directory: bool) -> io::Result<File> {
        let mut parent = root.try_clone()?;
        let parts: Vec<_> = path.split('/').collect();
        for (index, part) in parts.iter().enumerate() {
            let name = CString::new(*part).map_err(|_| io::Error::other("invalid path"))?;
            let flags = libc::O_RDONLY
                | libc::O_NOFOLLOW
                | libc::O_CLOEXEC
                | libc::O_NONBLOCK
                | if directory || index + 1 < parts.len() {
                    libc::O_DIRECTORY
                } else {
                    0
                };
            // Every component is opened relative to an already-open directory.
            // O_NONBLOCK also prevents a FIFO from stalling the regular-file check.
            let fd = unsafe { libc::openat(parent.as_raw_fd(), name.as_ptr(), flags) };
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }
            parent = unsafe { File::from_raw_fd(fd) };
        }
        Ok(parent)
    }
    pub fn list(file: File, budget: usize) -> Result<Vec<String>> {
        use std::os::fd::IntoRawFd;
        let fd = file.into_raw_fd();
        let raw = unsafe { libc::fdopendir(fd) };
        if raw.is_null() {
            let error = io::Error::last_os_error();
            unsafe {
                libc::close(fd);
            }
            return Err(error.into());
        }
        struct Directory(*mut libc::DIR);
        impl Drop for Directory {
            fn drop(&mut self) {
                unsafe {
                    libc::closedir(self.0);
                }
            }
        }
        let directory = Directory(raw);
        let mut names = Vec::new();
        loop {
            // readdir requires errno=0 to distinguish end-of-directory from failure.
            #[cfg(any(target_os = "macos", target_os = "ios", target_os = "freebsd"))]
            unsafe {
                *libc::__error() = 0;
            }
            #[cfg(any(target_os = "linux", target_os = "android"))]
            unsafe {
                *libc::__errno_location() = 0;
            }
            let entry = unsafe { libc::readdir(directory.0) };
            if entry.is_null() {
                let error = io::Error::last_os_error();
                if error.raw_os_error().unwrap_or(0) != 0 {
                    return Err(error.into());
                }
                break;
            }
            let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) }
                .to_str()
                .map_err(|_| CoreError::invalid("legacy directory entry is not UTF-8"))?;
            if name == "." || name == ".." {
                continue;
            }
            if names.len() >= budget {
                return Err(CoreError::invalid("legacy directory entry budget exceeded"));
            }
            names.push(name.to_owned());
        }
        names.sort();
        Ok(names)
    }
    pub fn unchanged(a: &Metadata, b: &Metadata) -> bool {
        (
            a.dev(),
            a.ino(),
            a.len(),
            a.mtime(),
            a.mtime_nsec(),
            a.ctime(),
            a.ctime_nsec(),
        ) == (
            b.dev(),
            b.ino(),
            b.len(),
            b.mtime(),
            b.mtime_nsec(),
            b.ctime(),
            b.ctime_nsec(),
        )
    }
}

#[cfg(not(unix))]
mod platform {
    use super::*;
    use std::{fs::Metadata, io};
    fn unsupported() -> io::Error {
        io::Error::other(
            "legacy read-only migration currently requires Unix descriptor-relative file access",
        )
    }
    pub fn root(_: &Path) -> io::Result<File> {
        Err(unsupported())
    }
    pub fn open(_: &File, _: &str, _: bool) -> io::Result<File> {
        Err(unsupported())
    }
    pub fn list(_: File, _: usize) -> Result<Vec<String>> {
        Err(unsupported().into())
    }
    pub fn unchanged(_: &Metadata, _: &Metadata) -> bool {
        false
    }
}
