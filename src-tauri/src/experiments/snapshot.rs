//! Copy an explicit file list into a managed directory. The source project is only read.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotFile {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub base_commit: String,
    pub files: Vec<SnapshotFile>,
}

pub fn validate_allowed_path(path: &str) -> Result<(), String> {
    if path.is_empty()
        || path.starts_with('/')
        || path.contains('\\')
        || path.contains('\0')
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err("path_not_allowed".into());
    }
    if path == "package.json"
        || path == "package-lock.json"
        || path
            .split('/')
            .any(|part| part == "node_modules" || part == ".git")
    {
        return Err("path_not_allowed".into());
    }
    Ok(())
}

pub fn require_clean_worktree(porcelain: &str) -> Result<(), String> {
    if porcelain.is_empty() {
        Ok(())
    } else {
        Err("workspace_dirty".into())
    }
}

pub fn capture_snapshot(
    project: &Path,
    destination: &Path,
    commit: &str,
    allowed: &[String],
) -> Result<Snapshot, String> {
    if allowed.is_empty() || commit.len() < 7 || commit.chars().any(|ch| !ch.is_ascii_hexdigit()) {
        return Err("invalid_snapshot_request".into());
    }
    fs::create_dir_all(destination).map_err(|error| error.to_string())?;
    let mut files = Vec::with_capacity(allowed.len());
    for relative in allowed {
        validate_allowed_path(relative)?;
        let source = project.join(relative);
        let meta = fs::symlink_metadata(&source)
            .map_err(|error| format!("snapshot_source_missing: {error}"))?;
        if meta.file_type().is_symlink() || !meta.is_file() {
            return Err("path_not_allowed".into());
        }
        let bytes = fs::read(&source).map_err(|error| error.to_string())?;
        if bytes.contains(&0) || bytes.len() > 1024 * 1024 {
            return Err("path_not_allowed".into());
        }
        let sha256 = format!("{:x}", Sha256::digest(&bytes));
        let copy_to = destination.join(relative);
        if let Some(parent) = copy_to.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::write(&copy_to, &bytes).map_err(|error| error.to_string())?;
        let reread = fs::read(&source).map_err(|error| error.to_string())?;
        if reread != bytes {
            return Err("source_changed_during_snapshot".into());
        }
        files.push(SnapshotFile {
            path: relative.clone(),
            sha256,
            bytes: bytes.len() as u64,
        });
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(Snapshot {
        base_commit: commit.to_ascii_lowercase(),
        files,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args([
                "-c",
                "user.name=E",
                "-c",
                "user.email=e@example.invalid",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .current_dir(dir)
            .status()
            .unwrap();
        assert!(status.success());
    }

    #[test]
    fn snapshot_copies_only_allowed_text_and_leaves_the_project_untouched() {
        let root = std::env::temp_dir().join(format!("loom-exp-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let project = root.join("project");
        fs::create_dir_all(project.join("src")).unwrap();
        fs::write(
            project.join("src/compute.js"),
            "export function compute(input){ return { n: input.n }; }\n",
        )
        .unwrap();
        fs::write(project.join("package.json"), "{}\n").unwrap();
        symlink("compute.js", project.join("src/linked.js")).unwrap();
        git(&project, &["init", "--quiet", "-b", "main"]);
        git(&project, &["add", "."]);
        git(&project, &["commit", "--quiet", "-m", "base"]);
        let commit = String::from_utf8(
            Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(&project)
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap();
        let before = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(&project)
            .output()
            .unwrap();
        assert!(before.stdout.is_empty());
        let managed = root.join("managed");
        let snapshot = capture_snapshot(
            &project,
            &managed,
            commit.trim(),
            &["src/compute.js".into()],
        )
        .unwrap();
        assert_eq!(snapshot.files.len(), 1);
        assert_eq!(
            fs::read(managed.join("src/compute.js")).unwrap(),
            fs::read(project.join("src/compute.js")).unwrap()
        );
        let after = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(&project)
            .output()
            .unwrap();
        assert_eq!(after.stdout, before.stdout);
        assert!(capture_snapshot(
            &project,
            &root.join("other"),
            commit.trim(),
            &["package.json".into()]
        )
        .is_err());
        assert!(capture_snapshot(
            &project,
            &root.join("link"),
            commit.trim(),
            &["src/linked.js".into()]
        )
        .is_err());
        assert!(capture_snapshot(
            &project,
            &root.join("escape"),
            commit.trim(),
            &["../secret.js".into()]
        )
        .is_err());
        fs::write(
            project.join("src/compute.js"),
            "export function compute(){ return { n: 1 }; }\n",
        )
        .unwrap();
        let dirty = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(&project)
            .output()
            .unwrap();
        assert!(!dirty.stdout.is_empty());
        assert_eq!(
            require_clean_worktree(&String::from_utf8(dirty.stdout.clone()).unwrap()),
            Err("workspace_dirty".into())
        );
        assert!(managed.join("src/compute.js").exists());
        fs::remove_dir_all(root).unwrap();
    }
}
