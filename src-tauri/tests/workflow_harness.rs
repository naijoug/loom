use loom_lib::{
    models::{Task, TaskStatus},
    task_repository,
    task_state::{transition, TaskAction},
};
use serde_json::Value;
use std::path::{Path, PathBuf};

struct TempProject(PathBuf);

impl TempProject {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "loom-workflow-harness-{label}-{}-{}",
            std::process::id(),
            loom_lib::models::now_ms()
        ));
        std::fs::create_dir_all(&path).expect("create temp project");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn task_fixture(project: &TempProject, id: &str) -> Task {
    let contract: Value = serde_json::from_str(include_str!("../../contracts/tauri-contract.json"))
        .expect("contract fixture");
    let mut task: Task = serde_json::from_value(contract["modelSamples"]["task"].clone())
        .expect("task model fixture");
    task.id = id.to_string();
    task.project_path = project.path().display().to_string();
    task.status = TaskStatus::DraftingRequirements;
    task.lifecycle = Default::default();
    task.events.clear();
    task.command_runs.clear();
    task.feedback.clear();
    task.created_at_ms = 1;
    task.updated_at_ms = 1;
    task
}

fn apply(project: &TempProject, task_id: &str, action: TaskAction) -> TaskStatus {
    task_repository::update(project.path(), task_id, |task| {
        task.status = transition(task.status, action)?;
        task.updated_at_ms += 1;
        Ok(task.status)
    })
    .expect("workflow transition")
    .1
}

#[test]
fn four_stage_workflow_replays_two_repair_rounds_headlessly() {
    let project = TempProject::new("repair-loop");
    let task = task_fixture(&project, "task-repair-loop");
    task_repository::save(&task).expect("save fixture");

    for action in [
        TaskAction::PlanningStarted,
        TaskAction::PlanningCompleted,
        TaskAction::PlanConfirmed,
        TaskAction::TodoStarted,
        TaskAction::TodoCompleted { all_done: true },
        TaskAction::TestingRequested,
        TaskAction::ValidationStarted,
        TaskAction::ValidationFailed,
        TaskAction::RepairStarted,
        TaskAction::ValidationStarted,
        TaskAction::ValidationFailed,
        TaskAction::RepairStarted,
        TaskAction::ValidationStarted,
        TaskAction::ValidationPassed,
        TaskAction::Accepted,
    ] {
        apply(&project, &task.id, action);
    }

    let restored = task_repository::load(project.path(), &task.id).expect("reload task");
    assert_eq!(restored.status, TaskStatus::Completed);
    assert_eq!(restored.updated_at_ms, 16);
}

#[test]
fn blocker_and_pause_state_survive_repository_reload() {
    let project = TempProject::new("recovery");
    let mut task = task_fixture(&project, "task-recovery");
    task.status = TaskStatus::Implementing;
    task.lifecycle.paused = true;
    task.lifecycle.pause_reason = Some("Waiting for user approval".to_string());
    task_repository::save(&task).expect("save paused task");

    let restored = task_repository::load(project.path(), &task.id).expect("reload paused task");
    assert!(restored.lifecycle.paused);
    assert_eq!(
        restored.lifecycle.pause_reason.as_deref(),
        Some("Waiting for user approval")
    );

    let invalid = transition(restored.status, TaskAction::TestingRequested)
        .expect_err("implementation review cannot be skipped");
    assert!(invalid.contains("invalid task transition"));

    task_repository::update(project.path(), &task.id, |current| {
        current.lifecycle.paused = false;
        current.lifecycle.pause_reason = None;
        current.lifecycle.resume_status = Some(current.status);
        current.status = transition(current.status, TaskAction::Blocked)?;
        current.lifecycle.blocked_reason = Some("Independent review found a blocker".to_string());
        Ok(())
    })
    .expect("persist blocker");

    let blocked = task_repository::load(project.path(), &task.id).expect("reload blocker");
    assert_eq!(blocked.status, TaskStatus::Blocked);
    assert_eq!(
        blocked.lifecycle.resume_status,
        Some(TaskStatus::Implementing)
    );
}
