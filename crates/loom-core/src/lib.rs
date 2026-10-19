//! Host-independent Loom application core. No UI, SDK or old Task dependency.
mod api;
mod approval;
mod connection;
mod error;
mod import_model;
mod journal;
mod legacy;
mod model;
mod permissions;
mod remember;
pub use permissions::{
    PermissionDocument, PermissionScope, PermissionSettings, SavePermissionRequest,
};
pub use remember::{GrantRevocation, RememberKey, RememberKind, RememberOffer, RememberedGrant};
mod projection;
mod repository;

pub use approval::{
    ApprovalResolution, ApprovalStatus, ToolApproval, ToolApprovalSpec, ToolPolicyDecision,
};
pub use connection::*;
pub use error::{CoreError, ErrorCode, Result};
pub use import_model::*;
pub use legacy::{inspect_legacy_project, LegacyFile, LegacyIssue, LegacyItem, LegacyPreview};
pub use model::*;
pub use repository::{ContextMessage, RuntimeContext};
pub use repository::{
    Core, Diagnostic, Subscription, ToolPermit, TurnLease, WorkspaceSubscription,
};

pub use api::{CoreCommand, CoreReply, CoreRequest, CoreResponse, Outcome, PROTOCOL_VERSION};

#[cfg(feature = "bindings")]
mod bindings;
#[cfg(feature = "bindings")]
pub use bindings::typescript_bindings;
