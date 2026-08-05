//! Per-project terminal slots for the Testing cockpit.
//!
//! Persisted to `.loom/terminal-slots.json` (travels with the repo, kept out
//! of the strict `loom.json` metadata so a schema bump never wipes them). The
//! frontend seeds sensible defaults on first load and may edit/add/remove.

use crate::{models::TerminalSlot, storage};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
};

fn slots_path(project_path: &Path) -> std::path::PathBuf {
    storage::project_loom_dir(project_path).join("terminal-slots.json")
}

const PREVIEW_SCRIPTS: [&str; 3] = ["dev", "serve", "start"];
const VALIDATION_SCRIPTS: [&str; 6] = ["test", "typecheck", "check", "build", "lint", "smoke"];
const IGNORED_DIRS: [&str; 9] = [
    "node_modules",
    "target",
    "dist",
    "build",
    "out",
    ".next",
    "coverage",
    "vendor",
    ".loom",
];

fn package_scripts(dir: &Path) -> Vec<String> {
    let Ok(content) = fs::read_to_string(dir.join("package.json")) else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<Value>(&content) else {
        return Vec::new();
    };
    value
        .get("scripts")
        .and_then(Value::as_object)
        .map(|scripts| scripts.keys().cloned().collect())
        .unwrap_or_default()
}

/// Node package-manager command prefix, preferring the subdir lockfile, then
/// the repo root (monorepos share a root lockfile).
fn detect_pm(dir: &Path, root: &Path) -> &'static str {
    for base in [dir, root] {
        if base.join("pnpm-lock.yaml").exists() {
            return "pnpm";
        }
        if base.join("yarn.lock").exists() {
            return "yarn";
        }
        if base.join("bun.lockb").exists() || base.join("bun.lock").exists() {
            return "bun";
        }
        if base.join("package-lock.json").exists() {
            return "npm run";
        }
    }
    "npm run"
}

fn titlecase(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => value.to_string(),
    }
}

fn make_slot(
    counter: &mut usize,
    label: Option<&str>,
    title: &str,
    command: String,
    kind: &str,
) -> TerminalSlot {
    let id = format!("slot-{}", *counter);
    *counter += 1;
    let name = match label {
        Some(dir) => format!("{dir} · {title}"),
        None => titlecase(title),
    };
    TerminalSlot {
        id,
        name,
        command,
        kind: kind.to_string(),
        cwd: label.map(str::to_string),
    }
}

/// Relative-path label for a dir under root (forward slashes); None for root.
fn rel_label(dir: &Path, root: &Path) -> Option<String> {
    dir.strip_prefix(root)
        .ok()
        .filter(|rel| !rel.as_os_str().is_empty())
        .map(|rel| rel.to_string_lossy().replace('\\', "/"))
}

/// Scan a single directory for manifests. Returns true if it looks like an app
/// root (so the caller stops descending into it).
fn scan_dir(
    dir: &Path,
    root: &Path,
    label: Option<&str>,
    counter: &mut usize,
    previews: &mut Vec<TerminalSlot>,
    validations: &mut Vec<TerminalSlot>,
) -> bool {
    let mut found = false;

    let scripts = package_scripts(dir);
    if !scripts.is_empty() {
        found = true;
        let pm = detect_pm(dir, root);
        if let Some(script) = PREVIEW_SCRIPTS
            .iter()
            .find(|candidate| scripts.iter().any(|name| name == *candidate))
        {
            previews.push(make_slot(
                counter,
                label,
                script,
                format!("{pm} {script}"),
                "preview",
            ));
        }
        for script in VALIDATION_SCRIPTS {
            if scripts.iter().any(|name| name == script) {
                validations.push(make_slot(
                    counter,
                    label,
                    script,
                    format!("{pm} {script}"),
                    "validation",
                ));
            }
        }
    }

    if dir.join("Cargo.toml").exists() {
        found = true;
        validations.push(make_slot(
            counter,
            label,
            "cargo test",
            "cargo test".to_string(),
            "validation",
        ));
    }

    if dir.join("go.mod").exists() {
        found = true;
        validations.push(make_slot(
            counter,
            label,
            "go test",
            "go test ./...".to_string(),
            "validation",
        ));
    }

    if dir.join("pubspec.yaml").exists() {
        found = true;
        previews.push(make_slot(
            counter,
            label,
            "flutter run",
            "flutter run".to_string(),
            "preview",
        ));
        validations.push(make_slot(
            counter,
            label,
            "flutter test",
            "flutter test".to_string(),
            "validation",
        ));
    }

    if ["pyproject.toml", "requirements.txt", "setup.py", "Pipfile"]
        .iter()
        .any(|file| dir.join(file).exists())
    {
        found = true;
        if dir.join("manage.py").exists() {
            previews.push(make_slot(
                counter,
                label,
                "Django server",
                "python manage.py runserver".to_string(),
                "preview",
            ));
            validations.push(make_slot(
                counter,
                label,
                "Django tests",
                "python manage.py test".to_string(),
                "validation",
            ));
        } else {
            validations.push(make_slot(
                counter,
                label,
                "pytest",
                "python -m pytest".to_string(),
                "validation",
            ));
        }
    }

    found
}

