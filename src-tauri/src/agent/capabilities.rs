use super::providers::ProviderKind;
use serde::{Deserialize, Serialize};

pub const ADAPTER_CODEX: &str = "codex_cli";
pub const ADAPTER_CLAUDE: &str = "claude_code_cli";
pub const ADAPTER_OPENCODE: &str = "opencode_cli";
pub const ADAPTER_GROK: &str = "grok_cli";
/// Mode C: no executable; the prompt goes straight to a model provider.
pub const ADAPTER_API: &str = "api";

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
    /// True only after diagnostics recognizes an accepted CLI version/configuration.
    #[serde(default, skip_serializing_if = "is_false")]
    pub evidence_only: bool,
    /// Model provider kinds this CLI can run against instead of its own login.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provider_kinds: Vec<ProviderKind>,
    pub modes: Vec<ModeCapability>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModeCapability {
    pub mode: String,
    pub policy: String,
}

fn is_false(value: &bool) -> bool {
    !value
}

pub fn capabilities(adapter: &str) -> AgentCapabilities {
    if adapter == ADAPTER_API {
        return AgentCapabilities {
            structured_stream: false,
            tool_events: false,
            native_resume: false,
            inline_cards: false,
            model_selection: true,
            evidence_only: false,
            provider_kinds: vec![
                ProviderKind::Anthropic,
                ProviderKind::OpenaiResponses,
                ProviderKind::OpenaiChat,
            ],
            modes: vec![ModeCapability {
                mode: "explore".into(),
                policy: "no tools; text answers only".into(),
            }],
        };
    }
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
        evidence_only: false,
        provider_kinds: super::injection::provider_kinds(adapter).to_vec(),
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
