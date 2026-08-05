use crate::models::TaskStatus;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskAction {
    PlanningStarted,
    PlanningCompleted,
    PlanConfirmed,
    TodoStarted,
    TodoCompleted { all_done: bool },
    TestingRequested,
    ValidationStarted,
    ValidationPassed,
    ValidationFailed,
    RepairStarted,
    Accepted,
    Paused,
    Resumed { target: TaskStatus },
    Blocked,
    Cancelled,
}

pub fn transition(current: TaskStatus, action: TaskAction) -> Result<TaskStatus, String> {
    use TaskAction as Action;
    use TaskStatus as Status;

    if current.is_terminal() {
        return Err(format!(
            "cannot apply {action:?} to terminal task status {current}"
        ));
    }

    let next = match (current, action) {
        (Status::DraftingRequirements, Action::PlanningStarted)
        | (Status::Planning, Action::PlanningStarted)
        | (Status::PlanReview, Action::PlanningStarted)
        | (Status::ReadyToImplement, Action::PlanningStarted) => Status::Planning,
        (Status::Planning, Action::PlanningCompleted)
        | (Status::PlanReview, Action::PlanningCompleted) => Status::PlanReview,
        (Status::Planning, Action::PlanConfirmed) | (Status::PlanReview, Action::PlanConfirmed) => {
            Status::ReadyToImplement
        }
        (
            Status::ReadyToImplement | Status::Implementing | Status::Reviewing,
            Action::TodoStarted,
        ) => Status::Implementing,
        (Status::Implementing, Action::TodoCompleted { all_done: true }) => Status::Reviewing,
        (Status::Implementing, Action::TodoCompleted { all_done: false }) => Status::Implementing,
        (Status::Reviewing, Action::TestingRequested) => Status::Debugging,
        (
            Status::Reviewing | Status::Debugging | Status::Fixing | Status::Verifying,
            Action::ValidationStarted,
        ) => Status::Debugging,
        (
            Status::Implementing | Status::Reviewing | Status::Debugging | Status::Fixing,
            Action::ValidationPassed,
        ) => Status::Verifying,
        (Status::Debugging | Status::Fixing | Status::Verifying, Action::ValidationFailed) => {
            Status::Debugging
        }
        (Status::Reviewing | Status::Debugging | Status::Verifying, Action::RepairStarted) => {
            Status::Fixing
        }
        (Status::Verifying, Action::Accepted) => Status::Completed,
        (
            Status::DraftingRequirements
            | Status::Planning
            | Status::PlanReview
            | Status::ReadyToImplement
            | Status::Implementing
            | Status::Reviewing
            | Status::Debugging
            | Status::Fixing
            | Status::Verifying,
            Action::Paused,
        ) => current,
        (Status::Blocked, Action::Resumed { target })
            if !matches!(
                target,
                Status::Blocked | Status::Completed | Status::Cancelled
            ) =>
        {
            target
        }
        (current, Action::Resumed { target }) if current == target => current,
        (
            Status::DraftingRequirements
            | Status::Planning
            | Status::PlanReview
            | Status::ReadyToImplement
            | Status::Implementing
            | Status::Reviewing
            | Status::Debugging
            | Status::Fixing
            | Status::Verifying,
            Action::Blocked,
        ) => Status::Blocked,
        (Status::Blocked, Action::Blocked) => Status::Blocked,
        (_, Action::Cancelled) => Status::Cancelled,
        _ => {
            return Err(format!(
                "invalid task transition: status={current}, action={action:?}"
            ))
        }
    };

    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_path_reaches_completed() {
        let mut status = TaskStatus::DraftingRequirements;
        for action in [
            TaskAction::PlanningStarted,
            TaskAction::PlanningCompleted,
            TaskAction::PlanConfirmed,
            TaskAction::TodoStarted,
            TaskAction::TodoCompleted { all_done: true },
            TaskAction::TestingRequested,
            TaskAction::ValidationStarted,
            TaskAction::ValidationPassed,
            TaskAction::Accepted,
        ] {
            status = transition(status, action).expect("valid transition");
        }
        assert_eq!(status, TaskStatus::Completed);
    }

    #[test]
    fn review_cannot_be_skipped() {
        let error = transition(TaskStatus::Implementing, TaskAction::TestingRequested)
            .expect_err("implementation must enter review first");
        assert!(error.contains("invalid task transition"));
    }

    #[test]
    fn terminal_tasks_reject_more_transitions() {
        for status in [TaskStatus::Completed, TaskStatus::Cancelled] {
            let error = transition(status, TaskAction::TodoStarted)
                .expect_err("terminal status must reject transitions");
            assert!(error.contains("terminal task status"));
        }
    }
}
