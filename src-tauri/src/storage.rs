use crate::models::{now_ms, ProjectMetadata, ProjectSummary, CURRENT_SCHEMA_VERSION};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use tauri::{AppHandle, Manager};

const RECENT_PROJECTS_FILE: &str = "recent-projects.json";
const LOOM_GITIGNORE_PATTERN: &str = "/.loom/";
static TEMP_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

pub struct ProjectStoreStatus {
    pub loom_dir_ready: bool,
    pub schema_version: u32,
}

pub fn ensure_project_store(
    project_path: &Path,
    project_id: &str,
    project_name: &str,
) -> Result<ProjectStoreStatus, String> {
    let loom_dir = project_path.join(".loom");
    fs::create_dir_all(&loom_dir)
        .map_err(|error| format!("failed to create .loom directory: {error}"))?;
    ensure_loom_dir_ignored(project_path)?;

    let metadata_path = loom_dir.join("loom.json");
    let metadata = ProjectMetadata {
        schema_version: CURRENT_SCHEMA_VERSION,
        project_id: project_id.to_string(),
        project_path: project_path.display().to_string(),
        project_name: project_name.to_string(),
        updated_at_ms: now_ms(),
    };

    if metadata_path.exists() {
        let current = read_json_value(&metadata_path)?;
        let schema_version = current
            .get("schemaVersion")
            .and_then(Value::as_u64)
            .unwrap_or_default() as u32;

        if schema_version != CURRENT_SCHEMA_VERSION {
            let backup_path = metadata_path.with_file_name(format!("loom.json.bak-{}", now_ms()));
            fs::rename(&metadata_path, &backup_path)
                .map_err(|error| format!("failed to back up incompatible loom.json: {error}"))?;
            atomic_write_json(&metadata_path, &metadata)?;
        }
    } else {
        atomic_write_json(&metadata_path, &metadata)?;
    }

    Ok(ProjectStoreStatus {
        loom_dir_ready: true,
        schema_version: CURRENT_SCHEMA_VERSION,
    })
}

fn ensure_loom_dir_ignored(project_path: &Path) -> Result<(), String> {
    let gitignore_path = project_path.join(".gitignore");
    let content = match fs::read_to_string(&gitignore_path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(format!("failed to read .gitignore: {error}")),
    };

    if gitignore_covers_loom_dir(&content) {
        return Ok(());
    }

    let mut updated = content;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(LOOM_GITIGNORE_PATTERN);
    updated.push('\n');

    fs::write(&gitignore_path, updated)
        .map_err(|error| format!("failed to update .gitignore: {error}"))
}

fn gitignore_covers_loom_dir(content: &str) -> bool {
    content.lines().any(|line| {
        let pattern = line.trim();
        if pattern.is_empty() || pattern.starts_with('#') || pattern.starts_with('!') {
            return false;
        }

        matches!(
            pattern,
            ".loom"
                | ".loom/"
                | ".loom/*"
                | ".loom/**"
                | "/.loom"
                | "/.loom/"
                | "/.loom/*"
                | "/.loom/**"
        )
    })
}

pub fn load_recent_projects(app: &AppHandle) -> Result<Vec<ProjectSummary>, String> {
    let path = recent_projects_path(app)?;

    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(&path)
        .map_err(|error| format!("failed to read recent projects: {error}"))?;
    serde_json::from_str(&content)
        .map_err(|error| format!("failed to parse recent projects: {error}"))
}

pub fn save_recent_project(app: &AppHandle, project: &ProjectSummary) -> Result<(), String> {
    let mut projects = load_recent_projects(app)?;
    projects.retain(|candidate| candidate.path != project.path);
    projects.insert(0, project.clone());
    projects.truncate(10);

    let path = recent_projects_path(app)?;
    ensure_parent_dir(&path)?;
    atomic_write_json(&path, &projects)
}

fn recent_projects_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(global_config_dir(app)?.join(RECENT_PROJECTS_FILE))
}

pub fn global_config_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("failed to resolve app data directory: {error}"))?
        .join("loom");
    fs::create_dir_all(&dir)
        .map_err(|error| format!("failed to create app data directory: {error}"))?;
    Ok(dir)
}

pub fn read_json_value(path: &Path) -> Result<Value, String> {
    let content = fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    serde_json::from_str(&content)
        .map_err(|error| format!("failed to parse {}: {error}", path.display()))
}

fn ensure_parent_dir(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }

    Ok(())
}

