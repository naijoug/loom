use crate::models::{now_ms, ChangedFileSummary, GitBaseline, GitBaselineFile, Task};
use std::{
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

const MAX_GIT_OUTPUT_BYTES: usize = 10 * 1024 * 1024;

fn run_git(project_path: &Path, args: &[&str]) -> Result<Output, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(project_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| format!("failed to run git: {error}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if detail.is_empty() {
            format!("git {} exited with {}", args.join(" "), output.status)
        } else {
            detail
        });
    }
    if output.stdout.len() > MAX_GIT_OUTPUT_BYTES {
        return Err(format!(
            "git {} output exceeded {} bytes",
            args.join(" "),
            MAX_GIT_OUTPUT_BYTES
        ));
    }
    Ok(output)
}

fn git_text(project_path: &Path, args: &[&str]) -> Result<String, String> {
    let output = run_git(project_path, args)?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn parse_status(output: &[u8]) -> Vec<GitBaselineFile> {
    let mut records = output.split(|byte| *byte == 0).peekable();
    let mut files = Vec::new();
    while let Some(record) = records.next() {
        if record.len() < 4 {
            continue;
        }
        let status = String::from_utf8_lossy(&record[..2]).to_string();
        let path = String::from_utf8_lossy(&record[3..]).to_string();
        if status.bytes().any(|value| matches!(value, b'R' | b'C')) {
            let _ = records.next();
        }
        files.push(GitBaselineFile { path, status });
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    files
}

fn status_files(project_path: &Path) -> Result<Vec<GitBaselineFile>, String> {
    let output = run_git(
        project_path,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )?;
    Ok(parse_status(&output.stdout))
}

pub fn capture_git_baseline(project_path: &Path) -> GitBaseline {
    let captured_at_ms = now_ms();
    let available = git_text(project_path, &["rev-parse", "--is-inside-work-tree"])
        .is_ok_and(|value| value == "true");
    if !available {
        return GitBaseline {
            captured_at_ms,
            available: false,
            head: None,
            files: Vec::new(),
            failure_detail: Some("project is not a readable Git worktree".to_string()),
        };
    }

    match status_files(project_path) {
        Ok(files) => GitBaseline {
            captured_at_ms,
            available: true,
            head: git_text(project_path, &["rev-parse", "HEAD"]).ok(),
            files,
            failure_detail: None,
        },
        Err(error) => GitBaseline {
            captured_at_ms,
            available: false,
            head: None,
            files: Vec::new(),
            failure_detail: Some(error),
        },
    }
}

#[derive(Default)]
struct NumStat {
    additions: Option<u64>,
    deletions: Option<u64>,
}

fn merge_numstat(target: &mut BTreeMap<String, NumStat>, output: &str) {
    for line in output.lines() {
        let mut fields = line.splitn(3, '\t');
        let Some(additions) = fields.next() else {
            continue;
        };
        let Some(deletions) = fields.next() else {
            continue;
        };
        let Some(path) = fields.next().filter(|path| !path.is_empty()) else {
            continue;
        };
        let entry = target.entry(path.to_string()).or_default();
        entry.additions = sum_optional(entry.additions, additions.parse::<u64>().ok());
        entry.deletions = sum_optional(entry.deletions, deletions.parse::<u64>().ok());
    }
}

fn sum_optional(left: Option<u64>, right: Option<u64>) -> Option<u64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.saturating_add(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn fallback_evidence_files(task: &Task) -> Vec<ChangedFileSummary> {
    let project_root = Path::new(&task.project_path);
    let mut paths = Vec::new();
    if let Some(path) = task.final_plan_path.as_deref() {
        paths.push(path.to_string());
    }
    for event in &task.events {
        if let Some(path) = event.evidence_ref.as_deref() {
            paths.push(path.to_string());
        }
    }
    for feedback in &task.feedback {
        paths.extend(
            feedback
                .attachments
                .iter()
                .map(|attachment| attachment.stored_path.clone()),
        );
    }
    let mut seen = HashSet::new();
    paths
        .into_iter()
        .filter_map(|path| {
            let path = PathBuf::from(path);
            let display = path
                .strip_prefix(project_root)
                .unwrap_or(&path)
                .display()
                .to_string();
            seen.insert(display.clone()).then_some(ChangedFileSummary {
                path: display,
                status: "evidence".to_string(),
                additions: None,
                deletions: None,
                attribution: "unknown".to_string(),
            })
        })
        .collect()
}

pub fn collect_changed_files(task: &Task) -> (Vec<ChangedFileSummary>, Option<String>) {
    let baseline = task.git_baseline.as_ref();
    if !baseline.is_some_and(|baseline| baseline.available) {
        return (
            fallback_evidence_files(task),
            Some(
                baseline
                    .and_then(|baseline| baseline.failure_detail.clone())
                    .unwrap_or_else(|| "Git baseline was not captured".to_string()),
            ),
        );
    }
    let baseline = baseline.expect("checked above");
    let project_path = Path::new(&task.project_path);
    let current_status = match status_files(project_path) {
        Ok(status) => status,
        Err(error) => return (fallback_evidence_files(task), Some(error)),
    };
    let mut stats = BTreeMap::new();
    if let Some(head) = baseline.head.as_deref() {
        match git_text(project_path, &["diff", "--numstat", head, "--"]) {
            Ok(output) => merge_numstat(&mut stats, &output),
            Err(error) => return (fallback_evidence_files(task), Some(error)),
        }
    } else {
        for args in [
            ["diff", "--numstat", "--"].as_slice(),
            ["diff", "--cached", "--numstat", "--"].as_slice(),
        ] {
            if let Ok(output) = git_text(project_path, args) {
                merge_numstat(&mut stats, &output);
            }
        }
    }

    let status_by_path = current_status
        .into_iter()
        .map(|file| (file.path, file.status))
        .collect::<BTreeMap<_, _>>();
    for path in status_by_path.keys() {
        stats.entry(path.clone()).or_default();
    }
    let baseline_paths = baseline
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect::<HashSet<_>>();
    let changed_files = stats
        .into_iter()
        .map(|(path, stat)| ChangedFileSummary {
            status: status_by_path
                .get(&path)
                .cloned()
                .unwrap_or_else(|| "committed".to_string()),
            attribution: if baseline_paths.contains(path.as_str()) {
                "pre_existing".to_string()
            } else {
                "task_introduced".to_string()
            },
            path,
            additions: stat.additions,
            deletions: stat.deletions,
        })
        .collect();
    (changed_files, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn git(root: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(root)
            .status()
            .expect("git should run");
        assert!(status.success());
    }

    #[test]
    fn baseline_distinguishes_pre_existing_and_task_changes() {
        let root = std::env::temp_dir().join(format!("loom-git-baseline-{}", now_ms()));
        fs::create_dir_all(&root).unwrap();
        git(&root, &["init", "-q"]);
        git(&root, &["config", "user.email", "loom@example.test"]);
        git(&root, &["config", "user.name", "Loom Test"]);
        fs::write(root.join("tracked.txt"), "base\n").unwrap();
        git(&root, &["add", "tracked.txt"]);
        git(&root, &["commit", "-qm", "base"]);
        fs::write(root.join("tracked.txt"), "base\nuser work\n").unwrap();
        let baseline = capture_git_baseline(&root);
        assert!(baseline.available);
        fs::write(root.join("tracked.txt"), "base\nuser work\ntask\n").unwrap();
        fs::write(root.join("new.txt"), "task\n").unwrap();

        let task = Task {
            id: "task-git".to_string(),
            project_path: root.display().to_string(),
            title: "Git attribution".to_string(),
            raw_requirement: "attribute changes".to_string(),
            status: crate::models::TaskStatus::Implementing,
            lifecycle: Default::default(),
            selected_planning_agent_ids: Vec::new(),
            primary_agent_id: None,
            review_agent_ids: Vec::new(),
            implementation_review_runs: Vec::new(),
            implementation_reviews: Vec::new(),
            implementation_review_decisions: Vec::new(),
            final_plan: None,
            final_plan_path: None,
            final_plan_html_path: None,
            discussion_summary: None,
            planning_runs: Vec::new(),
            agent_invocations: Vec::new(),
            plan_reviews: Vec::new(),
            planning_decisions: Vec::new(),
            plan_todos: Vec::new(),
            loop_trace: Vec::new(),
            events: Vec::new(),
            command_runs: Vec::new(),
            feedback: Vec::new(),
            loop_compact_summary: None,
            repair_context_preview: None,
            git_baseline: Some(baseline),
            summary: None,
            created_at_ms: 1,
            updated_at_ms: 1,
        };
        let (files, error) = collect_changed_files(&task);
        assert!(error.is_none());
        assert!(files
            .iter()
            .any(|file| { file.path == "tracked.txt" && file.attribution == "pre_existing" }));
        assert!(files
            .iter()
            .any(|file| { file.path == "new.txt" && file.attribution == "task_introduced" }));
        fs::remove_dir_all(root).unwrap();
    }
}
