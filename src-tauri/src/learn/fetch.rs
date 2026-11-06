//! Learn-facing wrapper around the shared repository fetch.
//! Study ownership stays here; Git execution lives in `repository_sources`.
use crate::{
    repository_sources::{self, GitRequest},
    tool_process::{ProcessKind, ProcessOwner},
};
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

pub use repository_sources::{
    fingerprint, parse_github_url, validate_ref, GithubRepo, MAX_CHECKOUT_BYTES, MAX_TRACKED_FILES,
};

pub struct GitContext<'a> {
    pub study_id: &'a str,
    pub job_id: &'a str,
    pub cancel: &'a AtomicBool,
}

fn request<'a>(ctx: &GitContext<'_>, cancelled: &'a (dyn Fn() -> bool + Sync)) -> GitRequest<'a> {
    GitRequest {
        owner: ProcessOwner::Learn {
            study_id: ctx.study_id.into(),
            job_id: ctx.job_id.into(),
        },
        kind: ProcessKind::Learn,
        operation_prefix: "learn-git",
        cancelled,
        file_limit: MAX_TRACKED_FILES,
        byte_limit: MAX_CHECKOUT_BYTES,
        disk_root: None,
    }
}

/// Runs one hardened git command without a shell. Returns trimmed stdout, or
/// a controlled message (never raw stderr) on failure.
pub async fn git(ctx: &GitContext<'_>, cwd: &Path, args: &[&str]) -> Result<String, String> {
    let cancelled = || ctx.cancel.load(Ordering::Relaxed);
    let request = request(ctx, &cancelled);
    repository_sources::git(&request, cwd, args).await
}

/// Clones `repo` at `git_ref` (default branch when `None`) into `checkout`,
/// enforcing size limits before any file is written. Returns the commit.
pub async fn fetch(
    ctx: &GitContext<'_>,
    repo: &GithubRepo,
    git_ref: Option<&str>,
    checkout: &Path,
) -> Result<String, String> {
    let cancelled = || ctx.cancel.load(Ordering::Relaxed);
    let request = request(ctx, &cancelled);
    repository_sources::fetch(&request, repo, git_ref, checkout).await
}

#[cfg(test)]
fn check_limits(tree: &str) -> Result<(), String> {
    repository_sources::enforce_tree_limits(tree, MAX_TRACKED_FILES, MAX_CHECKOUT_BYTES)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

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
