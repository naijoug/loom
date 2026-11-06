//! Fixed-commit public GitHub checkouts shared by Learn and Research.
//!
//! Callers pass the process owner. This module does not know about studies or
//! research runs, and it never executes repository code.
use crate::{
    git_fetch,
    tool_process::{self, BoundedCommand, ProcessKind, ProcessOwner},
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

pub const MAX_TRACKED_FILES: usize = 20_000;
pub const MAX_CHECKOUT_BYTES: u64 = 500 * 1024 * 1024;
const GIT_TIMEOUT: Duration = Duration::from_secs(600);
const SOURCE_FILE_BYTES: u64 = 1024 * 1024;
const SKIPPED_DIRS: &[&str] = &[
    "node_modules",
    "vendor",
    "third_party",
    "dist",
    "build",
    "target",
    "out",
    ".next",
    "__pycache__",
    ".venv",
    "venv",
    "Pods",
];

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
    if input.len() > 2048
        || input
            .bytes()
            .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control())
    {
        return Err("Invalid repository URL".into());
    }
    let rest = input.strip_prefix("https://").unwrap_or(input);
    let (host, raw_path) = rest
        .split_once('/')
        .ok_or("Repository URL needs a namespace")?;
    let host = host.to_ascii_lowercase();
    if host != "github.com" {
        return Err(if host == "gitlab.com" {
            "Only github.com repositories are supported".into()
        } else {
            "Only github.com and gitlab.com are supported; URL credentials are not accepted".into()
        });
    }
    let path = raw_path.trim_end_matches('/').trim_end_matches(".git");
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() > 2 && !matches!(parts.get(2), Some(&"tree") | Some(&"blob")) {
        return Err("Unsupported GitHub URL path".into());
    }
    if parts.len() < 2 {
        return Err("Repository URL needs an owner and repository".into());
    }
    let repo_parts = &parts[..2];
    if repo_parts.iter().any(|part| {
        part.is_empty()
            || *part == "."
            || *part == ".."
            || part.starts_with('-')
            || !part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    }) {
        return Err("Invalid repository namespace".into());
    }
    let owner = repo_parts[0];
    let repo = repo_parts[1];
    let url = format!("https://github.com/{owner}/{repo}");
    Ok(GithubRepo {
        owner: owner.into(),
        repo: repo.into(),
        url: url.clone(),
        clone_url: format!("{url}.git"),
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
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'/'));
    if valid {
        Ok(())
    } else {
        Err("Invalid branch, tag or commit".into())
    }
}

pub fn is_full_commit(git_ref: &str) -> bool {
    git_fetch::valid_revision(git_ref)
}

/// Relative path inside a checkout. Rejects escapes, `.git`, and absolute paths.
pub fn validate_repo_path(path: &str) -> Result<(), String> {
    if path.is_empty()
        || path.len() > 1024
        || path.contains(['\\', '\0'])
        || path.starts_with('/')
        || path.starts_with('-')
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == ".." || part == ".git")
    {
        return Err("unsafe_repository_path".into());
    }
    Ok(())
}

pub struct GitRequest<'a> {
    pub owner: ProcessOwner,
    pub kind: ProcessKind,
    pub operation_prefix: &'a str,
    pub cancelled: &'a (dyn Fn() -> bool + Sync),
    pub file_limit: usize,
    pub byte_limit: u64,
    /// Optional checkout directory and byte cap, checked while Git is running.
    pub disk_root: Option<(PathBuf, u64)>,
}

fn map_git_error(error: String) -> String {
    if error == "tool_cancelled" {
        "Cancelled".into()
    } else if error == "tool_stopped" {
        "Loom is shutting down".into()
    } else if error == "tool_timeout" {
        "Git operation timed out".into()
    } else if let Some(detail) = error.strip_prefix("tool_spawn_failed: ") {
        format!("failed to start git: {detail}")
    } else if error == "tool_pid_missing" {
        "git process has no id".into()
    } else {
        error
    }
}

