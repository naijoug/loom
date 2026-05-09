use crate::{
    models::{IdGenerator, ProjectSummary},
    storage,
};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use tauri::{AppHandle, State};

#[tauri::command]
pub fn list_recent_projects(app: AppHandle) -> Result<Vec<ProjectSummary>, String> {
    storage::load_recent_projects(&app)
}

#[tauri::command]
pub fn register_project(
    app: AppHandle,
    ids: State<'_, IdGenerator>,
    path: String,
) -> Result<ProjectSummary, String> {
    let project_path = canonical_project_path(&path)?;
    let project_id = ids.next("project");
    let name = project_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("project")
        .to_string();
    let store = storage::ensure_project_store(&project_path, &project_id, &name)?;
    let mut summary = analyze_project_path(&project_path, project_id, name)?;
    summary.loom_dir_ready = store.loom_dir_ready;
    summary.schema_version = store.schema_version;
    storage::save_recent_project(&app, &summary)?;

    Ok(summary)
}

fn canonical_project_path(path: &str) -> Result<PathBuf, String> {
    if path.trim().is_empty() {
        return Err("project path is required".to_string());
    }

    let candidate = expand_project_path(path.trim())?;
    let canonical = fs::canonicalize(&candidate)
        .map_err(|error| format!("failed to read project path: {error}"))?;

    if !canonical.is_dir() {
        return Err(format!(
            "project path is not a directory: {}",
            canonical.display()
        ));
    }

    Ok(canonical)
}

fn expand_project_path(path: &str) -> Result<PathBuf, String> {
    if path == "~" {
        return std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| "failed to resolve home directory".to_string());
    }

    if let Some(rest) = path.strip_prefix("~/") {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| "failed to resolve home directory".to_string())?;
        return Ok(home.join(rest));
    }

    Ok(PathBuf::from(path))
}

fn analyze_project_path(
    project_path: &Path,
    project_id: String,
    name: String,
) -> Result<ProjectSummary, String> {
    let package_json = read_package_json(project_path)?;
    let mut detected_stacks = BTreeSet::new();
    let mut suggested_commands = Vec::new();

    if let Some(package) = package_json.as_ref() {
        detected_stacks.insert("Node.js".to_string());

        if dependency_exists(package, "react") {
            detected_stacks.insert("React".to_string());
        }

        if dependency_exists(package, "vite") {
            detected_stacks.insert("Vite".to_string());
        }

        collect_package_commands(package, &mut suggested_commands);
    }

    if project_path.join("Cargo.toml").exists()
        || project_path.join("src-tauri").join("Cargo.toml").exists()
    {
        detected_stacks.insert("Rust".to_string());
        if project_path.join("Cargo.toml").exists() {
            push_unique(&mut suggested_commands, "cargo check".to_string());
            push_unique(&mut suggested_commands, "cargo test".to_string());
        }
    }

    if project_path
        .join("src-tauri")
        .join("tauri.conf.json")
        .exists()
    {
        detected_stacks.insert("Tauri".to_string());
        push_unique(
            &mut suggested_commands,
            "cargo check --manifest-path src-tauri/Cargo.toml".to_string(),
        );
    }

    let git_info = git_info(project_path);

    Ok(ProjectSummary {
        id: project_id,
        path: project_path.display().to_string(),
        name,
        detected_stacks: detected_stacks.into_iter().collect(),
        suggested_commands,
        is_git_repository: git_info.is_repository,
        git_branch: git_info.branch,
        has_uncommitted_changes: git_info.has_uncommitted_changes,
        loom_dir_ready: false,
        schema_version: 0,
    })
}

fn read_package_json(project_path: &Path) -> Result<Option<Value>, String> {
    let path = project_path.join("package.json");

    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&path)
        .map_err(|error| format!("failed to read package.json: {error}"))?;
    let value = serde_json::from_str(&content)
        .map_err(|error| format!("failed to parse package.json: {error}"))?;

    Ok(Some(value))
}

fn dependency_exists(package: &Value, name: &str) -> bool {
    ["dependencies", "devDependencies"]
        .iter()
        .filter_map(|section| package.get(section))
        .filter_map(Value::as_object)
        .any(|dependencies| dependencies.contains_key(name))
}

fn collect_package_commands(package: &Value, commands: &mut Vec<String>) {
    let Some(scripts) = package.get("scripts").and_then(Value::as_object) else {
        return;
    };

    for preferred in ["build", "test", "lint", "dev"] {
        if scripts.contains_key(preferred) {
            push_unique(commands, format!("pnpm {preferred}"));
        }
    }
}

fn push_unique(commands: &mut Vec<String>, command: String) {
    if !commands.iter().any(|candidate| candidate == &command) {
        commands.push(command);
    }
}

struct GitInfo {
    is_repository: bool,
    branch: Option<String>,
    has_uncommitted_changes: bool,
}

fn git_info(project_path: &Path) -> GitInfo {
    let is_repository = run_git(project_path, ["rev-parse", "--is-inside-work-tree"])
        .map(|output| output == "true")
        .unwrap_or(false);

    if !is_repository {
        return GitInfo {
            is_repository: false,
            branch: None,
            has_uncommitted_changes: false,
        };
    }

    let branch = run_git(project_path, ["branch", "--show-current"])
        .ok()
        .filter(|value| !value.is_empty());
    let has_uncommitted_changes = run_git(project_path, ["status", "--porcelain"])
        .map(|output| !output.is_empty())
        .unwrap_or(false);

    GitInfo {
        is_repository,
        branch,
        has_uncommitted_changes,
    }
}

fn run_git<const N: usize>(project_path: &Path, args: [&str; N]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(project_path)
        .args(args)
        .output()
        .map_err(|error| format!("failed to run git: {error}"))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyzes_current_loom_repository() {
        let project_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("tauri manifest should live under project root");

        let summary =
            analyze_project_path(project_path, "project-test".to_string(), "loom".to_string())
                .expect("current repository should be analyzable");

        assert!(summary.detected_stacks.contains(&"Node.js".to_string()));
        assert!(summary.detected_stacks.contains(&"React".to_string()));
        assert!(summary.detected_stacks.contains(&"Vite".to_string()));
        assert!(summary.detected_stacks.contains(&"Tauri".to_string()));
        assert!(summary.detected_stacks.contains(&"Rust".to_string()));
        assert!(summary
            .suggested_commands
            .contains(&"pnpm build".to_string()));
        assert!(summary
            .suggested_commands
            .contains(&"cargo check --manifest-path src-tauri/Cargo.toml".to_string()));
    }
}
