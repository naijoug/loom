//! Experiment foundation. This module does not import Research and has no UI.
//! `LOOM_RESEARCH_EXPERIMENTS=1` on macOS admits new experiments; the default is off.
mod decision;
mod snapshot;

pub use decision::{baseline_gate, judge, next_batch_size, Judgment, Sample};
pub use snapshot::{capture_snapshot, Snapshot};

pub fn experiments_allowed() -> bool {
    if std::env::var("LOOM_RESEARCH_EXPERIMENTS").as_deref() != Ok("1") {
        return false;
    }
    #[cfg(test)]
    let allowed = true;
    #[cfg(not(test))]
    let allowed = cfg!(target_os = "macos");
    allowed
}

/// What the capabilities response may advertise. Tests do not turn the UI on.
pub fn experiments_advertised() -> bool {
    cfg!(target_os = "macos") && std::env::var("LOOM_RESEARCH_EXPERIMENTS").as_deref() == Ok("1")
}
