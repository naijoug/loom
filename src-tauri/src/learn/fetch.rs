//! Public GitHub HTTPS fetch into a Loom-managed checkout pinned to one commit.
use crate::{
    git_fetch,
    process_supervisor::{self, ProcessKind, ProcessMetadata, ProcessOwner},
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::Path,
    process::Stdio,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use tokio::{io::AsyncReadExt, process::Command};

pub const MAX_TRACKED_FILES: usize = 20_000;
pub const MAX_CHECKOUT_BYTES: u64 = 500 * 1024 * 1024;
const GIT_TIMEOUT: Duration = Duration::from_secs(600);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubRepo {
    pub owner: String,
    pub repo: String,
    pub url: String,
    pub clone_url: String,
}

/// Accepts only `https://github.com/<owner>/<repo>` (optionally `.git`, a
/// trailing slash, or a `/tree/...` / `/blob/...` page path).
pub fn parse_github_url(input: &str) -> Result<GithubRepo, String> {
    let input = input.trim();
    if !input.starts_with("https://") {
        return Err("Use a public https://github.com/<owner>/<repo> URL".into());
    }
    let source = crate::skills::sources::normalize_git_url(input).map_err(|e| e.message)?;
    if source.provider != "github" {
        return Err("Only github.com repositories are supported".into());
    }
    let (owner, repo) = source
        .namespace
        .split_once('/')
        .ok_or("Repository URL needs an owner and repository")?;
    Ok(GithubRepo {
        owner: owner.into(),
        repo: repo.into(),
        url: source.url,
        clone_url: source.clone_url,
    })
}

/// Branch, tag or full commit. Rejects option-looking and traversal values.
pub fn validate_ref(git_ref: &str) -> Result<(), String> {
    let valid = !git_ref.is_empty()
        && git_ref.len() <= 200
        && !git_ref.starts_with('-')
        && !git_ref.starts_with('/')
        && !git_ref.ends_with('/')
        && !git_ref.ends_with(".lock")
        && !git_ref.contains("..")
        && !git_ref.contains("//")
        && !git_ref.contains("@{")
        && git_ref
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'/'));
    if valid {
        Ok(())
    } else {
        Err("Invalid branch, tag or commit".into())
    }
}

pub struct GitContext<'a> {
    pub study_id: &'a str,
    pub job_id: &'a str,
    pub cancel: &'a AtomicBool,
}

async fn drain(mut reader: impl tokio::io::AsyncRead + Unpin, cap: usize) -> Vec<u8> {
    let mut captured = Vec::new();
    let mut buffer = [0; 8192];
    while let Ok(n) = reader.read(&mut buffer).await {
        if n == 0 {
            break;
        }
        let keep = n.min(cap.saturating_sub(captured.len()));
        captured.extend_from_slice(&buffer[..keep]);
    }
    captured
}

