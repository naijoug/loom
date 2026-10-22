use super::{
    coordination::LifecycleLease,
    error::{ErrorCode, Result, SkillsError},
    files,
    repository::{Repository, Source, SourceKind},
    snapshots,
    sources::GitSource,
};
use crate::process_supervisor::{self, OperationGuard, ProcessKind, ProcessMetadata, ProcessOwner};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{io::AsyncReadExt, process::Command};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Running,
    Committed,
    Failed,
    Cancelled,
    Interrupted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationRecord {
    pub operation_id: String,
    pub source_id: String,
    pub action: String,
    pub phase: Phase,
    pub stage: String,
    pub sequence: u64,
    pub started_at_ms: u128,
    pub updated_at_ms: u128,
    pub error: Option<SkillsError>,
    #[serde(default)]
    pub executions: Vec<ExecutionRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionRecord {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub started_at_ms: u128,
    pub finished_at_ms: Option<u128>,
    pub exit_code: Option<i32>,
    pub error: Option<String>,
}

#[derive(Default)]
struct GitRunner {
    extra_args: Vec<String>,
}

struct ReaderTask(tokio::task::JoinHandle<std::io::Result<Vec<u8>>>);
impl Drop for ReaderTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

pub struct Operation {
    repository: Repository,
    pub record: OperationRecord,
    _lifecycle: LifecycleLease,
    _admission: OperationGuard<'static>,
    _owner: File,
}

fn operation_path(repository: &Repository, id: &str, suffix: &str) -> Result<PathBuf> {
    files::validate_id(id)?;
    Ok(repository
        .root
        .join("operations")
        .join(format!("{id}.{suffix}")))
}

impl Operation {
    pub fn begin(repository: &Repository, source_id: &str, action: &str) -> Result<Self> {
        let lifecycle = repository.coordinator.enter()?;
        let admission = process_supervisor::supervisor()
            .begin_operation()
            .map_err(|e| SkillsError::new(ErrorCode::Updating, e))?;
        let id = format!(
            "{}-{}",
            crate::models::IdGenerator::default().next("skills"),
            std::process::id()
        );
        files::private_directory(&repository.root.join("operations"))?;
        let mut options = OpenOptions::new();
        options.read(true).write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let owner = options.open(operation_path(repository, &id, "lock")?)?;
        owner
            .try_lock()
            .map_err(|_| SkillsError::new(ErrorCode::LockBusy, "Operation is already owned"))?;
        let now = crate::models::now_ms();
        let mut op = Self {
            repository: repository.clone(),
            record: OperationRecord {
                operation_id: id,
                source_id: source_id.into(),
                action: action.into(),
                phase: Phase::Running,
                stage: "preparing".into(),
                sequence: 0,
                started_at_ms: now,
                updated_at_ms: now,
                error: None,
                executions: Vec::new(),
            },
            _lifecycle: lifecycle,
            _admission: admission,
            _owner: owner,
        };
        op.save()?;
        Ok(op)
    }
    fn save(&mut self) -> Result<()> {
        self.record.sequence += 1;
        self.record.updated_at_ms = crate::models::now_ms();
        let writer = self.repository.coordinator.writer_wait(&self._lifecycle)?;
        let path = operation_path(&self.repository, &self.record.operation_id, "json")?;
        writer.check_write_set(std::slice::from_ref(&path))?;
        files::atomic_json(&path, &self.record)
    }
    pub fn stage(&mut self, stage: &str) -> Result<()> {
        self.record.stage = stage.into();
        self.save()
    }
    pub fn check_cancelled(&self) -> Result<()> {
        if operation_path(&self.repository, &self.record.operation_id, "cancel")?.try_exists()?
            || process_supervisor::supervisor().is_shutting_down()
        {
            return Err(SkillsError::new(
                ErrorCode::Cancelled,
                "Skills operation cancelled",
            ));
        }
        Ok(())
    }
    pub fn finish(&mut self, result: Result<()>) -> Result<()> {
        self.record.phase = match &result {
            Ok(()) => Phase::Committed,
            Err(e) if e.code == ErrorCode::Cancelled => Phase::Cancelled,
            Err(_) => Phase::Failed,
        };
        self.record.stage = if result.is_ok() {
            "complete"
        } else {
            "stopped"
        }
        .into();
        self.record.error = result.err();
        self.save()
    }
}
impl Drop for Operation {
    fn drop(&mut self) {
        let _ = self._owner.unlock();
    }
}

pub fn list(repository: &Repository) -> Result<Vec<OperationRecord>> {
    let directory = repository.root.join("operations");
    if !directory.try_exists()? {
        return Ok(vec![]);
    }
    let mut records = Vec::new();
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.extension().is_none_or(|v| v != "json") {
            continue;
        }
        if records.len() >= 1000 {
            return Err(SkillsError::invalid(
                "Operation history exceeds the current limit",
            ));
        }
        let mut record: OperationRecord = files::read_json(&path)?;
        if operation_path(repository, &record.operation_id, "json")? != path {
            return Err(SkillsError::recovery("Operation identity mismatch"));
        }
        if record.phase == Phase::Running {
            let lock_path = operation_path(repository, &record.operation_id, "lock")?;
            files::regular_file(&lock_path)?;
            let owner = OpenOptions::new().read(true).write(true).open(lock_path)?;
            if owner.try_lock().is_ok() {
                record.phase = Phase::Interrupted;
                record.sequence += 1;
                record.updated_at_ms = crate::models::now_ms();
                record.error = Some(SkillsError::recovery(
                    "Operation interrupted; inspect source state before retrying",
                ));
                let lease = repository.coordinator.enter()?;
                let writer = repository.coordinator.writer_wait(&lease)?;
                writer.check_write_set(std::slice::from_ref(&path))?;
                files::atomic_json(&path, &record)?;
                let _ = owner.unlock();
            }
        }
        records.push(record);
    }
    records.sort_by_key(|record| std::cmp::Reverse(record.started_at_ms));
    Ok(records)
}

