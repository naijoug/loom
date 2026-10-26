//! Private provider credentials. Only ProviderList may cross the output IPC boundary.
use crate::skills::{
    error::{ErrorCode, Result, SkillsError},
    files,
    repository::Repository,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ProviderId {
    #[serde(rename = "openai")]
    OpenAi,
    #[serde(rename = "openrouter")]
    OpenRouter,
    #[serde(rename = "codex")]
    Codex,
    #[serde(rename = "claude-code")]
    ClaudeCode,
}
impl ProviderId {
    pub const ALL: [Self; 4] = [
        Self::OpenAi,
        Self::OpenRouter,
        Self::Codex,
        Self::ClaudeCode,
    ];
    pub fn remote(self) -> bool {
        matches!(self, Self::OpenAi | Self::OpenRouter)
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::OpenAi => "OpenAI",
            Self::OpenRouter => "OpenRouter",
            Self::Codex => "Local Codex",
            Self::ClaudeCode => "Local Claude Code",
        }
    }
    pub fn default_model(self) -> Option<&'static str> {
        match self {
            Self::OpenAi => Some("gpt-5"),
            Self::OpenRouter => Some("openai/gpt-5"),
            _ => None,
        }
    }
}

// Deliberately no Serialize/Debug for IPC input containing a key.
#[derive(Deserialize)]
#[serde(
    tag = "action",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum KeyChange {
    Keep,
    Set(String),
    Clear,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigInput {
    pub provider_id: ProviderId,
    pub expected_revision: u64,
    pub key: KeyChange,
    // None clears the explicit model and restores the provider default.
    pub model: Option<String>,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredProvider {
    api_key: Option<String>,
    model: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Store {
    schema_version: u32,
    revision: u64,
    providers: BTreeMap<ProviderId, StoredProvider>,
}
impl Default for Store {
    fn default() -> Self {
        Self {
            schema_version: 1,
            revision: 0,
            providers: BTreeMap::new(),
        }
    }
}
impl fmt::Debug for Store {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TranslationConfig")
            .field("schema_version", &self.schema_version)
            .field("revision", &self.revision)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KeySource {
    Environment,
    Stored,
    None,
    LocalCli,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderDescriptor {
    pub id: ProviderId,
    pub label: String,
    pub configured: bool,
    pub supports_api_key: bool,
    pub stored_key: bool,
    pub key_source: KeySource,
    pub model: Option<String>,
    pub stored_model: Option<String>,
    pub model_from_environment: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderList {
    pub revision: u64,
    pub providers: Vec<ProviderDescriptor>,
}

/// Injected per request/test; never mutate process-wide environment for fixtures.
#[derive(Default)]
pub struct RuntimeConfig {
    keys: BTreeMap<ProviderId, String>,
    models: BTreeMap<ProviderId, String>,
    local: BTreeMap<ProviderId, bool>,
}
impl RuntimeConfig {
    #[cfg(test)]
    pub(crate) fn fixture_local(mut self, id: ProviderId) -> Self {
        self.local.insert(id, true);
        self
    }
    pub fn capture() -> Result<Self> {
        let mut value = Self::default();
        for (id, key, model) in [
            (
                ProviderId::OpenAi,
                "OPENAI_API_KEY",
                "LOOM_SKILLS_OPENAI_MODEL",
            ),
            (
                ProviderId::OpenRouter,
                "OPENROUTER_API_KEY",
                "LOOM_SKILLS_OPENROUTER_MODEL",
            ),
        ] {
            if let Some(key) = environment(key)? {
                validate_key(&key)?;
                value.keys.insert(id, key);
            }
            if let Some(model) = environment(model)? {
                validate_model(&model)?;
                value.models.insert(id, model);
            }
        }
        for (id, command) in [
            (ProviderId::Codex, "codex"),
            (ProviderId::ClaudeCode, "claude"),
        ] {
            value.local.insert(
                id,
                crate::agents::config::resolve_executable(command).is_some(),
            );
        }
        Ok(value)
    }
}
fn environment(name: &str) -> Result<Option<String>> {
    match std::env::var(name) {
        Ok(v) => Ok((!v.trim().is_empty()).then(|| v.trim().to_owned())),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(_) => Err(SkillsError::invalid(
            "Translation environment contains a non-text value",
        )),
    }
}
fn validate_key(value: &str) -> Result<()> {
    if value.is_empty() || value.len() > 4096 || !value.bytes().all(|b| b.is_ascii_graphic()) {
        return Err(SkillsError::invalid(
            "Translation key must be a bounded nonempty token",
        ));
    }
    Ok(())
}
fn validate_model(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.:/".contains(&b))
    {
        return Err(SkillsError::invalid(
            "Translation model must be a bounded model identifier",
        ));
    }
    Ok(())
}
fn load(repo: &Repository) -> Result<Store> {
    let path = repo.root.join("translation-config.json");
    files::regular_file(&path)?;
    let stored: Store = if path.try_exists()? {
        files::read_json(&path)?
    } else {
        Store::default()
    };
    if stored.schema_version != 1 {
        return Err(SkillsError::recovery(
            "Unsupported translation configuration schema",
        ));
    }
    for (id, config) in &stored.providers {
        if let Some(key) = &config.api_key {
            if !id.remote() {
                return Err(SkillsError::recovery(
                    "Local translation provider cannot contain an API key",
                ));
            }
            validate_key(key)?;
        }
        if let Some(model) = &config.model {
            validate_model(model)?;
        }
    }
    Ok(stored)
}
fn project(stored: &Store, runtime: &RuntimeConfig) -> ProviderList {
    ProviderList {
        revision: stored.revision,
        providers: ProviderId::ALL
            .into_iter()
            .map(|id| {
                let entry = stored.providers.get(&id);
                let stored_key = entry.and_then(|e| e.api_key.as_ref()).is_some();
                let key_source = if !id.remote() {
                    KeySource::LocalCli
                } else if runtime.keys.contains_key(&id) {
                    KeySource::Environment
                } else if stored_key {
                    KeySource::Stored
                } else {
                    KeySource::None
                };
                ProviderDescriptor {
                    id,
                    label: id.label().into(),
                    configured: if id.remote() {
                        key_source != KeySource::None
                    } else {
                        runtime.local.get(&id).copied().unwrap_or(false)
                    },
                    supports_api_key: id.remote(),
                    stored_key,
                    key_source,
                    model: runtime
                        .models
                        .get(&id)
                        .cloned()
                        .or_else(|| entry.and_then(|e| e.model.clone()))
                        .or_else(|| id.default_model().map(str::to_owned)),
                    stored_model: entry.and_then(|e| e.model.clone()),
                    model_from_environment: runtime.models.contains_key(&id),
                }
            })
            .collect(),
    }
}
pub fn list(repo: &Repository, runtime: &RuntimeConfig) -> Result<ProviderList> {
    let _lease = repo.coordinator.enter()?;
    Ok(project(&load(repo)?, runtime))
}
pub fn save(
    repo: &Repository,
    input: ConfigInput,
    runtime: &RuntimeConfig,
) -> Result<ProviderList> {
    // Validate before taking admission or writing any file. No error includes a key.
    let model = input
        .model
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty());
    if let Some(model) = &model {
        validate_model(model)?;
    }
    if !input.provider_id.remote() && !matches!(input.key, KeyChange::Keep) {
        return Err(SkillsError::invalid(
            "Local translation providers do not accept API key changes",
        ));
    }
    if let KeyChange::Set(key) = &input.key {
        validate_key(key.trim())?;
    }
    let lease = repo.coordinator.enter()?;
    let _admission = crate::process_supervisor::supervisor()
        .begin_operation()
        .map_err(|e| SkillsError::new(ErrorCode::Updating, e))?;
    let writer = repo.coordinator.writer(&lease)?;
    let path = repo.root.join("translation-config.json");
    writer.check_write_set(std::slice::from_ref(&path))?;
    let mut stored = load(repo)?;
    if stored.revision != input.expected_revision {
        return Err(SkillsError::new(
            ErrorCode::Conflict,
            "Translation settings changed; reload before saving",
        ));
    }
    let config = stored.providers.entry(input.provider_id).or_default();
    config.model = model;
    match input.key {
        KeyChange::Keep => {}
        KeyChange::Clear => config.api_key = None,
        KeyChange::Set(key) => config.api_key = Some(key.trim().into()),
    }
    stored.revision = stored
        .revision
        .checked_add(1)
        .ok_or_else(|| SkillsError::recovery("Translation configuration revision overflow"))?;
    files::atomic_json(&path, &stored)?;
    Ok(project(&stored, runtime))
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;

/// Captured once for an operation; never serialized or returned to the frontend.
pub(super) struct ResolvedProvider {
    pub id: ProviderId,
    pub model: Option<String>,
    pub api_key: Option<String>,
}
pub(super) fn resolve(
    repo: &Repository,
    id: ProviderId,
    runtime: &RuntimeConfig,
) -> Result<ResolvedProvider> {
    let _lease = repo.coordinator.enter()?;
    let stored = load(repo)?;
    let view = project(&stored, runtime);
    let provider = view.providers.into_iter().find(|p| p.id == id).unwrap();
    if !provider.configured {
        return Err(SkillsError::new(
            ErrorCode::AuthRequired,
            "Translation provider is not configured or its local CLI is unavailable",
        ));
    }
    Ok(ResolvedProvider {
        id,
        model: provider.model,
        api_key: runtime
            .keys
            .get(&id)
            .cloned()
            .or_else(|| stored.providers.get(&id).and_then(|p| p.api_key.clone())),
    })
}
