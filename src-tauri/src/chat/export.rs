//! Portable snapshot + readable transcript + explicitly bounded log copies.
use super::{
    repository::{validate_file, validate_id, ChatRepository},
    ChatExportResult, ChatSession,
};
use serde::Serialize;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LogEntry {
    turn_id: String,
    stream: String,
    path: Option<String>,
    source_prefix_bytes: u64,
    exported_bytes: u64,
    incomplete_tail_bytes: u64,
    captured_at_ms: u64,
    error: Option<String>,
}

fn new_file(path: &Path) -> Result<File, String> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(path)
        .map_err(|e| format!("cannot create export file: {e}"))
}

fn json_file(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let mut file = new_file(path)?;
    serde_json::to_writer_pretty(&mut file, value)
        .map_err(|e| format!("cannot write export JSON: {e}"))?;
    file.write_all(b"\n")
        .and_then(|_| file.flush())
        .map_err(|e| e.to_string())
}

fn fence(text: &str, language: &str) -> String {
    let longest = text.split(|ch| ch != '`').map(str::len).max().unwrap_or(0);
    let delimiter = "`".repeat(3.max(longest + 1));
    format!("{delimiter}{language}\n{text}\n{delimiter}\n\n")
}

fn inline(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace(['\n', '\r'], " ")
        .replace('[', "\\[")
        .replace(']', "\\]")
        .replace('`', "\\`")
}

fn markdown(
    session: &ChatSession,
    in_progress: bool,
    logs: &[LogEntry],
    warnings: &[String],
) -> Result<String, String> {
    let mut out = String::from("# Loom 会话导出\n\n## 标题\n\n");
    out.push_str(&fence(&session.title, "text"));
    out.push_str(&format!(
        "- 会话：{}\n- 快照序号：{}\n- 消息数：{}\n- 运行中快照：{}\n\n",
        inline(&session.id),
        session.last_seq,
        session.messages.len(),
        in_progress
    ));
    out.push_str("session.json 保留完整结构化快照；manifest.json 记录日志边界与缺失项。请备份整个导出目录。旧版 Loom 不会自动导入此 JSON。\n\n");
    for (index, message) in session.messages.iter().enumerate() {
        out.push_str(&format!(
            "## 消息 {} · {} · {}\n\n",
            index + 1,
            inline(&message.role),
            inline(&message.status)
        ));
        out.push_str(&fence(&message.content, "text"));
        if !message.parts.is_empty() {
            out.push_str("### 消息分段\n\n");
            out.push_str(&fence(
                &serde_json::to_string_pretty(&message.parts).map_err(|e| e.to_string())?,
                "json",
            ));
        }
        if let Some(error) = &message.error_summary {
            out.push_str("### 错误\n\n");
            out.push_str(&fence(error, "text"));
        }
    }
    out.push_str("## 回合执行记录\n\n");
    for turn in &session.turns {
        out.push_str(&fence(
            &serde_json::to_string_pretty(turn).map_err(|e| e.to_string())?,
            "json",
        ));
        for log in logs.iter().filter(|log| log.turn_id == turn.id) {
            if let Some(path) = &log.path {
                out.push_str(&format!(
                    "- [{}]({})（{} bytes）\n",
                    log.stream, path, log.exported_bytes
                ));
            }
        }
        out.push('\n');
    }
    if !warnings.is_empty() {
        out.push_str("## 不完整证据说明\n\n");
        out.push_str(&fence(&warnings.join("\n"), "text"));
    }
    Ok(out)
}

fn capture_log(
    repository: &ChatRepository,
    session_id: &str,
    turn_id: &str,
    stream: &str,
) -> Result<(Vec<u8>, u64), String> {
    let path = repository
        .run_directory(session_id, turn_id, false)?
        .join(format!("{stream}.log"));
    validate_file(&path)?;
    let file = File::open(&path).map_err(|e| format!("run log unavailable: {e}"))?;
    let length = file.metadata().map_err(|e| e.to_string())?.len();
    if length > 8 * 1024 * 1024 {
        return Err("run log exceeds the 8 MiB source limit".into());
    }
    let mut bytes = Vec::new();
    file.take(length)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 != length {
        return Err("run log was truncated during export".into());
    }
    let complete = bytes
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |index| index + 1);
    let tail = (bytes.len() - complete) as u64;
    bytes.truncate(complete);
    std::str::from_utf8(&bytes).map_err(|_| "run log contains invalid UTF-8")?;
    Ok((bytes, tail))
}