pub fn cancel(repository: &Repository, id: &str) -> Result<()> {
    let path = operation_path(repository, id, "json")?;
    let record: OperationRecord = files::read_json(&path)?;
    if record.phase != Phase::Running {
        return Ok(());
    }
    let _lease = repository.coordinator.enter()?;
    let cancel_path = operation_path(repository, id, "cancel")?;
    // This idempotent control signal does not modify a target, journal, or
    // receipt. It must remain writable while an installer owns the writer lock.
    files::atomic_json(&cancel_path, &true)
}

async fn drain(mut reader: impl tokio::io::AsyncRead + Unpin) -> std::io::Result<Vec<u8>> {
    let mut captured = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        let n = reader.read(&mut buffer).await?;
        if n == 0 {
            break;
        }
        let retain = n.min((128 * 1024usize).saturating_sub(captured.len()));
        captured.extend_from_slice(&buffer[..retain]);
    }
    Ok(captured)
}

async fn git(op: &mut Operation, cwd: &Path, args: &[&str], runner: &GitRunner) -> Result<String> {
    op.record.executions.push(ExecutionRecord {
        program: "git".into(),
        args: args
            .iter()
            .map(|arg| crate::agents::redact_sensitive_text(arg))
            .collect(),
        cwd: cwd.into(),
        started_at_ms: crate::models::now_ms(),
        finished_at_ms: None,
        exit_code: None,
        error: None,
    });
    op.save()?;
    let result = git_raw(op, cwd, args, runner).await;
    let execution = op.record.executions.last_mut().unwrap();
    execution.finished_at_ms = Some(crate::models::now_ms());
    execution.error = result
        .as_ref()
        .err()
        .map(|error| crate::agents::redact_sensitive_text(&error.message));
    op.save()?;
    result
}

