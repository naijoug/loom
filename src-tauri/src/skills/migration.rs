//! M4-01: observational preview only. No Repository::open, locks, writes, network,
//! credentials import, or execute/apply token. Unverified ownership stays explicit.
use super::{
    error::{Result, SkillsError},
    files,
    repository::{self, SourceKind},
    sources,
    targets::TargetContext,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};
const MAX_FILE: u64 = 1024 * 1024;
const MAX_TOTAL: usize = 16 * 1024 * 1024;
const MAX_ITEMS: usize = 2000;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MigrationInput {
    #[serde(default)]
    pub data_dirs: Vec<PathBuf>,
    pub catalog_dir: Option<PathBuf>,
    pub project_path: Option<String>,
    #[serde(default)]
    pub include_global: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Issue {
    pub code: String,
    pub path: PathBuf,
    pub message: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Observation {
    pub path: PathBuf,
    pub state: String,
    pub digest: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourcePreview {
    pub source_id: String,
    pub legacy_ids: Vec<String>,
    pub kind: SourceKind,
    pub location: String,
    pub cache_path: Option<PathBuf>,
    pub cache_state: String,
    pub already_registered: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigPreview {
    pub path: PathBuf,
    pub provider: String,
    pub key_present: bool,
    pub model_present: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstallationPreview {
    pub target_id: String,
    pub manifest_path: PathBuf,
    pub name: String,
    pub mode: String,
    pub source_path: PathBuf,
    pub source_state: String,
    pub destination_state: String,
    pub link_target: Option<PathBuf>,
    pub ownership: String,
    pub next_step: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MigrationPreview {
    pub schema_version: u32,
    pub preview_id: String,
    pub read_only: bool,
    pub can_apply: bool,
    pub data_directories: Vec<PathBuf>,
    pub catalog_directory: Option<PathBuf>,
    pub sources: Vec<SourcePreview>,
    pub installations: Vec<InstallationPreview>,
    pub configs: Vec<ConfigPreview>,
    pub files: Vec<Observation>,
    pub issues: Vec<Issue>,
    pub complete: bool,
}
struct Scan {
    report: MigrationPreview,
    bytes: usize,
    items: usize,
}
impl Scan {
    fn issue(&mut self, code: &str, path: &Path, message: &str) {
        self.report.issues.push(Issue {
            code: code.into(),
            path: path.into(),
            message: message.into(),
        });
        self.report.complete = false;
    }
    fn room(&mut self, path: &Path) -> bool {
        if self.items >= MAX_ITEMS {
            if !self.report.issues.iter().any(|i| i.code == "item_limit") {
                self.issue(
                    "item_limit",
                    path,
                    "Preview item limit reached; split the requested scope",
                );
            }
            false
        } else {
            self.items += 1;
            true
        }
    }
    fn read(&mut self, root: &Path, path: &Path) -> Option<Vec<u8>> {
        let outcome = (|| -> std::io::Result<Option<Vec<u8>>> {
            safe_child(root, path)?;
            let metadata = match fs::symlink_metadata(path) {
                Ok(v) => v,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(e) => return Err(e),
            };
            if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > MAX_FILE
            {
                return Err(std::io::Error::other("Unsafe or oversized metadata"));
            }
            let mut bytes = vec![];
            fs::File::open(path)?
                .take(MAX_FILE + 1)
                .read_to_end(&mut bytes)?;
            if bytes.len() as u64 > MAX_FILE || self.bytes + bytes.len() > MAX_TOTAL {
                return Err(std::io::Error::other("Metadata budget exceeded"));
            }
            Ok(Some(bytes))
        })();
        match outcome {
            Ok(Some(bytes)) => {
                self.bytes += bytes.len();
                self.report.files.push(Observation {
                    path: path.into(),
                    state: "read".into(),
                    digest: Some(files::digest(&bytes)),
                });
                Some(bytes)
            }
            Ok(None) => {
                self.report.files.push(Observation {
                    path: path.into(),
                    state: "missing".into(),
                    digest: None,
                });
                None
            }
            Err(_) => {
                self.report.files.push(Observation {
                    path: path.into(),
                    state: "unreadable".into(),
                    digest: None,
                });
                self.issue(
                    "unreadable_metadata",
                    path,
                    "Metadata is unreadable, linked, outside scope, or exceeds the size budget",
                );
                None
            }
        }
    }
    fn json(&mut self, root: &Path, path: &Path) -> Option<Value> {
        let bytes = self.read(root, path)?;
        match serde_json::from_slice(&bytes) {
            Ok(v) => Some(v),
            Err(_) => {
                self.issue(
                    "malformed_metadata",
                    path,
                    "Malformed JSON; no values were imported",
                );
                None
            }
        }
    }
}
fn absolute(path: &Path) -> bool {
    path.is_absolute()
        && path.parent().is_some()
        && !path.components().any(|c| matches!(c, Component::ParentDir))
        && !path.to_string_lossy().chars().any(char::is_control)
        && path.as_os_str().len() <= 4096
}
fn root_path(path: &Path) -> Result<PathBuf> {
    if !absolute(path) {
        return Err(SkillsError::invalid(
            "Choose an absolute dedicated directory without parent traversal",
        ));
    }
    // Canonicalize only the explicitly granted root/ancestors; child links never expand the grant.
    let root = files::resource_path(path)?;
    if root.exists() && !root.is_dir() {
        return Err(SkillsError::invalid("Preview scope must be a directory"));
    }
    Ok(root)
}
fn safe_child(root: &Path, path: &Path) -> std::io::Result<()> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| std::io::Error::other("Outside scope"))?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        if !matches!(component, Component::Normal(_)) {
            return Err(std::io::Error::other("Invalid path"));
        }
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(m) if m.file_type().is_symlink() => {
                return Err(std::io::Error::other("Linked path"))
            }
            Ok(_) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e),
        }
    }
    Ok(())
}
fn path_state(roots: &[PathBuf], path: &Path) -> String {
    if !absolute(path) {
        return "invalid_path".into();
    }
    let Some(root) = roots.iter().find(|root| path.starts_with(root)) else {
        return "outside_scope_unchecked".into();
    };
    if safe_child(root, path).is_err() {
        return "linked_or_unreadable".into();
    }
    match fs::symlink_metadata(path) {
        Ok(m) if m.is_dir() => "directory_unverified",
        Ok(_) => "not_a_directory",
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => "missing",
        Err(_) => "unreadable",
    }
    .into()
}
// A user-selected root may use a platform alias such as /var -> /private/var.
// Rebase only that explicitly granted prefix, never canonicalize arbitrary references.
fn rebase(path: PathBuf, aliases: &[(PathBuf, PathBuf)]) -> PathBuf {
    for (raw, canonical) in aliases {
        if let Ok(relative) = path.strip_prefix(raw) {
            return canonical.join(relative);
        }
    }
    path
}
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\', ':'])
        && !name.chars().any(char::is_control)
}