pub fn read_json_file<T>(path: &Path) -> Result<T, String>
where
    T: DeserializeOwned,
{
    let content = fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    serde_json::from_str(&content)
        .map_err(|error| format!("failed to parse {}: {error}", path.display()))
}

pub fn project_loom_dir(project_path: &Path) -> PathBuf {
    project_path.join(".loom")
}

pub fn project_tasks_dir(project_path: &Path) -> PathBuf {
    project_loom_dir(project_path).join("tasks")
}

pub fn project_logs_dir(project_path: &Path) -> PathBuf {
    project_loom_dir(project_path).join("logs")
}

pub fn project_plans_dir(project_path: &Path) -> PathBuf {
    project_path.join("docs").join("plans")
}

pub fn atomic_write_json<T>(path: &Path, value: &T) -> Result<(), String>
where
    T: Serialize,
{
    ensure_parent_dir(path)?;
    let content = serde_json::to_string_pretty(value)
        .map_err(|error| format!("failed to serialize {}: {error}", path.display()))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("invalid target file name: {}", path.display()))?;
    let sequence = TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temp_path = path.with_file_name(format!("{file_name}.tmp-{}-{sequence}", now_ms()));

    fs::write(&temp_path, content)
        .map_err(|error| format!("failed to write {}: {error}", temp_path.display()))?;
    fs::rename(&temp_path, path)
        .map_err(|error| format!("failed to replace {}: {error}", path.display()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backs_up_incompatible_project_schema() {
        let root = std::env::temp_dir().join(format!("loom-storage-test-{}", now_ms()));
        fs::create_dir_all(root.join(".loom")).expect("test dir should be created");
        fs::write(
            root.join(".loom").join("loom.json"),
            r#"{"schemaVersion":999}"#,
        )
        .expect("test metadata should be written");

        let status = ensure_project_store(&root, "project-test", "Test Project")
            .expect("project store should recover incompatible schema");

        assert!(status.loom_dir_ready);
        assert_eq!(status.schema_version, CURRENT_SCHEMA_VERSION);
        assert!(root.join(".loom").join("loom.json").exists());
        assert!(fs::read_dir(root.join(".loom"))
            .expect("loom dir should be readable")
            .any(|entry| entry
                .expect("backup entry should be readable")
                .file_name()
                .to_string_lossy()
                .starts_with("loom.json.bak-")));

        fs::remove_dir_all(root).expect("test dir should be removed");
    }

    #[test]
    fn ensure_project_store_adds_loom_to_gitignore() {
        let root = std::env::temp_dir().join(format!("loom-gitignore-test-{}", now_ms()));
        fs::create_dir_all(&root).expect("test dir should be created");

        ensure_project_store(&root, "project-test", "Test Project")
            .expect("project store should be created");

        let gitignore =
            fs::read_to_string(root.join(".gitignore")).expect("gitignore should be created");
        assert_eq!(gitignore, "/.loom/\n");
        assert!(root.join(".loom").join("loom.json").exists());

        fs::remove_dir_all(root).expect("test dir should be removed");
    }

    #[test]
    fn ensure_project_store_appends_loom_to_existing_gitignore_once() {
        let root = std::env::temp_dir().join(format!("loom-gitignore-existing-test-{}", now_ms()));
        fs::create_dir_all(&root).expect("test dir should be created");
        fs::write(root.join(".gitignore"), "node_modules\n.env")
            .expect("gitignore should be seeded");

        ensure_project_store(&root, "project-test", "Test Project")
            .expect("project store should be created");
        ensure_project_store(&root, "project-test", "Test Project")
            .expect("project store should remain idempotent");

        let gitignore =
            fs::read_to_string(root.join(".gitignore")).expect("gitignore should be readable");
        assert_eq!(gitignore, "node_modules\n.env\n/.loom/\n");

        fs::remove_dir_all(root).expect("test dir should be removed");
    }

    #[test]
    fn ensure_project_store_reuses_existing_loom_gitignore_pattern() {
        let root = std::env::temp_dir().join(format!("loom-gitignore-covered-test-{}", now_ms()));
        fs::create_dir_all(&root).expect("test dir should be created");
        fs::write(root.join(".gitignore"), "target/\n.loom/\n")
            .expect("gitignore should be seeded");

        ensure_project_store(&root, "project-test", "Test Project")
            .expect("project store should be created");

        let gitignore =
            fs::read_to_string(root.join(".gitignore")).expect("gitignore should be readable");
        assert_eq!(gitignore, "target/\n.loom/\n");

        fs::remove_dir_all(root).expect("test dir should be removed");
    }
}
