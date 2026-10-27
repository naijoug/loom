//! Agent invocation layer: discovers and configures local agent CLIs, declares
//! their capabilities, prepares invocations, checks that they can start, runs
//! them and parses their output into `AgentEvent`s.
//! Feature modules (Chat, Learn, …) use the re-exports below rather than the
//! submodules; only the Tauri command registration in `lib.rs` reaches into
//! `registry` and `diagnostics`.
mod capabilities;
pub(crate) mod diagnostics;
mod event;
mod invocation;
pub(crate) mod registry;
mod runner;
mod stream;
mod tools;

pub use capabilities::{
    capabilities, ADAPTER_CLAUDE, ADAPTER_CODEX, ADAPTER_GROK, ADAPTER_OPENCODE,
};
pub(crate) use diagnostics::check_startable;
pub use event::{AgentEvent, AgentOutcome, DiffStats, MessagePart};
pub(crate) use invocation::{parse_resume_session_id, valid_session_id};
pub use invocation::{
    prepare_invocation, validate_model_name, AdapterInvocationRequest, AgentStage,
    PreparedAgentInvocation,
};
pub(crate) use registry::{load_agents, redact_sensitive_text, resolve_executable};
pub(crate) use runner::{
    run, stop_outcome, Limits, LineEnricher, Observation, RunSpec, STOP_REASON_ABORT,
    STOP_REASON_SHUTDOWN, STOP_REASON_TIMEOUT,
};
#[cfg(test)]
pub(crate) use stream::parse_line;
pub(crate) use stream::redact_stream_line;
pub(crate) use tools::merge_part;
