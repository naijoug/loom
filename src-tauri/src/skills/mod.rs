//! Skills services shared by the desktop and the future headless CLI.
//! No window, environment-derived paths, or Chat session state belongs here.
pub mod coordination;
pub mod error;
pub mod files;
pub mod sources;

#[cfg(test)]
mod tests;
