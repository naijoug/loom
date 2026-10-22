//! Skills services shared by the desktop and the future headless CLI.
//! No window, environment-derived paths, or Chat session state belongs here.
pub mod catalog;
pub mod cli;
pub mod commands;
pub mod coordination;
pub mod error;
pub mod files;
pub mod installers;
pub mod instructions;
pub mod operations;
pub mod package;
pub mod repository;
pub mod runtime_paths;
pub mod snapshots;
pub mod sources;
pub mod targets;
pub mod transactions;

#[cfg(test)]
mod catalog_tests;
#[cfg(test)]
mod cli_tests;
#[cfg(test)]
mod installers_tests;
#[cfg(test)]
mod instructions_tests;
#[cfg(test)]
mod tests;
