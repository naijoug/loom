//! Agent invocation layer: discovers and configures local agent CLIs, declares
//! their capabilities, prepares invocations and checks that they can start.
//! Feature modules (Chat, Learn, …) use the re-exports below rather than the
//! submodules; only the Tauri command registration in `lib.rs` reaches into
//! `registry` and `diagnostics`.
mod capabilities;
pub(crate) mod diagnostics;
mod invocation;
pub(crate) mod registry;
mod stream;

pub use capabilities::{
    capabilities, ADAPTER_CLAUDE, ADAPTER_CODEX, ADAPTER_GROK, ADAPTER_OPENCODE,
};
pub(crate) use diagnostics::check_startable;
pub(crate) use invocation::{parse_resume_session_id, valid_session_id};
pub use invocation::{
    prepare_invocation, validate_model_name, AdapterInvocationRequest, AgentStage,
    PreparedAgentInvocation,
};
pub(crate) use registry::{load_agents, redact_sensitive_text, resolve_executable};
pub(crate) use stream::redact_stream_line;