/// Runs one hardened git command without a shell. Returns trimmed stdout, or
/// a controlled message (never raw stderr) on failure.
pub async fn git(request: &GitRequest<'_>, cwd: &Path, args: &[&str]) -> Result<String, String> {
    let mut command = Vec::with_capacity(git_fetch::HARDENED_CONFIG.len() + 2 + args.len());
    command.extend(
        git_fetch::HARDENED_CONFIG
            .iter()
            .map(|arg| (*arg).to_string()),
    );
    command.push("-c".into());
    command.push("credential.helper=".into());
    command.extend(args.iter().map(|arg| (*arg).to_string()));
    let mut env = BTreeMap::new();
    env.insert("GIT_TERMINAL_PROMPT".into(), "0".into());
    let output = tool_process::run_bounded(
        BoundedCommand {
            operation_id: crate::models::IdGenerator::default().next(request.operation_prefix),
            owner: request.owner.clone(),
            kind: request.kind,
            program: "git".into(),
            args: command,
            cwd: cwd.to_path_buf(),
            timeout: GIT_TIMEOUT,
            max_stdout: 16 * 1024 * 1024,
            max_stderr: 64 * 1024,
            disk_root: request.disk_root.clone(),
            env,
            remove_env: vec!["GIT_DIR".into(), "GIT_WORK_TREE".into()],
            on_registered: None,
        },
        request.cancelled,
    )
    .await
    .map_err(map_git_error)?;
    if !output.success {
        return Err(git_fetch::classify_failure(&output.stderr).into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().into())
}

/// Clones `repo` at `git_ref` (default branch when `None`) into `checkout`,
/// enforcing size limits before any file is written. Returns the commit.
pub async fn fetch(
    request: &GitRequest<'_>,
    repo: &GithubRepo,
    git_ref: Option<&str>,
    checkout: &Path,
) -> Result<String, String> {
    if !repo.clone_url.starts_with("https://github.com/") && !repo.clone_url.starts_with("file:///")
    {
        return Err("unsupported_repository_url".into());
    }
    if checkout.exists() {
        fs::remove_dir_all(checkout)
            .map_err(|error| format!("failed to reset checkout: {error}"))?;
    }
    fs::create_dir_all(checkout).map_err(|error| format!("failed to create checkout: {error}"))?;
    let parent = checkout.parent().ok_or("checkout has no parent")?;
    match git_ref {
        Some(commit) if git_fetch::valid_revision(commit) => {
            git(request, checkout, &["init", "--quiet"]).await?;
            git(
                request,
                checkout,
                &["remote", "add", "origin", "--", &repo.clone_url],
            )
            .await?;
            git(
                request,
                checkout,
                &["fetch", "--depth", "1", "--no-tags", "origin", commit],
            )
            .await?;
            git(
                request,
                checkout,
                &["update-ref", "--no-deref", "HEAD", "FETCH_HEAD"],
            )
            .await?;
        }
        Some(name) => {
            validate_ref(name)?;
            let target = checkout.to_string_lossy().into_owned();
            git(
                request,
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
                request,
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
    let commit = git(request, checkout, &["rev-parse", "HEAD"]).await?;
    if !git_fetch::valid_revision(&commit) {
        return Err("Invalid Git revision".into());
    }
    let tree = git(request, checkout, &["ls-tree", "-r", "-l", "-z", "HEAD"]).await?;
    enforce_tree_limits(&tree, request.file_limit, request.byte_limit)?;
    fs::create_dir_all(checkout.join(".git/info")).map_err(|error| error.to_string())?;
    fs::write(
        checkout.join(".git/info/attributes"),
        git_fetch::NO_FILTER_ATTRIBUTES,
    )
    .map_err(|error| error.to_string())?;
    git(
        request,
        checkout,
        &["checkout", "--quiet", "--detach", "HEAD"],
    )
    .await?;
    Ok(commit)
}

/// Parses `git ls-tree -r -l -z` output: `<mode> <type> <oid> <size>\t<path>`.
pub fn enforce_tree_limits(tree: &str, file_limit: usize, byte_limit: u64) -> Result<(), String> {
    let mut files = 0usize;
    let mut bytes = 0u64;
    for entry in tree.split('\0').filter(|entry| !entry.is_empty()) {
        let Some((meta, _path)) = entry.split_once('\t') else {
            continue;
        };
        files += 1;
        bytes += meta
            .split_whitespace()
            .nth(3)
            .and_then(|size| size.parse::<u64>().ok())
            .unwrap_or(0);
        if files > file_limit {
            return Err(if file_limit == MAX_TRACKED_FILES {
                format!(
                    "Repository has more than {MAX_TRACKED_FILES} files; Learn supports smaller repositories"
                )
            } else {
                format!("Repository has more than {file_limit} tracked files")
            });
        }
        if bytes > byte_limit {
            return Err(if byte_limit == MAX_CHECKOUT_BYTES {
                "Repository checkout would exceed 500 MiB".into()
            } else {
                "Repository checkout exceeds the research disk budget".into()
            });
        }
    }
    Ok(())
}

#[derive(Debug)]
pub struct SourceInventory {
    pub paths: Vec<String>,
    pub excluded: usize,
    pub truncated: bool,
    pub bytes: u64,
}

/// Text files eligible for an immutable source snapshot. Malicious paths fail
/// the inventory; symlinks, binaries and vendored trees are omitted.
pub fn select_sources(
    checkout: &Path,
    listed: &str,
    max_bytes: u64,
) -> Result<SourceInventory, String> {
    let mut paths = Vec::new();
    let mut excluded = 0usize;
    let mut truncated = false;
    let mut bytes = 0u64;
    let mut seen = BTreeMap::<String, ()>::new();
    let mut entries: Vec<&str> = listed.split('\0').filter(|path| !path.is_empty()).collect();
    entries.sort_unstable();
    for path in entries {
        validate_repo_path(path)?;
        if seen.insert(path.to_string(), ()).is_some() {
            return Err("duplicate_repository_path".into());
        }
        let full = checkout.join(path);
        if !full.starts_with(checkout) {
            return Err("unsafe_repository_path".into());
        }
        let skipped = path.split('/').any(|part| SKIPPED_DIRS.contains(&part));
        let meta = fs::symlink_metadata(&full).map_err(|_| "repository_path_unavailable")?;
        let omit = skipped
            || meta.file_type().is_symlink()
            || !meta.is_file()
            || meta.len() > SOURCE_FILE_BYTES
            || crate::source_files::is_binary(&full);
        if omit || bytes.saturating_add(meta.len()) > max_bytes || paths.len() >= MAX_TRACKED_FILES
        {
            if !omit {
                truncated = true;
            }
            excluded += 1;
            continue;
        }
        bytes = bytes.saturating_add(meta.len());
        paths.push(path.to_string());
    }
    Ok(SourceInventory {
        paths,
        excluded,
        truncated,
        bytes,
    })
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
    for entry in fs::read_dir(dir).map_err(|error| format!("fingerprint failed: {error}"))? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .into_owned();
        if relative == ".git" {
            continue;
        }
        let meta = fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
        let modified = meta
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_nanos())
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
    use std::sync::atomic::{AtomicBool, Ordering};

    fn temp_root() -> PathBuf {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "loom-repo-sources-{}-{}-{}",
            std::process::id(),
            crate::models::now_ms(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn tree_limits_keep_the_learn_messages_at_the_shared_ceiling() {
        let entry = |size: u64| format!("100644 blob {} {size}\tf\0", "a".repeat(40));
        enforce_tree_limits(&entry(10).repeat(10), MAX_TRACKED_FILES, MAX_CHECKOUT_BYTES).unwrap();
        assert!(enforce_tree_limits(
            &entry(1).repeat(MAX_TRACKED_FILES + 1),
            MAX_TRACKED_FILES,
            MAX_CHECKOUT_BYTES
        )
        .unwrap_err()
        .contains("Learn supports smaller"));
        assert_eq!(
            enforce_tree_limits(&entry(MAX_CHECKOUT_BYTES + 1), 10, MAX_CHECKOUT_BYTES)
                .unwrap_err(),
            "Repository checkout would exceed 500 MiB"
        );
        assert!(enforce_tree_limits(&entry(100), 10, 50)
            .unwrap_err()
            .contains("research disk budget"));
    }

    #[test]
    fn source_selection_rejects_escapes_and_skips_links() {
        let root = temp_root();
        fs::create_dir(root.join("src")).unwrap();
        fs::write(root.join("src/item.rs"), "fn version() -> i32 { 1 }\n").unwrap();
        fs::write(root.join("src/other.rs"), "fn other() {}\n").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("/etc/passwd", root.join("src/linked.rs")).unwrap();
        let listed = "src/item.rs\0src/other.rs\0src/linked.rs\0vendor/skip.rs\0";
        fs::create_dir_all(root.join("vendor")).unwrap();
        fs::write(root.join("vendor/skip.rs"), "skipped\n").unwrap();
        let inventory = select_sources(&root, listed, 1024 * 1024).unwrap();
        assert_eq!(
            inventory.paths,
            vec!["src/item.rs".to_string(), "src/other.rs".to_string()]
        );
        assert!(inventory.excluded >= 2);
        for bad in [
            "../secret",
            "/etc/passwd",
            "src/../../x",
            ".git/config",
            "a/../b",
        ] {
            let err = select_sources(&root, &format!("{bad}\0"), 1024).unwrap_err();
            assert_eq!(err, "unsafe_repository_path", "{bad}");
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn feature_owner_can_pin_a_local_commit() {
        let root = temp_root();
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
        let request = GitRequest {
            owner: ProcessOwner::Feature {
                feature: "research".into(),
                scope_id: "research-t".into(),
                run_id: "run-t".into(),
            },
            kind: ProcessKind::Tool,
            operation_prefix: "research-git",
            cancelled: &|| cancel.load(Ordering::Relaxed),
            file_limit: MAX_TRACKED_FILES,
            byte_limit: MAX_CHECKOUT_BYTES,
            disk_root: None,
        };
        let checkout = root.join("checkout");
        let commit = fetch(&request, &repo, Some("main"), &checkout)
            .await
            .unwrap();
        assert_eq!(commit, head);
        assert_eq!(
            fs::read_to_string(checkout.join("main.rs")).unwrap(),
            "fn main() {}\n"
        );
        fs::remove_dir_all(root).unwrap();
    }
}