/// Walk the project tree (bounded depth), scanning each directory and stopping
/// descent once an app root is found, so monorepo subdir apps are discovered.
fn scan_tree(
    dir: &Path,
    root: &Path,
    depth: usize,
    counter: &mut usize,
    previews: &mut Vec<TerminalSlot>,
    validations: &mut Vec<TerminalSlot>,
) {
    let label = rel_label(dir, root);
    let found = scan_dir(dir, root, label.as_deref(), counter, previews, validations);
    if found || depth >= 3 {
        return;
    }

    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut child_dirs: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| !name.starts_with('.') && !IGNORED_DIRS.contains(&name))
                .unwrap_or(false)
        })
        .collect();
    child_dirs.sort();
    for child in child_dirs {
        scan_tree(&child, root, depth + 1, counter, previews, validations);
    }
}

#[tauri::command]
pub fn suggest_terminal_slots(project_path: String) -> Result<Vec<TerminalSlot>, String> {
    let root = Path::new(&project_path);
    let mut counter = 0usize;
    let mut previews = Vec::new();
    let mut validations = Vec::new();

    scan_tree(root, root, 0, &mut counter, &mut previews, &mut validations);

    previews.truncate(4);
    validations.truncate(6);

    if previews.is_empty() {
        previews.push(make_slot(
            &mut counter,
            None,
            "Preview",
            String::new(),
            "preview",
        ));
    }
    if validations.is_empty() {
        validations.push(make_slot(
            &mut counter,
            None,
            "Validation",
            String::new(),
            "validation",
        ));
    }

    previews.extend(validations);
    Ok(previews)
}

#[tauri::command]
pub fn list_terminal_slots(project_path: String) -> Result<Vec<TerminalSlot>, String> {
    let path = slots_path(Path::new(&project_path));
    if !path.exists() {
        return Ok(Vec::new());
    }
    storage::read_json_file::<Vec<TerminalSlot>>(&path)
}

