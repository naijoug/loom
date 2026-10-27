//! Bounded, contained reads of a directory tree for source viewers, and
//! `loom-src:` reference validation. Shared by features that cite source files
//! (Learn studies a checkout, Research a registered project); no processes here.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Read,
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
    if is_binary(&path) {
        return Err("Binary file cannot be displayed".into());
    }
    let bytes = fs::read(&path).map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&bytes);
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let total = lines.len() as u32;
    let language = language_of(&normalized).to_string();
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

pub fn is_binary(path: &Path) -> bool {
    let mut buffer = [0; 8192];
    fs::File::open(path)
        .and_then(|mut file| file.read(&mut buffer))
        .map(|n| buffer[..n].contains(&0))
        .unwrap_or(true)
}

pub fn language_of(path: &str) -> &'static str {
    let name = path.rsplit('/').next().unwrap_or(path);
    if name == "Dockerfile" {
        return "Dockerfile";
    }
    if name == "Makefile" {
        return "Makefile";
    }
    match name
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("rs") => "Rust",
        Some("go") => "Go",
        Some("py") => "Python",
        Some("ts" | "mts" | "cts") => "TypeScript",
        Some("tsx") => "TSX",
        Some("js" | "mjs" | "cjs") => "JavaScript",
        Some("jsx") => "JSX",
        Some("c" | "h") => "C",
        Some("cc" | "cpp" | "cxx" | "hpp" | "hh") => "C++",
        Some("java") => "Java",
        Some("kt" | "kts") => "Kotlin",
        Some("swift") => "Swift",
        Some("m" | "mm") => "Objective-C",
        Some("rb") => "Ruby",
        Some("php") => "PHP",
        Some("cs") => "C#",
        Some("scala") => "Scala",
        Some("ex" | "exs") => "Elixir",
        Some("lua") => "Lua",
        Some("dart") => "Dart",
        Some("zig") => "Zig",
        Some("vue") => "Vue",
        Some("svelte") => "Svelte",
        Some("sh" | "bash" | "zsh" | "fish") => "Shell",
        Some("vim") => "Vim script",
        Some("sql") => "SQL",
        Some("html" | "htm") => "HTML",
        Some("css" | "scss" | "less") => "CSS",
        Some("md" | "markdown" | "rst" | "txt") => "Text",
        Some("json" | "yaml" | "yml" | "toml" | "xml" | "ini") => "Config",
        _ => "Other",
    }
}

/// Finds every `](loom-src:TARGET)` link and validates it against the checkout.
pub fn page_refs(markdown: &str, checkout: &Path) -> Vec<SourceRef> {
    let mut refs = Vec::new();
    let mut rest = markdown;
    while let Some(start) = rest.find("](loom-src:") {
        let body = &rest[start + "](loom-src:".len()..];
        let Some(end) = body.find(')') else { break };
        let target = body[..end].trim().to_string();
        rest = &body[end..];
        if refs.iter().any(|r: &SourceRef| r.target == target) {
            continue;
        }
        refs.push(check_target(&target, checkout));
    }
    refs
}

pub fn parse_target(target: &str) -> (String, Option<(u32, u32)>) {
    let (path, fragment) = target.split_once('#').unwrap_or((target, ""));
    let range = fragment.strip_prefix('L').and_then(|lines| {
        let (a, b) = lines
            .split_once("-L")
            .or_else(|| lines.split_once('-'))
            .unwrap_or((lines, lines));
        Some((a.parse().ok()?, b.parse().ok()?))
    });
    (path.to_string(), range)
}

fn check_target(target: &str, checkout: &Path) -> SourceRef {
    let (path, range) = parse_target(target);
    let (start, end) = range.unwrap_or((0, 0));
    let status = match normalize(&path) {
        Err(_) => "outside".to_string(),
        Ok(normalized) if normalized.is_empty() => "missing".into(),
        Ok(normalized) => match line_count(checkout, &normalized) {
            Err(_) => "missing".into(),
            Ok(total) if range.is_none() || (start >= 1 && start <= end && end <= total) => {
                "valid".into()
            }
            Ok(_) => "range".into(),
        },
    };
    SourceRef {
        target: target.into(),
        path: normalize(&path).unwrap_or(path),
        start_line: start,
        end_line: end,
        status,
    }
}

pub fn ref_stats(refs: &[SourceRef]) -> RefStats {
    let valid = refs.iter().filter(|r| r.status == "valid").count();
    RefStats {
        valid,
        invalid: refs.len() - valid,
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RefStats {
    pub valid: usize,
    pub invalid: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourceRef {
    /// Raw link target after `loom-src:`.
    pub target: String,
    pub path: String,
    pub start_line: u32,
    pub end_line: u32,
    /// `valid`, `missing`, `range` or `outside`.
    pub status: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceText {
    pub path: String,
    pub text: String,
    pub first_line: u32,
    pub total_lines: u32,
    pub truncated: bool,
    pub language: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeEntry {
    pub name: String,
    pub path: String,
    /// `dir`, `file` or `symlink`.
    pub kind: String,
    pub size: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root() -> PathBuf {
        static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "loom-source-files-test-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn paths_are_contained_and_symlinks_refused() {
        let root = temp_root();
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
        let root = temp_root();
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