/// Runs one hardened git command without a shell. Returns trimmed stdout, or
/// a controlled message (never raw stderr) on failure.
pub async fn git(ctx: &GitContext<'_>, cwd: &Path, args: &[&str]) -> Result<String, String> {
    if ctx.cancel.load(Ordering::Relaxed) {
        return Err("Cancelled".into());
    }
    let mut operation = process_supervisor::supervisor().begin_operation()?;
    let mut command = Command::new("git");
    command
        .args(git_fetch::HARDENED_CONFIG)
        // Public repositories only: never consult stored credentials.
        .args(["-c", "credential.helper="])
        .args(args)
        .current_dir(cwd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    process_supervisor::configure_process_group(&mut command);
    let mut child = command
        .spawn()
        .map_err(|e| format!("failed to start git: {e}"))?;
    let pid = child.id().ok_or("git process has no id")?;
    let run_id = super::store::next_id("learn-git");
    let mut meta = ProcessMetadata::unscoped(
        &run_id,
        ProcessKind::Learn,
        pid,
        Some(GIT_TIMEOUT.as_millis() as u64),
    );
    meta.owner = ProcessOwner::Learn {
        study_id: ctx.study_id.into(),
        job_id: ctx.job_id.into(),
    };
    if let Err(error) = operation.register(meta) {
        let _ = process_supervisor::force_terminate_process_group(pid);
        let _ = child.kill().await;
        return Err(error);
    }
    let out = tokio::spawn(drain(child.stdout.take().unwrap(), 16 * 1024 * 1024));
    let err = tokio::spawn(drain(child.stderr.take().unwrap(), 64 * 1024));
    let began = tokio::time::Instant::now();
    let status = loop {
        tokio::select! {
            result = child.wait() => break result.map_err(|e| e.to_string()),
            _ = tokio::time::sleep(Duration::from_millis(50)) => {
                if ctx.cancel.load(Ordering::Relaxed) { break Err("Cancelled".into()); }
                if process_supervisor::supervisor().is_shutting_down() { break Err("Loom is shutting down".into()); }
                if began.elapsed() > GIT_TIMEOUT { break Err("Git operation timed out".into()); }
            }
        }
    };
    let _ = process_supervisor::force_terminate_process_group(pid);
    if status.is_err() {
        let _ = child.start_kill();
        let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
    }
    process_supervisor::supervisor().complete(&run_id);
    let stdout = tokio::time::timeout(Duration::from_secs(2), out)
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or_default();
    let stderr = tokio::time::timeout(Duration::from_secs(2), err)
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or_default();
    let status = status?;
    if !status.success() {
        return Err(git_fetch::classify_failure(&stderr).into());
    }
    Ok(String::from_utf8_lossy(&stdout).trim().into())
}

/// Clones `repo` at `git_ref` (default branch when `None`) into `checkout`,
/// enforcing size limits before any file is written. Returns the commit.
pub async fn fetch(
    ctx: &GitContext<'_>,
    repo: &GithubRepo,
    git_ref: Option<&str>,
    checkout: &Path,
) -> Result<String, String> {
    if checkout.exists() {
        fs::remove_dir_all(checkout).map_err(|e| format!("failed to reset checkout: {e}"))?;
    }
    fs::create_dir_all(checkout).map_err(|e| format!("failed to create checkout: {e}"))?;
    let parent = checkout.parent().ok_or("checkout has no parent")?;
    match git_ref {
        Some(commit) if git_fetch::valid_revision(commit) => {
            git(ctx, checkout, &["init", "--quiet"]).await?;
            git(
                ctx,
                checkout,
                &["remote", "add", "origin", "--", &repo.clone_url],
            )
            .await?;
            git(
                ctx,
                checkout,
                &["fetch", "--depth", "1", "--no-tags", "origin", commit],
            )
            .await?;
            git(
                ctx,
                checkout,
                &["update-ref", "--no-deref", "HEAD", "FETCH_HEAD"],
            )
            .await?;
        }
        Some(name) => {
            validate_ref(name)?;
            let target = checkout.to_string_lossy().into_owned();
            git(
                ctx,
                parent,
                &[
                    "clone",
                    "--depth",
                    "1",
                    "--no-checkout",
                    "--no-tags",
                    "--single-branch",
                    "--branch",
                    name,
                    "--",
                    &repo.clone_url,
                    &target,
                ],
            )
            .await?;
        }
        None => {
            let target = checkout.to_string_lossy().into_owned();
            git(
                ctx,
                parent,
                &[
                    "clone",
                    "--depth",
                    "1",
                    "--no-checkout",
                    "--no-tags",
                    "--single-branch",
                    "--",
                    &repo.clone_url,
                    &target,
                ],
            )
            .await?;
        }
    }
    let commit = git(ctx, checkout, &["rev-parse", "HEAD"]).await?;
    if !git_fetch::valid_revision(&commit) {
        return Err("Invalid Git revision".into());
    }
    let tree = git(ctx, checkout, &["ls-tree", "-r", "-l", "-z", "HEAD"]).await?;
    check_limits(&tree)?;
    fs::create_dir_all(checkout.join(".git/info")).map_err(|e| e.to_string())?;
    fs::write(
        checkout.join(".git/info/attributes"),
        git_fetch::NO_FILTER_ATTRIBUTES,
    )
    .map_err(|e| e.to_string())?;
    git(ctx, checkout, &["checkout", "--quiet", "--detach", "HEAD"]).await?;
    Ok(commit)
}

/// Parses `git ls-tree -r -l -z` output: `<mode> <type> <oid> <size>\t<path>`.
fn check_limits(tree: &str) -> Result<(), String> {
    let mut files = 0usize;
    let mut bytes = 0u64;
    for entry in tree.split('\0').filter(|e| !e.is_empty()) {
        let Some((meta, _path)) = entry.split_once('\t') else {
            continue;
        };
        files += 1;
        bytes += meta
            .split_whitespace()
            .nth(3)
            .and_then(|size| size.parse::<u64>().ok())
            .unwrap_or(0);
        if files > MAX_TRACKED_FILES {
            return Err(format!(
                "Repository has more than {MAX_TRACKED_FILES} files; Learn supports smaller repositories"
            ));
        }
        if bytes > MAX_CHECKOUT_BYTES {
            return Err("Repository checkout would exceed 500 MiB".into());
        }
    }
    Ok(())
}

/// Stable digest of every path, type, size and modification time under the
/// checkout (excluding `.git`). Agent runs must leave it unchanged.
pub fn fingerprint(checkout: &Path) -> Result<String, String> {
    let mut entries = Vec::new();
    collect(checkout, checkout, &mut entries)?;
    entries.sort();
    let mut hasher = Sha256::new();
    for entry in entries {
        hasher.update(entry.as_bytes());
        hasher.update([0]);
    }
    Ok(format!("{:x}", hasher.finalize())[..32].into())
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|e| format!("fingerprint failed: {e}"))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .into_owned();
        if relative == ".git" {
            continue;
        }
        let meta = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let kind = if meta.file_type().is_symlink() {
            "l"
        } else if meta.is_dir() {
            "d"
        } else {
            "f"
        };
        out.push(format!("{kind}:{relative}:{}:{modified}", meta.len()));
        if meta.is_dir() {
            collect(root, &path, out)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_public_github_https_urls_are_accepted() {
        let repo = parse_github_url("https://github.com/sharkdp/hexyl.git/").unwrap();
        assert_eq!(
            (repo.owner.as_str(), repo.repo.as_str()),
            ("sharkdp", "hexyl")
        );
        assert_eq!(repo.clone_url, "https://github.com/sharkdp/hexyl.git");
        assert_eq!(
            parse_github_url("https://github.com/a/b/tree/main/src")
                .unwrap()
                .repo,
            "b"
        );
        for bad in [
            "git@github.com:a/b.git",
            "http://github.com/a/b",
            "https://gitlab.com/a/b",
            "https://token@github.com/a/b",
            "https://github.com/a",
            "https://example.com/a/b",
            "file:///tmp/x",
        ] {
            assert!(parse_github_url(bad).is_err(), "accepted {bad}");
        }
    }

    #[test]
    fn refs_reject_options_and_traversal() {
        for ok in ["main", "v1.2.3", "release/2026", &"a".repeat(40)] {
            validate_ref(ok).unwrap();
        }
        for bad in [
            "",
            "-x",
            "--upload-pack=x",
            "a..b",
            "/a",
            "a/",
            "a b",
            "a@{1}",
            "x.lock",
        ] {
            assert!(validate_ref(bad).is_err(), "accepted {bad}");
        }
    }

    #[test]
    fn tree_limits_count_files_and_bytes() {
        let entry = |size: u64| format!("100644 blob {} {size}\tf\0", "a".repeat(40));
        check_limits(&entry(10).repeat(10)).unwrap();
        assert!(check_limits(&entry(1).repeat(MAX_TRACKED_FILES + 1)).is_err());
        assert!(check_limits(&entry(MAX_CHECKOUT_BYTES + 1)).is_err());
    }

    #[test]
    fn fingerprint_detects_writes_and_ignores_git_metadata() {
        let root = super::super::store::tests::temp_root();
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join("a.txt"), "a").unwrap();
        let before = fingerprint(&root).unwrap();
        fs::write(root.join(".git/index"), "x").unwrap();
        assert_eq!(fingerprint(&root).unwrap(), before);
        fs::write(root.join("b.txt"), "b").unwrap();
        assert_ne!(fingerprint(&root).unwrap(), before);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn local_fixture_repository_is_fetched_and_pinned() {
        // file:// clones exercise the same commands without network access.
        let root = super::super::store::tests::temp_root();
        let source = root.join("source");
        fs::create_dir_all(&source).unwrap();
        let run = |args: &[&str]| {
            let status = std::process::Command::new("git")
                .args([
                    "-c",
                    "user.name=F",
                    "-c",
                    "user.email=f@example.invalid",
                    "-c",
                    "commit.gpgsign=false",
                ])
                .args(args)
                .current_dir(&source)
                .output()
                .unwrap();
            assert!(
                status.status.success(),
                "{}",
                String::from_utf8_lossy(&status.stderr)
            );
            String::from_utf8_lossy(&status.stdout).trim().to_string()
        };
        run(&["init", "--quiet", "-b", "main"]);
        fs::write(source.join("main.rs"), "fn main() {}\n").unwrap();
        fs::write(source.join(".gitattributes"), "*.rs filter=evil\n").unwrap();
        run(&["add", "."]);
        run(&["commit", "--quiet", "-m", "init"]);
        let head = run(&["rev-parse", "HEAD"]);
        let repo = GithubRepo {
            owner: "o".into(),
            repo: "r".into(),
            url: "https://github.com/o/r".into(),
            clone_url: format!("file://{}", source.display()),
        };
        let cancel = AtomicBool::new(false);
        let ctx = GitContext {
            study_id: "study-t",
            job_id: "job-t",
            cancel: &cancel,
        };
        let checkout = root.join("study").join("checkout");
        let commit = fetch(&ctx, &repo, None, &checkout).await.unwrap();
        assert_eq!(commit, head);
        assert_eq!(
            fs::read_to_string(checkout.join("main.rs")).unwrap(),
            "fn main() {}\n"
        );
        let pinned = fetch(&ctx, &repo, Some("main"), &checkout).await.unwrap();
        assert_eq!(pinned, head);
        assert!(fetch(&ctx, &repo, Some("missing-branch"), &checkout)
            .await
            .is_err());
        cancel.store(true, Ordering::Relaxed);
        assert_eq!(
            fetch(&ctx, &repo, None, &checkout).await.unwrap_err(),
            "Cancelled"
        );
        fs::remove_dir_all(root).unwrap();
    }
}
