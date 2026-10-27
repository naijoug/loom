use serde::{Deserialize, Serialize};

pub const ADAPTER_CODEX: &str = "codex_cli";
pub const ADAPTER_CLAUDE: &str = "claude_code_cli";
pub const ADAPTER_OPENCODE: &str = "opencode_cli";
pub const ADAPTER_GROK: &str = "grok_cli";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCapabilities {
    pub structured_stream: bool,
    pub tool_events: bool,
    pub native_resume: bool,
    /// Accepts an appended system prompt for experimental inline cards.
    pub inline_cards: bool,
    /// Accepts `AdapterInvocationRequest::model` as a `--model` argument.
    #[serde(default)]
    pub model_selection: bool,
    pub modes: Vec<ModeCapability>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModeCapability {
    pub mode: String,
    pub policy: String,
}

pub fn capabilities(adapter: &str) -> AgentCapabilities {
    let policies = match adapter {
        ADAPTER_CODEX => [
            "--sandbox read-only; approval_policy=never",
            "--sandbox workspace-write; approval_policy=never",
            "--sandbox workspace-write; approval_policy=never",
        ],
        ADAPTER_CLAUDE | ADAPTER_GROK => [
            "--permission-mode plan",
            "--permission-mode acceptEdits",
            "--permission-mode auto",
        ],
        ADAPTER_OPENCODE => [
            "read tools only; edit/bash/task denied",
            "build; CLI permissions",
            "build; automatic permissions; external_directory=deny",
        ],
        _ => [
            "Custom CLI policy",
            "Custom CLI policy",
            "Custom CLI policy",
        ],
    };
    let structured = matches!(
        adapter,
        ADAPTER_CODEX | ADAPTER_CLAUDE | ADAPTER_GROK | ADAPTER_OPENCODE
    );
    AgentCapabilities {
        structured_stream: structured,
        tool_events: structured,
        native_resume: structured,
        inline_cards: adapter == ADAPTER_CLAUDE,
        model_selection: structured,
        modes: ["explore", "ask", "auto"]
            .into_iter()
            .zip(policies)
            .map(|(mode, policy)| ModeCapability {
                mode: mode.into(),
                policy: policy.into(),
            })
            .collect(),
    }
}
