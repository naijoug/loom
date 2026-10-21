//! Skills services shared by the desktop and the future headless CLI.
//! No window, environment-derived paths, or Chat session state belongs here.
pub mod catalog;
pub mod commands;
pub mod coordination;
pub mod error;
pub mod files;
pub mod operations;
pub mod repository;
pub mod snapshots;
pub mod sources;

#[cfg(test)]
mod catalog_tests;
#[cfg(test)]
mod tests;
