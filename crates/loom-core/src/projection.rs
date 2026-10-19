use crate::*;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub(crate) fn checksum(value: &impl Serialize) -> Result<String> {
    // Normalize object key order across Rust and TS consumers.
    let value = serde_json::to_value(value).map_err(|e| CoreError::invalid(e.to_string()))?;
    let bytes = serde_json::to_vec(&value).map_err(|e| CoreError::invalid(e.to_string()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
pub(crate) fn stream_name(key: &SessionKey) -> String {
    format!("session/{}/{}", key.workspace_id, key.session_id)
}
pub(crate) fn validate_name(value: &str) -> Result<()> {
    if value.trim().is_empty() || value.len() > 1024 || value.chars().any(char::is_control) {
        return Err(CoreError::invalid(
            "name must contain 1–1024 bytes without control characters",
        ));
    }
    Ok(())
}
pub(crate) fn validate_selection(selection: &ConnectionSelection) -> Result<()> {
    for text in [&selection.model, &selection.thinking]
        .into_iter()
        .flatten()
    {
        validate_name(text)?;
    }
    Ok(())
}
fn validate_patch(patch: &SessionPatch, active: bool) -> Result<()> {
    if let Change::Set(title) = &patch.title {
        validate_name(title)?;
    }
    if let Some(connection) = &patch.connection {
        validate_selection(connection)?;
    }
    if patch.notes.as_ref().is_some_and(|s| s.len() > 64 * 1024) {
        return Err(CoreError::invalid("notes exceed 64 KiB"));
    }
    if let Some(labels) = &patch.labels {
        if labels.len() > 100
            || labels.iter().any(|s| s.trim().is_empty() || s.len() > 1024)
            || labels.iter().collect::<BTreeSet<_>>().len() != labels.len()
        {
            return Err(CoreError::invalid("invalid or duplicate labels"));
        }
    }
    if let Some(sources) = &patch.enabled_sources {
        if sources.len() > 100 || sources.iter().collect::<BTreeSet<_>>().len() != sources.len() {
            return Err(CoreError::invalid("invalid or duplicate Sources"));
        }
    }
    if active
        && (patch.connection.is_some()
            || patch.permission_mode.is_some()
            || patch.archived == Some(true))
    {
        return Err(CoreError::new(
            ErrorCode::Busy,
            "stop the active turn before archiving or changing execution configuration",
        ));
    }
    Ok(())
}
fn turn<'a>(view: &'a SessionView, id: &Id) -> Result<&'a Turn> {
    view.turns
        .iter()
        .find(|t| &t.id == id)
        .ok_or_else(|| CoreError::missing("turn not found"))
}
fn active_turn<'a>(view: &'a SessionView, id: &Id) -> Result<&'a Turn> {
    let turn = turn(view, id)?;
    if view.active_turn_id.as_ref() != Some(id)
        || !matches!(turn.status, TurnStatus::Running | TurnStatus::Cancelling)
    {
        return Err(CoreError::conflict(
            "event does not belong to the active turn",
        ));
    }
    Ok(turn)
}
pub(crate) fn validate(
    view: Option<&SessionView>,
    event: &EventEnvelope<SessionEvent>,
) -> Result<()> {
    if event.schema_version != SCHEMA_VERSION
        || event.seq == 0
        || event.seq > MAX_SAFE_SEQUENCE
        || event.timestamp_ms > MAX_SAFE_SEQUENCE
    {
        return Err(CoreError::invalid(
            "event version/sequence/timestamp is invalid",
        ));
    }
    if let SessionEvent::Created { metadata } = &event.payload {
        if view.is_some() || event.seq != 1 || event.stream != stream_name(&metadata.key) {
            return Err(CoreError::conflict(
                "session creation identity or sequence is invalid",
            ));
        }
        if let Some(title) = &metadata.title {
            validate_name(title)?;
        }
        validate_selection(&metadata.connection)?;
        if metadata.created_at_ms > event.timestamp_ms
            || (metadata.updated_at_ms < metadata.created_at_ms
                || metadata.updated_at_ms > event.timestamp_ms)
        {
            return Err(CoreError::invalid(
                "session creation timestamps are invalid",
            ));
        }
        return Ok(());
    }
    let view = view.ok_or_else(|| CoreError::corrupt("session journal has no creation event"))?;
    if event.seq != view.last_seq + 1 || event.stream != stream_name(&view.metadata.key) {
        return Err(CoreError::conflict(
            "session event sequence or identity mismatch",
        ));
    }
    match &event.payload {
        SessionEvent::Created { .. } => unreachable!(),
        SessionEvent::RememberedGrantsRevoked { grant_ids, .. } => {
            if grant_ids.is_empty()
                || grant_ids.iter().any(|id| {
                    view.remembered_grants
                        .get(id)
                        .is_none_or(|g| g.revoked_at_ms.is_some())
                })
            {
                return Err(CoreError::conflict(
                    "remembered authorization is not active",
                ));
            }
        }
        SessionEvent::ToolApprovalRequested { .. }
        | SessionEvent::ToolApprovalResolved { .. }
        | SessionEvent::ToolApprovalConsumed { .. } => {
            crate::approval::validate_approval_event(view, event)?
        }
        SessionEvent::HistoryImported { history } => {
            if view.last_seq != 1
                || view.imported_history.is_some()
                || !view.messages.is_empty()
                || !view.turns.is_empty()
                || view.metadata.permission_mode != PermissionMode::Explore
                || view.metadata.connection != ConnectionSelection::default()
                || !matches!(history.origin.kind.as_str(), "chat" | "task")
                || history.origin.imported_at_ms > event.timestamp_ms
                || !std::path::Path::new(&history.origin.project_directory).is_absolute()
                || [&history.manifest_digest, &history.origin.content_digest]
                    .iter()
                    .any(|hash| hash.len() != 64 || !hash.bytes().all(|c| c.is_ascii_hexdigit()))
            {
                return Err(CoreError::invalid(
                    "invalid historical import or non-empty target",
                ));
            }
        }
        SessionEvent::MetadataChanged { patch } => {
            validate_patch(patch, view.active_turn_id.is_some())?
        }
        SessionEvent::MessageAccepted {
            receipt,
            text,
            attachment_ids,
            configuration,
        } => {
            if view.metadata.archived {
                return Err(CoreError::conflict(
                    "restore the archived session before sending",
                ));
            }
            if text.trim().is_empty() && attachment_ids.is_empty() || text.len() > MAX_MESSAGE_BYTES
            {
                return Err(CoreError::invalid("message is empty or exceeds 1 MiB"));
            }
            if receipt.accepted_seq != event.seq
                || receipt.user_message_id == receipt.assistant_message_id
                || view.turns.iter().any(|t| {
                    t.receipt.client_request_id == receipt.client_request_id
                        || t.id == receipt.turn_id
                })
                || view.messages.iter().any(|m| {
                    m.id == receipt.user_message_id || m.id == receipt.assistant_message_id
                })
            {
                return Err(CoreError::conflict("duplicate or invalid send receipt"));
            }
            let request = SendMessage {
                key: view.metadata.key.clone(),
                client_request_id: receipt.client_request_id.clone(),
                text: text.clone(),
                attachment_ids: attachment_ids.clone(),
            };
            if checksum(&request)? != receipt.request_digest
                || configuration.permission_mode != view.metadata.permission_mode
                || configuration.connection != view.metadata.connection
            {
                return Err(CoreError::conflict(
                    "accepted message configuration/digest mismatch",
                ));
            }
            if let Some(connection) = &configuration.resolved_connection {
                connection.config.validate()?;
                if configuration.connection.connection_id.as_ref() != Some(&connection.id)
                    || connection.revision == 0
                    || connection.revision > MAX_SAFE_SEQUENCE
                {
                    return Err(CoreError::conflict(
                        "accepted connection snapshot does not match selection",
                    ));
                }
            }
        }
        SessionEvent::TurnPreparationFailed { turn_id, error } => {
            if view.active_turn_id.is_some()
                || view
                    .turns
                    .iter()
                    .find(|t| t.status == TurnStatus::Queued)
                    .is_none_or(|t| &t.id != turn_id)
                || error.is_empty()
                || error.len() > 16 * 1024
            {
                return Err(CoreError::conflict("preparation failure must belong to the first queued turn without an active owner"));
            }
        }
        SessionEvent::TurnStarted { turn_id, binding } => {
            if view.metadata.archived || view.active_turn_id.is_some() {
                return Err(CoreError::new(
                    ErrorCode::Busy,
                    "session cannot start another turn",
                ));
            }
            let next = view
                .turns
                .iter()
                .find(|t| t.status == TurnStatus::Queued)
                .ok_or_else(|| CoreError::missing("no queued turn"))?;
            if &next.id != turn_id {
                return Err(CoreError::conflict("turns must start in accepted order"));
            }
            validate_name(&binding.adapter)?;
            validate_selection(&binding.connection)?;
            let expected_snapshot = next
                .configuration
                .resolved_connection
                .as_ref()
                .map(checksum)
                .transpose()?;
            if binding.connection_snapshot_digest != expected_snapshot {
                return Err(CoreError::conflict(
                    "runtime model connection differs from accepted snapshot",
                ));
            }
            let requested = &next.configuration.connection;
            if let Some(connection) = &next.configuration.resolved_connection {
                let model = next
                    .configuration
                    .connection
                    .model
                    .as_ref()
                    .unwrap_or(&connection.config.default_model);
                if binding.connection.model.as_ref() != Some(model) {
                    return Err(CoreError::conflict(
                        "runtime model differs from accepted connection default/selection",
                    ));
                }
            }
            if binding.policy_version == 0
                || binding.policy_version > MAX_SAFE_SEQUENCE
                || binding
                    .policy_snapshot_digest
                    .as_ref()
                    .is_some_and(|digest| {
                        digest.len() != 64 || !digest.bytes().all(|c| c.is_ascii_hexdigit())
                    })
                || binding.permission_mode != next.configuration.permission_mode
                || requested
                    .connection_id
                    .as_ref()
                    .is_some_and(|id| binding.connection.connection_id.as_ref() != Some(id))
                || requested
                    .model
                    .as_ref()
                    .is_some_and(|model| binding.connection.model.as_ref() != Some(model))
                || requested
                    .thinking
                    .as_ref()
                    .is_some_and(|level| binding.connection.thinking.as_ref() != Some(level))
            {
                return Err(CoreError::conflict(
                    "runtime binding differs from the accepted configuration",
                ));
            }
        }
        SessionEvent::RuntimeApplied {
            turn_id,
            input_id,
            update,
        } => {
            let turn = active_turn(view, turn_id)?;
            if turn.status == TurnStatus::Cancelling {
                return Err(CoreError::conflict(
                    "turn is cancelling; late runtime output is rejected",
                ));
            }
            if turn.runtime_receipts.contains_key(input_id) {
                return Err(CoreError::conflict("runtime input is already committed"));
            }
            let message = view
                .messages
                .iter()
                .find(|m| m.id == turn.receipt.assistant_message_id)
                .ok_or_else(|| CoreError::corrupt("assistant message is missing"))?;
            match update {
                RuntimeUpdate::ToolPolicy {
                    tool_call_id,
                    name,
                    input,
                    arguments_digest,
                    reason,
                    authorization_ids,
                    ..
                } => {
                    validate_name(name)?;
                    if !authorization_ids.is_empty() {
                        let context = crate::remember::context_digest(
                            turn.binding
                                .as_ref()
                                .ok_or_else(|| CoreError::corrupt("missing runtime binding"))?,
                        )?;
                        if authorization_ids.len() > 32
                            || authorization_ids.iter().any(|id| {
                                view.remembered_grants.get(id).is_none_or(|g| {
                                    g.revoked_at_ms.is_some() || g.context_digest != context
                                })
                            })
                        {
                            return Err(CoreError::conflict(
                                "remembered tool admission references invalid grants",
                            ));
                        }
                    }
                    if view.tools.contains_key(tool_call_id)
                        || view
                            .approvals
                            .values()
                            .any(|a| &a.tool_call_id == tool_call_id)
                        || reason.len() > 4096
                        || arguments_digest.len() != 64
                        || !arguments_digest.bytes().all(|c| c.is_ascii_hexdigit())
                        || serde_json::to_vec(input)
                            .map_err(|_| CoreError::invalid("invalid tool preview"))?
                            .len()
                            > 64 * 1024
                    {
                        return Err(CoreError::invalid(
                            "invalid or duplicate policy tool record",
                        ));
                    }
                }
                RuntimeUpdate::Usage { usage } => {
                    if usage.input_tokens > MAX_SAFE_SEQUENCE
                        || usage.output_tokens > MAX_SAFE_SEQUENCE
                        || turn.usage.as_ref().is_some_and(|previous| {
                            usage.input_tokens < previous.input_tokens
                                || usage.output_tokens < previous.output_tokens
                        })
                    {
                        return Err(CoreError::invalid(
                            "runtime usage must be monotonic safe integer totals",
                        ));
                    }
                }
                RuntimeUpdate::TextDelta {
                    part_id,
                    text_kind,
                    delta,
                } => {
                    if delta.is_empty() || delta.len() > MAX_MESSAGE_BYTES {
                        return Err(CoreError::invalid("text delta is empty or oversized"));
                    }
                    if let Some(MessagePart::Text { kind, text, .. }) = message
                        .parts
                        .iter()
                        .find(|part| matches!(part, MessagePart::Text{id,..} if id==part_id))
                    {
                        if kind != text_kind || text.len() + delta.len() > 16 * 1024 * 1024 {
                            return Err(CoreError::invalid(
                                "text part kind changed or exceeds 16 MiB",
                            ));
                        }
                    }
                }
                RuntimeUpdate::ToolStarted {
                    tool_call_id,
                    name,
                    input,
                } => {
                    validate_name(name)?;
                    if view
                        .approvals
                        .values()
                        .any(|approval| &approval.tool_call_id == tool_call_id)
                    {
                        return Err(CoreError::conflict(
                            "approved tool must start through one-time approval consumption",
                        ));
                    }
                    if view.tools.contains_key(tool_call_id)
                        || serde_json::to_vec(input)
                            .map_err(|e| CoreError::invalid(e.to_string()))?
                            .len()
                            > MAX_MESSAGE_BYTES
                    {
                        return Err(CoreError::conflict(
                            "tool ID is duplicated or input is oversized",
                        ));
                    }
                }
                RuntimeUpdate::ToolFinished {
                    tool_call_id,
                    status,
                    output,
                } => {
                    let tool = view
                        .tools
                        .get(tool_call_id)
                        .ok_or_else(|| CoreError::missing("tool not found"))?;
                    if &tool.turn_id != turn_id
                        || tool.status != ToolStatus::Running
                        || *status == ToolStatus::Running
                    {
                        return Err(CoreError::conflict(
                            "tool result does not match a running tool",
                        ));
                    }
                    if serde_json::to_vec(output)
                        .map_err(|e| CoreError::invalid(e.to_string()))?
                        .len()
                        > MAX_MESSAGE_BYTES
                    {
                        return Err(CoreError::invalid(
                            "tool output exceeds 1 MiB; persist large artifacts separately",
                        ));
                    }
                }
            }
        }
        SessionEvent::TurnCancelRequested { turn_id } => {
            if !matches!(
                turn(view, turn_id)?.status,
                TurnStatus::Queued | TurnStatus::Running
            ) {
                return Err(CoreError::conflict(
                    "turn cannot be cancelled in its current state",
                ));
            }
        }
        SessionEvent::TurnFinished {
            turn_id,
            status,
            error,
        } => {
            let turn = active_turn(view, turn_id)?;
            if *status == FinishStatus::Completed
                && (turn.status == TurnStatus::Cancelling
                    || view.approvals.values().any(|approval| {
                        &approval.turn_id == turn_id
                            && matches!(
                                approval.status,
                                ApprovalStatus::Pending | ApprovalStatus::Allowed
                            )
                    })
                    || view
                        .tools
                        .values()
                        .any(|tool| &tool.turn_id == turn_id && tool.status == ToolStatus::Running))
            {
                return Err(CoreError::conflict(
                    "cancelled or unfinished tool turn cannot complete successfully",
                ));
            }
            if error.as_ref().is_some_and(|s| s.len() > 16 * 1024) {
                return Err(CoreError::invalid("error description exceeds 16 KiB"));
            }
        }
        SessionEvent::ReadThrough { message_id } => {
            if !view.messages.iter().any(|m| {
                &m.id == message_id
                    && m.role == Role::Assistant
                    && view
                        .turns
                        .iter()
                        .any(|t| t.id == m.turn_id && t.status.is_terminal())
            }) {
                return Err(CoreError::invalid(
                    "read cursor must identify a finished assistant message",
                ));
            }
            if let Some(previous) = view
                .metadata
                .last_read_message_id
                .as_ref()
                .and_then(|id| view.messages.iter().position(|message| &message.id == id))
            {
                let next = view
                    .messages
                    .iter()
                    .position(|message| &message.id == message_id)
                    .expect("validated read cursor");
                if next < previous {
                    return Err(CoreError::conflict("read cursor cannot move backwards"));
                }
            }
        }
    }
    Ok(())
}
pub(crate) fn apply(
    view: &mut Option<SessionView>,
    event: &EventEnvelope<SessionEvent>,
) -> Result<()> {
    validate(view.as_ref(), event)?;
    if let SessionEvent::Created { metadata } = &event.payload {
        let mut metadata = metadata.clone();
        metadata.updated_at_ms = event.timestamp_ms;
        *view = Some(SessionView {
            metadata,
            notes: String::new(),
            imported_history: None,
            approvals: Default::default(),
            remembered_grants: Default::default(),
            last_seq: event.seq,
            metadata_revision: event.seq,
            messages: Vec::new(),
            turns: Vec::new(),
            tools: Default::default(),
            active_turn_id: None,
        });
        return Ok(());
    }
    let view = view.as_mut().expect("validated creation");
    match &event.payload {
        SessionEvent::Created { .. } => unreachable!(),
        SessionEvent::RememberedGrantsRevoked { grant_ids, .. } => {
            for id in grant_ids {
                view.remembered_grants
                    .get_mut(id)
                    .expect("validated grant")
                    .revoked_at_ms = Some(event.timestamp_ms);
            }
        }
        SessionEvent::ToolApprovalRequested { .. }
        | SessionEvent::ToolApprovalResolved { .. }
        | SessionEvent::ToolApprovalConsumed { .. } => {
            crate::approval::apply_approval_event(view, event)
        }
        SessionEvent::HistoryImported { history } => {
            view.imported_history = Some(history.clone());
        }
        SessionEvent::MetadataChanged { patch } => {
            view.metadata_revision = event.seq;
            let metadata = &mut view.metadata;
            match &patch.title {
                Change::Keep => {}
                Change::Clear => metadata.title = None,
                Change::Set(title) => metadata.title = Some(title.clone()),
            }
            if let Some(value) = &patch.status_id {
                metadata.status_id = value.clone();
            }
            if let Some(value) = patch.archived {
                metadata.archived = value;
            }
            if let Some(value) = patch.flagged {
                metadata.flagged = value;
            }
            if let Some(value) = &patch.labels {
                metadata.labels = value.clone();
            }
            if let Some(value) = patch.permission_mode {
                metadata.permission_mode = value;
            }
            if let Some(value) = &patch.connection {
                metadata.connection = value.clone();
            }
            if let Some(value) = &patch.enabled_sources {
                metadata.enabled_sources = value.clone();
            }
            if let Some(value) = &patch.notes {
                view.notes = value.clone();
            }
        }
        SessionEvent::MessageAccepted {
            receipt,
            text,
            attachment_ids,
            configuration,
        } => {
            let mut parts = vec![MessagePart::Text {
                id: receipt.user_message_id.clone(),
                kind: TextKind::Text,
                text: text.clone(),
            }];
            parts.extend(
                attachment_ids
                    .iter()
                    .cloned()
                    .map(|attachment_id| MessagePart::Attachment { attachment_id }),
            );
            view.messages.push(Message {
                id: receipt.user_message_id.clone(),
                turn_id: receipt.turn_id.clone(),
                role: Role::User,
                parts,
                created_at_ms: event.timestamp_ms,
            });
            view.messages.push(Message {
                id: receipt.assistant_message_id.clone(),
                turn_id: receipt.turn_id.clone(),
                role: Role::Assistant,
                parts: Vec::new(),
                created_at_ms: event.timestamp_ms,
            });
            view.turns.push(Turn {
                id: receipt.turn_id.clone(),
                receipt: receipt.clone(),
                status: TurnStatus::Queued,
                configuration: configuration.clone(),
                accepted_at_ms: event.timestamp_ms,
                started_at_ms: None,
                ended_at_ms: None,
                binding: None,
                error: None,
                usage: None,
                runtime_receipts: Default::default(),
            });
        }
        SessionEvent::TurnPreparationFailed { turn_id, error } => {
            let turn = view.turns.iter_mut().find(|t| &t.id == turn_id).unwrap();
            turn.status = TurnStatus::Failed;
            turn.error = Some(error.clone());
            turn.ended_at_ms = Some(event.timestamp_ms);
        }
        SessionEvent::TurnStarted { turn_id, binding } => {
            let turn = view.turns.iter_mut().find(|t| &t.id == turn_id).unwrap();
            turn.status = TurnStatus::Running;
            turn.started_at_ms = Some(event.timestamp_ms);
            turn.binding = Some(binding.clone());
            view.active_turn_id = Some(turn_id.clone());
        }
        SessionEvent::RuntimeApplied {
            turn_id,
            input_id,
            update,
        } => {
            let turn = view.turns.iter_mut().find(|t| &t.id == turn_id).unwrap();
            turn.runtime_receipts.insert(
                input_id.clone(),
                RuntimeReceipt {
                    digest: checksum(update)?,
                    seq: event.seq,
                },
            );
            let message = view
                .messages
                .iter_mut()
                .find(|m| m.id == turn.receipt.assistant_message_id)
                .unwrap();
            match update {
                RuntimeUpdate::ToolPolicy {
                    tool_call_id,
                    name,
                    input,
                    allowed,
                    reason,
                    ..
                } => {
                    view.tools.insert(
                        tool_call_id.clone(),
                        ToolCall {
                            id: tool_call_id.clone(),
                            turn_id: turn_id.clone(),
                            name: name.clone(),
                            input: input.clone(),
                            output: if *allowed {
                                None
                            } else {
                                Some(serde_json::json!({"denied":true,"reason":reason}))
                            },
                            status: if *allowed {
                                ToolStatus::Running
                            } else {
                                ToolStatus::Denied
                            },
                        },
                    );
                    message.parts.push(MessagePart::Tool {
                        tool_call_id: tool_call_id.clone(),
                    });
                }
                RuntimeUpdate::Usage { usage } => turn.usage = Some(usage.clone()),
                RuntimeUpdate::TextDelta {
                    part_id,
                    text_kind,
                    delta,
                } => {
                    if let Some(MessagePart::Text { text, .. }) = message
                        .parts
                        .iter_mut()
                        .find(|p| matches!(p,MessagePart::Text{id,..} if id==part_id))
                    {
                        text.push_str(delta);
                    } else {
                        message.parts.push(MessagePart::Text {
                            id: part_id.clone(),
                            kind: *text_kind,
                            text: delta.clone(),
                        });
                    }
                }
                RuntimeUpdate::ToolStarted {
                    tool_call_id,
                    name,
                    input,
                } => {
                    view.tools.insert(
                        tool_call_id.clone(),
                        ToolCall {
                            id: tool_call_id.clone(),
                            turn_id: turn_id.clone(),
                            name: name.clone(),
                            input: input.clone(),
                            output: None,
                            status: ToolStatus::Running,
                        },
                    );
                    message.parts.push(MessagePart::Tool {
                        tool_call_id: tool_call_id.clone(),
                    });
                }
                RuntimeUpdate::ToolFinished {
                    tool_call_id,
                    status,
                    output,
                } => {
                    let tool = view.tools.get_mut(tool_call_id).unwrap();
                    tool.status = *status;
                    tool.output = Some(output.clone());
                }
            }
        }
        SessionEvent::TurnCancelRequested { turn_id } => {
            crate::approval::cancel_turn_approvals(view, turn_id, event.timestamp_ms);
            let turn = view.turns.iter_mut().find(|t| &t.id == turn_id).unwrap();
            if turn.status == TurnStatus::Queued {
                turn.status = TurnStatus::Cancelled;
                turn.ended_at_ms = Some(event.timestamp_ms);
            } else {
                turn.status = TurnStatus::Cancelling;
            }
        }
        SessionEvent::TurnFinished {
            turn_id,
            status,
            error,
        } => {
            crate::approval::cancel_turn_approvals(view, turn_id, event.timestamp_ms);
            let turn = view.turns.iter_mut().find(|t| &t.id == turn_id).unwrap();
            turn.status = (*status).into();
            turn.ended_at_ms = Some(event.timestamp_ms);
            turn.error = error.clone();
            for tool in view
                .tools
                .values_mut()
                .filter(|t| &t.turn_id == turn_id && t.status == ToolStatus::Running)
            {
                tool.status = ToolStatus::Interrupted;
            }
            view.active_turn_id = None;
        }
        SessionEvent::ReadThrough { message_id } => {
            view.metadata.last_read_message_id = Some(message_id.clone());
        }
    }
    view.last_seq = event.seq;
    view.metadata.updated_at_ms = view.metadata.updated_at_ms.max(event.timestamp_ms);
    Ok(())
}
pub(crate) fn summary(view: &SessionView) -> SessionSummary {
    let latest = view.messages.iter().rev().find(|m| {
        m.role == Role::Assistant
            && !m.parts.is_empty()
            && view
                .turns
                .iter()
                .any(|t| t.id == m.turn_id && t.status.is_terminal())
    });
    let preview = view
        .messages
        .iter()
        .find(|m| m.role == Role::User)
        .and_then(|m| {
            m.parts.iter().find_map(|p| match p {
                MessagePart::Text { text, .. } => Some(text),
                _ => None,
            })
        })
        .map(|s| s.chars().take(160).collect())
        .unwrap_or_default();
    SessionSummary {
        metadata: view.metadata.clone(),
        last_seq: view.last_seq,
        message_count: view.messages.len()
            + view
                .imported_history
                .as_ref()
                .map(|history| history.message_count)
                .unwrap_or(0),
        preview,
        active_turn_id: view.active_turn_id.clone(),
        queued_turn_count: view
            .turns
            .iter()
            .filter(|t| t.status == TurnStatus::Queued)
            .count(),
        has_unread: latest
            .is_some_and(|m| Some(&m.id) != view.metadata.last_read_message_id.as_ref()),
    }
}
