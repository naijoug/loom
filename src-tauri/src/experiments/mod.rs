//! Experiment foundation. This module does not import Research and has no UI.
#![allow(dead_code)]
//! `LOOM_RESEARCH_EXPERIMENTS=1` on macOS admits new experiments; the default is off.
mod baseline;
mod decision;
mod snapshot;

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
