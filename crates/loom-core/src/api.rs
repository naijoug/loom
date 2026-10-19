//! User-intent commands. Runtime mutation deliberately has no wire command:
//! an in-process host must own a TurnLease to append output or finish a turn.
use crate::*;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const PROTOCOL_VERSION: u32 = 1;
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    tag = "method",
    content = "params",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum CoreCommand {
    RememberToolApproval {
        key: SessionKey,
        request_id: Id,
        expected_digest: String,
    },
    ClearRememberedGrants {
        key: SessionKey,
    },
    PermissionDocuments {
        workspace_id: Id,
    },
    ReadPolicySnapshot {
        key: SessionKey,
        turn_id: Id,
    },
    ListConnections,
    GetConnection {
        id: Id,
    },
    ListWorkspaces,
    CreateWorkspace {
        id: Id,
        name: String,
    },
    ListProjects {
        workspace_id: Id,
    },
    RegisterProject {
        workspace_id: Id,
        id: Id,
        name: String,
        directory: String,
    },
    PreviewLegacyImport {
        workspace_id: Id,
        project_id: Id,
    },
    PrepareLegacyImport {
        workspace_id: Id,
        project_id: Id,
        expected_fingerprint: String,
    },
    ExecuteLegacyImport {
        workspace_id: Id,
        plan_id: Id,
    },
    GetLegacyImportReport {
        workspace_id: Id,
        plan_id: Id,
    },
    ReadImportHistory {
        key: SessionKey,
        offset: usize,
        limit: usize,
    },
    ReadImportResource {
        key: SessionKey,
        resource_id: Id,
        offset: u64,
    },
    RespondToolApproval {
        key: SessionKey,
        request_id: Id,
        expected_digest: String,
        allowed: bool,
    },
    CreateSession(CreateSession),
    ListSessions {
        workspace_id: Id,
    },
    GetSession {
        key: SessionKey,
    },
    PatchSession {
        key: SessionKey,
        expected_seq: u64,
        patch: SessionPatch,
    },
    SendMessage(SendMessage),
    CancelTurn {
        key: SessionKey,
        turn_id: Id,
    },
    ReadEvents {
        key: SessionKey,
        after: u64,
        limit: usize,
    },
    MarkRead {
        key: SessionKey,
        message_id: Id,
    },
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CoreRequest {
    pub protocol_version: u32,
    pub request_id: Id,
    pub command: CoreCommand,
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum CoreResponse {
    PermissionDocuments(Vec<PermissionDocument>),
    PolicySnapshot(serde_json::Value),
    Connections(Vec<ModelConnection>),
    Connection(Box<ModelConnection>),
    Workspaces(Vec<Workspace>),
    Workspace(Workspace),
    Projects(Vec<Project>),
    Project(Project),
    Sessions(Vec<SessionListEntry>),
    Session(Box<SessionView>),
    Send(SendResult),
    Events(EventPage),
    LegacyPreview(LegacyPreview),
    LegacyImportPlan(LegacyImportPlan),
    LegacyImportReport(LegacyImportReport),
    ImportHistory(ImportHistoryPage),
    ImportResource(ImportResourceChunk),
    ToolApproval(Box<ToolApproval>),
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Outcome {
    Success { result: CoreResponse },
    Failure { error: CoreError },
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreReply {
    pub protocol_version: u32,
    pub request_id: Option<Id>,
    #[serde(flatten)]
    pub outcome: Outcome,
}
impl CoreReply {
    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            request_id: None,
            outcome: Outcome::Failure {
                error: CoreError::invalid(message),
            },
        }
    }
}
impl Core {
    pub fn dispatch_json(&self, bytes: &[u8]) -> CoreReply {
        if bytes.len() > 8 * 1024 * 1024 {
            return CoreReply::invalid_request("request exceeds 8 MiB");
        }
        let value: serde_json::Value = match serde_json::from_slice(bytes) {
            Ok(value) => value,
            Err(error) => return CoreReply::invalid_request(error.to_string()),
        };
        let request_id = value
            .get("requestId")
            .and_then(|id| serde_json::from_value::<Id>(id.clone()).ok());
        match serde_json::from_value::<CoreRequest>(value) {
            Ok(request) => self.dispatch(request),
            Err(error) => {
                let mut reply = CoreReply::invalid_request(error.to_string());
                reply.request_id = request_id;
                reply
            }
        }
    }
    pub fn dispatch(&self, request: CoreRequest) -> CoreReply {
        let outcome = if request.protocol_version != PROTOCOL_VERSION {
            Err(CoreError::invalid("unsupported protocol version"))
        } else {
            self.execute(request.command)
        };
        CoreReply {
            protocol_version: PROTOCOL_VERSION,
            request_id: Some(request.request_id),
            outcome: match outcome {
                Ok(result) => Outcome::Success { result },
                Err(error) => Outcome::Failure { error },
            },
        }
    }
    fn execute(&self, command: CoreCommand) -> Result<CoreResponse> {
        Ok(match command {
            CoreCommand::PermissionDocuments { workspace_id } => {
                CoreResponse::PermissionDocuments(self.permission_documents(&workspace_id)?)
            }
            CoreCommand::ReadPolicySnapshot { key, turn_id } => {
                CoreResponse::PolicySnapshot(self.read_policy_snapshot(&key, &turn_id)?)
            }
            CoreCommand::RememberToolApproval {
                key,
                request_id,
                expected_digest,
            } => CoreResponse::ToolApproval(Box::new(self.respond_tool_approval_with_memory(
                &key,
                &request_id,
                &expected_digest,
                true,
                true,
            )?)),
            CoreCommand::ClearRememberedGrants { key } => {
                CoreResponse::Session(Box::new(self.clear_remembered_grants(&key)?))
            }
            CoreCommand::ListConnections => CoreResponse::Connections(self.list_connections()?),
            CoreCommand::GetConnection { id } => {
                CoreResponse::Connection(Box::new(self.get_connection(&id)?))
            }
            CoreCommand::ListWorkspaces => CoreResponse::Workspaces(self.list_workspaces()?),
            CoreCommand::CreateWorkspace { id, name } => {
                CoreResponse::Workspace(self.create_workspace(id, name)?)
            }
            CoreCommand::ListProjects { workspace_id } => {
                CoreResponse::Projects(self.list_projects(&workspace_id)?)
            }
            CoreCommand::RegisterProject {
                workspace_id,
                id,
                name,
                directory,
            } => CoreResponse::Project(self.register_project(
                workspace_id,
                id,
                name,
                Path::new(&directory),
            )?),
            CoreCommand::PreviewLegacyImport {
                workspace_id,
                project_id,
            } => {
                let project = self
                    .list_projects(&workspace_id)?
                    .into_iter()
                    .find(|project| project.id == project_id)
                    .ok_or_else(|| {
                        CoreError::missing("project is not registered in this workspace")
                    })?;
                CoreResponse::LegacyPreview(inspect_legacy_project(Path::new(&project.directory))?)
            }
            CoreCommand::PrepareLegacyImport {
                workspace_id,
                project_id,
                expected_fingerprint,
            } => CoreResponse::LegacyImportPlan(self.prepare_legacy_import(
                &workspace_id,
                &project_id,
                &expected_fingerprint,
            )?),
            CoreCommand::ExecuteLegacyImport {
                workspace_id,
                plan_id,
            } => CoreResponse::LegacyImportReport(
                self.execute_legacy_import(&workspace_id, &plan_id)?,
            ),
            CoreCommand::GetLegacyImportReport {
                workspace_id,
                plan_id,
            } => CoreResponse::LegacyImportReport(
                self.legacy_import_report(&workspace_id, &plan_id)?,
            ),
            CoreCommand::ReadImportHistory { key, offset, limit } => {
                CoreResponse::ImportHistory(self.read_import_history(&key, offset, limit)?)
            }
            CoreCommand::ReadImportResource {
                key,
                resource_id,
                offset,
            } => CoreResponse::ImportResource(self.read_import_resource(
                &key,
                &resource_id,
                offset,
            )?),
            CoreCommand::RespondToolApproval {
                key,
                request_id,
                expected_digest,
                allowed,
            } => CoreResponse::ToolApproval(Box::new(self.respond_tool_approval(
                &key,
                &request_id,
                &expected_digest,
                allowed,
            )?)),
            CoreCommand::CreateSession(input) => {
                CoreResponse::Session(Box::new(self.create_session(input)?))
            }
            CoreCommand::ListSessions { workspace_id } => {
                CoreResponse::Sessions(self.list_sessions(&workspace_id)?)
            }
            CoreCommand::GetSession { key } => {
                CoreResponse::Session(Box::new(self.get_session(&key)?))
            }
            CoreCommand::PatchSession {
                key,
                expected_seq,
                patch,
            } => CoreResponse::Session(Box::new(self.patch_session(&key, expected_seq, patch)?)),
            CoreCommand::SendMessage(input) => CoreResponse::Send(self.send_message(input)?),
            CoreCommand::CancelTurn { key, turn_id } => {
                CoreResponse::Session(Box::new(self.cancel_turn(&key, &turn_id)?))
            }
            CoreCommand::ReadEvents { key, after, limit } => {
                CoreResponse::Events(self.read_events(&key, after, limit)?)
            }
            CoreCommand::MarkRead { key, message_id } => {
                CoreResponse::Session(Box::new(self.mark_read(&key, message_id)?))
            }
        })
    }
}
