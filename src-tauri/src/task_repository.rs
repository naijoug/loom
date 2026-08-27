use crate::{
    migrations::{self, TASK_SCHEMA_VERSION},
    models::{CommandRunStatus, Task, TaskStatus},
    storage,
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

fn task_locks() -> &'static Mutex<HashMap<String, Arc<Mutex<()>>>> {
    static LOCKS: OnceLock<Mutex<HashMap<String, Arc<Mutex<()>>>>> = OnceLock::new();
    LOCKS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lock_key(project_path: &Path, task_id: &str) -> String {
    format!("{}::{task_id}", project_path.display())
}

fn acquire_lock(project_path: &Path, task_id: &str) -> Result<(String, Arc<Mutex<()>>), String> {
    let key = lock_key(project_path, task_id);
    let lock = task_locks()
        .lock()
        .map_err(|_| "task lock registry is unavailable".to_string())?
        .entry(key.clone())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone();
    Ok((key, lock))
}

fn release_lock(key: &str, lock: &Arc<Mutex<()>>) {
    if let Ok(mut locks) = task_locks().lock() {
        if Arc::strong_count(lock) == 2
            && locks
                .get(key)
                .is_some_and(|registered| Arc::ptr_eq(registered, lock))
        {
            locks.remove(key);
        }
    }
}

fn with_task_lock<T>(
    project_path: &Path,
    task_id: &str,
    operation: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let (key, lock) = acquire_lock(project_path, task_id)?;
    let result = match lock.lock() {
        Ok(_guard) => operation(),
        Err(_) => Err("task write lock is unavailable".to_string()),
    };
    release_lock(&key, &lock);
    result
}

pub fn load(project_path: &Path, task_id: &str) -> Result<Task, String> {
    validate_task_id(task_id)?;
    migrations::read_versioned_json(
        &task_path(project_path, task_id),
        "task",
        TASK_SCHEMA_VERSION,
    )
}

pub fn save(task: &Task) -> Result<(), String> {
    validate_task_id(&task.id)?;
    let project_path = Path::new(&task.project_path);
    with_task_lock(project_path, &task.id, || {
        let path = task_path(project_path, &task.id);
        let mut persisted = task.clone();
        if let Ok(existing) =
            migrations::read_versioned_json::<Task>(&path, "task", TASK_SCHEMA_VERSION)
        {
            merge_append_only_task_state(&mut persisted, existing);
        }
        migrations::write_versioned_json(&path, TASK_SCHEMA_VERSION, &persisted)
    })
}

pub fn update<T>(
    project_path: &Path,
    task_id: &str,
    update: impl FnOnce(&mut Task) -> Result<T, String>,
) -> Result<(Task, T), String> {
    validate_task_id(task_id)?;
    with_task_lock(project_path, task_id, || {
        let path = task_path(project_path, task_id);
        let mut task: Task = migrations::read_versioned_json(&path, "task", TASK_SCHEMA_VERSION)?;
        let output = update(&mut task)?;
        migrations::write_versioned_json(&path, TASK_SCHEMA_VERSION, &task)?;
        Ok((task, output))
    })
}

pub fn delete(project_path: &Path, task_id: &str) -> Result<bool, String> {
    validate_task_id(task_id)?;
    with_task_lock(project_path, task_id, || {
        let path = task_path(project_path, task_id);
        if !path.exists() {
            return Ok(false);
        }
        std::fs::remove_file(&path).map_err(|error| format!("failed to delete task: {error}"))?;
        Ok(true)
    })
}

pub(crate) fn task_path(project_path: &Path, task_id: &str) -> PathBuf {
    storage::project_tasks_dir(project_path).join(format!("{task_id}.json"))
}

pub(crate) fn validate_task_id(task_id: &str) -> Result<(), String> {
    if task_id.is_empty()
        || task_id.len() > 160
        || !task_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err("invalid task id".to_string());
    }
    Ok(())
}

