//! Codex exec reports changed paths without patch bodies. Capture bounded text
//! baselines before the turn; never derive a patch against HEAD/pre-existing dirt.
use super::{ChatMessagePart, DiffStats};
use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path, PathBuf},
};

const MAX_FILE: u64 = 256 * 1024;
const MAX_TOTAL: usize = 8 * 1024 * 1024;
const MAX_ENTRIES: usize = 4096;
const EXCLUDED: &[&str] = &[".git", ".loom", "node_modules", "target", "dist", ".env"];

pub(super) struct FileChanges {
    root: PathBuf,
    files: BTreeMap<PathBuf, Option<String>>,
    complete: bool,
    bytes: usize,
}

impl FileChanges {
    pub(super) fn capture(root: &Path) -> Self {
        let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        let mut snapshot = Self {
            root: root.clone(),
            files: BTreeMap::new(),
            complete: true,
            bytes: 0,
        };
        let mut pending = vec![root];
        let mut count = 0;
        let mut bytes = 0;
        while let Some(directory) = pending.pop() {
            let Ok(entries) = fs::read_dir(directory) else {
                snapshot.complete = false;
                continue;
            };
            for entry in entries {
                count += 1;
                if count > MAX_ENTRIES || bytes >= MAX_TOTAL {
                    snapshot.complete = false;
                    return snapshot;
                }
                let Ok(entry) = entry else {
                    snapshot.complete = false;
                    continue;
                };
                let path = entry.path();
                if excluded(&path, &snapshot.root) {
                    continue;
                }
                let Ok(kind) = entry.file_type() else {
                    snapshot.complete = false;
                    continue;
                };
                if kind.is_symlink() {
                    snapshot.files.insert(path, None);
                    continue;
                }
                if kind.is_dir() {
                    pending.push(path);
                    continue;
                }
                let text = read_text(&path);
                if let Some(text) = &text {
                    if bytes + text.len() > MAX_TOTAL {
                        snapshot.complete = false;
                        snapshot.files.insert(path, None);
                        continue;
                    }
                    bytes += text.len();
                    snapshot.bytes = bytes;
                }
                snapshot.files.insert(path, text);
            }
        }
        snapshot
    }

    pub(super) fn enrich(&mut self, line: &str, parts: &mut [ChatMessagePart]) {
        let Ok(event) = serde_json::from_str::<serde_json::Value>(line) else {
            return;
        };
        if event["type"] != "item.completed"
            || event["item"]["type"] != "file_change"
            || event["item"]["status"] != "completed"
        {
            return;
        }
        let Some(changes) = event["item"]["changes"].as_array() else {
            return;
        };
        if changes.len() > 64 {
            return;
        }
        let mut removed_count = 0;
        let mut added_count = 0;
        let mut preview = Vec::new();
        for change in changes {
            let Some(path) = change["path"].as_str() else {
                return;
            };
            let path = Path::new(path);
            let path = if path.is_absolute() {
                path.to_path_buf()
            } else {
                self.root.join(path)
            };
            if !path.starts_with(&self.root)
                || path.components().any(|part| part == Component::ParentDir)
                || excluded(&path, &self.root)
            {
                return;
            }
            // Reject symlinks in the complete path, including its parents.
            let mut current = Some(path.as_path());
            while let Some(item) = current {
                if item == self.root {
                    break;
                }
                if fs::symlink_metadata(item).is_ok_and(|meta| meta.file_type().is_symlink()) {
                    return;
                }
                current = item.parent();
            }
            let old = match self.files.get(&path) {
                Some(Some(text)) => text.clone(),
                None if self.complete => String::new(),
                _ => return,
            };
            let new = if !path.exists() && change["kind"] == "delete" {
                Some(String::new())
            } else {
                read_text(&path)
            };
            let stored = self
                .files
                .get(&path)
                .and_then(|text| text.as_ref())
                .map_or(0, String::len);
            if self.bytes.saturating_sub(stored) + new.as_ref().map_or(0, String::len) > MAX_TOTAL {
                self.files.insert(path, None);
                self.bytes = self.bytes.saturating_sub(stored);
                self.complete = false;
                return;
            }
            self.bytes = self.bytes.saturating_sub(stored) + new.as_ref().map_or(0, String::len);
            self.files.insert(path.clone(), new.clone());
            let Some(new) = new else { return };
            let before: Vec<_> = old.lines().collect();
            let after: Vec<_> = new.lines().collect();
            let prefix = before
                .iter()
                .zip(&after)
                .take_while(|(a, b)| a == b)
                .count();
            let suffix = before[prefix..]
                .iter()
                .rev()
                .zip(after[prefix..].iter().rev())
                .take_while(|(a, b)| a == b)
                .count();
            let removed = &before[prefix..before.len() - suffix];
            let added = &after[prefix..after.len() - suffix];
            preview.push(format!(
                "--- {}",
                path.strip_prefix(&self.root).unwrap_or(&path).display()
            ));
            preview.extend(removed.iter().map(|line| format!("-{line}")));
            preview.extend(added.iter().map(|line| format!("+{line}")));
            removed_count += removed.len();
            added_count += added.len();
        }
        let mut bounded = String::new();
        for line in preview.into_iter().take(40) {
            let line = crate::agents::redact_sensitive_text(&line);
            for character in line.chars().chain(std::iter::once('\n')) {
                if bounded.len() + character.len_utf8() > 8192 {
                    break;
                }
                bounded.push(character);
            }
            if bounded.len() >= 8189 {
                break;
            }
        }
        for part in parts {
            if let ChatMessagePart::Tool { metadata, .. } = part {
                if metadata.kind.as_deref() == Some("edit") && metadata.diff_preview.is_none() {
                    metadata.diff_stats = Some(DiffStats {
                        added: added_count,
                        removed: removed_count,
                    });
                    metadata.diff_preview = Some(bounded.clone());
                }
            }
        }
    }
}

fn excluded(path: &Path, root: &Path) -> bool {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .any(|component| {
            let value = component.as_os_str().to_string_lossy();
            EXCLUDED.contains(&value.as_ref()) || value.starts_with(".env.")
        })
}

fn read_text(path: &Path) -> Option<String> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_FILE {
        return None;
    }
    let text = fs::read_to_string(path).ok()?;
    (!text.contains('\0')).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compares_against_actual_dirty_contents_and_skips_unknown_baselines() {
        let root =
            std::env::temp_dir().join(crate::models::IdGenerator::default().next("loom-diff"));
        fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        fs::write(root.join("a.txt"), "existing dirty content\n").unwrap();
        let mut snapshot = FileChanges::capture(&root);
        fs::write(root.join("a.txt"), "new content\n").unwrap();
        let line = serde_json::json!({"type":"item.completed","item":{"id":"patch","type":"file_change","status":"completed","changes":[{"path":"a.txt","kind":"update"}]}}).to_string();
        let mut parts = super::super::parse_chat_stream_line("codex_json", &line).parts;
        snapshot.enrich(&line, &mut parts);
        let ChatMessagePart::Tool { metadata, .. } = &parts[0] else {
            panic!()
        };
        assert_eq!(
            metadata.diff_stats,
            Some(DiffStats {
                added: 1,
                removed: 1
            })
        );
        assert!(metadata
            .diff_preview
            .as_ref()
            .unwrap()
            .contains("-existing dirty content"));
        fs::remove_dir_all(root).unwrap();
    }
}