pub fn preview(
    default_data: &Path,
    loom_data: &Path,
    targets: &TargetContext,
    input: &MigrationInput,
) -> Result<MigrationPreview> {
    if input.data_dirs.len() > 8 {
        return Err(SkillsError::invalid(
            "Preview at most eight legacy data directories at a time",
        ));
    }
    let requested = if input.data_dirs.is_empty() {
        vec![default_data.to_path_buf()]
    } else {
        input.data_dirs.clone()
    };
    let mut dirs = requested
        .iter()
        .map(|p| root_path(p))
        .collect::<Result<Vec<_>>>()?;
    dirs.sort();
    dirs.dedup();
    let catalog = input
        .catalog_dir
        .as_ref()
        .map(|p| root_path(p))
        .transpose()?;
    let mut aliases = requested
        .iter()
        .map(|raw| Ok((raw.clone(), root_path(raw)?)))
        .collect::<Result<Vec<_>>>()?;
    if let (Some(raw), Some(canonical)) = (&input.catalog_dir, &catalog) {
        aliases.push((raw.clone(), canonical.clone()));
    }
    aliases.sort_by_key(|(raw, _)| std::cmp::Reverse(raw.components().count()));
    let mut scan = Scan {
        bytes: 0,
        items: 0,
        report: MigrationPreview {
            schema_version: 1,
            preview_id: String::new(),
            read_only: true,
            can_apply: false,
            data_directories: dirs.clone(),
            catalog_directory: catalog.clone(),
            sources: vec![],
            installations: vec![],
            configs: vec![],
            files: vec![],
            issues: vec![],
            complete: true,
        },
    };
    let loom_root = root_path(loom_data)?;
    let loom_file = loom_root.join("library.json");
    let existing = if let Some(bytes) = scan.read(&loom_root, &loom_file) {
        match repository::parse_library(&bytes) {
            Ok(v) => v.sources,
            Err(_) => {
                scan.issue(
                    "loom_library_invalid",
                    &loom_file,
                    "Loom library cannot be compared safely",
                );
                vec![]
            }
        }
    } else {
        vec![]
    };
    let mut allowed_roots = dirs.clone();
    if let Some(c) = &catalog {
        allowed_roots.push(c.clone());
    }
    for root in &dirs {
        let path = root.join("library.json");
        if let Some(value) = scan.json(root, &path) {
            if value.get("version").and_then(Value::as_u64) != Some(1)
                || !value["repositories"].is_array()
            {
                scan.issue(
                    "unsupported_library",
                    &path,
                    "Expected legacy library version 1 with repositories",
                );
            } else {
                for item in value["repositories"].as_array().unwrap() {
                    if !scan.room(&path) {
                        break;
                    }
                    match sources::legacy_remote_descriptor(item) {
                        Ok(d) => {
                            let cache = d.cache_path.map(|p| rebase(p, &aliases)).or_else(|| {
                                d.cache_slug
                                    .as_ref()
                                    .map(|slug| root.join("repos").join(slug))
                            });
                            let state = cache
                                .as_ref()
                                .map(|p| path_state(std::slice::from_ref(root), p))
                                .unwrap_or_else(|| "not_recorded".into());
                            if d.kind != SourceKind::GithubApi && state != "directory_unverified" {
                                scan.issue("cache_unavailable", &path, "A legacy Git cache is missing or outside the granted data directory");
                            }
                            scan.report.sources.push(SourcePreview {
                                already_registered: existing.iter().any(|s| s.id == d.source_id),
                                source_id: d.source_id,
                                legacy_ids: d.legacy_ids,
                                kind: d.kind,
                                location: d.git.url,
                                cache_path: cache,
                                cache_state: state,
                            });
                        }
                        Err(_) => scan.issue(
                            "invalid_source",
                            &path,
                            "Invalid legacy source metadata; URL and values were not echoed",
                        ),
                    }
                }
            }
        }
        let path = root.join("config.json");
        if let Some(value) = scan.json(root, &path) {
            if !value.is_object()
                || value.get("version").is_some_and(|v| v.as_u64() != Some(1))
                || value.get("translation").is_some_and(|v| !v.is_object())
            {
                scan.issue(
                    "unsupported_config",
                    &path,
                    "Unsupported or malformed legacy configuration",
                );
            } else {
                for provider in ["openai", "openrouter", "codex", "claude-code"] {
                    if let Some(v) = value.get("translation").and_then(|v| v.get(provider)) {
                        if !v.is_object()
                            || ["apiKey", "model"].iter().any(|key| {
                                v.get(*key).is_some_and(|v| !v.is_null() && !v.is_string())
                            })
                        {
                            scan.issue(
                                "invalid_config",
                                &path,
                                "Invalid provider configuration; values were not echoed",
                            );
                            continue;
                        }
                        scan.report.configs.push(ConfigPreview {
                            path: path.clone(),
                            provider: provider.into(),
                            key_present: v["apiKey"].as_str().is_some_and(|s| !s.trim().is_empty()),
                            model_present: v["model"]
                                .as_str()
                                .is_some_and(|s| !s.trim().is_empty()),
                        });
                    }
                }
            }
        }
    }
    if let Some(root) = &catalog {
        if !root.is_dir() {
            scan.issue(
                "catalog_missing",
                root,
                "Explicit legacy catalog directory is missing",
            );
        }
        let source_id = format!(
            "src-{}",
            &files::digest(root.to_string_lossy().as_bytes())[..32]
        );
        scan.report.sources.push(SourcePreview {
            already_registered: existing.iter().any(|s| s.id == source_id),
            source_id,
            legacy_ids: vec!["local:workspace".into()],
            kind: SourceKind::Local,
            location: root.to_string_lossy().into(),
            cache_path: Some(root.clone()),
            cache_state: path_state(std::slice::from_ref(root), root),
        });
    }
    let mut source_aliases = BTreeMap::<String, String>::new();
    let mut identities = BTreeSet::new();
    for source in scan.report.sources.clone() {
        if !identities.insert(source.source_id.clone()) {
            scan.issue(
                "duplicate_source",
                &loom_file,
                "Multiple legacy records map to the same Loom source; review before merging",
            );
        }
        for alias in source.legacy_ids {
            if source_aliases
                .insert(alias.clone(), source.source_id.clone())
                .is_some_and(|old| old != source.source_id)
            {
                scan.issue(
                    "alias_conflict",
                    &loom_file,
                    "A legacy alias maps to different source identities; manual review required",
                );
            }
            if existing.iter().any(|s| {
                s.id != source.source_id && s.git.as_ref().is_some_and(|g| g.legacy_id == alias)
            }) {
                scan.issue(
                    "existing_alias_conflict",
                    &loom_file,
                    "Legacy alias conflicts with an existing Loom source",
                );
            }
        }
    }
    for target in targets.targets()? {
        if target.scope == "global" && !input.include_global {
            continue;
        }
        if target.blocked_reason.is_some() {
            scan.issue(
                "target_blocked",
                &target.skills_dir,
                "Target directory contains an unsafe path; skipped without following it",
            );
            continue;
        }
        let root = &target.skills_dir;
        let json_path = root.join(".skills-linker-manifest.json");
        let tsv_path = root.join(".skills-linker-manifest.tsv");
        let json = scan.json(root, &json_path);
        let tsv = scan.read(root, &tsv_path);
        if json.is_some() && tsv.is_some() {
            scan.issue(
                "multiple_manifests",
                root,
                "Both JSON and TSV manifests exist; ownership is ambiguous",
            );
        }
        let mut names = BTreeSet::new();
        if let Some(value) = json {
            match value["version"].as_u64() {
                Some(1) if value["skills"].is_object() => {
                    for (name, entry) in value["skills"].as_object().unwrap() {
                        if !scan.room(&json_path) {
                            break;
                        }
                        if let (Some(mode), Some(source)) =
                            (entry["mode"].as_str(), entry["source"].as_str())
                        {
                            installation(
                                &mut scan,
                                &target.id,
                                root,
                                &json_path,
                                name,
                                mode,
                                source,
                                &allowed_roots,
                                &aliases,
                                &mut names,
                            );
                        } else {
                            scan.issue(
                                "invalid_installation",
                                &json_path,
                                "Invalid legacy installation fields",
                            );
                        }
                    }
                }
                Some(2) => scan.issue(
                    "already_v2",
                    &json_path,
                    "Version 2 detected; validity and ownership are unverified and excluded from legacy adoption",
                ),
                _ => scan.issue(
                    "unsupported_manifest",
                    &json_path,
                    "Unsupported or malformed installation manifest; left unchanged",
                ),
            }
        }
        if let Some(bytes) = tsv {
            match std::str::from_utf8(&bytes) {
                Ok(text) => {
                    for line in text.lines().filter(|s| !s.trim().is_empty()) {
                        if !scan.room(&tsv_path) {
                            break;
                        }
                        let fields = line.split('\t').collect::<Vec<_>>();
                        if fields.len() == 4 {
                            installation(
                                &mut scan,
                                &target.id,
                                root,
                                &tsv_path,
                                fields[0],
                                fields[1],
                                fields[2],
                                &allowed_roots,
                                &aliases,
                                &mut names,
                            );
                        } else {
                            scan.issue(
                                "invalid_tsv",
                                &tsv_path,
                                "Expected four legacy TSV columns",
                            );
                        }
                    }
                }
                Err(_) => scan.issue("invalid_tsv", &tsv_path, "Legacy TSV is not UTF-8"),
            }
        }
    }
    scan.report
        .sources
        .sort_by(|a, b| a.source_id.cmp(&b.source_id));
    scan.report.preview_id = format!(
        "preview-{}",
        files::digest(
            &serde_json::to_vec(&scan.report)
                .map_err(|_| SkillsError::invalid("Cannot encode preview"))?
        )
    );
    Ok(scan.report)
}
#[allow(clippy::too_many_arguments)]
fn installation(
    scan: &mut Scan,
    target: &str,
    root: &Path,
    manifest: &Path,
    name: &str,
    mode: &str,
    source: &str,
    roots: &[PathBuf],
    aliases: &[(PathBuf, PathBuf)],
    names: &mut BTreeSet<String>,
) {
    let source = rebase(PathBuf::from(source), aliases);
    if !valid_name(name) || !["copy", "symlink"].contains(&mode) || !absolute(&source) {
        scan.issue(
            "invalid_installation",
            manifest,
            "Invalid installation name, mode or source path",
        );
        return;
    }
    if !names.insert(name.into()) {
        scan.issue(
            "duplicate_installation",
            manifest,
            "Duplicate installation identity; review both manifest records",
        );
    }
    let destination = root.join(name);
    let (state, link) = match fs::symlink_metadata(&destination) {
        Ok(m) if m.file_type().is_symlink() => match fs::read_link(&destination) {
            Ok(p) => ("symlink_unverified", Some(p)),
            Err(_) => ("unreadable", None),
        },
        Ok(m) if m.is_dir() => ("copy_directory_unverified", None),
        Ok(_) => ("unexpected_file", None),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => ("missing", None),
        Err(_) => ("unreadable", None),
    };
    let source_state = path_state(roots, &source);
    if source_state != "directory_unverified" {
        scan.issue(
            "source_unverified",
            &destination,
            "Source is missing, unsafe, or outside the granted scope; resolve it before adoption",
        );
    }
    let mut next = if mode == "symlink" {
        "pin_source_after_ownership_review"
    } else {
        "compare_copy_before_adoption"
    };
    if source_state != "directory_unverified" {
        next = "resolve_source_before_adoption";
    }
    if source.to_string_lossy().contains(".app/Contents/") {
        next = "preserve_bundle_source_before_adoption";
    }
    if state == "missing" || state == "unreadable" || state == "unexpected_file" {
        scan.issue(
            "destination_unavailable",
            &destination,
            "Installed destination is missing, unreadable or not a skill directory",
        );
    }
    if let Some(link) = &link {
        let resolved = if link.is_absolute() {
            link.clone()
        } else {
            root.join(link)
        };
        let resolved = rebase(resolved, aliases);
        let linked_state = path_state(roots, &resolved);
        if linked_state == "missing" {
            scan.issue(
                "broken_link",
                &destination,
                "Installation link target is missing inside the granted scope",
            );
        }
        if resolved != source {
            scan.issue(
                "link_source_mismatch",
                &destination,
                "Actual link differs from the manifest source; manual review required",
            );
        }
    }
    if (mode == "copy" && link.is_some())
        || (mode == "symlink" && link.is_none() && state != "missing")
    {
        scan.issue(
            "mode_mismatch",
            &destination,
            "Filesystem entry does not match the manifest mode",
        );
    }
    scan.report.installations.push(InstallationPreview {
        target_id: target.into(),
        manifest_path: manifest.into(),
        name: name.into(),
        mode: mode.into(),
        source_path: source,
        source_state,
        destination_state: state.into(),
        link_target: link,
        ownership: "legacy-unverified".into(),
        next_step: next.into(),
    });
}
#[cfg(test)]
#[path = "migration_tests.rs"]
mod tests;
