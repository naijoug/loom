use super::{ChatInvocationSnapshot, ChatSession, ChatTurn, ChatTurnStatus};
use crate::{
    agent_adapter::PreparedAgentInvocation,
    agents::redact_sensitive_text,
    models::{now_ms, AgentConfig},
};

pub(crate) fn snapshot(
    agent: &AgentConfig,
    session: &ChatSession,
    prepared: &PreparedAgentInvocation,
    prompt: &str,
    fingerprint: &str,
) -> ChatInvocationSnapshot {
    let mut hide_next = false;
    let args = prepared
        .args
        .iter()
        .map(|arg| {
            if hide_next {
                hide_next = false;
                return "[REDACTED]".into();
            }
            let lower = arg.to_ascii_lowercase();
            if arg.starts_with('-')
                && [
                    "api-key",
                    "api_key",
                    "token",
                    "password",
                    "secret",
                    "authorization",
                ]
                .iter()
                .any(|key| lower.contains(key))
            {
                hide_next = !arg.contains('=');
                return if hide_next {
                    arg.clone()
                } else {
                    super::logs::redact(arg)
                };
            }
            let safe = if prompt.is_empty() {
                arg.clone()
            } else {
                arg.replace(prompt, "[PROMPT]")
            };
            super::logs::redact(&safe)
        })
        .collect();
    ChatInvocationSnapshot {
        agent_id: agent.id.clone(),
        adapter_type: agent.adapter_type.clone(),
        program: redact_sensitive_text(&prepared.program),
        args,
        cwd: prepared.cwd.clone(),
        permission_mode: session.permission_mode.clone(),
        stdin_prompt: prepared.stdin_prompt,
        output_mode: prepared.output_mode.clone(),
        config_fingerprint: fingerprint.into(),
    }
}

pub(super) fn new_turn(
    id: &str,
    request: &str,
    user: &str,
    assistant: &str,
    invocation: ChatInvocationSnapshot,
) -> ChatTurn {
    ChatTurn {
        id: id.into(),
        client_request_id: request.into(),
        user_message_id: user.into(),
        assistant_message_id: assistant.into(),
        invocation,
        status: ChatTurnStatus::Starting,
        accepted_at_ms: now_ms() as u64,
        started_at_ms: None,
        finished_at_ms: None,
        process_id: None,
        exit_code: None,
        termination_reason: None,
        error_summary: None,
        stdout_log_ref: format!("runs/{id}/stdout.log"),
        stderr_log_ref: format!("runs/{id}/stderr.log"),
    }
}

pub(super) fn validate(before: &ChatSession, after: &ChatSession) -> Result<(), String> {
    if after.turns.len() < before.turns.len() {
        return Err("chat turns cannot be removed".into());
    }
    let mut ids = std::collections::HashSet::new();
    for (index, turn) in after.turns.iter().enumerate() {
        if !ids.insert(&turn.id) {
            return Err("duplicate chat turn id".into());
        }
        if !after
            .messages
            .iter()
            .any(|m| m.id == turn.user_message_id && m.role == "user")
            || !after
                .messages
                .iter()
                .any(|m| m.id == turn.assistant_message_id && m.role == "assistant")
        {
            return Err("chat turn message association is invalid".into());
        }
        if turn.status.is_terminal() != turn.finished_at_ms.is_some() {
            return Err("chat turn terminal timestamp mismatch".into());
        }
        if let Some(old) = before.turns.get(index) {
            if old.id != turn.id
                || old.client_request_id != turn.client_request_id
                || old.user_message_id != turn.user_message_id
                || old.assistant_message_id != turn.assistant_message_id
                || old.invocation != turn.invocation
                || old.accepted_at_ms != turn.accepted_at_ms
                || old.stdout_log_ref != turn.stdout_log_ref
                || old.stderr_log_ref != turn.stderr_log_ref
                || (old.started_at_ms.is_some() && old.started_at_ms != turn.started_at_ms)
                || (old.process_id.is_some() && old.process_id != turn.process_id)
                || (old.status.is_terminal() && old != turn)
            {
                return Err("chat turn invocation or terminal record is immutable".into());
            }
            if (old.status == ChatTurnStatus::Running && turn.status == ChatTurnStatus::Starting)
                || (old.status == ChatTurnStatus::Cancelling
                    && matches!(
                        turn.status,
                        ChatTurnStatus::Starting | ChatTurnStatus::Running
                    ))
            {
                return Err("chat turn state cannot move backwards".into());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invocation_audit_masks_prompt_and_secret_options_without_changing_execution() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../../contracts/tauri-contract.json")).unwrap();
        let agent: AgentConfig =
            serde_json::from_value(fixture["modelSamples"]["agentConfig"].clone()).unwrap();
        let session: ChatSession =
            serde_json::from_value(fixture["modelSamples"]["chatSession"].clone()).unwrap();
        let prepared = PreparedAgentInvocation {
            program: "agent".into(),
            args: vec![
                "--token".into(),
                "private-token".into(),
                "--api-key=private-key".into(),
                "X-API-Key: private-header".into(),
                "--single=USER PROMPT".into(),
            ],
            cwd: "/project".into(),
            stdin_prompt: false,
            output_mode: "plain".into(),
            resumed: false,
        };
        let audit = snapshot(&agent, &session, &prepared, "USER PROMPT", "fingerprint");
        let encoded = serde_json::to_string(&audit).unwrap();
        for secret in [
            "private-token",
            "private-key",
            "private-header",
            "USER PROMPT",
        ] {
            assert!(!encoded.contains(secret));
        }
        assert_eq!(audit.args.last().unwrap(), "--single=[PROMPT]");
        assert_eq!(prepared.args[1], "private-token");
    }
}
