//! Bounded, contained reads of a study checkout for the source viewer.
use super::{inventory, models::*};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

pub const FULL_READ_BYTES: u64 = 512 * 1024;
const WINDOW_LINES: u32 = 2_000;
const MAX_READ_BYTES: u64 = 16 * 1024 * 1024;

/// Normalizes a repository-relative path; rejects absolute, `..`, `.git` and
/// NUL paths. Returns the forward-slash form.
pub fn normalize(relative: &str) -> Result<String, String> {
    let trimmed = relative.trim().trim_start_matches("./");
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    if trimmed.contains('\0') || trimmed.contains('\\') {
        return Err("Invalid path".into());
    }
    let mut parts = Vec::new();
    for component in Path::new(trimmed).components() {
        match component {
            Component::Normal(part) => {
                let part = part.to_str().ok_or("Invalid path")?;
                if part == ".git" {
                    return Err("Path is outside the repository content".into());
                }
                parts.push(part);
            }
            Component::CurDir => {}
            _ => return Err("Path is outside the repository content".into()),
        }
    }
    Ok(parts.join("/"))
}

/// Resolves `relative` inside `checkout`, refusing any symlink on the way.
pub fn resolve(checkout: &Path, relative: &str) -> Result<PathBuf, String> {
    let normalized = normalize(relative)?;
    let mut current = checkout.to_path_buf();
    for part in normalized.split('/').filter(|p| !p.is_empty()) {
        current.push(part);
        let meta = fs::symlink_metadata(&current).map_err(|_| "File not found".to_string())?;
        if meta.file_type().is_symlink() {
            return Err("Symlinks are not followed".into());
        }
    }
    Ok(current)
}

pub fn list_tree(checkout: &Path, dir: &str) -> Result<Vec<TreeEntry>, String> {
    let normalized = normalize(dir)?;
    let path = resolve(checkout, &normalized)?;
    if !path.is_dir() {
        return Err("Not a directory".into());
    }
    let mut entries = Vec::new();
    for entry in fs::read_dir(&path).map_err(|e| e.to_string())?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == ".git" {
            continue;
        }
        let Ok(meta) = fs::symlink_metadata(entry.path()) else {
            continue;
        };
        let kind = if meta.file_type().is_symlink() {
            "symlink"
        } else if meta.is_dir() {
            "dir"
        } else {
            "file"
        };
        entries.push(TreeEntry {
            path: if normalized.is_empty() {
                name.clone()
            } else {
                format!("{normalized}/{name}")
            },
            name,
            kind: kind.into(),
            size: if meta.is_file() { meta.len() } else { 0 },
        });
    }
    entries.sort_by(|a, b| (a.kind != "dir", &a.name).cmp(&(b.kind != "dir", &b.name)));
    Ok(entries)
}

/// Whole file up to 512 KiB; larger files return a line window around the
/// requested range. Binary files are refused.
pub fn read_source(
    checkout: &Path,
    relative: &str,
    start_line: Option<u32>,
    end_line: Option<u32>,
) -> Result<SourceText, String> {
    let normalized = normalize(relative)?;
    let path = resolve(checkout, &normalized)?;
    let meta = fs::metadata(&path).map_err(|_| "File not found".to_string())?;
    if !meta.is_file() {
        return Err("Not a file".into());
    }
    if meta.len() > MAX_READ_BYTES {
        return Err("File is too large to display".into());
    }
    if inventory::is_binary(&path) {
        return Err("Binary file cannot be displayed".into());
    }
    let bytes = fs::read(&path).map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&bytes);
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let total = lines.len() as u32;
    let language = inventory::language_of(&normalized).to_string();
    if meta.len() <= FULL_READ_BYTES {
        return Ok(SourceText {
            path: normalized,
            text: text.into_owned(),
            first_line: 1,
            total_lines: total,
            truncated: false,
            language,
        });
    }
    let start = start_line.unwrap_or(1).max(1);
    let end = end_line.unwrap_or(start).max(start);
    let middle = start + (end - start) / 2;
    let first = middle
        .saturating_sub(WINDOW_LINES / 2)
        .max(1)
        .min(total.max(1));
    let last = (first + WINDOW_LINES - 1).min(total);
    let window = lines[(first - 1) as usize..last as usize].concat();
    Ok(SourceText {
        path: normalized,
        text: window,
        first_line: first,
        total_lines: total,
        truncated: true,
        language,
    })
}

/// Line count of a contained regular text file, for reference validation.
pub fn line_count(checkout: &Path, relative: &str) -> Result<u32, String> {
    let path = resolve(checkout, relative)?;
    let meta = fs::metadata(&path).map_err(|_| "missing".to_string())?;
    if !meta.is_file() || meta.len() > MAX_READ_BYTES {
        return Err("missing".into());
    }
    let bytes = fs::read(&path).map_err(|_| "missing".to_string())?;
    let newlines = bytes.iter().filter(|b| **b == b'\n').count();
    Ok((newlines + usize::from(!bytes.is_empty() && !bytes.ends_with(b"\n"))) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_contained_and_symlinks_refused() {
        let root = super::super::store::tests::temp_root();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join("src/a.rs"), "one\ntwo\nthree\n").unwrap();
        fs::write(root.join(".git/config"), "secret").unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("/etc", root.join("etc")).unwrap();
            std::os::unix::fs::symlink(root.join("src/a.rs"), root.join("alias.rs")).unwrap();
        }
        for bad in ["../x", "/etc/passwd", ".git/config", "src/../../x", "a\\b"] {
            assert!(read_source(&root, bad, None, None).is_err(), "read {bad}");
        }
        #[cfg(unix)]
        {
            assert!(read_source(&root, "etc/passwd", None, None).is_err());
            assert!(read_source(&root, "alias.rs", None, None).is_err());
        }
        let source = read_source(&root, "./src/a.rs", None, None).unwrap();
        assert_eq!(
            (source.total_lines, source.first_line, source.truncated),
            (3, 1, false)
        );
        assert_eq!(source.language, "Rust");
        assert_eq!(line_count(&root, "src/a.rs").unwrap(), 3);
        let tree = list_tree(&root, "").unwrap();
        let names: Vec<_> = tree
            .iter()
            .map(|e| (e.name.as_str(), e.kind.as_str()))
            .collect();
        assert!(names.contains(&("src", "dir")));
        assert!(!names.iter().any(|(n, _)| *n == ".git"));
        assert_eq!(list_tree(&root, "src").unwrap()[0].path, "src/a.rs");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn large_files_return_a_window_and_binary_is_refused() {
        let root = super::super::store::tests::temp_root();
        let line = "x".repeat(99) + "\n";
        fs::write(root.join("big.txt"), line.repeat(10_000)).unwrap();
        let window = read_source(&root, "big.txt", Some(5_000), Some(5_010)).unwrap();
        assert!(window.truncated);
        assert_eq!(window.total_lines, 10_000);
        assert!(window.first_line <= 5_000 && window.first_line + WINDOW_LINES > 5_010);
        fs::write(root.join("bin.dat"), [0u8, 159, 146]).unwrap();
        assert!(read_source(&root, "bin.dat", None, None).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
