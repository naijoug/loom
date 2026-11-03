//! Feature-facing lifecycle boundary. The supervisor remains the single owner
//! of admission, process registration and signals; these calls preserve its semantics.
use crate::process_supervisor::{self, OperationGuard, ProcessMetadata};

pub(crate) fn begin_operation() -> Result<OperationGuard<'static>, String> {
    process_supervisor::supervisor().begin_operation()
}
pub(crate) fn begin_shutdown() -> Result<(), String> {
    process_supervisor::supervisor().begin_shutdown()
}
pub(crate) fn is_shutting_down() -> bool {
    process_supervisor::supervisor().is_shutting_down()
}
pub(crate) fn shutdown_snapshot() -> Result<(usize, Vec<ProcessMetadata>), String> {
    process_supervisor::supervisor().shutdown_snapshot()
}
pub(crate) fn request_stop(run_id: &str, reason: &str) -> Result<bool, String> {
    process_supervisor::supervisor().request_stop(run_id, reason)
}
pub(crate) fn force_stop(run_id: &str) -> Result<bool, String> {
    process_supervisor::supervisor().force_stop(run_id)
}