fn push_missing_by_id<T, F>(target: &mut Vec<T>, source: Vec<T>, id: F)
where
    F: Fn(&T) -> &str,
{
    for item in source {
        if target.iter().all(|current| id(current) != id(&item)) {
            target.push(item);
        }
    }
}

pub(crate) fn merge_append_only_task_state(target: &mut Task, existing: Task) {
    for run in existing.command_runs {
        if let Some(current) = target
            .command_runs
            .iter_mut()
            .find(|current| current.id == run.id)
        {
            if current.status == CommandRunStatus::Running
                && run.status != CommandRunStatus::Running
            {
                *current = run;
            }
        } else {
            target.command_runs.push(run);
        }
    }
    push_missing_by_id(&mut target.events, existing.events, |event| &event.id);
    push_missing_by_id(&mut target.feedback, existing.feedback, |feedback| {
        &feedback.id
    });
    push_missing_by_id(&mut target.loop_trace, existing.loop_trace, |entry| {
        &entry.id
    });
    push_missing_by_id(
        &mut target.implementation_review_runs,
        existing.implementation_review_runs,
        |run| &run.id,
    );
    push_missing_by_id(
        &mut target.implementation_reviews,
        existing.implementation_reviews,
        |review| &review.id,
    );
    push_missing_by_id(
        &mut target.implementation_review_decisions,
        existing.implementation_review_decisions,
        |decision| &decision.id,
    );
    if target.git_baseline.is_none() {
        target.git_baseline = existing.git_baseline;
    }
    if target.summary.is_none() {
        target.summary = existing.summary;
    }
    if existing.updated_at_ms > target.updated_at_ms
        && (existing.lifecycle.paused
            || matches!(existing.status, TaskStatus::Blocked | TaskStatus::Cancelled))
    {
        target.status = existing.status;
        target.lifecycle = existing.lifecycle;
        target.updated_at_ms = existing.updated_at_ms;
    }
}

#[cfg(test)]
fn lock_is_registered(project_path: &Path, task_id: &str) -> bool {
    task_locks()
        .lock()
        .map(|locks| locks.contains_key(&lock_key(project_path, task_id)))
        .unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn task_fixture(root: &Path, id: &str) -> Task {
        let contract: Value =
            serde_json::from_str(include_str!("../../contracts/tauri-contract.json"))
                .expect("contract fixture");
        let mut task: Task =
            serde_json::from_value(contract["modelSamples"]["task"].clone()).expect("task fixture");
        task.id = id.to_string();
        task.project_path = root.display().to_string();
        task
    }

    #[test]
    fn repository_releases_lock_after_save_update_and_delete() {
        let root =
            std::env::temp_dir().join(format!("loom-repository-lock-{}", crate::models::now_ms()));
        std::fs::create_dir_all(&root).expect("create lock release task repository fixture");
        let task = task_fixture(&root, "task-lock-release");

        save(&task).expect("save lock release task fixture");
        assert!(!lock_is_registered(&root, &task.id));
        update(&root, &task.id, |task| {
            task.title = "Updated".to_string();
            Ok(())
        })
        .expect("update lock release task fixture");
        assert!(!lock_is_registered(&root, &task.id));
        assert!(delete(&root, &task.id).expect("delete lock release task fixture"));
        assert!(!lock_is_registered(&root, &task.id));

        std::fs::remove_dir_all(root).expect("clean up lock release task repository fixture");
    }

    #[test]
    fn repository_releases_lock_when_mutation_fails() {
        let root =
            std::env::temp_dir().join(format!("loom-repository-error-{}", crate::models::now_ms()));
        std::fs::create_dir_all(&root).expect("create lock error task repository fixture");
        let task = task_fixture(&root, "task-lock-error");
        save(&task).expect("save lock error task fixture");

        let error = match update(&root, &task.id, |_| Err::<(), _>("rejected".to_string())) {
            Ok(_) => panic!("mutation should fail"),
            Err(error) => error,
        };
        assert_eq!(error, "rejected");
        assert!(!lock_is_registered(&root, &task.id));

        std::fs::remove_dir_all(root).expect("clean up lock error task repository fixture");
    }
}