struct Staging {
    path: PathBuf,
    published: bool,
}
impl Drop for Staging {
    fn drop(&mut self) {
        if !self.published {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

pub(super) fn write(
    repository: &ChatRepository,
    session: ChatSession,
    root: &Path,
    export_id: &str,
) -> Result<ChatExportResult, String> {
    let destination = root.join(export_id);
    if fs::symlink_metadata(&destination).is_ok() {
        return Err("export already exists; refusing to overwrite".into());
    }
    let path = root.join(format!(".partial-{export_id}"));
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(&path)
        .map_err(|e| format!("cannot reserve export directory: {e}"))?;
    let mut staging = Staging {
        path,
        published: false,
    };
    let in_progress = session.turn_status == "streaming"
        || session.turns.iter().any(|turn| !turn.status.is_terminal());
    let mut warnings = Vec::new();
    let mut logs = Vec::new();
    if in_progress {
        warnings.push("导出发生在回合运行中：转录固定在快照序号，各日志只包含分别捕获的完整行前缀，不代表最终执行结果。".into());
    }
    if session.turns.is_empty() && !session.messages.is_empty() {
        warnings.push("此会话没有已记录的回合/独立日志；旧运行证据未被补造。".into());
    }
    json_file(&staging.path.join("session.json"), &session)?;
    fs::create_dir(staging.path.join("runs")).map_err(|e| e.to_string())?;
    for turn in &session.turns {
        validate_id(&turn.id)?;
        fs::create_dir(staging.path.join("runs").join(&turn.id)).map_err(|e| e.to_string())?;
        for stream in ["stdout", "stderr"] {
            let mut entry = LogEntry {
                turn_id: turn.id.clone(),
                stream: stream.into(),
                path: None,
                source_prefix_bytes: 0,
                exported_bytes: 0,
                incomplete_tail_bytes: 0,
                captured_at_ms: crate::models::now_ms() as u64,
                error: None,
            };
            match capture_log(repository, &session.id, &turn.id, stream) {
                Ok((bytes, tail)) => {
                    let relative = format!("runs/{}/{stream}.log", turn.id);
                    let mut target = new_file(&staging.path.join(&relative))?;
                    target
                        .write_all(&bytes)
                        .and_then(|_| target.flush())
                        .map_err(|e| format!("cannot copy export log: {e}"))?;
                    entry.source_prefix_bytes = bytes.len() as u64;
                    entry.exported_bytes = bytes.len() as u64;
                    entry.incomplete_tail_bytes = tail;
                    entry.path = Some(relative);
                    if tail > 0 {
                        warnings.push(format!(
                            "{} {stream}：末尾 {tail} bytes 未形成完整行，未导出。",
                            turn.id
                        ));
                    }
                }
                Err(error) => {
                    warnings.push(format!("{} {stream}：{error}", turn.id));
                    entry.error = Some(error);
                }
            }
            logs.push(entry);
        }
    }
    let mut transcript = new_file(&staging.path.join("transcript.md"))?;
    transcript
        .write_all(markdown(&session, in_progress, &logs, &warnings)?.as_bytes())
        .and_then(|_| transcript.flush())
        .map_err(|e| e.to_string())?;
    json_file(
        &staging.path.join("manifest.json"),
        &serde_json::json!({"exportSchemaVersion":1,"sessionSchemaVersion":session.schema_version,
        "sessionId":session.id,"snapshotSeq":session.last_seq,"exportedAtMs":crate::models::now_ms() as u64,"inProgress":in_progress,
        "jsonFile":"session.json","markdownFile":"transcript.md","logs":logs,"warnings":warnings}),
    )?;
    if fs::symlink_metadata(&destination).is_ok() {
        return Err("export destination appeared; refusing to overwrite".into());
    }
    fs::rename(&staging.path, &destination)
        .map_err(|e| format!("cannot publish export directory: {e}"))?;
    staging.published = true;
    Ok(ChatExportResult {
        directory: destination.to_string_lossy().into_owned(),
        json_path: destination
            .join("session.json")
            .to_string_lossy()
            .into_owned(),
        markdown_path: destination
            .join("transcript.md")
            .to_string_lossy()
            .into_owned(),
        snapshot_seq: session.last_seq,
        in_progress,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        chat::{models::ChatStreamEvent, turns},
        models::IdGenerator,
    };
    struct Fixture {
        path: PathBuf,
        keep: bool,
    }
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(IdGenerator::default().next("loom-export-sample"));
            fs::create_dir_all(&path).unwrap();
            Self { path, keep: false }
        }
        fn repo(&self) -> ChatRepository {
            ChatRepository::open(&self.path).unwrap()
        }
        fn session(&self) -> ChatSession {
            let value: serde_json::Value =
                serde_json::from_str(include_str!("../../../contracts/tauri-contract.json"))
                    .unwrap();
            let mut session: ChatSession =
                serde_json::from_value(value["modelSamples"]["chatSession"].clone()).unwrap();
            session.project_path = self.repo().project_path().to_string_lossy().into_owned();
            session.title = "中文导出 / 安全围栏".into();
            session.messages[0].content =
                "原始文本\n```\n<script>not executable</script>\n````\n中文与尾部空白  ".into();
            session.status = "archived".into();
            session
        }
        fn populate(&self) -> ChatRepository {
            let repo = self.repo();
            let session = self.session();
            repo.create(&session).unwrap();
            let directory = self
                .path
                .join(".loom/chat/v2/sessions/chat-contract/runs/turn-contract");
            fs::create_dir_all(&directory).unwrap();
            fs::write(
                directory.join("stdout.log"),
                "已完成输出 中\napi_key=[REDACTED]\n未提交尾部",
            )
            .unwrap();
            fs::write(directory.join("stderr.log"), "diagnostic line\n").unwrap();
            repo
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            if !self.keep {
                let _ = fs::remove_dir_all(&self.path);
            }
        }
    }

    #[test]
    fn exports_complete_portable_snapshot_literal_markdown_and_log_prefixes_without_overwrite() {
        let f = Fixture::new();
        let repo = f.populate();
        let before = repo.load("chat-contract").unwrap();
        let result = repo.export_session("chat-contract", "export-one").unwrap();
        let exported: ChatSession =
            serde_json::from_slice(&fs::read(&result.json_path).unwrap()).unwrap();
        assert_eq!(
            serde_json::to_value(&exported).unwrap(),
            serde_json::to_value(&before).unwrap()
        );
        assert_eq!(exported.status, "archived");
        assert!(!result.in_progress);
        let manifest: serde_json::Value = serde_json::from_slice(
            &fs::read(Path::new(&result.directory).join("manifest.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(manifest["snapshotSeq"], before.last_seq);
        assert!(manifest["logs"][0]["incompleteTailBytes"].as_u64().unwrap() > 0);
        let stdout =
            fs::read_to_string(Path::new(&result.directory).join("runs/turn-contract/stdout.log"))
                .unwrap();
        assert_eq!(stdout, "已完成输出 中\napi_key=[REDACTED]\n");
        let md = fs::read_to_string(&result.markdown_path).unwrap();
        assert!(md.contains(&before.messages[0].content));
        let mut html = String::new();
        pulldown_cmark::html::push_html(&mut html, pulldown_cmark::Parser::new(&md));
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(repo.export_session("chat-contract", "export-one").is_err());
        assert_eq!(fs::read_to_string(&result.markdown_path).unwrap(), md);
        assert!(repo.export_session("chat-contract", "export-two").is_ok());
        assert_eq!(
            repo.load("chat-contract").unwrap().last_seq,
            before.last_seq
        );
        assert!(fs::read_dir(self_root(&f)).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".partial")));
    }

    fn self_root(f: &Fixture) -> PathBuf {
        f.path.join(".loom/chat/exports")
    }

    #[test]
    fn active_export_is_a_fixed_snapshot_and_does_not_capture_later_output() {
        let f = Fixture::new();
        let repo = f.populate();
        repo.update("chat-contract", |s| {
            s.status = "active".into();
            Ok(())
        })
        .unwrap();
        let (_, (), lease) = repo
            .begin("chat-contract", "turn-live", |session| {
                for (id, role) in [("live-user", "user"), ("live-assistant", "assistant")] {
                    let mut message = session.messages[0].clone();
                    message.id = id.into();
                    message.role = role.into();
                    message.content = "prefix".into();
                    message.status = if role == "assistant" {
                        "streaming"
                    } else {
                        "complete"
                    }
                    .into();
                    session.messages.push(message);
                }
                session.turns.push(turns::new_turn(
                    "turn-live",
                    "request-live",
                    "live-user",
                    "live-assistant",
                    session.turns[0].invocation.clone(),
                ));
                Ok(())
            })
            .unwrap();
        let mut logs = repo.open_run_logs(&lease).unwrap();
        logs.append(false, "first log").unwrap();
        let result = repo.export_session("chat-contract", "export-live").unwrap();
        assert!(result.in_progress);
        logs.append(false, "later log").unwrap();
        repo.append_stream(
            &lease,
            ChatStreamEvent {
                session_id: "chat-contract".into(),
                turn_id: "turn-live".into(),
                message_id: "live-assistant".into(),
                delta: " LATER".into(),
                done: false,
                part: None,
            },
        )
        .unwrap();
        let exported: ChatSession =
            serde_json::from_slice(&fs::read(&result.json_path).unwrap()).unwrap();
        assert_eq!(exported.last_seq, result.snapshot_seq);
        assert!(!exported.messages.last().unwrap().content.contains("LATER"));
        assert_eq!(
            fs::read_to_string(Path::new(&result.directory).join("runs/turn-live/stdout.log"))
                .unwrap(),
            "first log\n"
        );
        assert!(repo.load("chat-contract").unwrap().last_seq > result.snapshot_seq);
    }

    #[test]
    fn legacy_export_preserves_original_v1_bytes_and_does_not_invent_run_evidence() {
        let f = Fixture::new();
        let repo = f.repo();
        let mut value = serde_json::to_value(f.session()).unwrap();
        value["schemaVersion"] = serde_json::json!(1);
        for key in [
            "turns",
            "sendReceipts",
            "resumeHandle",
            "lastSeq",
            "revision",
        ] {
            value.as_object_mut().unwrap().remove(key);
        }
        let raw = serde_json::to_string_pretty(&value).unwrap();
        let path = f.path.join(".loom/chat/sessions/chat-contract.json");
        fs::write(&path, &raw).unwrap();
        let result = repo
            .export_session("chat-contract", "export-legacy")
            .unwrap();
        assert_eq!(fs::read_to_string(path).unwrap(), raw);
        assert!(result
            .warnings
            .iter()
            .any(|warning| warning.contains("旧运行证据")));
        let session: ChatSession =
            serde_json::from_slice(&fs::read(result.json_path).unwrap()).unwrap();
        assert_eq!(session.schema_version, 2);
        assert!(session.turns.is_empty());
    }

    #[test]
    fn damaged_missing_and_symlink_logs_are_explicit_and_export_root_cannot_escape() {
        let f = Fixture::new();
        let repo = f.populate();
        let logs = f
            .path
            .join(".loom/chat/v2/sessions/chat-contract/runs/turn-contract");
        fs::write(logs.join("stdout.log"), [255, b'\n']).unwrap();
        fs::remove_file(logs.join("stderr.log")).unwrap();
        let result = repo
            .export_session("chat-contract", "export-damaged")
            .unwrap();
        assert_eq!(result.warnings.len(), 2);
        let manifest: serde_json::Value = serde_json::from_slice(
            &fs::read(Path::new(&result.directory).join("manifest.json")).unwrap(),
        )
        .unwrap();
        assert!(manifest["logs"]
            .as_array()
            .unwrap()
            .iter()
            .all(|log| log["path"].is_null() && log["error"].is_string()));
        assert!(repo.export_session("chat-contract", "../escape").is_err());
        // A corrupt stored turn id must fail after staging starts, without
        // publishing or leaving an apparently complete export directory.
        let bad = Fixture::new();
        let bad_repo = bad.repo();
        let mut bad_session = bad.session();
        bad_session.turns[0].id = "../outside".into();
        bad_repo.create(&bad_session).unwrap();
        assert!(bad_repo
            .export_session("chat-contract", "export-invalid-turn")
            .is_err());
        assert!(fs::read_dir(self_root(&bad)).unwrap().next().is_none());
        #[cfg(unix)]
        {
            let outside = f.path.join("outside.txt");
            fs::write(&outside, "OUTSIDE SENTINEL").unwrap();
            std::os::unix::fs::symlink(&outside, logs.join("stderr.log")).unwrap();
            let symlinked = repo
                .export_session("chat-contract", "export-symlink")
                .unwrap();
            assert!(symlinked
                .warnings
                .iter()
                .any(|warning| warning.contains("regular file")));
            let g = Fixture::new();
            let other = g.populate();
            let external = g.path.join("outside-dir");
            fs::create_dir(&external).unwrap();
            std::os::unix::fs::symlink(&external, self_root(&g)).unwrap();
            assert!(other
                .export_session("chat-contract", "export-blocked")
                .is_err());
            assert!(fs::read_dir(external).unwrap().next().is_none());
        }
    }

    #[test]
    #[ignore = "explicitly generates a local rollback sample; no network or agent calls"]
    fn generate_rollback_sample() {
        assert_eq!(std::env::var("LOOM_EXPORT_SAMPLE").as_deref(), Ok("1"));
        let mut f = Fixture::new();
        let repo = f.populate();
        let logs = f
            .path
            .join(".loom/chat/v2/sessions/chat-contract/runs/turn-contract");
        fs::write(
            logs.join("stdout.log"),
            "已完成输出 中\napi_key=[REDACTED]\n",
        )
        .unwrap();
        let result = repo
            .export_session("chat-contract", "export-rollback-sample")
            .unwrap();
        assert!(result.warnings.is_empty());
        f.keep = true;
        println!("EXPORT_SAMPLE={}", result.directory);
    }
}
