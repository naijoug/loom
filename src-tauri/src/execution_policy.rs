use crate::{
    models::{now_ms, IdGenerator},
    settings,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};
use tauri::{AppHandle, State};

const APPROVAL_TTL_MS: u128 = 5 * 60 * 1000;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionRequest {
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub cwd: String,
    pub project_path: String,
    #[serde(default)]
    pub agent_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionDecision {
    Allowed,
    ApprovalRequired,
    Denied,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionRiskLevel {
    Safe,
    Medium,
    High,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionRiskCategory {
    None,
    DependencyInstall,
    DestructiveFilesystem,
    DestructiveGit,
    ProductionExternal,
    ProjectBoundary,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionAssessment {
    pub decision: ExecutionDecision,
    pub risk_level: ExecutionRiskLevel,
    pub category: ExecutionRiskCategory,
    pub detail: String,
    pub normalized_project_path: Option<String>,
    pub normalized_cwd: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionApproval {
    pub id: String,
    pub expires_at_ms: u128,
}

pub struct AuthorizedExecution {
    pub project_path: PathBuf,
    pub cwd: PathBuf,
}

struct StoredApproval {
    fingerprint: String,
    expires_at_ms: u128,
}

#[derive(Default)]
pub struct ExecutionApprovalRegistry {
    approvals: Mutex<HashMap<String, StoredApproval>>,
}

impl ExecutionApprovalRegistry {
    fn insert(&self, id: String, fingerprint: String, expires_at_ms: u128) -> Result<(), String> {
        let mut approvals = self
            .approvals
            .lock()
            .map_err(|_| "execution approval registry is unavailable".to_string())?;
        approvals.retain(|_, approval| approval.expires_at_ms > now_ms());
        approvals.insert(
            id,
            StoredApproval {
                fingerprint,
                expires_at_ms,
            },
        );
        Ok(())
    }

    fn consume(&self, id: &str, fingerprint: &str) -> Result<(), String> {
        let mut approvals = self
            .approvals
            .lock()
            .map_err(|_| "execution approval registry is unavailable".to_string())?;
        let approval = approvals
            .remove(id)
            .ok_or_else(|| "execution approval is missing, expired, or already used".to_string())?;
        if approval.expires_at_ms <= now_ms() {
            return Err("execution approval has expired".to_string());
        }
        if approval.fingerprint != fingerprint {
            return Err("execution approval does not match this command".to_string());
        }
        Ok(())
    }
}

#[tauri::command]
pub fn assess_execution(
    app: AppHandle,
    request: ExecutionRequest,
) -> Result<ExecutionAssessment, String> {
    let settings = settings::load_app_settings_for_app(&app)?;
    Ok(evaluate_execution(
        &request,
        settings.confirm_before_commands,
    ))
}

#[tauri::command]
pub fn approve_execution(
    app: AppHandle,
    approvals: State<'_, ExecutionApprovalRegistry>,
    ids: State<'_, IdGenerator>,
    request: ExecutionRequest,
) -> Result<ExecutionApproval, String> {
    let settings = settings::load_app_settings_for_app(&app)?;
    let assessment = evaluate_execution(&request, settings.confirm_before_commands);
    if assessment.decision == ExecutionDecision::Denied {
        return Err(assessment.detail);
    }
    if assessment.decision != ExecutionDecision::ApprovalRequired {
        return Err("this command does not require explicit approval".to_string());
    }

    let fingerprint = assessment_fingerprint(&request, &assessment)?;
    let approval = ExecutionApproval {
        id: ids.next("approval"),
        expires_at_ms: now_ms() + APPROVAL_TTL_MS,
    };
    approvals.insert(approval.id.clone(), fingerprint, approval.expires_at_ms)?;
    Ok(approval)
}

pub fn authorize_execution(
    app: &AppHandle,
    approvals: &ExecutionApprovalRegistry,
    request: &ExecutionRequest,
    approval_id: Option<&str>,
) -> Result<AuthorizedExecution, String> {
    let settings = settings::load_app_settings_for_app(app)?;
    let assessment = evaluate_execution(request, settings.confirm_before_commands);
    match assessment.decision {
        ExecutionDecision::Denied => return Err(assessment.detail),
        ExecutionDecision::ApprovalRequired => {
            let approval_id = approval_id.ok_or_else(|| {
                format!(
                    "execution approval required: {} ({:?})",
                    assessment.detail, assessment.category
                )
            })?;
            approvals.consume(approval_id, &assessment_fingerprint(request, &assessment)?)?;
        }
        ExecutionDecision::Allowed => {}
    }

    Ok(AuthorizedExecution {
        project_path: assessment
            .normalized_project_path
            .map(PathBuf::from)
            .ok_or_else(|| "project path could not be normalized".to_string())?,
        cwd: assessment
            .normalized_cwd
            .map(PathBuf::from)
            .ok_or_else(|| "working directory could not be normalized".to_string())?,
    })
}

pub fn evaluate_execution(
    request: &ExecutionRequest,
    confirm_before_commands: bool,
) -> ExecutionAssessment {
    if request.program.trim().is_empty() {
        return denied(
            ExecutionRiskCategory::ProjectBoundary,
            "command program is required",
        );
    }

    let project_path = match canonical_directory(&request.project_path, "project path") {
        Ok(path) => path,
        Err(error) => return denied(ExecutionRiskCategory::ProjectBoundary, error),
    };
    let cwd = match canonical_directory(&request.cwd, "working directory") {
        Ok(path) => path,
        Err(error) => return denied(ExecutionRiskCategory::ProjectBoundary, error),
    };
    if !cwd.starts_with(&project_path) {
        return ExecutionAssessment {
            decision: ExecutionDecision::Denied,
            risk_level: ExecutionRiskLevel::High,
            category: ExecutionRiskCategory::ProjectBoundary,
            detail: format!(
                "working directory '{}' is outside project root '{}'",
                cwd.display(),
                project_path.display()
            ),
            normalized_project_path: Some(project_path.display().to_string()),
            normalized_cwd: Some(cwd.display().to_string()),
        };
    }

    let (risk_level, category, detail) = classify_command(&request.program, &request.args);
    let decision = match risk_level {
        ExecutionRiskLevel::Safe => ExecutionDecision::Allowed,
        ExecutionRiskLevel::Medium if !confirm_before_commands => ExecutionDecision::Allowed,
        ExecutionRiskLevel::Medium | ExecutionRiskLevel::High => {
            ExecutionDecision::ApprovalRequired
        }
    };

    ExecutionAssessment {
        decision,
        risk_level,
        category,
        detail,
        normalized_project_path: Some(project_path.display().to_string()),
        normalized_cwd: Some(cwd.display().to_string()),
    }
}

fn denied(category: ExecutionRiskCategory, detail: impl Into<String>) -> ExecutionAssessment {
    ExecutionAssessment {
        decision: ExecutionDecision::Denied,
        risk_level: ExecutionRiskLevel::High,
        category,
        detail: detail.into(),
        normalized_project_path: None,
        normalized_cwd: None,
    }
}

fn canonical_directory(value: &str, label: &str) -> Result<PathBuf, String> {
    let path = Path::new(value);
    let canonical = fs::canonicalize(path)
        .map_err(|error| format!("{label} '{}' is not accessible: {error}", path.display()))?;
    if !canonical.is_dir() {
        return Err(format!(
            "{label} '{}' is not a directory",
            canonical.display()
        ));
    }
    Ok(canonical)
}

fn classify_command(
    program: &str,
    args: &[String],
) -> (ExecutionRiskLevel, ExecutionRiskCategory, String) {
    let executable = Path::new(program)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(program)
        .to_ascii_lowercase();
    let joined = std::iter::once(program)
        .chain(args.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();

    if matches!(executable.as_str(), "rm" | "rmdir" | "unlink")
        || joined.contains(" rm -")
        || joined.contains("rm --recursive")
    {
        return (
            ExecutionRiskLevel::High,
            ExecutionRiskCategory::DestructiveFilesystem,
            "command may permanently delete files".to_string(),
        );
    }

    if (executable == "git" || joined.contains(" git "))
        && (joined.contains(" reset ")
            || joined.contains(" clean ")
            || joined.contains(" checkout --"))
    {
        return (
            ExecutionRiskLevel::High,
            ExecutionRiskCategory::DestructiveGit,
            "command may discard Git worktree or index changes".to_string(),
        );
    }

    let production_tool = matches!(
        executable.as_str(),
        "curl" | "wget" | "ssh" | "kubectl" | "aws" | "gcloud" | "vercel"
    ) || joined.contains(" kubectl ")
        || joined.contains(" aws ")
        || joined.contains(" gcloud ");
    if production_tool && (joined.contains("prod") || joined.contains("production")) {
        return (
            ExecutionRiskLevel::High,
            ExecutionRiskCategory::ProductionExternal,
            "command appears to target an external production environment".to_string(),
        );
    }

    let package_manager_mutations = [
        "add",
        "ci",
        "i",
        "install",
        "remove",
        "uninstall",
        "update",
        "upgrade",
    ];
    let mutates_dependencies = |candidate_args: &[String]| {
        let first_command = candidate_args
            .iter()
            .find(|arg| !arg.starts_with('-'))
            .map(String::as_str);
        if matches!(first_command, Some("run" | "exec" | "dlx" | "x")) {
            return false;
        }
        candidate_args
            .iter()
            .any(|arg| package_manager_mutations.contains(&arg.as_str()))
    };
    let pip_module_args = args
        .windows(2)
        .position(|window| window[0] == "-m" && window[1] == "pip")
        .map(|pip_index| &args[(pip_index + 2)..]);

    let installs_dependency = match executable.as_str() {
        "npm" | "pnpm" | "yarn" | "bun" => mutates_dependencies(args),
        "pip" | "pip3" => mutates_dependencies(args),
        "python" | "python3" => pip_module_args.is_some_and(mutates_dependencies),
        "cargo" => mutates_dependencies(args),
        "go" => args.iter().any(|arg| arg == "get"),
        _ => {
            joined.contains(" npm install")
                || joined.contains(" npm ci")
                || joined.contains(" pnpm install")
                || joined.contains(" pnpm add")
                || joined.contains(" yarn install")
                || joined.contains(" yarn add")
                || joined.contains(" bun install")
                || joined.contains(" bun add")
                || joined.contains(" pip install")
                || joined.contains(" pip uninstall")
                || joined.contains(" cargo add")
                || joined.contains(" go get")
        }
    };
    if installs_dependency {
        return (
            ExecutionRiskLevel::Medium,
            ExecutionRiskCategory::DependencyInstall,
            "command installs or changes project dependencies".to_string(),
        );
    }

    (
        ExecutionRiskLevel::Safe,
        ExecutionRiskCategory::None,
        "command stays inside the project and has no recognized high-risk pattern".to_string(),
    )
}

fn assessment_fingerprint(
    request: &ExecutionRequest,
    assessment: &ExecutionAssessment,
) -> Result<String, String> {
    let project_path = assessment
        .normalized_project_path
        .as_deref()
        .ok_or_else(|| "project path is unavailable for approval".to_string())?;
    let cwd = assessment
        .normalized_cwd
        .as_deref()
        .ok_or_else(|| "working directory is unavailable for approval".to_string())?;
    Ok(format!(
        "{project_path}\u{1f}{cwd}\u{1f}{}\u{1f}{}\u{1f}{}",
        request.program,
        request.args.join("\u{1e}"),
        request.agent_id.as_deref().unwrap_or("")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

    fn project_fixture() -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "loom-policy-{}-{}",
            now_ms(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        let nested = root.join("apps/web");
        fs::create_dir_all(&nested).expect("policy fixture");
        (root, nested)
    }

    fn request(root: &Path, cwd: &Path, program: &str, args: &[&str]) -> ExecutionRequest {
        ExecutionRequest {
            program: program.to_string(),
            args: args.iter().map(|value| (*value).to_string()).collect(),
            cwd: cwd.display().to_string(),
            project_path: root.display().to_string(),
            agent_id: None,
        }
    }

    #[test]
    fn allows_safe_commands_in_nested_project_directories() {
        let (root, nested) = project_fixture();
        let assessment = evaluate_execution(&request(&root, &nested, "pnpm", &["test"]), true);

        assert_eq!(assessment.decision, ExecutionDecision::Allowed);
        assert_eq!(assessment.risk_level, ExecutionRiskLevel::Safe);
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn denies_working_directories_outside_the_project() {
        let (root, _) = project_fixture();
        let outside = std::env::temp_dir().join(format!(
            "loom-policy-outside-{}-{}",
            now_ms(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&outside).expect("outside fixture");

        let assessment = evaluate_execution(&request(&root, &outside, "pnpm", &["test"]), false);

        assert_eq!(assessment.decision, ExecutionDecision::Denied);
        assert_eq!(assessment.category, ExecutionRiskCategory::ProjectBoundary);
        fs::remove_dir_all(root).ok();
        fs::remove_dir_all(outside).ok();
    }

    #[test]
    fn high_risk_commands_always_require_approval() {
        let (root, nested) = project_fixture();
        for (program, args, category) in [
            (
                "rm",
                vec!["-rf", "build"],
                ExecutionRiskCategory::DestructiveFilesystem,
            ),
            (
                "git",
                vec!["reset", "--hard"],
                ExecutionRiskCategory::DestructiveGit,
            ),
            (
                "kubectl",
                vec!["delete", "pod", "--context", "production"],
                ExecutionRiskCategory::ProductionExternal,
            ),
        ] {
            let assessment = evaluate_execution(&request(&root, &nested, program, &args), false);
            assert_eq!(assessment.decision, ExecutionDecision::ApprovalRequired);
            assert_eq!(assessment.category, category);
        }
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn dependency_install_respects_confirmation_setting() {
        let (root, nested) = project_fixture();
        let request = request(&root, &nested, "pnpm", &["add", "zod"]);

        assert_eq!(
            evaluate_execution(&request, true).decision,
            ExecutionDecision::ApprovalRequired
        );
        assert_eq!(
            evaluate_execution(&request, false).decision,
            ExecutionDecision::Allowed
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn package_mutations_cover_common_lockfile_and_removal_commands() {
        let (root, nested) = project_fixture();

        for (program, args) in [
            ("npm", vec!["ci"]),
            ("pnpm", vec!["--filter", "web", "update"]),
            ("yarn", vec!["remove", "left-pad"]),
            ("bun", vec!["upgrade"]),
            ("pip3", vec!["uninstall", "requests"]),
            ("python", vec!["-m", "pip", "uninstall", "requests"]),
            ("cargo", vec!["remove", "serde"]),
        ] {
            let assessment = evaluate_execution(&request(&root, &nested, program, &args), true);

            assert_eq!(assessment.decision, ExecutionDecision::ApprovalRequired);
            assert_eq!(assessment.risk_level, ExecutionRiskLevel::Medium);
            assert_eq!(
                assessment.category,
                ExecutionRiskCategory::DependencyInstall
            );
        }

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn package_like_script_names_do_not_trigger_dependency_approval() {
        let (root, nested) = project_fixture();
        let assessment = evaluate_execution(
            &request(&root, &nested, "npm", &["run", "add-fixture"]),
            true,
        );
        let pnpm_script_assessment =
            evaluate_execution(&request(&root, &nested, "pnpm", &["run", "update"]), true);

        assert_eq!(assessment.decision, ExecutionDecision::Allowed);
        assert_eq!(assessment.category, ExecutionRiskCategory::None);
        assert_eq!(pnpm_script_assessment.decision, ExecutionDecision::Allowed);
        assert_eq!(pnpm_script_assessment.category, ExecutionRiskCategory::None);
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn approvals_are_single_use_and_bound_to_the_exact_command() {
        let registry = ExecutionApprovalRegistry::default();
        registry
            .insert(
                "approval-1".to_string(),
                "fingerprint-1".to_string(),
                now_ms() + APPROVAL_TTL_MS,
            )
            .expect("approval insert");

        assert!(registry.consume("approval-1", "fingerprint-2").is_err());
        assert!(registry.consume("approval-1", "fingerprint-1").is_err());

        registry
            .insert(
                "approval-2".to_string(),
                "fingerprint-2".to_string(),
                now_ms() + APPROVAL_TTL_MS,
            )
            .expect("approval insert");
        registry
            .consume("approval-2", "fingerprint-2")
            .expect("matching approval");
        assert!(registry.consume("approval-2", "fingerprint-2").is_err());
    }
}
