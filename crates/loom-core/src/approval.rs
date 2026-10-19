use crate::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Runtime-only input. Raw arguments are never serialized into approval events.
pub struct ToolApprovalSpec {
    pub request_id: Id,
    pub tool_call_id: Id,
    pub tool_name: String,
    pub input: Value,
    pub ttl_ms: u64,
    pub remember: Option<RememberOffer>,
}
/// Trusted Rust policy result, distinct from a user approval.
pub struct ToolPolicyDecision {
    pub allowed: bool,
    pub reason: String,
}

/// Remove known secrets before preview truncation, including object keys.
pub(crate) fn private_preview(value: &Value, secrets: &[String]) -> Value {
    fn clean(text: &str, secrets: &[String]) -> String {
        secrets
            .iter()
            .filter(|s| !s.is_empty())
            .fold(text.to_owned(), |text, secret| {
                text.replace(secret, "[credential redacted]")
            })
    }
    fn visit(value: &Value, secrets: &[String]) -> Value {
        match value {
            Value::String(s) => Value::String(clean(s, secrets)),
            Value::Array(items) => Value::Array(items.iter().map(|v| visit(v, secrets)).collect()),
            Value::Object(items) => {
                let mut fields = serde_json::Map::new();
                for (key, value) in items {
                    let key = clean(key, secrets);
                    let mut unique = key.clone();
                    let mut suffix = 1;
                    while fields.contains_key(&unique) {
                        unique = format!("{key}-{suffix}");
                        suffix += 1;
                    }
                    fields.insert(unique, visit(value, secrets));
                }
                Value::Object(fields)
            }
            value => value.clone(),
        }
    }
    approval_preview(&visit(value, secrets))
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    Pending,
    Allowed,
    Denied,
    Expired,
    Cancelled,
    Consumed,
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalResolution {
    Allow,
    AllowRemember,
    Deny,
    Expire,
    Cancel,
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolApproval {
    pub id: Id,
    pub turn_id: Id,
    pub tool_call_id: Id,
    pub tool_name: String,
    pub preview: Value,
    pub arguments_digest: String,
    pub binding_digest: String,
    pub working_directory: Option<String>,
    pub permission_mode: PermissionMode,
    pub policy_version: u64,
    pub created_at_ms: u64,
    pub expires_at_ms: u64,
    pub resolved_at_ms: Option<u64>,
    pub status: ApprovalStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remember: Option<RememberOffer>,
}

/// Approval audit deliberately omits file bodies, headers, environments and
/// credential-shaped values. The digest still binds the complete raw arguments.
pub(crate) fn approval_preview(value: &Value) -> Value {
    fn sensitive(value: &str) -> bool {
        let text = value.to_ascii_lowercase().replace(['-', '_'], "");
        [
            "password",
            "passwd",
            "secret",
            "token",
            "apikey",
            "authorization",
            "bearer ",
            "cookie",
            "privatekey",
            "credential",
        ]
        .iter()
        .any(|key| text.contains(&key.replace(['-', '_'], "")))
    }
    fn credential_value(value: &str) -> bool {
        sensitive(value)
            || value
                .split(|c: char| c.is_whitespace() || ['\'', '"', '=', ':', ',', '('].contains(&c))
                .any(|word| {
                    ["sk-", "ghp_", "github_pat_", "sk_live_", "sk_test_"]
                        .iter()
                        .any(|prefix| word.starts_with(prefix))
                })
    }
    fn visit(value: &Value, depth: usize) -> Value {
        if depth > 12 {
            return Value::String("[nested arguments omitted]".into());
        }
        match value {
            Value::Object(fields) => Value::Object(
                fields
                    .iter()
                    .take(100)
                    .map(|(key, value)| {
                        let hidden = sensitive(key)
                            || [
                                "content",
                                "body",
                                "text",
                                "prompt",
                                "env",
                                "environment",
                                "headers",
                                "stdin",
                                "new_string",
                                "old_string",
                                "patch",
                                "code",
                                "script",
                            ]
                            .contains(&key.to_ascii_lowercase().as_str());
                        (
                            key.chars().take(256).collect(),
                            if hidden {
                                Value::String("[content withheld; bound by digest]".into())
                            } else {
                                visit(value, depth + 1)
                            },
                        )
                    })
                    .collect(),
            ),
            Value::Array(items) => {
                Value::Array(items.iter().take(50).map(|v| visit(v, depth + 1)).collect())
            }
            Value::String(text) => Value::String(if credential_value(text) {
                "[sensitive value withheld]".into()
            } else {
                text.chars().take(2048).collect()
            }),
            other => other.clone(),
        }
    }
    let preview = visit(value, 0);
    if serde_json::to_vec(&preview)
        .map(|bytes| bytes.len())
        .unwrap_or(usize::MAX)
        > 64 * 1024
    {
        Value::String("[large arguments omitted; bound by digest]".into())
    } else {
        preview
    }
}

pub(crate) fn approval_binding(
    key: &SessionKey,
    turn_id: &Id,
    request_id: &Id,
    tool_call_id: &Id,
    tool_name: &str,
    arguments_digest: &str,
    binding: &RuntimeBinding,
) -> Result<String> {
    crate::projection::checksum(&(
        key,
        turn_id,
        request_id,
        tool_call_id,
        tool_name,
        arguments_digest,
        binding,
    ))
}

pub(crate) fn validate_approval_event(
    view: &SessionView,
    event: &EventEnvelope<SessionEvent>,
) -> Result<()> {
    let require_running = |turn_id: &Id| -> Result<&Turn> {
        view.turns
            .iter()
            .find(|turn| {
                &turn.id == turn_id
                    && turn.status == TurnStatus::Running
                    && view.active_turn_id.as_ref() == Some(turn_id)
            })
            .ok_or_else(|| CoreError::conflict("tool approval has no running owner"))
    };
    match &event.payload {
        SessionEvent::ToolApprovalRequested { approval } => {
            if view
                .approvals
                .values()
                .filter(|a| matches!(a.status, ApprovalStatus::Pending | ApprovalStatus::Allowed))
                .count()
                >= 32
            {
                return Err(CoreError::new(
                    ErrorCode::Busy,
                    "too many outstanding tool approvals",
                ));
            }
            let turn = require_running(&approval.turn_id)?;
            let binding = turn
                .binding
                .as_ref()
                .ok_or_else(|| CoreError::corrupt("tool approval lacks runtime binding"))?;
            if !matches!(
                binding.permission_mode,
                PermissionMode::Ask | PermissionMode::Guarded
            ) {
                return Err(CoreError::conflict(
                    "interactive approval requires Ask or Guarded mode",
                ));
            }
            crate::projection::validate_name(&approval.tool_name)?;
            if view.approvals.contains_key(&approval.id)
                || view
                    .approvals
                    .values()
                    .any(|a| a.tool_call_id == approval.tool_call_id)
                || view.tools.contains_key(&approval.tool_call_id)
                || approval.status != ApprovalStatus::Pending
                || approval.resolved_at_ms.is_some()
                || approval.created_at_ms > event.timestamp_ms
                || approval.expires_at_ms <= event.timestamp_ms
                || approval.expires_at_ms - approval.created_at_ms > 300_000
                || approval.expires_at_ms > MAX_SAFE_SEQUENCE
                || approval.arguments_digest.len() != 64
                || !approval
                    .arguments_digest
                    .bytes()
                    .all(|c| c.is_ascii_hexdigit())
                || approval.permission_mode != binding.permission_mode
                || approval.policy_version != binding.policy_version
                || approval.working_directory != binding.working_directory
                || approval.binding_digest
                    != crate::remember::remember_binding(
                        approval_binding(
                            &view.metadata.key,
                            &approval.turn_id,
                            &approval.id,
                            &approval.tool_call_id,
                            &approval.tool_name,
                            &approval.arguments_digest,
                            binding,
                        )?,
                        approval.remember.as_ref(),
                    )?
                || serde_json::to_vec(&approval.preview)
                    .map_err(|_| CoreError::invalid("invalid approval preview"))?
                    .len()
                    > 64 * 1024
            {
                return Err(CoreError::conflict(
                    "tool approval identity, binding, expiry or preview is invalid",
                ));
            }
        }
        SessionEvent::ToolApprovalResolved {
            request_id,
            resolution,
            remembered_grant,
        } => {
            let approval = view
                .approvals
                .get(request_id)
                .ok_or_else(|| CoreError::missing("tool approval not found"))?;
            if !matches!(
                approval.status,
                ApprovalStatus::Pending | ApprovalStatus::Allowed
            ) {
                return Err(CoreError::conflict("tool approval is already settled"));
            }
            if *resolution == ApprovalResolution::AllowRemember {
                let binding = require_running(&approval.turn_id)?
                    .binding
                    .as_ref()
                    .ok_or_else(|| CoreError::corrupt("missing approval binding"))?;
                let grant = remembered_grant
                    .as_ref()
                    .ok_or_else(|| CoreError::conflict("remembered grant is missing"))?;
                if &grant.id != request_id
                    || Some(&grant.offer) != approval.remember.as_ref()
                    || grant.context_digest != crate::remember::context_digest(binding)?
                    || grant.created_at_ms > event.timestamp_ms
                    || grant.created_at_ms < approval.created_at_ms
                    || grant.revoked_at_ms.is_some()
                {
                    return Err(CoreError::conflict(
                        "remembered grant does not match its approved scope",
                    ));
                }
            } else if remembered_grant.is_some() {
                return Err(CoreError::conflict(
                    "ordinary approval cannot create a remembered grant",
                ));
            }
            match resolution {
                ApprovalResolution::Allow
                | ApprovalResolution::AllowRemember
                | ApprovalResolution::Deny => {
                    require_running(&approval.turn_id)?;
                    if approval.status != ApprovalStatus::Pending
                        || event.timestamp_ms >= approval.expires_at_ms
                    {
                        return Err(CoreError::conflict("tool approval is no longer pending"));
                    }
                    if *resolution == ApprovalResolution::AllowRemember
                        && (approval.remember.is_none()
                            || view.remembered_grants.contains_key(request_id)
                            || view
                                .remembered_grants
                                .values()
                                .filter(|g| g.revoked_at_ms.is_none())
                                .count()
                                >= 1024)
                    {
                        return Err(CoreError::conflict(
                            "approval has no eligible remembered scope or grant limit reached",
                        ));
                    }
                }
                ApprovalResolution::Expire => {
                    if event.timestamp_ms < approval.expires_at_ms {
                        return Err(CoreError::conflict("tool approval has not expired"));
                    }
                }
                ApprovalResolution::Cancel => {}
            }
        }
        SessionEvent::ToolApprovalConsumed { request_id } => {
            let approval = view
                .approvals
                .get(request_id)
                .ok_or_else(|| CoreError::missing("tool approval not found"))?;
            require_running(&approval.turn_id)?;
            if approval.status != ApprovalStatus::Allowed
                || event.timestamp_ms >= approval.expires_at_ms
                || view.tools.contains_key(&approval.tool_call_id)
            {
                return Err(CoreError::conflict("tool approval cannot be consumed"));
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}

pub(crate) fn apply_approval_event(view: &mut SessionView, event: &EventEnvelope<SessionEvent>) {
    match &event.payload {
        SessionEvent::ToolApprovalRequested { approval } => {
            view.approvals.insert(approval.id.clone(), approval.clone());
        }
        SessionEvent::ToolApprovalResolved {
            request_id,
            resolution,
            remembered_grant,
        } => {
            let approval = view
                .approvals
                .get_mut(request_id)
                .expect("validated approval");
            approval.status = match resolution {
                ApprovalResolution::Allow | ApprovalResolution::AllowRemember => {
                    ApprovalStatus::Allowed
                }
                ApprovalResolution::Deny => ApprovalStatus::Denied,
                ApprovalResolution::Expire => ApprovalStatus::Expired,
                ApprovalResolution::Cancel => ApprovalStatus::Cancelled,
            };
            approval.resolved_at_ms = Some(event.timestamp_ms);
            if *resolution == ApprovalResolution::AllowRemember {
                view.remembered_grants.insert(
                    request_id.clone(),
                    remembered_grant.clone().expect("validated grant"),
                );
            }
        }
        SessionEvent::ToolApprovalConsumed { request_id } => {
            let approval = view
                .approvals
                .get_mut(request_id)
                .expect("validated approval");
            approval.status = ApprovalStatus::Consumed;
            approval.resolved_at_ms = Some(event.timestamp_ms);
            view.tools.insert(
                approval.tool_call_id.clone(),
                ToolCall {
                    id: approval.tool_call_id.clone(),
                    turn_id: approval.turn_id.clone(),
                    name: approval.tool_name.clone(),
                    input: approval.preview.clone(),
                    output: None,
                    status: ToolStatus::Running,
                },
            );
            let turn = view
                .turns
                .iter()
                .find(|t| t.id == approval.turn_id)
                .expect("validated turn");
            view.messages
                .iter_mut()
                .find(|m| m.id == turn.receipt.assistant_message_id)
                .expect("validated assistant")
                .parts
                .push(MessagePart::Tool {
                    tool_call_id: approval.tool_call_id.clone(),
                });
        }
        _ => unreachable!(),
    }
}
pub(crate) fn cancel_turn_approvals(view: &mut SessionView, turn_id: &Id, timestamp: u64) {
    for approval in view.approvals.values_mut().filter(|a| {
        &a.turn_id == turn_id
            && matches!(a.status, ApprovalStatus::Pending | ApprovalStatus::Allowed)
    }) {
        approval.status = ApprovalStatus::Cancelled;
        approval.resolved_at_ms = Some(timestamp);
    }
}
