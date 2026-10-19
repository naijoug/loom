//! Native session identifiers are data, bound to the invocation that created them.
use super::{ChatResumeHandle, ChatSession};
use crate::{
    agent_adapter::{self, AgentStage},
    models::AgentConfig,
    session_capture,
};
use sha2::{Digest, Sha256};

#[derive(Debug)]
pub(crate) struct ResumeBinding {
    pub adapter_type: String,
    pub program: String,
    pub config_fingerprint: String,
}

pub(crate) fn binding(
    agent: &AgentConfig,
    session: &ChatSession,
    stage: AgentStage,
) -> ResumeBinding {
    let mut capabilities = agent.capabilities.clone();
    capabilities.sort();
    let config = serde_json::json!([
        "loom-chat-invocation-v1",
        agent.id,
        agent.adapter_type,
        agent.command.trim(),
        agent.args,
        agent.working_directory_policy,
        capabilities,
        agent.can_write_files,
        agent.can_run_commands,
        session.project_path,
        session.permission_mode,
        stage,
    ]);
    ResumeBinding {
        adapter_type: agent.adapter_type.clone(),
        program: agent.command.trim().into(),
        config_fingerprint: format!("{:x}", Sha256::digest(config.to_string().as_bytes())),
    }
}

pub(crate) fn resolve(
    session: &ChatSession,
    binding: &ResumeBinding,
) -> Result<Option<String>, String> {
    let Some(handle) = &session.resume_handle else {
        if session.resume_command.is_some() {
            return Err("旧续聊记录缺少执行配置校验。请在会话菜单选择“开新 CLI 会话”后重试；历史消息会保留。".into());
        }
        return Ok(None);
    };
    if handle.version != 1
        || handle.adapter_type != binding.adapter_type
        || handle.config_fingerprint != binding.config_fingerprint
        || !agent_adapter::valid_session_id(&handle.native_session_id)
    {
        return Err(
            "续聊配置已变化或句柄无效。请在会话菜单选择“开新 CLI 会话”后重试；历史消息会保留。"
                .into(),
        );
    }
    session_capture::resume_command_for_adapter(
        &binding.adapter_type,
        &binding.program,
        &handle.native_session_id,
    )
    .map(Some)
    .ok_or_else(|| "当前 Agent 不支持原生续聊，请开新 CLI 会话。".into())
}

pub(crate) fn capture(binding: &ResumeBinding, command: &str) -> Option<ChatResumeHandle> {
    let native_session_id =
        agent_adapter::parse_resume_session_id(&binding.adapter_type, &binding.program, command)
            .ok()?;
    Some(ChatResumeHandle {
        version: 1,
        adapter_type: binding.adapter_type.clone(),
        native_session_id,
        config_fingerprint: binding.config_fingerprint.clone(),
    })
}
