//! Bounded, read-only skill discovery. A malformed package is an issue, not an
//! empty successful library, and no source scripts or symlinks are executed.
use super::{
    error::{Result, SkillsError},
    files,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

pub const MAX_TEXT_BYTES: usize = 256 * 1024;
pub const MAX_ENTRIES: usize = 20_000;
const MAX_DEPTH: usize = 32;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillSummary {
    pub id: String,
    pub source_id: String,
    pub relative_path: String,
    pub name: String,
    pub title: String,
    pub description: String,
    pub category: String,
    pub keywords: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelatedFile {
    pub relative_path: String,
    pub size_bytes: u64,
    pub content: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDetail {
    pub summary: SkillSummary,
    pub content: String,
    pub manifest: BTreeMap<String, String>,
    pub related_files: Vec<RelatedFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogIssue {
    pub source_id: String,
    pub relative_path: String,
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Catalog {
    pub skills: Vec<SkillSummary>,
    pub issues: Vec<CatalogIssue>,
}

pub fn skip_directory(name: &str) -> bool {
    name.starts_with('.')
        || matches!(
            name,
            "node_modules" | "target" | "dist" | "__pycache__" | "archive" | "venv"
        )
}

pub fn scan_root(root: &Path) -> Result<PathBuf> {
    let root = root.canonicalize()?;
    if !root.is_dir() || root.parent().is_none() {
        return Err(SkillsError::invalid(
            "Choose a skill or repository directory, not a filesystem root",
        ));
    }
    let nested = root.join("skills");
    if !root.join("SKILL.md").exists()
        && fs::symlink_metadata(&nested).is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink())
        && contains_skill(&nested)?
    {
        Ok(nested)
    } else {
        Ok(root)
    }
}

fn contains_skill(root: &Path) -> Result<bool> {
    let mut found = false;
    walk(root, root, 0, &mut 0, &mut |path| {
        found |= path.file_name().is_some_and(|name| name == "SKILL.md");
        Ok(())
    })?;
    Ok(found)
}

pub fn text(path: &Path) -> Result<String> {
    files::regular_file(path)?;
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(MAX_TEXT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_TEXT_BYTES {
        return Err(SkillsError::invalid("Skill text exceeds 256 KiB"));
    }
    String::from_utf8(bytes).map_err(|_| SkillsError::invalid("Skill text is not UTF-8"))
}

pub fn resolve_file(root: &Path, relative: &str) -> Result<PathBuf> {
    let relative = Path::new(relative);
    if relative.as_os_str().is_empty()
        || relative.is_absolute()
        || relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(SkillsError::invalid(
            "Expected a relative package file path",
        ));
    }
    let mut result = root.canonicalize()?;
    for component in relative.components() {
        result.push(component);
        if fs::symlink_metadata(&result)?.file_type().is_symlink() {
            return Err(SkillsError::invalid(
                "Skill paths must not traverse symbolic links",
            ));
        }
    }
    Ok(result)
}

pub fn scan(source_id: &str, directory: &Path) -> Result<Catalog> {
    files::validate_id(source_id)?;
    let root = scan_root(directory)?;
    scan_repository(source_id, &root)
}

/// GitHub API identities retain full repository-relative paths, including
/// sources that have both a conventional skills/ folder and other packages.
pub fn scan_repository(source_id: &str, directory: &Path) -> Result<Catalog> {
    files::validate_id(source_id)?;
    let root = directory.canonicalize()?;
    let mut result = Catalog::default();
    let mut count = 0;
    walk(&root, &root, 0, &mut count, &mut |path| {
        if path.file_name().is_some_and(|name| name == "SKILL.md") {
            let relative = path
                .strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            match read_summary(source_id, &root, &relative) {
                Ok(summary) => result.skills.push(summary),
                Err(error) => result.issues.push(CatalogIssue {
                    source_id: source_id.into(),
                    relative_path: relative,
                    message: error.message,
                }),
            }
        }
        Ok(())
    })?;
    result.skills.sort_by(|a, b| {
        a.title
            .to_lowercase()
            .cmp(&b.title.to_lowercase())
            .then(a.id.cmp(&b.id))
    });
    Ok(result)
}

fn walk(
    root: &Path,
    directory: &Path,
    depth: usize,
    count: &mut usize,
    visit: &mut dyn FnMut(&Path) -> Result<()>,
) -> Result<()> {
    if depth > MAX_DEPTH {
        return Err(SkillsError::invalid(
            "Skill directory nesting exceeds 32 levels",
        ));
    }
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        *count += 1;
        if *count > MAX_ENTRIES {
            return Err(SkillsError::invalid("Skill source exceeds 20,000 entries"));
        }
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            if !skip_directory(&entry.file_name().to_string_lossy()) {
                walk(root, &entry.path(), depth + 1, count, visit)?;
            }
        } else if kind.is_file() && entry.path().starts_with(root) {
            visit(&entry.path())?;
        }
    }
    Ok(())
}

pub fn detail(source_id: &str, directory: &Path, relative: &str) -> Result<SkillDetail> {
    let root = scan_root(directory)?;
    detail_repository(source_id, &root, relative)
}

pub fn detail_repository(source_id: &str, directory: &Path, relative: &str) -> Result<SkillDetail> {
    let root = directory.canonicalize()?;
    let path = resolve_file(&root, relative)?;
    if path.file_name().is_none_or(|name| name != "SKILL.md") {
        return Err(SkillsError::invalid("Expected a SKILL.md entry"));
    }
    let summary = read_summary(source_id, &root, relative)?;
    let package = path.parent().unwrap();
    let mut related_files = Vec::new();
    let mut count = 0;
    let mut text_budget = 1024 * 1024;
    walk(package, package, 0, &mut count, &mut |file| {
        if file == path {
            return Ok(());
        }
        if related_files.len() >= 500 {
            return Err(SkillsError::invalid(
                "Skill package has more than 500 preview files",
            ));
        }
        let size_bytes = fs::metadata(file)?.len();
        let content = if size_bytes <= (MAX_TEXT_BYTES.min(text_budget)) as u64 {
            text(file).ok().inspect(|value| text_budget -= value.len())
        } else {
            None
        };
        related_files.push(RelatedFile {
            relative_path: file
                .strip_prefix(package)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/"),
            size_bytes,
            content,
        });
        Ok(())
    })?;
    related_files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    let manifest = read_manifest(package)?;
    Ok(SkillDetail {
        summary,
        content: text(&path)?,
        manifest,
        related_files,
    })
}

fn read_manifest(package: &Path) -> Result<BTreeMap<String, String>> {
    let path = package.join("skill.yaml");
    match fs::symlink_metadata(&path) {
        Ok(_) => metadata(&text(&path)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(e) => Err(e.into()),
    }
}

fn read_summary(source_id: &str, root: &Path, relative: &str) -> Result<SkillSummary> {
    let path = resolve_file(root, relative)?;
    let content = text(&path)?;
    let (frontmatter, body) = frontmatter(&content)?;
    let manifest = read_manifest(path.parent().unwrap())?;
    let get = |values: &BTreeMap<String, String>, key: &str| {
        values.get(key).filter(|v| !v.trim().is_empty()).cloned()
    };
    let heading = body
        .lines()
        .find_map(|line| line.strip_prefix("# "))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    let fallback = path
        .parent()
        .and_then(Path::file_name)
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Skill".into());
    let title = get(&manifest, "title")
        .or_else(|| get(&frontmatter, "title"))
        .or(heading)
        .or_else(|| get(&frontmatter, "name"))
        .or_else(|| get(&manifest, "id"))
        .unwrap_or(fallback);
    let name = get(&frontmatter, "name")
        .or_else(|| get(&manifest, "id"))
        .unwrap_or_else(|| title.clone());
    let description = get(&frontmatter, "description")
        .or_else(|| get(&manifest, "summary"))
        .unwrap_or_else(|| {
            body.lines()
                .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
                .take(3)
                .collect::<Vec<_>>()
                .join(" ")
        });
    let category = category_for(relative);
    Ok(SkillSummary {
        id: encode_id(source_id, relative),
        source_id: source_id.into(),
        relative_path: relative.into(),
        name,
        title,
        description: description
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(1000)
            .collect(),
        category,
        keywords: manifest.values().cloned().collect::<Vec<_>>().join(" "),
    })
}

pub(crate) fn frontmatter(content: &str) -> Result<(BTreeMap<String, String>, String)> {
    let mut lines = content.lines();
    if lines.next().map(str::trim) != Some("---") {
        return Ok((BTreeMap::new(), content.into()));
    }
    let mut header = Vec::new();
    for line in lines.by_ref() {
        if line.trim() == "---" {
            return Ok((
                metadata(&header.join("\n"))?,
                lines.collect::<Vec<_>>().join("\n"),
            ));
        }
        header.push(line);
    }
    Err(SkillsError::invalid("Unterminated skill frontmatter"))
}

/// Preserve the source manager's scalar/list YAML subset, including common
/// folded/literal descriptions. Not a YAML evaluator; tags/aliases are inert.
pub fn metadata(input: &str) -> Result<BTreeMap<String, String>> {
    let mut values = BTreeMap::<String, String>::new();
    let mut current = String::new();
    let mut block = false;
    for line in input.lines() {
        let stripped = line.trim();
        if stripped.is_empty() || stripped.starts_with('#') {
            continue;
        }
        if !current.is_empty()
            && ((block && line.starts_with(char::is_whitespace)) || stripped.starts_with("- "))
        {
            let part = stripped
                .strip_prefix("- ")
                .unwrap_or(stripped)
                .trim_matches(['\'', '"']);
            let value = values.entry(current.clone()).or_default();
            if !value.is_empty() {
                value.push(' ');
            }
            value.push_str(part);
            continue;
        }
        if let Some((key, value)) = stripped.split_once(':') {
            if key.is_empty() {
                return Err(SkillsError::invalid("Empty skill metadata key"));
            }
            current = key.trim().into();
            block = matches!(value.trim(), ">" | ">-" | "|" | "|-");
            values.insert(
                current.clone(),
                if block {
                    String::new()
                } else {
                    value.trim().trim_matches(['\'', '"']).into()
                },
            );
        }
    }
    Ok(values)
}

pub fn encode_id(source: &str, relative: &str) -> String {
    fn encode(value: &str) -> String {
        let mut result = String::new();
        for b in value.bytes() {
            if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
                result.push(b as char);
            } else {
                result.push_str(&format!("%{b:02X}"));
            }
        }
        result
    }
    format!("{}::{}", encode(source), encode(relative))
}

pub fn decode_id(id: &str) -> Result<(String, String)> {
    fn decode(value: &str) -> Result<String> {
        let bytes = value.as_bytes();
        let mut decoded = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'%' {
                let hex = bytes
                    .get(i + 1..i + 3)
                    .ok_or_else(|| SkillsError::invalid("Invalid skill ID encoding"))?;
                let hex = std::str::from_utf8(hex)
                    .map_err(|_| SkillsError::invalid("Invalid skill ID"))?;
                decoded.push(
                    u8::from_str_radix(hex, 16)
                        .map_err(|_| SkillsError::invalid("Invalid skill ID encoding"))?,
                );
                i += 3;
            } else {
                decoded.push(bytes[i]);
                i += 1;
            }
        }
        String::from_utf8(decoded).map_err(|_| SkillsError::invalid("Invalid skill ID UTF-8"))
    }
    if id.len() > 8192 {
        return Err(SkillsError::invalid("Skill ID too long"));
    }
    let (source, relative) = id
        .split_once("::")
        .ok_or_else(|| SkillsError::invalid("Invalid skill identity"))?;
    Ok((decode(source)?, decode(relative)?))
}

/// Preserve the old Manager's display category while retaining full source IDs.
pub fn category_for(relative: &str) -> String {
    let relative = relative.strip_prefix("skills/").unwrap_or(relative);
    let mut parts = relative.split('/').collect::<Vec<_>>();
    parts.pop();
    if parts.len() > 1 {
        parts.pop();
    }
    if parts.is_empty() {
        "uncategorized".into()
    } else {
        parts.join("/")
    }
}

/// Activation policy shared by instruction management and CLI filtering.
pub fn activation_category(relative: &str, metadata: &BTreeMap<String, String>) -> String {
    let first = relative
        .strip_prefix("skills/")
        .unwrap_or(relative)
        .split('/')
        .next()
        .unwrap_or("");
    if ["global", "auto", "manual", "cron"].contains(&first) {
        return first.into();
    }
    if let Some(category) = metadata.get("category") {
        return category.clone();
    }
    match metadata.get("activation").map(String::as_str) {
        Some("always_on" | "auto") => "auto",
        Some("scheduled" | "cron") => "cron",
        Some("global") => "global",
        _ => "manual",
    }
    .into()
}
