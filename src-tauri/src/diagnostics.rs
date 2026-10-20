use crate::{agents, models::now_ms, projects::canonical_project_path, settings, storage};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    path::Path,
};
use tauri::AppHandle;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportDiagnosticBundleInput {
    pub project_path: String,
    pub target_path: String,
    #[serde(default)]
    pub include_log_tails: bool,
}

fn redact(value: &str, project: &Path) -> String {
    let mut text = agents::redact_sensitive_text(value)
        .replace(&project.to_string_lossy().to_string(), "<project>");
    if let Some(home) = std::env::var_os("HOME") {
        text = text.replace(&home.to_string_lossy().to_string(), "<home>");
    }
    text
}

fn collect_logs(project: &Path) -> Result<Vec<Value>, String> {
    let root = project.join(".loom/chat/v2/sessions");
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut logs = Vec::new();
    let mut directories = vec![root];
    let mut visited = 0;
    while let Some(directory) = directories.pop() {
        visited += 1;
        if visited > 1000 || logs.len() >= 20 {
            break;
        }
        if !directory
            .canonicalize()
            .map_err(|error| error.to_string())?
            .starts_with(project)
        {
            continue;
        }
        for entry in fs::read_dir(&directory).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();
            let kind = entry.file_type().map_err(|error| error.to_string())?;
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                directories.push(path);
                continue;
            }
            if !matches!(
                path.file_name().and_then(|value| value.to_str()),
                Some("stdout.log" | "stderr.log")
            ) {
                continue;
            }
            if logs.len() >= 20 {
                break;
            }
            let mut file = fs::File::open(&path).map_err(|error| error.to_string())?;
            let len = file.metadata().map_err(|error| error.to_string())?.len();
            file.seek(SeekFrom::Start(len.saturating_sub(65536)))
                .map_err(|error| error.to_string())?;
            let mut bytes = Vec::new();
            file.take(65536)
                .read_to_end(&mut bytes)
                .map_err(|error| error.to_string())?;
            let text = String::from_utf8_lossy(&bytes);
            let lines = text
                .lines()
                .rev()
                .take(200)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .map(agents::stream::redact_stream_line)
                .collect::<Vec<_>>()
                .join("\n");
            logs.push(json!({"path":path.strip_prefix(project).unwrap_or(&path).to_string_lossy(), "tail":redact(&lines, project), "truncated":len > 65536 || text.lines().count() > 200}));
        }
    }
    Ok(logs)
}

#[tauri::command]
pub fn export_diagnostic_bundle(
    app: AppHandle,
    input: ExportDiagnosticBundleInput,
) -> Result<String, String> {
    let project = canonical_project_path(&input.project_path)?;
    if !storage::load_recent_projects(&app)?
        .iter()
        .any(|item| Path::new(&item.path) == project)
    {
        return Err("open this project in Loom before exporting diagnostics".into());
    }
    let target = Path::new(&input.target_path);
    if !target.is_absolute() || target.file_name().is_none() {
        return Err("an absolute export file path is required".into());
    }
    if target
        .symlink_metadata()
        .is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        return Err("export target cannot be a symlink".into());
    }
    let bundle = json!({
        "schemaVersion":2, "generatedAtMs":now_ms(),
        "loom":{"version":env!("CARGO_PKG_VERSION"), "backend":"tauri"},
        "system":{"os":std::env::consts::OS,"arch":std::env::consts::ARCH},
        "project":{"name":project.file_name().unwrap_or_default().to_string_lossy(),"path":"<project>"},
        "settings":settings::load_app_settings_for_app(&app)?,
        "agents":agents::load_agents(&app)?.into_iter().map(|agent| json!({"id":agent.id,"adapterType":agent.adapter_type,"enabled":agent.enabled,"available":agent.available})).collect::<Vec<_>>(),
        "logs":if input.include_log_tails { collect_logs(&project)? } else { Vec::new() },
        "note":"Chat logs only; legacy Task data is not read or modified. Log tails are bounded and may be incomplete. Review before sharing."
    });
    storage::atomic_write_json(target, &bundle)?;
    Ok(target.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn diagnostic_bundle_ignores_legacy_task_logs_and_bounds_chat_tails() {
        let root = std::env::temp_dir()
            .join(crate::models::IdGenerator::default().next("loom-diagnostics"));
        fs::create_dir_all(root.join(".loom/logs/task-old")).unwrap();
        fs::write(
            root.join(".loom/logs/task-old/stdout.log"),
            "legacy-private",
        )
        .unwrap();
        fs::create_dir_all(root.join(".loom/chat/v2/sessions/chat-a/runs/turn-a")).unwrap();
        fs::write(
            root.join(".loom/chat/v2/sessions/chat-a/runs/turn-a/stdout.log"),
            "hello\nAPI_KEY=private\n",
        )
        .unwrap();
        let logs = collect_logs(&root.canonicalize().unwrap()).unwrap();
        assert_eq!(logs.len(), 1);
        assert!(!logs[0]["tail"].as_str().unwrap().contains("private"));
        fs::remove_dir_all(root).unwrap();
    }
}
