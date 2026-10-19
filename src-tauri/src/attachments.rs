use crate::models::{now_ms, FeedbackAttachment, IdGenerator};
use crate::storage;
use std::{fs, path::Path};

const MAX_ATTACHMENT_BYTES: u64 = 10 * 1024 * 1024;
const MAX_TOTAL_ATTACHMENT_BYTES: u64 = 30 * 1024 * 1024;
const MAX_ATTACHMENTS: usize = 8;

fn attachment_mime(extension: &str) -> Option<&'static str> {
    match extension {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        "gif" => Some("image/gif"),
        "txt" | "log" => Some("text/plain"),
        "md" => Some("text/markdown"),
        "json" => Some("application/json"),
        "pdf" => Some("application/pdf"),
        _ => None,
    }
}

fn safe_file_name(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    let trimmed = sanitized.trim_matches(['.', '_']);
    if trimmed.is_empty() {
        "attachment".to_string()
    } else {
        trimmed.chars().take(120).collect()
    }
}

pub fn copy_feedback_attachments(
    project_path: &Path,
    task_id: &str,
    ids: &IdGenerator,
    source_paths: &[String],
) -> Result<Vec<FeedbackAttachment>, String> {
    if source_paths.len() > MAX_ATTACHMENTS {
        return Err(format!(
            "feedback supports at most {MAX_ATTACHMENTS} attachments"
        ));
    }
    let target_dir = storage::project_tasks_dir(project_path)
        .join(task_id)
        .join("attachments");
    fs::create_dir_all(&target_dir)
        .map_err(|error| format!("failed to create feedback attachment directory: {error}"))?;
    let mut total_size = 0_u64;
    let mut attachments = Vec::new();
    for source_path in source_paths {
        let source = fs::canonicalize(source_path)
            .map_err(|error| format!("failed to access attachment '{source_path}': {error}"))?;
        let metadata = fs::metadata(&source).map_err(|error| {
            format!(
                "failed to inspect attachment '{}': {error}",
                source.display()
            )
        })?;
        if !metadata.is_file() {
            return Err(format!("attachment '{}' is not a file", source.display()));
        }
        if metadata.len() > MAX_ATTACHMENT_BYTES {
            return Err(format!(
                "attachment '{}' exceeds the 10 MiB limit",
                source.display()
            ));
        }
        total_size = total_size.saturating_add(metadata.len());
        if total_size > MAX_TOTAL_ATTACHMENT_BYTES {
            return Err("feedback attachments exceed the 30 MiB total limit".to_string());
        }
        let original_name = source
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| format!("attachment '{}' has no valid file name", source.display()))?;
        let extension = source
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let mime_type = attachment_mime(&extension).ok_or_else(|| {
            format!(
                "attachment '{}' has an unsupported file type; use images, text, Markdown, JSON, logs, or PDF",
                source.display()
            )
        })?;
        let id = ids.next("attachment");
        let target = target_dir.join(format!("{}-{}", id, safe_file_name(original_name)));
        let temporary = target.with_extension(format!("{}.tmp", extension));
        fs::copy(&source, &temporary).map_err(|error| {
            format!("failed to copy attachment '{}': {error}", source.display())
        })?;
        fs::rename(&temporary, &target).map_err(|error| {
            format!(
                "failed to finalize attachment '{}': {error}",
                target.display()
            )
        })?;
        attachments.push(FeedbackAttachment {
            id,
            name: original_name.to_string(),
            mime_type: mime_type.to_string(),
            size_bytes: metadata.len(),
            stored_path: target.display().to_string(),
            created_at_ms: now_ms(),
        });
    }
    Ok(attachments)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_supported_attachment_into_task_store() {
        let root = std::env::temp_dir().join(format!("loom-attachment-{}", now_ms()));
        fs::create_dir_all(&root).expect("fixture root");
        let source = root.join("failure log.txt");
        fs::write(&source, "failed at line 42").expect("fixture source");

        let copied = copy_feedback_attachments(
            &root,
            "task-1",
            &IdGenerator::default(),
            &[source.display().to_string()],
        )
        .expect("copy attachment");

        assert_eq!(copied.len(), 1);
        assert_eq!(copied[0].mime_type, "text/plain");
        assert!(Path::new(&copied[0].stored_path).is_file());
        assert!(Path::new(&copied[0].stored_path).starts_with(root.join(".loom/tasks/task-1")));
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn rejects_unsupported_attachment_types() {
        let root = std::env::temp_dir().join(format!("loom-attachment-invalid-{}", now_ms()));
        fs::create_dir_all(&root).expect("fixture root");
        let source = root.join("program.sh");
        fs::write(&source, "echo unsafe").expect("fixture source");

        let error = copy_feedback_attachments(
            &root,
            "task-1",
            &IdGenerator::default(),
            &[source.display().to_string()],
        )
        .unwrap_err();

        assert!(error.contains("unsupported file type"));
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn rejects_attachments_over_the_per_file_limit() {
        let root = std::env::temp_dir().join(format!("loom-attachment-large-{}", now_ms()));
        fs::create_dir_all(&root).expect("fixture root");
        let source = root.join("oversized.log");
        fs::File::create(&source)
            .and_then(|file| file.set_len(MAX_ATTACHMENT_BYTES + 1))
            .expect("sparse oversized fixture");

        let error = copy_feedback_attachments(
            &root,
            "task-1",
            &IdGenerator::default(),
            &[source.display().to_string()],
        )
        .unwrap_err();

        assert!(error.contains("10 MiB limit"));
        fs::remove_dir_all(root).ok();
    }
}
