//! Learn: study a GitHub repository with a local agent CLI and browse the
//! generated architecture, wiki pages and source references.
pub mod commands;
mod fetch;
mod generate;
mod inventory;
mod jobs;
pub mod models;
#[cfg(test)]
mod real_tests;
mod store;

pub const LEARN_EVENT: &str = "loom://learn-event";
