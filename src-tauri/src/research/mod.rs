//! Research: answer a question with one local agent CLI working read-only in a
//! registered project (or an empty directory), as a cited report with follow-ups.
mod admission;
pub mod commands;
mod files;
mod iteration;
mod jobs;
mod materials;
pub mod models;
mod prompt;
#[cfg(test)]
mod real_tests;
mod storage_budget;
mod store;
pub mod target_commands;
pub mod target_models;
mod target_runtime;
mod target_service;
mod target_storage;
mod target_store;

pub const RESEARCH_EVENT: &str = "loom://research-event";

#[cfg(test)]
mod ipc_tests;