async fn git_raw(
    op: &mut Operation,
    cwd: &Path,
    args: &[&str],
    runner: &GitRunner,
) -> Result<String> {
    op.check_cancelled()?;
    let run_id = crate::models::IdGenerator::default().next("skills-git");
    let mut guard = process_supervisor::supervisor()
        .begin_operation()
        .map_err(|e| SkillsError::new(ErrorCode::Updating, e))?;
    let mut command = Command::new("git");
    command
        .args(&runner.extra_args)
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "protocol.ext.allow=never",
            "-c",
            "submodule.recurse=false",
            "-c",
            "filter.lfs.required=false",
            "-c",
            "filter.lfs.smudge=",
            "-c",
            "filter.lfs.process=",
        ])
        .args(args)
        .current_dir(cwd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_SSH_COMMAND", "ssh -oBatchMode=yes")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    process_supervisor::configure_process_group(&mut command);
    let mut child = command.spawn()?;
    let pid = child
        .id()
        .ok_or_else(|| SkillsError::invalid("Git process has no ID"))?;
    let mut meta = ProcessMetadata::unscoped(&run_id, ProcessKind::Skills, pid, Some(120_000));
    meta.owner = ProcessOwner::Skills {
        operation_id: op.record.operation_id.clone(),
    };
    if let Err(error) = guard.register(meta) {
        let _ = process_supervisor::force_terminate_process_group(pid);
        let _ = child.kill().await;
        return Err(SkillsError::invalid(error));
    }
    let mut out = ReaderTask(tokio::spawn(drain(child.stdout.take().unwrap())));
    let mut err = ReaderTask(tokio::spawn(drain(child.stderr.take().unwrap())));
    let began = tokio::time::Instant::now();
    let result = loop {
        tokio::select! {
            result=child.wait()=>break result.map_err(SkillsError::from),
            _=tokio::time::sleep(Duration::from_millis(40))=>{
                if let Err(error)=op.check_cancelled(){break Err(error);}
                if began.elapsed()>Duration::from_secs(120){break Err(SkillsError::new(ErrorCode::Timeout,"Git operation timed out"));}
            }
        }
    };
    let cleanup = process_supervisor::force_terminate_process_group(pid);
    if result.is_err() {
        let _ = child.start_kill();
        let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
    }
    process_supervisor::supervisor().complete(&run_id);
    cleanup.map_err(SkillsError::invalid)?;
    op.record.executions.last_mut().unwrap().exit_code =
        result.as_ref().ok().and_then(|s| s.code());
    let stdout = tokio::time::timeout(Duration::from_secs(2), &mut out.0)
        .await
        .map_err(|_| SkillsError::new(ErrorCode::Timeout, "Git output did not close"))?
        .map_err(|_| SkillsError::invalid("Git output task failed"))??;
    let stderr = tokio::time::timeout(Duration::from_secs(2), &mut err.0)
        .await
        .map_err(|_| SkillsError::new(ErrorCode::Timeout, "Git error output did not close"))?
        .map_err(|_| SkillsError::invalid("Git output task failed"))??;
    let status = result?;
    if !status.success() {
        return Err(SkillsError::new(
            ErrorCode::SourceUnavailable,
            format!(
                "Git failed ({}): {}",
                status,
                crate::agents::redact_sensitive_text(&String::from_utf8_lossy(&stderr))
            ),
        ));
    }
    Ok(String::from_utf8_lossy(&stdout).trim().into())
}

pub async fn import_git(
    repository: Repository,
    op: Operation,
    source: GitSource,
    expected_revision: Option<String>,
    emit: impl Fn(&OperationRecord),
) {
    import_using(
        repository,
        op,
        source,
        expected_revision,
        emit,
        GitRunner::default(),
    )
    .await;
}