#[tauri::command]
pub fn save_terminal_slots(
    project_path: String,
    slots: Vec<TerminalSlot>,
) -> Result<Vec<TerminalSlot>, String> {
    let path = slots_path(Path::new(&project_path));
    storage::atomic_write_json(&path, &slots)?;
    Ok(slots)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::now_ms;
    use std::fs;

    fn slot(id: &str, kind: &str, command: &str) -> TerminalSlot {
        TerminalSlot {
            id: id.to_string(),
            name: id.to_string(),
            command: command.to_string(),
            kind: kind.to_string(),
            cwd: None,
        }
    }

    #[test]
    fn list_returns_empty_before_anything_is_saved() {
        let root = std::env::temp_dir().join(format!("loom-slots-empty-{}", now_ms()));
        fs::create_dir_all(&root).expect("test dir");

        let slots = list_terminal_slots(root.display().to_string()).expect("list should succeed");
        assert!(slots.is_empty());

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn suggest_scans_root_and_subdir_manifests_with_cwd() {
        let root = std::env::temp_dir().join(format!("loom-slots-suggest-{}", now_ms()));
        // Monorepo: no root package.json; apps live in subdirs.
        fs::create_dir_all(root.join("web")).expect("web dir");
        fs::create_dir_all(root.join("api")).expect("api dir");
        fs::write(
            root.join("web/package.json"),
            r#"{"scripts":{"dev":"vite","test":"vitest"}}"#,
        )
        .expect("web package.json");
        fs::write(root.join("api/Cargo.toml"), "[package]\nname=\"api\"\n").expect("api cargo");

        let slots = suggest_terminal_slots(root.display().to_string()).expect("suggest");

        // A preview for web's dev, run inside web/.
        let preview = slots
            .iter()
            .find(|slot| slot.kind == "preview" && slot.command == "npm run dev")
            .expect("web dev preview");
        assert_eq!(preview.cwd.as_deref(), Some("web"));

        // Validation slots: web test (cwd web) and api cargo test (cwd api).
        assert!(slots
            .iter()
            .any(|slot| slot.command == "npm run test" && slot.cwd.as_deref() == Some("web")));
        assert!(slots
            .iter()
            .any(|slot| slot.command == "cargo test" && slot.cwd.as_deref() == Some("api")));

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn suggest_finds_apps_nested_two_levels_deep() {
        let root = std::env::temp_dir().join(format!("loom-slots-nested-{}", now_ms()));
        fs::create_dir_all(root.join("frontend/todo-react")).expect("react dir");
        fs::create_dir_all(root.join("frontend/todo-flutter")).expect("flutter dir");
        fs::write(
            root.join("frontend/todo-react/package.json"),
            r#"{"scripts":{"dev":"vite","test":"vitest"}}"#,
        )
        .expect("react package.json");
        fs::write(
            root.join("frontend/todo-flutter/pubspec.yaml"),
            "name: todo\n",
        )
        .expect("flutter pubspec");

        let slots = suggest_terminal_slots(root.display().to_string()).expect("suggest");

        assert!(slots.iter().any(|slot| slot.command == "npm run dev"
            && slot.cwd.as_deref() == Some("frontend/todo-react")));
        assert!(slots.iter().any(|slot| slot.command == "flutter test"
            && slot.cwd.as_deref() == Some("frontend/todo-flutter")));

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn suggest_detects_python_and_django_commands() {
        let root = std::env::temp_dir().join(format!("loom-slots-python-{}", now_ms()));
        fs::create_dir_all(root.join("service")).unwrap();
        fs::write(
            root.join("service/pyproject.toml"),
            "[project]\nname='service'\n",
        )
        .unwrap();
        fs::write(root.join("service/manage.py"), "# django\n").unwrap();

        let slots = suggest_terminal_slots(root.display().to_string()).unwrap();
        assert!(slots.iter().any(|slot| {
            slot.command == "python manage.py runserver"
                && slot.cwd.as_deref() == Some("service")
                && slot.kind == "preview"
        }));
        assert!(slots.iter().any(|slot| {
            slot.command == "python manage.py test"
                && slot.cwd.as_deref() == Some("service")
                && slot.kind == "validation"
        }));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn suggest_falls_back_to_empty_editable_slots() {
        let root = std::env::temp_dir().join(format!("loom-slots-suggest-empty-{}", now_ms()));
        fs::create_dir_all(&root).expect("test dir");

        let slots = suggest_terminal_slots(root.display().to_string()).expect("suggest");
        assert_eq!(slots.len(), 2);
        assert_eq!(slots[0].kind, "preview");
        assert_eq!(slots[0].command, "");
        assert_eq!(slots[1].kind, "validation");

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn save_then_list_round_trips_slots() {
        let root = std::env::temp_dir().join(format!("loom-slots-roundtrip-{}", now_ms()));
        fs::create_dir_all(&root).expect("test dir");

        let saved = save_terminal_slots(
            root.display().to_string(),
            vec![
                slot("preview", "preview", "pnpm dev"),
                slot("validation", "validation", "pnpm test"),
            ],
        )
        .expect("save should succeed");
        assert_eq!(saved.len(), 2);

        let loaded = list_terminal_slots(root.display().to_string()).expect("list should succeed");
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].command, "pnpm dev");
        assert_eq!(loaded[1].kind, "validation");

        fs::remove_dir_all(root).ok();
    }
}
