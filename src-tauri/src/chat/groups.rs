use super::{list_session_summaries, repository::ChatRepository, ChatSessionSummary};
use crate::{models::ProjectSummary, projects::canonical_project_path, storage};
use serde::Serialize;
use std::{collections::BTreeSet, fs, path::Path};
use tauri::AppHandle;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatProjectGroup {
    project: ProjectSummary,
    session_count: usize,
    sessions: Vec<ChatSessionSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    storage_error: Option<String>,
}

// Counting collapsed projects never opens, migrates or replays a transcript.
fn count_sessions(project: &Path) -> Result<usize, String> {
    let root = project.join(".loom/chat");
    let mut ids = BTreeSet::new();
    for (directory, legacy) in [
        (root.join("sessions"), true),
        (root.join("v2/sessions"), false),
    ] {
        if directory.exists()
            && !directory
                .canonicalize()
                .map_err(|error| error.to_string())?
                .starts_with(project)
        {
            return Err("chat storage is outside the registered project".into());
        }
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.to_string()),
        };
        // Do not follow a replaced chat directory outside the registered project.
        if !directory
            .canonicalize()
            .map_err(|error| error.to_string())?
            .starts_with(project)
        {
            return Err("chat storage is outside the registered project".into());
        }
        for entry in entries {
            let path = entry.map_err(|error| error.to_string())?.path();
            let id = if legacy && path.extension().is_some_and(|value| value == "json") {
                path.file_stem()
            } else if !legacy {
                path.file_name()
            } else {
                None
            };
            if let Some(id) = id
                .and_then(|value| value.to_str())
                .filter(|id| super::repository::validate_id(id).is_ok())
            {
                ids.insert(id.to_string());
            }
        }
    }
    Ok(ids.len())
}

fn groups(projects: Vec<ProjectSummary>, requested: Option<&[String]>) -> Vec<ChatProjectGroup> {
    projects
        .into_iter()
        .map(|project| {
            let result = (|| {
                let path = canonical_project_path(&project.path)?;
                let count = count_sessions(&path)?;
                let expanded =
                    requested.is_some_and(|paths| paths.iter().any(|value| value == &project.path));
                let sessions = if expanded && count > 0 {
                    list_session_summaries(&ChatRepository::open(&path)?)?
                } else {
                    Vec::new()
                };
                Ok::<_, String>((count, sessions))
            })();
            match result {
                Ok((session_count, sessions)) => ChatProjectGroup {
                    project,
                    session_count,
                    sessions,
                    storage_error: None,
                },
                Err(error) => ChatProjectGroup {
                    project,
                    session_count: 0,
                    sessions: Vec::new(),
                    storage_error: Some(error),
                },
            }
        })
        .collect()
}

#[tauri::command]
pub fn chat_list_project_groups(
    app: AppHandle,
    project_paths: Option<Vec<String>>,
) -> Result<Vec<ChatProjectGroup>, String> {
    Ok(groups(
        storage::load_recent_projects(&app)?,
        project_paths.as_deref(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn counting_does_not_read_broken_transcripts_or_create_storage() {
        let root =
            std::env::temp_dir().join(crate::models::IdGenerator::default().next("loom-group"));
        fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        assert_eq!(count_sessions(&root).unwrap(), 0);
        assert!(!root.join(".loom").exists());
        fs::create_dir_all(root.join(".loom/chat/v2/sessions/broken")).unwrap();
        fs::write(
            root.join(".loom/chat/v2/sessions/broken/snapshot.json"),
            "broken",
        )
        .unwrap();
        assert_eq!(count_sessions(&root).unwrap(), 1);
        fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod isolation_tests {
    use super::*;
    #[test]
    fn one_missing_project_does_not_hide_other_groups() {
        let root = std::env::temp_dir()
            .join(crate::models::IdGenerator::default().next("loom-group-isolation"));
        fs::create_dir_all(&root).unwrap();
        let summary = |path: &Path| {
            serde_json::from_value::<ProjectSummary>(serde_json::json!({
            "id":path.to_string_lossy(),"path":path.to_string_lossy(),"name":"test","detectedStacks":[],"suggestedCommands":[],"isGitRepository":false,"hasUncommittedChanges":false,"loomDirReady":true,"schemaVersion":1
        })).unwrap()
        };
        let result = groups(vec![summary(&root.join("missing")), summary(&root)], None);
        assert!(result[0].storage_error.is_some());
        assert!(result[1].storage_error.is_none());
        assert_eq!(result[1].session_count, 0);
        assert!(!root.join(".loom").exists());
        fs::remove_dir_all(root).unwrap();
    }
}
