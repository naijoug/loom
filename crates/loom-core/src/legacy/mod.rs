//! Read-only inventory of legacy Loom project data. Preview never imports,
//! upgrades, truncates, executes, or returns source credentials/invocations.
mod chat;
mod source;

use crate::{projection, CoreError, Id, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, path::Path};

#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyFile {
    pub path: String,
    pub bytes: u64,
    pub sha256: Option<String>,
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyIssue {
    pub severity: String,
    pub code: String,
    pub path: String,
    pub message: String,
}
impl LegacyIssue {
    fn warning(code: &str, path: &str, message: &str) -> Self {
        Self {
            severity: "warning".into(),
            code: code.into(),
            path: path.into(),
            message: message.into(),
        }
    }
    fn error(code: &str, path: &str, error: impl std::fmt::Display) -> Self {
        Self {
            severity: "error".into(),
            code: code.into(),
            path: path.into(),
            message: error.to_string(),
        }
    }
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyItem {
    pub kind: String,
    pub source_id: String,
    pub source_path: String,
    pub source_version: u32,
    pub title: Option<String>,
    pub message_count: usize,
    pub content_hash: Option<String>,
    pub issues: Vec<LegacyIssue>,
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyPreview {
    pub project_directory: String,
    pub fingerprint: String,
    pub items: Vec<LegacyItem>,
    pub files: Vec<LegacyFile>,
    pub issues: Vec<LegacyIssue>,
}

fn parse(bytes: &[u8]) -> Result<Value> {
    // Do not include untrusted source values in diagnostics (may contain secrets).
    serde_json::from_slice(bytes).map_err(|e| {
        CoreError::corrupt(format!(
            "invalid legacy JSON at line {}, column {}",
            e.line(),
            e.column()
        ))
    })
}

/// Preview a canonical project root. Missing legacy directories mean no data;
/// inaccessible/symlinked directories are reported, never silently skipped.
pub fn inspect_legacy_project(directory: &Path) -> Result<LegacyPreview> {
    Ok(capture_project(directory, false)?.preview)
}

pub(crate) struct CapturedProject {
    pub preview: LegacyPreview,
    pub documents: BTreeMap<String, Value>,
    pub files: BTreeMap<String, Vec<u8>>,
    pub item_files: BTreeMap<String, Vec<String>>,
}
pub(crate) fn capture_project(directory: &Path, capture: bool) -> Result<CapturedProject> {
    let mut source = source::Source::open(directory)?;
    source.capture = capture;
    let mut documents = BTreeMap::new();
    let mut item_files = BTreeMap::new();
    let mut issues = Vec::new();
    let mut candidates = BTreeMap::<(String, String), (String, u32)>::new();
    for (base, kind, version, suffix) in [
        (".loom/chat/sessions", "chat", 1, true),
        (".loom/chat/v2/sessions", "chat", 2, false),
        (".loom/tasks", "task", 1, true),
    ] {
        let names = match source.list(base) {
            Ok(names) => names,
            Err(error) => {
                issues.push(LegacyIssue::error("directory_unreadable", base, error));
                continue;
            }
        };
        for name in names {
            let id = if suffix {
                let Some(id) = name.strip_suffix(".json") else {
                    continue;
                };
                id
            } else {
                &name
            };
            if let Err(error) = Id::new(id) {
                issues.push(LegacyIssue::error(
                    "invalid_id",
                    &format!("{base}/{name}"),
                    error,
                ));
                continue;
            }
            let path = if suffix {
                format!("{base}/{name}")
            } else {
                format!("{base}/{name}/events.jsonl")
            };
            if candidates
                .insert((kind.into(), id.into()), (path, version))
                .is_some()
            {
                issues.push(LegacyIssue::warning(
                    "v1_shadowed",
                    &format!(".loom/chat/sessions/{id}.json"),
                    "同 ID 已有 v2；不导入 v1，也不在 v2 损坏时回退",
                ));
            }
        }
    }
    // If v2 cannot be enumerated, v1 candidates cannot be safely deemed current.
    let v2_unreadable = issues
        .iter()
        .any(|i| i.severity == "error" && i.path == ".loom/chat/v2/sessions");
    let mut items = Vec::new();
    for ((kind, id), (path, version)) in candidates {
        source.touched.clear();
        let item_key = format!("{kind}/{id}");
        let mut item = LegacyItem {
            kind: kind.clone(),
            source_id: id.clone(),
            source_path: path.clone(),
            source_version: version,
            title: None,
            message_count: 0,
            content_hash: None,
            issues: Vec::new(),
        };
        let result = if kind == "chat" && version == 1 && v2_unreadable {
            Err(CoreError::corrupt(
                "v2 inventory unavailable; refusing ambiguous v1 fallback",
            ))
        } else if kind == "chat" {
            chat::load(&mut source, &path, &id, version, &mut item.issues)
        } else {
            load_task(&mut source, &path, &id)
        };
        match result {
            Ok(value) => {
                item.title = Some(chat::string(&value, "title")?.into());
                item.message_count = value
                    .get("messages")
                    .and_then(Value::as_array)
                    .map(Vec::len)
                    .unwrap_or(0);
                item.content_hash = Some(projection::checksum(&value)?);
                inspect_resources(&mut source, &item, &value)
                    .into_iter()
                    .for_each(|issue| item.issues.push(issue));
                if kind == "chat" {
                    item.issues.push(LegacyIssue::warning(
                        "runtime_reset",
                        &path,
                        "导入不会继承旧进程、续聊句柄、凭据或权限授权",
                    ));
                } else {
                    item.issues.push(LegacyIssue::warning(
                        "historical_task",
                        &path,
                        "旧 Task 将保留为历史记录，不启动或映射为新 DAG",
                    ));
                }
                if capture {
                    documents.insert(item_key.clone(), value);
                }
            }
            Err(error) => item
                .issues
                .push(LegacyIssue::error("invalid_document", &path, error)),
        }
        item_files.insert(item_key, source.touched.iter().cloned().collect());
        items.push(item);
    }
    let files: Vec<_> = source.files.into_values().collect();
    let fingerprint = projection::checksum(&(
        &source.project,
        &source.directories,
        &files,
        &items,
        &issues,
    ))?;
    Ok(CapturedProject {
        preview: LegacyPreview {
            project_directory: source.project,
            fingerprint,
            items,
            files,
            issues,
        },
        documents,
        files: source.captured,
        item_files,
    })
}

fn load_task(source: &mut source::Source, path: &str, id: &str) -> Result<Value> {
    let mut value = parse(&source.required(path)?)?;
    if value.get("schemaVersion").is_some() {
        if value.get("schemaVersion").and_then(Value::as_u64) != Some(1) {
            return Err(CoreError::corrupt("unsupported legacy Task schema"));
        }
        value = value
            .get("data")
            .cloned()
            .ok_or_else(|| CoreError::corrupt("missing legacy Task envelope data"))?;
    }
    if chat::string(&value, "id")? != id || chat::string(&value, "projectPath")? != source.project {
        return Err(CoreError::corrupt("legacy Task identity mismatch"));
    }
    for field in ["title", "rawRequirement", "status"] {
        chat::string(&value, field)?;
    }
    for field in ["createdAtMs", "updatedAtMs"] {
        if value
            .get(field)
            .and_then(Value::as_u64)
            .is_none_or(|n| n > crate::MAX_SAFE_SEQUENCE)
        {
            return Err(CoreError::corrupt("invalid legacy Task timestamp"));
        }
    }
    for field in ["events", "commandRuns", "feedback"] {
        if !value.get(field).is_some_and(Value::is_array) {
            return Err(CoreError::corrupt(format!(
                "legacy Task {field} must be an array"
            )));
        }
    }
    Ok(value)
}

fn inspect_resources(
    source: &mut source::Source,
    item: &LegacyItem,
    value: &Value,
) -> Vec<LegacyIssue> {
    let mut refs = Vec::new();
    if item.kind == "chat" {
        let base = format!(".loom/chat/v2/sessions/{}", item.source_id);
        if let Some(turns) = value.get("turns").and_then(Value::as_array) {
            for turn in turns {
                for field in ["stdoutLogRef", "stderrLogRef"] {
                    if let Some(path) = turn
                        .get(field)
                        .and_then(Value::as_str)
                        .filter(|s| !s.is_empty())
                    {
                        refs.push((path.to_owned(), format!("{base}/{path}")));
                    }
                }
            }
        }
    } else {
        collect_task_refs(value, &mut refs);
        for (_, path) in &mut refs {
            if Path::new(path).is_absolute() {
                *path = Path::new(path)
                    .strip_prefix(&source.project)
                    .ok()
                    .and_then(Path::to_str)
                    .unwrap_or("../outside-project")
                    .into();
            }
        }
    }
    refs.sort();
    refs.dedup();
    let mut issues = Vec::new();
    for (original, path) in refs {
        if source::validate_path(&original).is_err() && !Path::new(&original).is_absolute() {
            issues.push(LegacyIssue::warning(
                "unsafe_resource",
                &original,
                "资源路径不安全；不会读取",
            ));
            continue;
        }
        // Chat log refs must be session-relative; Task paths may be project-absolute.
        if item.kind == "chat"
            && (Path::new(&original).is_absolute() || !original.starts_with("runs/"))
        {
            issues.push(LegacyIssue::warning(
                "unsafe_resource",
                &original,
                "Chat 日志必须属于当前会话 runs 目录",
            ));
            continue;
        }
        match source.read(&path) {
            Ok(Some(_)) => {}
            Ok(None) => issues.push(LegacyIssue::warning(
                "missing_resource",
                &original,
                "引用资源不存在；预检已记录缺失",
            )),
            Err(error) => issues.push(LegacyIssue::warning(
                "unreadable_resource",
                &original,
                &error.to_string(),
            )),
        }
    }
    issues
}

fn collect_task_refs(value: &Value, refs: &mut Vec<(String, String)>) {
    match value {
        Value::Object(fields) => {
            for (key, value) in fields {
                if [
                    "storedPath",
                    "finalPlanPath",
                    "finalPlanHtmlPath",
                    "stdoutLogRef",
                    "stderrLogRef",
                    "logPath",
                    "logFile",
                    "planPath",
                    "stderrRef",
                    "jsonPath",
                    "markdownPath",
                ]
                .contains(&key.as_str())
                {
                    if let Some(path) = value.as_str().filter(|s| !s.is_empty()) {
                        refs.push((path.into(), path.into()));
                    }
                } else if key == "evidenceRef" {
                    // Historical evidenceRef is either a run ID or a file path.
                    if let Some(path) = value
                        .as_str()
                        .filter(|s| s.contains('/') || s.contains('\\'))
                    {
                        refs.push((path.into(), path.into()));
                    }
                } else {
                    collect_task_refs(value, refs);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_task_refs(item, refs);
            }
        }
        _ => {}
    }
}
