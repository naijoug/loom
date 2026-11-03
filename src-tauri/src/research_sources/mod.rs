//! Target acquisition primitives. Returns observations, never writes business state.
use crate::rea::{Client, Tool};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::{Component, Path, PathBuf};
mod asar;

pub struct AppLayout {
    pub root: PathBuf,
    pub archive: Option<asar::Archive>,
    pub source_paths: Vec<String>,
}

pub fn inspect_app(path: &str) -> Result<AppLayout, String> {
    let selected = Path::new(path);
    if !selected.is_absolute()
        || selected
            .components()
            .any(|c| matches!(c, Component::ParentDir))
    {
        return Err("app_path_must_be_absolute".into());
    }
    let meta = std::fs::symlink_metadata(selected).map_err(|_| "app_target_unavailable")?;
    if meta.file_type().is_symlink() || (!meta.is_dir() && !meta.is_file()) {
        return Err("unsupported_app_path".into());
    }
    let root = std::fs::canonicalize(selected).map_err(|_| "app_target_unavailable")?;
    let mut source_paths = Vec::new();
    if root.is_dir() {
        collect_paths(&root, &root, &mut source_paths, 0, &mut 0)?;
    } else if !matches!(
        root.extension().and_then(|x| x.to_str()),
        Some("asar" | "js" | "mjs" | "cjs")
    ) {
        return Err("unsupported_app_file".into());
    }
    source_paths.sort();
    let archive_path = if root.is_file() && root.extension().is_some_and(|x| x == "asar") {
        Some(root.clone())
    } else if root.is_dir() {
        [
            "Contents/Resources/app.asar",
            "resources/app.asar",
            "app.asar",
        ]
        .into_iter()
        .find(|relative| root.join(relative).exists())
        .map(|relative| scoped_path(&root, relative))
        .transpose()?
    } else {
        None
    };
    let archive = archive_path
        .map(|path| asar::Archive::inspect(&path))
        .transpose()?;
    Ok(AppLayout {
        root,
        archive,
        source_paths,
    })
}

fn collect_paths(
    root: &Path,
    directory: &Path,
    out: &mut Vec<String>,
    depth: usize,
    visited: &mut usize,
) -> Result<(), String> {
    if depth > 64 {
        return Err("app_depth_limit".into());
    }
    let mut entries = std::fs::read_dir(directory)
        .map_err(|_| "app_directory_unreadable")?
        .take(20_001)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "app_directory_unreadable")?;
    if entries.len() > 20_000 {
        return Err("app_inventory_limit".into());
    }
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        *visited += 1;
        if *visited > 20_000 {
            return Err("app_inventory_limit".into());
        }
        if entry.file_name() == ".git" {
            continue;
        }
        let meta = entry.file_type().map_err(|_| "app_entry_unreadable")?;
        if meta.is_symlink() {
            continue;
        }
        if meta.is_dir() {
            collect_paths(root, &entry.path(), out, depth + 1, visited)?;
        } else if meta.is_file() && is_source_path(&entry.path()) {
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .map_err(|_| "app_scope_mismatch")?
                .to_str()
                .ok_or("app_path_not_utf8")?;
            // Configuration secrets are never copied into the published source manifest.
            if relative.split('/').any(|p| p.starts_with('.')) {
                continue;
            }
            out.push(relative.into());
            if out.len() > 20_000 {
                return Err("app_inventory_limit".into());
            }
        }
    }
    Ok(())
}

pub async fn identify(
    client: &mut Client,
    directory: &Path,
    cancelled: &impl Fn() -> bool,
) -> Result<Value, String> {
    client
        .call(
            Tool::InspectApp,
            json!({"path":directory,"detail":"complete"}),
            cancelled,
        )
        .await
}
pub async fn analyze(
    client: &mut Client,
    input: &Path,
    cancelled: &impl Fn() -> bool,
) -> Result<Value, String> {
    client
        .call(
            Tool::AnalyzeJavascript,
            json!({"input_path":input,"detail":"summary","analysis_timeout_ms":30000}),
            cancelled,
        )
        .await
}

