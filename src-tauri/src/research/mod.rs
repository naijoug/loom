//! Research: answer a question with one local agent CLI working read-only in a
//! registered project (or an empty directory), as a cited report with follow-ups.
pub mod commands;
mod jobs;
pub mod models;
mod prompt;
#[cfg(test)]
mod real_tests;
mod store;

pub const RESEARCH_EVENT: &str = "loom://research-event";
