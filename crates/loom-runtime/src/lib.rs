//! Host-independent SDK process supervision. No Tauri or fixture model policy.
mod core_bridge;
pub mod policy;
mod protocol;
mod rea;
pub use core_bridge::{
    run_session, Authorization, ExternalToolRunner, SessionRunOptions, ToolPolicy,
};
pub mod shell;
mod supervisor;
pub use protocol::{
    ExternalTool, Frame, HistoryMessage, RunConfig, Scope, ToolDecision, WorkerEvent, MAX_FRAME,
    VERSION,
};
pub use rea::{
    active as rea_source_active, executable as rea_executable,
    external_tools as rea_external_tools, probe_identity as probe_rea_identity, split_tool_name,
    worker_command as rea_worker_command, ReaCatalog, ReaMcpClient, REA_CATALOG_DIGEST,
    REA_PACKAGE, REA_SOURCE, REA_VERSION,
};
pub use supervisor::{run, ExternalToolRequest, Handler, RunOutcome, RuntimeError, WorkerCommand};

#[cfg(feature = "bindings")]
pub fn typescript_bindings() -> String {
    use ts_rs::{Config, TS};
    let config = Config::new().with_large_int("number");
    let mut output = String::from(
        "// Generated from loom-runtime Rust types. Do not edit.\nimport type { Id, ConnectionConfig, JsonValue } from './workbench.js';\n\n",
    );
    output.push_str(&format!(
        "export const RUNTIME_PROTOCOL_VERSION = {} as const;\n",
        VERSION
    ));
    macro_rules! export {($($ty:ty),* $(,)?)=>{$(output.push_str("export ");output.push_str(&<$ty>::decl(&config));output.push_str("\n");)*};}
    export!(
        Scope,
        HistoryMessage,
        ExternalTool,
        RunConfig,
        WorkerEvent,
        Frame,
        ToolDecision,
        protocol::Start<'static>,
        protocol::DecisionFrame<'static>,
        protocol::CancelFrame<'static>
    );
    output
}