pub fn is_source_path(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|x| x.to_str()),
        Some("js" | "mjs" | "cjs" | "ts" | "tsx" | "jsx" | "json" | "map")
    )
}
pub fn scoped_path(root: &Path, relative: &str) -> Result<PathBuf, String> {
    if relative.is_empty()
        || relative.contains(['\\', '\0'])
        || relative
            .split('/')
            .any(|s| s.is_empty() || s == "." || s == "..")
    {
        return Err("unsafe_source_path".into());
    }
    if std::fs::symlink_metadata(root).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err("source_symlink".into());
    }
    let mut path = root.to_path_buf();
    for part in Path::new(relative).components() {
        match part {
            Component::Normal(p) => path.push(p),
            _ => return Err("unsafe_source_path".into()),
        };
        if std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err("source_symlink".into());
        }
    }
    Ok(path)
}
pub fn read_scoped(root: &Path, relative: &str) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let path = scoped_path(root, relative)?;
    let before = std::fs::metadata(&path).map_err(|_| "source_unavailable")?;
    if !before.is_file() || before.len() > 16 * 1024 * 1024 {
        return Err("source_size_limit".into());
    }
    let file = std::fs::File::open(&path).map_err(|_| "source_unavailable")?;
    let opened = file.metadata().map_err(|_| "source_unavailable")?;
    if opened.len() != before.len() || opened.modified().ok() != before.modified().ok() {
        return Err("source_changed".into());
    }
    if !std::fs::canonicalize(&path)
        .map_err(|_| "source_unavailable")?
        .starts_with(std::fs::canonicalize(root).map_err(|_| "source_unavailable")?)
    {
        return Err("source_outside_scope".into());
    }
    let mut bytes = Vec::new();
    file.take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "source_read_failed")?;
    if bytes.len() as u64 != before.len() {
        return Err("source_changed".into());
    }
    Ok(bytes)
}
pub struct Capture {
    pub paths: Vec<String>,
    pub has_javascript: bool,
    pub bytes: u64,
    pub provenance: Value,
}
impl Capture {
    pub fn verify(&self, directory: &Path) -> Result<(), String> {
        for path in &self.paths {
            let bytes = read_scoped(directory, path)?;
            if self.provenance["files"][path] != format!("{:x}", Sha256::digest(&bytes)) {
                return Err("captured_source_changed".into());
            }
        }
        Ok(())
    }
}
impl AppLayout {
    pub fn estimate_bytes(&self) -> Result<u64, String> {
        let mut bytes = 0u64;
        for path in &self.source_paths {
            bytes = bytes
                .checked_add(
                    std::fs::metadata(scoped_path(&self.root, path)?)
                        .map_err(|_| "source_unavailable")?
                        .len(),
                )
                .ok_or("capture_size_limit")?;
        }
        if let Some(archive) = &self.archive {
            for file in &archive.entries {
                bytes = bytes.checked_add(file.size).ok_or("capture_size_limit")?;
            }
        } else if self.root.is_file() {
            bytes = std::fs::metadata(&self.root)
                .map_err(|_| "source_unavailable")?
                .len();
        }
        if bytes > 512 * 1024 * 1024 {
            return Err("capture_size_limit".into());
        }
        Ok(bytes)
    }
    pub fn additional_identity(&self) -> Result<Value, String> {
        if let Some(archive) = &self.archive {
            return Ok(
                json!({"asarSha256":archive.sha256,"companions":archive.companion_identity()?}),
            );
        }
        if self.root.is_file() {
            let parent = self.root.parent().ok_or("source_parent_missing")?;
            let name = self
                .root
                .file_name()
                .and_then(|x| x.to_str())
                .ok_or("source_path_invalid")?;
            return Ok(json!({"sha256":format!("{:x}",Sha256::digest(read_scoped(parent,name)?))}));
        }
        Ok(Value::Null)
    }
    pub fn capture(
        &self,
        max_bytes: u64,
        mut emit: impl FnMut(&str, &[u8]) -> Result<(), String>,
        cancelled: impl Fn() -> bool,
    ) -> Result<Capture, String> {
        let mut paths = Vec::new();
        let mut hashes = std::collections::BTreeMap::new();
        let mut total = 0u64;
        let mut retain = |path: String, bytes: Vec<u8>| -> Result<(), String> {
            if cancelled() {
                return Err("cancelled".into());
            }
            if std::str::from_utf8(&bytes).is_err() {
                return Err("source_not_utf8".into());
            }
            total = total
                .checked_add(bytes.len() as u64)
                .ok_or("capture_size_limit")?;
            if total > max_bytes {
                return Err("capture_size_limit".into());
            }
            if hashes.contains_key(&path) {
                return Err("capture_path_collision".into());
            }
            emit(&path, &bytes)?;
            hashes.insert(path.clone(), format!("{:x}", Sha256::digest(&bytes)));
            paths.push(path);
            Ok(())
        };
        for relative in &self.source_paths {
            retain(relative.clone(), read_scoped(&self.root, relative)?)?;
        }
        if let Some(archive) = &self.archive {
            for entry in &archive.entries {
                let path = if self.root.is_file() {
                    entry.path.clone()
                } else {
                    format!("__loom_asar__/{}", entry.path)
                };
                retain(path, archive.read(entry)?)?;
            }
        } else if self.root.is_file() {
            let name = self
                .root
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or("source_path_invalid")?;
            retain(
                name.into(),
                read_scoped(self.root.parent().ok_or("source_parent_missing")?, name)?,
            )?;
        }
        paths.sort();
        let has_javascript = paths.iter().any(|p| {
            matches!(
                Path::new(p).extension().and_then(|e| e.to_str()),
                Some("js" | "mjs" | "cjs" | "jsx" | "ts" | "tsx")
            )
        });
        let provenance = json!({"kind":"loom-static-source-capture","target":self.root,"identity":self.additional_identity()?,"files":hashes,
            "coverage":"partial","limitations":["Only captured UTF-8 JS/TS/JSON/source-map files are analyzed; native assets, hidden files and symlinks are not source evidence.","Static capture does not prove runtime behavior or original dependency completeness."],
            "archiveOmissions":self.archive.as_ref().map(|a|a.omissions.clone()).unwrap_or_default(),"nativeEngineDiagnosticOnly":!has_javascript});
        Ok(Capture {
            paths,
            has_javascript,
            bytes: total,
            provenance,
        })
    }
}

