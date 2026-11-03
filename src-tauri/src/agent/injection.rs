//! Mode B: a local CLI runs against a model provider from `providers.json`
//! instead of its own login. Each adapter uses the switch its CLI already
//! reads, checked against Claude Code 2.1.317, Codex 0.166.0 and OpenCode
//! 2.1.2. The key travels only in `secret_environment`, never in args, and no
//! agent configuration file is written.
use super::{
    capabilities::{ADAPTER_CLAUDE, ADAPTER_CODEX, ADAPTER_OPENCODE},
    providers::{ProviderKind, ResolvedProvider},
    PreparedAgentInvocation,
};

/// Variable that carries the key to CLIs configured by variable name.
const KEY_ENV: &str = "LOOM_PROVIDER_API_KEY";
const PROVIDER_ID: &str = "loom";
/// Claude Code falls back to its own login when no key is set, which would
/// send that login to the provider; a keyless local provider gets this instead.
const CLAUDE_NO_KEY: &str = "loom-no-key";

/// Provider kinds each adapter can be pointed at; empty means mode B is
/// unsupported. Codex removed `wire_api = "chat"`, so it takes Responses only.
pub(super) fn provider_kinds(adapter: &str) -> &'static [ProviderKind] {
    match adapter {
        ADAPTER_CLAUDE => &[ProviderKind::Anthropic],
        ADAPTER_CODEX => &[ProviderKind::OpenaiResponses],
        ADAPTER_OPENCODE => &[
            ProviderKind::Anthropic,
            ProviderKind::OpenaiResponses,
            ProviderKind::OpenaiChat,
        ],
        _ => &[],
    }
}

/// The `--model` value an adapter needs for the provider's model.
pub(super) fn model_arg(adapter: &str, model: &str) -> String {
    if adapter == ADAPTER_OPENCODE {
        format!("{PROVIDER_ID}/{model}")
    } else {
        model.to_string()
    }
}

/// Adds the provider to an invocation that already carries `--model`.
pub(super) fn apply(
    adapter: &str,
    prepared: &mut PreparedAgentInvocation,
    provider: &ResolvedProvider,
    model: &str,
) -> Result<(), String> {
    match adapter {
        ADAPTER_CLAUDE => {
            claude(prepared, provider, model);
            Ok(())
        }
        ADAPTER_CODEX => codex(prepared, provider),
        ADAPTER_OPENCODE => opencode(prepared, provider, model),
        _ => Err("this agent does not support a model provider".into()),
    }
}

fn claude(prepared: &mut PreparedAgentInvocation, provider: &ResolvedProvider, model: &str) {
    prepared.environment_remove_prefixes = vec!["ANTHROPIC_".into(), "CLAUDE_CODE_USE_".into()];
    let environment = &mut prepared.environment;
    environment.insert("ANTHROPIC_BASE_URL".into(), provider.base_url.clone());
    // Background calls use these aliases rather than `--model`.
    for name in [
        "ANTHROPIC_DEFAULT_OPUS_MODEL",
        "ANTHROPIC_DEFAULT_SONNET_MODEL",
        "ANTHROPIC_DEFAULT_HAIKU_MODEL",
        "ANTHROPIC_SMALL_FAST_MODEL",
    ] {
        environment.insert(name.into(), model.into());
    }
    // The `env` block of the user's Claude settings fills only unset
    // variables, so empty values keep its token, headers and cloud switches
    // out of the run without editing that file.
    for name in [
        "ANTHROPIC_AUTH_TOKEN",
        "ANTHROPIC_CUSTOM_HEADERS",
        "CLAUDE_CODE_USE_BEDROCK",
        "CLAUDE_CODE_USE_VERTEX",
        "CLAUDE_CODE_USE_FOUNDRY",
    ] {
        environment.insert(name.into(), String::new());
    }
    prepared.secret_environment.insert(
        "ANTHROPIC_API_KEY",
        provider.api_key.as_deref().unwrap_or(CLAUDE_NO_KEY),
    );
}

fn codex(
    prepared: &mut PreparedAgentInvocation,
    provider: &ResolvedProvider,
) -> Result<(), String> {
    if prepared.args.first().map(String::as_str) != Some("exec") {
        return Err("Codex invocation has no exec subcommand".into());
    }
    let quoted = |value: &str| serde_json::Value::from(value).to_string();
    let mut config = vec![
        format!("model_provider={}", quoted(PROVIDER_ID)),
        format!("model_providers.{PROVIDER_ID}.name=\"Loom\""),
        format!(
            "model_providers.{PROVIDER_ID}.base_url={}",
            quoted(&provider.base_url)
        ),
        format!("model_providers.{PROVIDER_ID}.wire_api=\"responses\""),
    ];
    if let Some(key) = &provider.api_key {
        config.push(format!(
            "model_providers.{PROVIDER_ID}.env_key={}",
            quoted(KEY_ENV)
        ));
        prepared.secret_environment.insert(KEY_ENV, key);
    }
    let options = config
        .into_iter()
        .flat_map(|value| ["--config".to_string(), value]);
    prepared.args.splice(1..1, options);
    prepared.environment_remove_prefixes = vec!["OPENAI_".into()];
    Ok(())
}

fn opencode(
    prepared: &mut PreparedAgentInvocation,
    provider: &ResolvedProvider,
    model: &str,
) -> Result<(), String> {
    if prepared.args.first().map(String::as_str) != Some("run") {
        return Err("OpenCode invocation has no run subcommand".into());
    }
    // The shared background service would ignore this process's environment.
    prepared.args.insert(1, "--standalone".into());
    let (package, base_url) = match provider.kind {
        ProviderKind::Anthropic => ("@ai-sdk/anthropic", format!("{}/v1", provider.base_url)),
        ProviderKind::OpenaiResponses => ("@ai-sdk/openai", provider.base_url.clone()),
        ProviderKind::OpenaiChat => ("@ai-sdk/openai-compatible", provider.base_url.clone()),
    };
    let mut options = serde_json::json!({ "baseURL": base_url });
    if let Some(key) = &provider.api_key {
        options["apiKey"] = format!("{{env:{KEY_ENV}}}").into();
        prepared.secret_environment.insert(KEY_ENV, key);
    }
    let config = serde_json::json!({
        "provider": { PROVIDER_ID: {
            "npm": package,
            "name": "Loom",
            "options": options,
            "models": { model: { "name": model } },
        }},
    });
    prepared
        .environment
        .insert("OPENCODE_CONFIG_CONTENT".into(), config.to_string());
    Ok(())
}
