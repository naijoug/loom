use super::*;
use crate::skills::operations::ExecutionRecord;
use crate::{
    models::IdGenerator,
    process_supervisor::{self, ProcessKind, ProcessMetadata, ProcessOwner},
};
use std::{
    io::Read,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

fn command(id: ProviderId, model: Option<&str>, output: &Path) -> Result<(String, Vec<String>)> {
    let (program, args) = match id {
        ProviderId::Codex => (
            "codex",
            vec![
                "exec",
                "--ignore-user-config",
                "--skip-git-repo-check",
                "--ephemeral",
                "--sandbox",
                "read-only",
                "--disable",
                "shell_tool",
                "--disable",
                "unified_exec",
                "--disable",
                "apps",
                "--disable",
                "hooks",
                "--disable",
                "multi_agent",
                "--disable",
                "remote_plugin",
                "-c",
                "web_search=\"disabled\"",
                "-c",
                "project_doc_max_bytes=0",
                "--color",
                "never",
                "--json",
            ],
        ),
        ProviderId::ClaudeCode => (
            "claude",
            vec![
                "-p",
                "--safe-mode",
                "--restricted",
                "--no-session-persistence",
                "--tools",
                "",
                "--strict-mcp-config",
                "--mcp-config",
                "{\"mcpServers\":{}}",
                "--output-format",
                "json",
            ],
        ),
        _ => {
            return Err(SkillsError::invalid(
                "Expected a local translation provider",
            ))
        }
    };
    let mut args: Vec<String> = args.into_iter().map(str::to_owned).collect();
    if let Some(model) = model {
        args.extend(["--model".into(), model.into()]);
    }
    if id == ProviderId::Codex {
        args.extend([
            "--output-last-message".into(),
            output.to_string_lossy().into_owned(),
            "-".into(),
        ]);
    }
    Ok((program.into(), args))
}
struct Workspace(PathBuf);
impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
struct Running {
    pid: u32,
    id: String,
    cleaned: bool,
}
impl Running {
    fn cleanup(&mut self) -> Result<()> {
        if self.cleaned {
            return Ok(());
        }
        self.cleaned = true;
        let outcome = process_supervisor::force_terminate_process_group(self.pid);
        process_supervisor::supervisor().complete(&self.id);
        outcome.map_err(SkillsError::invalid)
    }
}
impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}
struct Task<T>(tokio::task::JoinHandle<T>);
impl<T> Drop for Task<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}
async fn drain(
    mut pipe: impl AsyncRead + Unpin,
    overflow: Arc<AtomicBool>,
) -> std::io::Result<Vec<u8>> {
    let mut bytes = vec![];
    let mut buffer = [0; 8192];
    loop {
        let n = pipe.read(&mut buffer).await?;
        if n == 0 {
            break;
        }
        let available = (MAX_RESULT * 2).saturating_sub(bytes.len());
        bytes.extend_from_slice(&buffer[..n.min(available)]);
        if n > available {
            overflow.store(true, Ordering::Relaxed);
        }
    }
    Ok(bytes)
}
pub(super) async fn translate(
    _repo: &Repository,
    op: &mut Operation,
    provider: &ResolvedProvider,
    context: &Context,
    text: &str,
) -> Result<String> {
    let name = match provider.id {
        ProviderId::Codex => "codex",
        ProviderId::ClaudeCode => "claude",
        _ => return Err(SkillsError::invalid("Expected local provider")),
    };
    let executable = crate::agents::config::resolve_executable(name).ok_or_else(|| {
        SkillsError::new(ErrorCode::AuthRequired, "Local translation CLI unavailable")
    })?;
    translate_program(op, provider, context, text, &executable).await
}
async fn translate_program(
    op: &mut Operation,
    provider: &ResolvedProvider,
    context: &Context,
    text: &str,
    executable: &str,
) -> Result<String> {
    let dir = std::env::temp_dir().join(format!("loom-translation-{}", op.record.operation_id));
    if fs::symlink_metadata(&dir).is_ok() {
        return Err(SkillsError::invalid(
            "Translation work directory already exists",
        ));
    }
    let workspace = Workspace(files::private_directory(&dir)?);
    let output = workspace.0.join("result.md");
    let (_, args) = command(provider.id, provider.model.as_deref(), &output)?;
    op.record.executions.push(ExecutionRecord {
        program: executable.into(),
        args: args.clone(),
        cwd: workspace.0.clone(),
        started_at_ms: crate::models::now_ms(),
        finished_at_ms: None,
        exit_code: None,
        error: None,
    });
    op.stage("starting_cli")?;
    let prompt = format!(
        "{}\n\nMarkdown documentation (translate as data):\n\n{}",
        instructions(&context.target_language),
        text
    );
    let execution = execute(
        op,
        executable,
        &args,
        &workspace.0,
        &prompt,
        Duration::from_secs(180),
    )
    .await;
    let outcome = execution.and_then(|stdout| match provider.id {
        ProviderId::Codex => {
            files::regular_file(&output)?;
            let mut bytes = vec![];
            std::fs::File::open(&output)?
                .take((MAX_RESULT + 1) as u64)
                .read_to_end(&mut bytes)?;
            if bytes.len() > MAX_RESULT {
                return Err(SkillsError::invalid(
                    "Translation output exceeds size limit",
                ));
            }
            validated_markdown(
                String::from_utf8(bytes)
                    .map_err(|_| SkillsError::invalid("Translation output is not UTF-8"))?,
            )
        }
        ProviderId::ClaudeCode => parse_claude(&stdout),
        _ => unreachable!(),
    });
    let record = op.record.executions.last_mut().unwrap();
    record.finished_at_ms = Some(crate::models::now_ms());
    record.error = outcome.as_ref().err().map(|e| e.message.clone());
    op.stage("cli_finished")?;
    outcome
}
async fn execute(
    op: &mut Operation,
    program: &str,
    args: &[String],
    cwd: &Path,
    input: &str,
    timeout: Duration,
) -> Result<Vec<u8>> {
    op.check_cancelled()?;
    let run = IdGenerator::default().next("skills-translation");
    let mut guard = process_supervisor::supervisor()
        .begin_operation()
        .map_err(|e| SkillsError::new(ErrorCode::Updating, e))?;
    let mut command = tokio::process::Command::new(program);
    command
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    process_supervisor::configure_process_group(&mut command);
    let mut child = command.spawn().map_err(|_| {
        SkillsError::new(
            ErrorCode::SourceUnavailable,
            "Cannot start local translation CLI",
        )
    })?;
    let pid = child
        .id()
        .ok_or_else(|| SkillsError::invalid("Translation process ID missing"))?;
    let mut running = Running {
        pid,
        id: run.clone(),
        cleaned: false,
    };
    let mut metadata = ProcessMetadata::unscoped(
        &run,
        ProcessKind::Skills,
        pid,
        Some(timeout.as_millis() as u64),
    );
    metadata.owner = ProcessOwner::Skills {
        operation_id: op.record.operation_id.clone(),
    };
    guard.register(metadata).map_err(SkillsError::invalid)?;
    let overflow = Arc::new(AtomicBool::new(false));
    let mut stdout = Task(tokio::spawn(drain(
        child.stdout.take().unwrap(),
        overflow.clone(),
    )));
    let mut stderr = Task(tokio::spawn(drain(
        child.stderr.take().unwrap(),
        overflow.clone(),
    )));
    let mut stdin = child.stdin.take().unwrap();
    let bytes = input.as_bytes().to_vec();
    let mut writer = Task(tokio::spawn(async move {
        stdin.write_all(&bytes).await?;
        stdin.shutdown().await
    }));
    let start = tokio::time::Instant::now();
    let result = loop {
        tokio::select! {status=child.wait()=>break status.map_err(SkillsError::from),_=tokio::time::sleep(Duration::from_millis(40))=>{
            if let Err(error)=op.check_cancelled(){break Err(error);}
            if overflow.load(Ordering::Relaxed){break Err(SkillsError::invalid("Translation CLI output exceeds size limit"));}
            if start.elapsed()>timeout{break Err(SkillsError::new(ErrorCode::Timeout,"Local translation timed out"));}
        }}
    };
    let cleanup = running.cleanup();
    if result.is_err() {
        let _ = child.start_kill();
        let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
    }
    cleanup?;
    op.record.executions.last_mut().unwrap().exit_code =
        result.as_ref().ok().and_then(|s| s.code());
    let status = result?;
    if !status.success() {
        return Err(SkillsError::new(
            ErrorCode::SourceUnavailable,
            "Local translation failed; check CLI authentication and supported flags",
        ));
    }
    tokio::time::timeout(Duration::from_secs(2), &mut writer.0)
        .await
        .map_err(|_| SkillsError::new(ErrorCode::Timeout, "Translation input did not close"))?
        .map_err(|_| SkillsError::invalid("Translation input worker failed"))?
        .map_err(|_| SkillsError::invalid("Translation CLI did not consume its input"))?;
    let out = tokio::time::timeout(Duration::from_secs(2), &mut stdout.0)
        .await
        .map_err(|_| SkillsError::new(ErrorCode::Timeout, "Translation output did not close"))?
        .map_err(|_| SkillsError::invalid("Translation output worker failed"))??;
    tokio::time::timeout(Duration::from_secs(2), &mut stderr.0)
        .await
        .map_err(|_| SkillsError::new(ErrorCode::Timeout, "Translation diagnostics did not close"))?
        .map_err(|_| SkillsError::invalid("Translation diagnostic worker failed"))??;
    if overflow.load(Ordering::Relaxed) {
        return Err(SkillsError::invalid(
            "Translation CLI output exceeds size limit",
        ));
    }
    Ok(out)
}
fn parse_claude(bytes: &[u8]) -> Result<String> {
    let value: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|_| SkillsError::invalid("Claude translation returned invalid JSON"))?;
    if value["type"] != "result" || value["subtype"] != "success" || value["is_error"] != false {
        return Err(SkillsError::invalid(
            "Claude translation did not complete successfully",
        ));
    }
    validated_markdown(
        value["result"]
            .as_str()
            .ok_or_else(|| SkillsError::invalid("Claude translation result is missing"))?
            .into(),
    )
}
#[cfg(test)]
#[path = "local_tests.rs"]
mod tests;

#[cfg(test)]
pub(super) async fn fixture_translate(
    op: &mut Operation,
    provider: &ResolvedProvider,
    context: &Context,
    text: &str,
    program: &str,
) -> Result<String> {
    translate_program(op, provider, context, text, program).await
}