/// Removes known sensitive fields recursively, preserving separate raw identity.
/// This is a reduced view, not a claim to detect every possible embedded secret.
pub fn model_view(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .filter_map(|(key, value)| {
                    let lower = key.to_ascii_lowercase();
                    if [
                        "raw_result",
                        "cookie",
                        "set-cookie",
                        "authorization",
                        "headers",
                        "postdata",
                        "post_data",
                        "body",
                        "storage",
                        "local_path",
                        "caller_input_path",
                    ]
                    .contains(&lower.as_str())
                    {
                        return None;
                    }
                    Some((key.clone(), model_view(value)))
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(model_view).collect()),
        Value::String(text) => {
            if let Ok(mut url) = reqwest::Url::parse(text) {
                if matches!(url.scheme(), "http" | "https") {
                    let _ = url.set_username("");
                    let _ = url.set_password(None);
                    url.set_query(None);
                    url.set_fragment(None);
                    return Value::String(url.to_string());
                }
            }
            Value::String(crate::agent::redact_sensitive_text(text))
        }
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn official_asar_and_standalone_script_capture_only_selected_sources() {
        let root = std::env::temp_dir().join(format!(
            "loom-capture-{}-{}",
            std::process::id(),
            crate::models::now_ms()
        ));
        std::fs::create_dir(&root).unwrap();
        let archive = root.join("app.asar");
        std::fs::write(
            &archive,
            include_bytes!("../../../tests/fixtures/research/electron-4.2.0.asar"),
        )
        .unwrap();
        let layout = inspect_app(archive.to_str().unwrap()).unwrap();
        let output = root.join("captured");
        std::fs::create_dir(&output).unwrap();
        let captured = layout
            .capture(
                layout.estimate_bytes().unwrap(),
                |path, bytes| std::fs::write(output.join(path), bytes).map_err(|e| e.to_string()),
                || false,
            )
            .unwrap();
        assert_eq!(captured.paths, vec!["index.js", "package.json"]);
        assert!(captured.has_javascript);
        captured.verify(&output).unwrap();
        assert!(!root.join("APP_EXECUTED").exists());
        let standalone = root.join("single.js");
        std::fs::write(&standalone, "export const single=1;").unwrap();
        std::fs::write(root.join("secret.js"), "PRIVATE SIBLING").unwrap();
        let layout = inspect_app(standalone.to_str().unwrap()).unwrap();
        let mut selected = Vec::new();
        layout
            .capture(
                4096,
                |path, bytes| {
                    selected.push((path.to_string(), String::from_utf8(bytes.to_vec()).unwrap()));
                    Ok(())
                },
                || false,
            )
            .unwrap();
        assert_eq!(
            selected,
            vec![("single.js".into(), "export const single=1;".into())]
        );
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(root.join("secret.js"), output.join("escape.js")).unwrap();
            assert!(read_scoped(&output, "escape.js").is_err());
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn official_unpacked_members_are_hash_checked_and_cannot_follow_companion_symlinks() {
        let root = std::env::temp_dir().join(format!(
            "loom-unpacked-{}-{}",
            std::process::id(),
            crate::models::now_ms()
        ));
        std::fs::create_dir(&root).unwrap();
        let archive = root.join("app.asar");
        std::fs::write(
            &archive,
            include_bytes!("../../../tests/fixtures/research/electron-unpacked-4.2.0.asar"),
        )
        .unwrap();
        let companion = root.join("app.asar.unpacked");
        std::fs::create_dir(&companion).unwrap();
        std::fs::write(
            companion.join("index.js"),
            include_bytes!(
                "../../../tests/fixtures/research/electron-unpacked-4.2.0.asar.unpacked/index.js"
            ),
        )
        .unwrap();
        let layout = inspect_app(archive.to_str().unwrap()).unwrap();
        layout.additional_identity().unwrap();
        let mut files = Vec::new();
        layout
            .capture(
                4096,
                |path, _| {
                    files.push(path.to_string());
                    Ok(())
                },
                || false,
            )
            .unwrap();
        assert!(files.contains(&"index.js".into()));
        std::fs::write(companion.join("index.js"), "tampered").unwrap();
        assert!(layout.capture(4096, |_, _| Ok(()), || false).is_err());
        #[cfg(unix)]
        {
            std::fs::remove_file(companion.join("index.js")).unwrap();
            std::fs::write(root.join("private.js"), "private").unwrap();
            std::os::unix::fs::symlink(root.join("private.js"), companion.join("index.js"))
                .unwrap();
            assert!(layout.additional_identity().is_err());
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn model_projection_omits_sensitive_network_fields() {
        let value = model_view(
            &json!({"headers":{"Authorization":"secret"},"url":"https://user:pass@example.com/path?token=secret#private","nested":[{"cookie":"private","fact":"observed"}]}),
        );
        assert_eq!(value["url"], "https://example.com/path");
        assert!(value.get("headers").is_none());
        assert_eq!(value["nested"][0], json!({"fact":"observed"}));
    }
}