async fn import_using(
    repository: Repository,
    mut op: Operation,
    source: GitSource,
    expected_revision: Option<String>,
    emit: impl Fn(&OperationRecord),
    runner: GitRunner,
) {
    let result = async {
        let directory = repository.prepare_directory(&op.record.operation_id)?;
        let result = async {
            op.stage("cloning")?;
            emit(&op.record);
            git(
                &mut op,
                &directory,
                &[
                    "clone",
                    "--depth",
                    "1",
                    "--no-checkout",
                    "--",
                    &source.clone_url,
                    "repository",
                ],
                &runner,
            )
            .await?;
            let checkout = directory.join("repository");
            let revision = git(&mut op, &checkout, &["rev-parse", "HEAD"], &runner).await?;
            if !matches!(revision.len(), 40 | 64)
                || !revision.bytes().all(|b| b.is_ascii_hexdigit())
            {
                return Err(SkillsError::invalid("Invalid Git revision"));
            }
            // info/attributes has precedence over repository/global attributes;
            // source-selected smudge filters must never execute during import.
            fs::create_dir_all(checkout.join(".git/info"))?;
            fs::write(
                checkout.join(".git/info/attributes"),
                "* -filter -working-tree-encoding -text\n",
            )?;
            git(
                &mut op,
                &checkout,
                &["checkout", "--detach", "HEAD"],
                &runner,
            )
            .await?;
            op.check_cancelled()?;
            op.stage("validating")?;
            emit(&op.record);
            let staging = directory.join("snapshot");
            let snapshot = snapshots::prepare(&checkout, &staging, &source.source_id, &revision)?;
            op.check_cancelled()?;
            op.stage("publishing")?;
            emit(&op.record);
            let registered = Source {
                id: source.source_id.clone(),
                name: source.namespace.clone(),
                kind: SourceKind::Git,
                local_path: None,
                git: Some(source.clone()),
                revision: Some(revision),
                updated_at_ms: crate::models::now_ms(),
            };
            repository.commit_git(
                registered,
                expected_revision.as_deref(),
                &staging,
                &snapshot,
            )
        }
        .await;
        let _ = fs::remove_dir_all(directory);
        result
    }
    .await;
    if let Err(error) = op.finish(result) {
        op.record.phase = Phase::Failed;
        op.record.error = Some(error);
    }
    emit(&op.record);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::{sources::normalize_git_url, tests::Fixture};

    fn local_git(path: &Path, args: &[&str]) -> String {
        let output = std::process::Command::new("git")
            .args([
                "-c",
                "core.hooksPath=/dev/null",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
            ])
            .args(args)
            .current_dir(path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().into()
    }

    #[tokio::test]
    async fn real_git_import_refresh_and_source_removal_preserve_previous_snapshot() {
        let f = Fixture::new();
        let remote = f.0.join("remote");
        fs::create_dir(&remote).unwrap();
        local_git(&remote, &["init", "-q"]);
        fs::write(
            remote.join("SKILL.md"),
            "---\nname: demo\n---\n# Version A\n",
        )
        .unwrap();
        local_git(&remote, &["add", "."]);
        local_git(&remote, &["commit", "-qm", "version a"]);
        let a = local_git(&remote, &["rev-parse", "HEAD"]);
        let source = normalize_git_url("https://gitlab.com/fixture/nested/skills").unwrap();
        let runner = || GitRunner {
            extra_args: vec![
                "-c".into(),
                format!("url.{}.insteadOf={}", remote.display(), source.clone_url),
            ],
        };
        let repo = Repository::open(&f.0.join("data"), &f.0.join("coord")).unwrap();
        let op = Operation::begin(&repo, &source.source_id, "import").unwrap();
        import_using(repo.clone(), op, source.clone(), None, |_| {}, runner()).await;
        let record = list(&repo).unwrap().remove(0);
        assert_eq!(record.phase, Phase::Committed, "{:?}", record.error);
        assert_eq!(record.executions.len(), 3);
        assert!(record
            .executions
            .iter()
            .all(|e| e.exit_code == Some(0) && e.finished_at_ms.is_some()));
        let initial = repo.get_source(&source.source_id).unwrap();
        assert_eq!(initial.revision.as_deref(), Some(a.as_str()));
        let old_path = repo.source_directory(&initial).unwrap();
        fs::write(
            remote.join("SKILL.md"),
            "---\nname: demo\n---\n# Version B\n",
        )
        .unwrap();
        local_git(&remote, &["add", "."]);
        local_git(&remote, &["commit", "-qm", "version b"]);
        let op = Operation::begin(&repo, &source.source_id, "refresh").unwrap();
        import_using(repo.clone(), op, source.clone(), Some(a), |_| {}, runner()).await;
        assert_eq!(list(&repo).unwrap()[0].phase, Phase::Committed);
        assert_eq!(repo.list().unwrap().skills[0].title, "Version B");
        assert!(fs::read_to_string(old_path.join("SKILL.md"))
            .unwrap()
            .contains("Version A"));
        repo.remove_source(&source.source_id).unwrap();
        assert!(old_path.join("SKILL.md").exists());
        assert!(repo.list().unwrap().sources.is_empty());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn git_refresh_does_not_update_installed_copy_or_symlink_until_reviewed() {
        use crate::skills::{
            installers::{self, ConflictPolicy, InstallMode, InstallRequest, UninstallRequest},
            targets::TargetContext,
        };
        let f = Fixture::new();
        let remote = f.0.join("remote");
        fs::create_dir_all(remote.join("demo/references")).unwrap();
        fs::create_dir(f.0.join("home")).unwrap();
        fs::create_dir(f.0.join("project")).unwrap();
        local_git(&remote, &["init", "-q"]);
        fs::write(
            remote.join("demo/SKILL.md"),
            "---\nname: pinned-demo\n---\n# Demo\n",
        )
        .unwrap();
        fs::write(remote.join("demo/references/version.txt"), "A").unwrap();
        local_git(&remote, &["add", "."]);
        local_git(&remote, &["commit", "-qm", "revision A"]);
        let a = local_git(&remote, &["rev-parse", "HEAD"]);
        let source = normalize_git_url("https://github.com/fixture/pinned-skills").unwrap();
        let runner = || GitRunner {
            extra_args: vec![
                "-c".into(),
                format!("url.{}.insteadOf={}", remote.display(), source.clone_url),
            ],
        };
        let repo = Repository::open(&f.0.join("data"), &f.0.join("coord")).unwrap();
        let context = TargetContext::new(&f.0.join("home"), Some(&f.0.join("project"))).unwrap();
        let op = Operation::begin(&repo, &source.source_id, "import").unwrap();
        import_using(repo.clone(), op, source.clone(), None, |_| {}, runner()).await;
        assert_eq!(list(&repo).unwrap()[0].phase, Phase::Committed);
        let skill_id = repo.list().unwrap().skills[0].id.clone();
        let apply = |plan_id: &str| {
            let mut operation = Operation::begin(&repo, &source.source_id, "install").unwrap();
            let receipt = installers::apply(&repo, &context, plan_id, &mut operation).unwrap();
            operation.finish(Ok(())).unwrap();
            receipt
        };
        for (mode, target) in [
            (InstallMode::Copy, "codex-project"),
            (InstallMode::Symlink, "amp-project"),
        ] {
            let plan = installers::plan_install(
                &repo,
                &context,
                &InstallRequest {
                    with_slash_commands: false,
                    skill_id: skill_id.clone(),
                    mode,
                    conflict_policy: ConflictPolicy::Fail,
                    target_ids: vec![target.into()],
                    project_path: None,
                },
            )
            .unwrap();
            assert_eq!(apply(&plan.plan_id).items[0].status, "installed");
        }
        let installed = installers::statuses(&repo, &context, &skill_id).unwrap();
        let installed: Vec<_> = installed
            .iter()
            .filter(|s| s.installation.is_some())
            .collect();
        assert_eq!(installed.len(), 2);
        let link = installed
            .iter()
            .find(|s| s.target.id == "amp-project")
            .unwrap();
        let a_link = fs::read_link(&link.destination).unwrap();
        for status in &installed {
            assert_eq!(
                status.installation.as_ref().unwrap().revision.as_deref(),
                Some(a.as_str())
            );
        }
        // A new source commit changes only the library, including package reference files.
        fs::write(remote.join("demo/references/version.txt"), "B").unwrap();
        local_git(&remote, &["add", "."]);
        local_git(&remote, &["commit", "-qm", "revision B"]);
        let b = local_git(&remote, &["rev-parse", "HEAD"]);
        let op = Operation::begin(&repo, &source.source_id, "refresh").unwrap();
        import_using(
            repo.clone(),
            op,
            source.clone(),
            Some(a.clone()),
            |_| {},
            runner(),
        )
        .await;
        assert_eq!(list(&repo).unwrap()[0].phase, Phase::Committed);
        for status in installers::statuses(&repo, &context, &skill_id)
            .unwrap()
            .iter()
            .filter(|s| s.installation.is_some())
        {
            assert!(status.update_available);
            assert_eq!(
                fs::read_to_string(status.destination.join("references/version.txt")).unwrap(),
                "A"
            );
            assert_eq!(
                status.installation.as_ref().unwrap().revision.as_deref(),
                Some(a.as_str())
            );
        }
        assert_eq!(fs::read_link(&link.destination).unwrap(), a_link);
        // Only explicitly planned installation updates switch the selected targets to B.
        for (mode, target) in [
            (InstallMode::Copy, "codex-project"),
            (InstallMode::Symlink, "amp-project"),
        ] {
            let plan = installers::plan_install(
                &repo,
                &context,
                &InstallRequest {
                    with_slash_commands: false,
                    skill_id: skill_id.clone(),
                    mode,
                    conflict_policy: ConflictPolicy::Overwrite,
                    target_ids: vec![target.into()],
                    project_path: None,
                },
            )
            .unwrap();
            assert_eq!(apply(&plan.plan_id).items[0].status, "installed");
        }
        for status in installers::statuses(&repo, &context, &skill_id)
            .unwrap()
            .iter()
            .filter(|s| s.installation.is_some())
        {
            assert!(!status.update_available);
            assert_eq!(
                fs::read_to_string(status.destination.join("references/version.txt")).unwrap(),
                "B"
            );
            assert_eq!(
                status.installation.as_ref().unwrap().revision.as_deref(),
                Some(b.as_str())
            );
        }
        assert_ne!(fs::read_link(&link.destination).unwrap(), a_link);
        assert_eq!(
            fs::read_to_string(a_link.join("references/version.txt")).unwrap(),
            "A"
        );
        // Upstream removal and library removal cannot invalidate installed snapshot links.
        fs::remove_dir_all(remote.join("demo")).unwrap();
        fs::write(remote.join("README.md"), "Skill removed upstream").unwrap();
        local_git(&remote, &["add", "-A"]);
        local_git(&remote, &["commit", "-qm", "remove skill"]);
        let op = Operation::begin(&repo, &source.source_id, "refresh").unwrap();
        import_using(
            repo.clone(),
            op,
            source.clone(),
            Some(b.clone()),
            |_| {},
            runner(),
        )
        .await;
        // An entirely empty upstream is rejected, preserving the last successful revision.
        assert_eq!(list(&repo).unwrap()[0].phase, Phase::Failed);
        assert_eq!(
            repo.get_source(&source.source_id)
                .unwrap()
                .revision
                .as_deref(),
            Some(b.as_str())
        );
        assert_eq!(
            fs::read_to_string(link.destination.join("references/version.txt")).unwrap(),
            "B"
        );
        fs::create_dir(remote.join("other")).unwrap();
        fs::write(
            remote.join("other/SKILL.md"),
            "---\nname: other\n---\n# Other skill\n",
        )
        .unwrap();
        local_git(&remote, &["add", "."]);
        local_git(&remote, &["commit", "-qm", "retain a different skill"]);
        let op = Operation::begin(&repo, &source.source_id, "refresh").unwrap();
        import_using(repo.clone(), op, source.clone(), Some(b), |_| {}, runner()).await;
        let record = list(&repo).unwrap().remove(0);
        assert_eq!(record.phase, Phase::Committed, "{:?}", record.error);
        assert!(repo
            .list()
            .unwrap()
            .skills
            .iter()
            .all(|skill| skill.id != skill_id));
        repo.remove_source(&source.source_id).unwrap();
        let inventory = installers::inventory(&repo, &context).unwrap();
        let mut ids = Vec::new();
        for status in inventory.iter().filter(|s| s.installation.is_some()) {
            assert_eq!(
                fs::read_to_string(status.destination.join("references/version.txt")).unwrap(),
                "B"
            );
            ids.push(
                status
                    .installation
                    .as_ref()
                    .unwrap()
                    .installation_id
                    .clone(),
            );
        }
        assert_eq!(ids.len(), 2);
        let removal = installers::plan_uninstall(
            &repo,
            &context,
            &UninstallRequest {
                installation_ids: ids,
                project_path: None,
            },
        )
        .unwrap();
        assert!(apply(&removal.plan_id)
            .items
            .iter()
            .all(|i| i.status == "uninstalled"));
        for status in installed {
            assert!(fs::symlink_metadata(&status.destination).is_err());
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn checkout_does_not_run_source_selected_smudge_filters() {
        use std::os::unix::fs::PermissionsExt;
        let f = Fixture::new();
        let remote = f.0.join("remote");
        fs::create_dir(&remote).unwrap();
        local_git(&remote, &["init", "-q"]);
        fs::write(remote.join("SKILL.md"), "# Safe content\n").unwrap();
        fs::write(remote.join(".gitattributes"), "* filter=untrusted\n").unwrap();
        local_git(&remote, &["add", "."]);
        local_git(&remote, &["commit", "-qm", "source with filter"]);
        let script = f.0.join("filter.sh");
        fs::write(
            &script,
            "#!/bin/sh\ntouch \"$(dirname \"$0\")/FILTER_EXECUTED\"\ncat\n",
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
        let source = normalize_git_url("https://github.com/fixture/skills").unwrap();
        let repo = Repository::open(&f.0.join("data"), &f.0.join("coord")).unwrap();
        let op = Operation::begin(&repo, &source.source_id, "import").unwrap();
        let runner = GitRunner {
            extra_args: vec![
                "-c".into(),
                format!("url.{}.insteadOf={}", remote.display(), source.clone_url),
                "-c".into(),
                format!("filter.untrusted.smudge={}", script.display()),
                "-c".into(),
                "filter.untrusted.required=true".into(),
            ],
        };
        import_using(repo.clone(), op, source, None, |_| {}, runner).await;
        let record = list(&repo).unwrap().remove(0);
        assert_eq!(record.phase, Phase::Committed, "{:?}", record.error);
        assert!(!f.0.join("FILTER_EXECUTED").exists());
    }

    #[tokio::test]
    async fn cancellation_before_clone_is_persistent_and_never_creates_a_source() {
        let f = Fixture::new();
        let repo = Repository::open(&f.0.join("data"), &f.0.join("coord")).unwrap();
        let source = normalize_git_url("https://github.com/fixture/skills").unwrap();
        let op = Operation::begin(&repo, &source.source_id, "import").unwrap();
        cancel(&repo, &op.record.operation_id).unwrap();
        import_git(repo.clone(), op, source, None, |_| {}).await;
        assert_eq!(list(&repo).unwrap()[0].phase, Phase::Cancelled);
        assert!(repo.list().unwrap().sources.is_empty());
    }

    #[test]
    fn abandoned_operation_is_marked_interrupted_and_not_reexecuted() {
        let f = Fixture::new();
        let repo = Repository::open(&f.0.join("data"), &f.0.join("coord")).unwrap();
        let op = Operation::begin(&repo, "src-fixture", "import").unwrap();
        assert_eq!(list(&repo).unwrap()[0].phase, Phase::Running);
        let sequence = op.record.sequence;
        drop(op);
        assert_eq!(list(&repo).unwrap()[0].phase, Phase::Interrupted);
        assert!(list(&repo).unwrap()[0].sequence > sequence);
    }
}
