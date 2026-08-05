use crate::{
    agents,
    execution_policy::{self, ExecutionRequest},
    models::{now_ms, CommandRun, Task},
    settings, storage, task_repository,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};
use tauri::AppHandle;

const DIAGNOSTIC_SCHEMA_VERSION: u32 = 1;
const MAX_RUNS: usize = 100;
const MAX_LOGS: usize = 20;
const MAX_LOG_BYTES: u64 = 64 * 1024;
const MAX_LOG_LINES: usize = 200;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportDiagnosticBundleInput {
    pub project_path: String,
    #[serde(default)]
    pub task_id: Option<String>,
    pub target_path: String,
    #[serde(default)]
    pub include_log_tails: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticBundle {
    schema_version: u32,
    generated_at_ms: u128,
    loom: DiagnosticRuntime,
    system: DiagnosticSystem,
    project: DiagnosticProject,
    task: Option<DiagnosticTask>,
    policy_decisions: Vec<DiagnosticPolicyDecision>,
    logs: Vec<DiagnosticLogTail>,
    omitted_log_count: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticRuntime {
    version: &'static str,
    backend: &'static str,
}

#[derive(Serialize)]
struct DiagnosticSystem {
    os: &'static str,
    arch: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticProject {
    name: String,
    path: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticTask {
    id: String,
    status: String,
    paused: bool,
    resume_status: Option<String>,
    pause_reason: Option<String>,
    blocked_reason: Option<String>,
    cancelled_reason: Option<String>,
    primary_agent_id: Option<String>,
    updated_at_ms: u128,
    runs: Vec<DiagnosticRun>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticRun {
    id: String,
    command: String,
    cwd: String,
    intent: String,
    status: String,
    exit_code: Option<i32>,
    loop_id: Option<String>,
    iteration: Option<u32>,
    attempt: Option<u32>,
    termination_reason: Option<String>,
    started_at_ms: u128,
    ended_at_ms: Option<u128>,
    has_session: bool,
    can_resume: bool,
    error_summary: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticPolicyDecision {
    run_id: String,
    decision: String,
    risk_level: String,
    category: String,
    detail: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticLogTail {
    run_id: String,
    stream: String,
    source: String,
    lines: Vec<String>,
}

fn serde_name<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string())
}

fn redact(value: &str, project_path: &Path) -> String {
    let mut output = value.replace(&project_path.display().to_string(), "[PROJECT_ROOT]");
    if let Some(home) = std::env::var_os("HOME").and_then(|value| value.into_string().ok()) {
        if !home.is_empty() {
            output = output.replace(&home, "~");
        }
    }
    agents::redact_sensitive_text(&output)
}

fn cwd_label(cwd: &str, project_path: &Path) -> String {
    let cwd = Path::new(cwd);
    cwd.strip_prefix(project_path)
        .ok()
        .map(|relative| {
            if relative.as_os_str().is_empty() {
                ".".to_string()
            } else {
                relative.display().to_string()
            }
        })
        .unwrap_or_else(|| "[OUTSIDE_PROJECT]".to_string())
}

fn error_summary(run: &CommandRun, project_path: &Path) -> Option<String> {
    let summary = run.error_summary.as_ref()?;
    summary
        .matched_lines
        .first()
        .or_else(|| summary.test_failures.first())
        .or_else(|| summary.stderr_tail.last())
        .map(|line| redact(line, project_path))
}

fn diagnostic_run(run: &CommandRun, project_path: &Path) -> DiagnosticRun {
    DiagnosticRun {
        id: run.id.clone(),
        command: redact(&run.command, project_path),
        cwd: cwd_label(&run.cwd, project_path),
        intent: serde_name(&run.intent),
        status: run.status.as_str().to_string(),
        exit_code: run.exit_code,
        loop_id: run.loop_id.clone(),
        iteration: run.iteration,
        attempt: run.attempt,
        termination_reason: run
            .termination_reason
            .as_deref()
            .map(|value| redact(value, project_path)),
        started_at_ms: run.started_at_ms,
        ended_at_ms: run.ended_at_ms,
        has_session: run.session_id.is_some(),
        can_resume: run.resume_command.is_some(),
        error_summary: error_summary(run, project_path),
    }
}

fn policy_decision(
    run: &CommandRun,
    project_path: &Path,
    confirm_before_commands: bool,
) -> DiagnosticPolicyDecision {
    let mut command = run.command.split_whitespace();
    let request = ExecutionRequest {
        program: command.next().unwrap_or_default().to_string(),
        args: command.map(str::to_string).collect(),
        cwd: run.cwd.clone(),
        project_path: project_path.display().to_string(),
        agent_id: None,
    };
    let assessment = execution_policy::evaluate_execution(&request, confirm_before_commands);
    DiagnosticPolicyDecision {
        run_id: run.id.clone(),
        decision: serde_name(&assessment.decision),
        risk_level: serde_name(&assessment.risk_level),
        category: serde_name(&assessment.category),
        detail: redact(&assessment.detail, project_path),
    }
}

fn resolve_log_path(project_path: &Path, log_ref: &str) -> Option<PathBuf> {
    let logs_root = fs::canonicalize(storage::project_logs_dir(project_path)).ok()?;
    let candidate = Path::new(log_ref);
    let candidate = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        project_path.join(candidate)
    };
    let canonical = fs::canonicalize(candidate).ok()?;
    canonical.starts_with(&logs_root).then_some(canonical)
}

fn read_log_tail(path: &Path, project_path: &Path) -> Option<Vec<String>> {
    let mut file = File::open(path).ok()?;
    let length = file.metadata().ok()?.len();
    let start = length.saturating_sub(MAX_LOG_BYTES);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::with_capacity((length - start) as usize);
    file.read_to_end(&mut bytes).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let mut lines = text
        .lines()
        .rev()
        .take(MAX_LOG_LINES)
        .map(|line| redact(line, project_path))
        .collect::<Vec<_>>();
    lines.reverse();
    Some(lines)
}

fn collect_logs(task: &Task, project_path: &Path) -> (Vec<DiagnosticLogTail>, usize) {
    let candidates = task
        .command_runs
        .iter()
        .rev()
        .flat_map(|run| {
            [
                (run, "stdout", run.stdout_log_ref.as_deref()),
                (run, "stderr", run.stderr_log_ref.as_deref()),
            ]
        })
        .filter_map(|(run, stream, log_ref)| log_ref.map(|log_ref| (run, stream, log_ref)))
        .collect::<Vec<_>>();
    let omitted = candidates.len().saturating_sub(MAX_LOGS);
    let logs = candidates
        .into_iter()
        .take(MAX_LOGS)
        .filter_map(|(run, stream, log_ref)| {
            let path = resolve_log_path(project_path, log_ref)?;
            Some(DiagnosticLogTail {
                run_id: run.id.clone(),
                stream: stream.to_string(),
                source: path.file_name()?.to_string_lossy().to_string(),
                lines: read_log_tail(&path, project_path)?,
            })
        })
        .collect();
    (logs, omitted)
}

fn build_bundle(
    project_path: &Path,
    task: Option<&Task>,
    include_log_tails: bool,
    confirm_before_commands: bool,
) -> DiagnosticBundle {
    let runs = task
        .map(|task| {
            task.command_runs
                .iter()
                .rev()
                .take(MAX_RUNS)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let policy_decisions = runs
        .iter()
        .map(|run| policy_decision(run, project_path, confirm_before_commands))
        .collect();
    let (logs, omitted_log_count) = if include_log_tails {
        task.map(|task| collect_logs(task, project_path))
            .unwrap_or_default()
    } else {
        (Vec::new(), 0)
    };
    let task = task.map(|task| DiagnosticTask {
        id: task.id.clone(),
        status: task.status.as_str().to_string(),
        paused: task.lifecycle.paused,
        resume_status: task
            .lifecycle
            .resume_status
            .map(|status| status.as_str().to_string()),
        pause_reason: task
            .lifecycle
            .pause_reason
            .as_deref()
            .map(|value| redact(value, project_path)),
        blocked_reason: task
            .lifecycle
            .blocked_reason
            .as_deref()
            .map(|value| redact(value, project_path)),
        cancelled_reason: task
            .lifecycle
            .cancelled_reason
            .as_deref()
            .map(|value| redact(value, project_path)),
        primary_agent_id: task.primary_agent_id.clone(),
        updated_at_ms: task.updated_at_ms,
        runs: runs
            .into_iter()
            .map(|run| diagnostic_run(run, project_path))
            .collect(),
    });

    DiagnosticBundle {
        schema_version: DIAGNOSTIC_SCHEMA_VERSION,
        generated_at_ms: now_ms(),
        loom: DiagnosticRuntime {
            version: env!("CARGO_PKG_VERSION"),
            backend: "tauri",
        },
        system: DiagnosticSystem {
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
        },
        project: DiagnosticProject {
            name: project_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("project")
                .to_string(),
            path: "[PROJECT_ROOT]",
        },
        task,
        policy_decisions,
        logs,
        omitted_log_count,
    }
}

#[tauri::command]
pub fn export_diagnostic_bundle(
    app: AppHandle,
    input: ExportDiagnosticBundleInput,
) -> Result<String, String> {
    let project_path = fs::canonicalize(&input.project_path)
        .map_err(|error| format!("failed to resolve project path: {error}"))?;
    if !project_path.is_dir() {
        return Err("project path must be a directory".to_string());
    }
    let task = input
        .task_id
        .as_deref()
        .map(|task_id| task_repository::load(&project_path, task_id))
        .transpose()?;
    let settings = settings::load_app_settings_for_app(&app)?;
    let bundle = build_bundle(
        &project_path,
        task.as_ref(),
        input.include_log_tails,
        settings.confirm_before_commands,
    );
    let target_path = PathBuf::from(&input.target_path);
    storage::atomic_write_json(&target_path, &bundle)?;
    Ok(target_path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CommandRun;
    use serde_json::Value;

    fn task_fixture(root: &Path) -> Task {
        let contract: Value =
            serde_json::from_str(include_str!("../../contracts/tauri-contract.json"))
                .expect("contract fixture");
        let mut task: Task =
            serde_json::from_value(contract["modelSamples"]["task"].clone()).expect("task");
        let mut run: CommandRun =
            serde_json::from_value(contract["modelSamples"]["commandRun"].clone()).expect("run");
        task.project_path = root.display().to_string();
        task.id = "diagnostic-task".to_string();
        run.task_id = task.id.clone();
        run.command = "curl -H Authorization: Bearer secret-command-token".to_string();
        run.cwd = root.display().to_string();
        run.stdout_log_ref = Some(root.join(".loom/logs/run.stdout.log").display().to_string());
        task.lifecycle.blocked_reason = Some("api_key=secret-reason".to_string());
        task.command_runs.push(run);
        task
    }

    #[test]
    fn diagnostic_bundle_redacts_secrets_paths_and_log_tails() {
        let root = std::env::temp_dir().join(format!("loom-diagnostic-{}", now_ms()));
        fs::create_dir_all(root.join(".loom/logs")).expect("logs dir");
        fs::write(
            root.join(".loom/logs/run.stdout.log"),
            format!(
                "project={}\nOPENAI_API_KEY=sk-test-secret\nAuthorization: Bearer secret-log-token\n",
                root.display()
            ),
        )
        .expect("log fixture");
        let task = task_fixture(&root);

        let bundle = build_bundle(&root, Some(&task), true, true);
        let json = serde_json::to_string_pretty(&bundle).expect("diagnostic json");

        assert!(json.contains("[REDACTED]"));
        assert!(json.contains("[PROJECT_ROOT]"));
        assert!(!json.contains("sk-test-secret"));
        assert!(!json.contains("secret-command-token"));
        assert!(!json.contains("secret-log-token"));
        assert!(!json.contains(&root.display().to_string()));
        assert_eq!(bundle.logs.len(), 1);

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn diagnostic_bundle_omits_logs_unless_user_selects_them() {
        let root = std::env::temp_dir().join(format!("loom-diagnostic-no-logs-{}", now_ms()));
        fs::create_dir_all(&root).expect("test root");
        let task = task_fixture(&root);

        let bundle = build_bundle(&root, Some(&task), false, true);

        assert!(bundle.logs.is_empty());
        assert_eq!(bundle.omitted_log_count, 0);
        fs::remove_dir_all(root).ok();
    }
}
