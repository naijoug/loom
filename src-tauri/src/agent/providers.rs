//! Model providers for API-key runs: a private `providers.json` beside
//! `agents.json`. Keys stay inside this module and the run that resolves one;
//! IPC and CLI output only say whether a key exists and where it comes from.
use crate::storage::{self, private};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fmt,
    fs::{self, File, OpenOptions},
    io::Read,
    net::IpAddr,
    path::{Path, PathBuf},
    time::Duration,
};
use tauri::AppHandle;

const PROVIDERS_FILE: &str = "providers.json";
const PROVIDERS_LOCK: &str = "providers.lock";
const SCHEMA_VERSION: u32 = 1;
const MAX_STORE_BYTES: u64 = 1024 * 1024;
const MAX_TEST_RESPONSE_BYTES: usize = 1024 * 1024;
const LOCK_WAIT: Duration = Duration::from_millis(500);

/// Wire protocol of a provider. Base URLs follow each CLI's own convention:
/// Anthropic's excludes `/v1`, OpenAI's includes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderKind {
    Anthropic,
    OpenaiResponses,
    OpenaiChat,
}

impl ProviderKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Anthropic => "anthropic",
            Self::OpenaiResponses => "openai-responses",
            Self::OpenaiChat => "openai-chat",
        }
    }
}

// Deliberately no Serialize/Debug for input that can carry a key.
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

/// Creates the provider when `id` is new, otherwise replaces its settings.
// Deliberately no Serialize/Debug: `key` may hold a secret.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderInput {
    pub id: String,
    pub expected_revision: u64,
    pub name: String,
    pub kind: ProviderKind,
    pub base_url: String,
    pub model: Option<String>,
    pub key: KeyChange,
    /// Environment variable read at run time; it wins over a stored key.
    pub key_env: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredProvider {
    id: String,
    name: String,
    kind: ProviderKind,
    base_url: String,
    model: Option<String>,
    api_key: Option<String>,
    key_env: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Store {
    schema_version: u32,
    revision: u64,
    providers: Vec<StoredProvider>,
}
impl Default for Store {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            revision: 0,
            providers: Vec::new(),
        }
    }
}
impl fmt::Debug for Store {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProviderStore")
            .field("schema_version", &self.schema_version)
            .field("revision", &self.revision)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeySource {
    Environment,
    Stored,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderView {
    pub id: String,
    pub name: String,
    pub kind: ProviderKind,
    pub base_url: String,
    pub model: Option<String>,
    pub stored_key: bool,
    pub key_env: Option<String>,
    pub key_source: KeySource,
    /// A key is available, or the base URL is local and may not need one.
    pub configured: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderList {
    pub revision: u64,
    pub providers: Vec<ProviderView>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderTestOutcome {
    Ok,
    KeyMissing,
    AuthFailed,
    NotFound,
    RateLimited,
    HttpError,
    InvalidResponse,
    Timeout,
    Unreachable,
}

/// Classified result of one minimal request; never carries the upstream body.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderTestResult {
    pub outcome: ProviderTestOutcome,
    pub http_status: Option<u16>,
}

/// Reads environment variables named by providers. Production passes
/// `process_environment`; tests inject fixtures instead of mutating the process.
pub(crate) type Environment<'a> = &'a dyn Fn(&str) -> Option<String>;

pub(crate) fn process_environment(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

/// Captured once for a run; deliberately neither Debug nor Serialize.
pub(crate) struct ResolvedProvider {
    pub kind: ProviderKind,
    pub base_url: String,
    pub model: Option<String>,
    pub api_key: Option<String>,
}

pub(crate) fn list_at(dir: &Path, env: Environment) -> Result<ProviderList, String> {
    Ok(project(&load(dir)?, env))
}

pub(crate) fn save_at(
    dir: &Path,
    input: ProviderInput,
    env: Environment,
) -> Result<ProviderList, String> {
    // Validate before locking or writing. No error message includes the key.
    validate_id(&input.id)?;
    let name = validate_name(&input.name)?;
    let base_url = validate_base_url(&input.base_url)?;
    let model = input
        .model
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    if let Some(model) = &model {
        super::validate_model_name(model).map_err(|_| "Provider model name is invalid")?;
    }
    let key_env = input
        .key_env
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    if let Some(name) = &key_env {
        validate_env_name(name)?;
    }
    let key = match input.key {
        KeyChange::Set(key) => {
            let key = key.trim().to_owned();
            validate_key(&key)?;
            KeyChange::Set(key)
        }
        other => other,
    };

    update(dir, input.expected_revision, env, |store| {
        let index = match store.providers.iter().position(|p| p.id == input.id) {
            Some(index) => index,
            None => {
                store.providers.push(StoredProvider {
                    id: input.id.clone(),
                    name: String::new(),
                    kind: input.kind,
                    base_url: String::new(),
                    model: None,
                    api_key: None,
                    key_env: None,
                });
                store.providers.len() - 1
            }
        };
        let provider = &mut store.providers[index];
        provider.name = name;
        provider.kind = input.kind;
        provider.base_url = base_url;
        provider.model = model;
        provider.key_env = key_env;
        match key {
            KeyChange::Keep => {}
            KeyChange::Clear => provider.api_key = None,
            KeyChange::Set(key) => provider.api_key = Some(key),
        }
        Ok(())
    })
}

pub(crate) fn delete_at(
    dir: &Path,
    id: &str,
    expected_revision: u64,
    env: Environment,
) -> Result<ProviderList, String> {
    update(dir, expected_revision, env, |store| {
        let before = store.providers.len();
        store.providers.retain(|provider| provider.id != id);
        if store.providers.len() == before {
            return Err(format!("provider '{id}' not found"));
        }
        Ok(())
    })
}

pub(crate) fn resolve_at(
    dir: &Path,
    id: &str,
    env: Environment,
) -> Result<ResolvedProvider, String> {
    let store = load(dir)?;
    let provider = store
        .providers
        .iter()
        .find(|provider| provider.id == id)
        .ok_or_else(|| format!("provider '{id}' not found"))?;
    let api_key = provider
        .key_env
        .as_deref()
        .and_then(env)
        .or_else(|| provider.api_key.clone());
    Ok(ResolvedProvider {
        kind: provider.kind,
        base_url: provider.base_url.clone(),
        model: provider.model.clone(),
        api_key,
    })
}

/// Writers hold an exclusive lock across load → revision check → write, so a
/// desktop save and a CLI save cannot interleave.
fn update(
    dir: &Path,
    expected_revision: u64,
    env: Environment,
    change: impl FnOnce(&mut Store) -> Result<(), String>,
) -> Result<ProviderList, String> {
    let _lock = lock(dir)?;
    let mut store = load(dir)?;
    if store.revision != expected_revision {
        return Err("Provider settings changed; reload before saving".into());
    }
    change(&mut store)?;
    store.revision = store
        .revision
        .checked_add(1)
        .ok_or("provider settings revision overflow")?;
    let bytes = serde_json::to_vec_pretty(&store)
        .map_err(|_| "failed to encode provider settings".to_string())?;
    private::atomic_bytes(&dir.join(PROVIDERS_FILE), &bytes)
        .map_err(|error| format!("failed to write provider settings: {error}"))?;
    Ok(project(&store, env))
}

fn lock(dir: &Path) -> Result<File, String> {
    let path = dir.join(PROVIDERS_LOCK);
    private::regular_file(&path).map_err(|error| error.to_string())?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options
        .open(&path)
        .map_err(|error| format!("failed to open provider settings lock: {error}"))?;
    // A child spawned elsewhere in this process can hold an inherited copy of
    // the descriptor until its exec closes it, so a busy lock is retried
    // briefly before it is reported.
    let deadline = std::time::Instant::now() + LOCK_WAIT;
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(fs::TryLockError::WouldBlock) if std::time::Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(fs::TryLockError::WouldBlock) => {
                return Err("Another Loom process is saving provider settings; retry".into())
            }
            Err(other) => return Err(format!("failed to lock provider settings: {other}")),
        }
    }
}

fn load(dir: &Path) -> Result<Store, String> {
    let path = dir.join(PROVIDERS_FILE);
    private::regular_file(&path).map_err(|error| error.to_string())?;
    let file = match File::open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Store::default()),
        Err(error) => return Err(format!("failed to read provider settings: {error}")),
    };
    let mut bytes = Vec::new();
    file.take(MAX_STORE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("failed to read provider settings: {error}"))?;
    if bytes.len() as u64 > MAX_STORE_BYTES {
        return Err("provider settings exceed the size limit".into());
    }
    // Errors name the problem only: serde messages could quote a stored key.
    let store: Store = serde_json::from_slice(&bytes)
        .map_err(|_| "provider settings are malformed; refusing to treat them as empty")?;
    if store.schema_version != SCHEMA_VERSION {
        return Err("unsupported provider settings schema".into());
    }
    for provider in &store.providers {
        validate_id(&provider.id)?;
        validate_base_url(&provider.base_url)?;
        if let Some(key) = &provider.api_key {
            validate_key(key)?;
        }
        if let Some(name) = &provider.key_env {
            validate_env_name(name)?;
        }
    }
    Ok(store)
}

fn project(store: &Store, env: Environment) -> ProviderList {
    ProviderList {
        revision: store.revision,
        providers: store
            .providers
            .iter()
            .map(|provider| {
                let stored_key = provider.api_key.is_some();
                let key_source = if provider.key_env.as_deref().and_then(env).is_some() {
                    KeySource::Environment
                } else if stored_key {
                    KeySource::Stored
                } else {
                    KeySource::None
                };
                ProviderView {
                    id: provider.id.clone(),
                    name: provider.name.clone(),
                    kind: provider.kind,
                    base_url: provider.base_url.clone(),
                    model: provider.model.clone(),
                    stored_key,
                    key_env: provider.key_env.clone(),
                    key_source,
                    configured: key_source != KeySource::None || local_base_url(&provider.base_url),
                }
            })
            .collect(),
    }
}

fn validate_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > 64
        || !id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        || id.starts_with('-')
    {
        return Err("Provider id must be 1-64 lowercase letters, digits or '-'".into());
    }
    Ok(())
}

fn validate_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 64 || name.chars().any(char::is_control) {
        return Err("Provider name must be 1-64 printable characters".into());
    }
    Ok(name.to_owned())
}

fn validate_key(key: &str) -> Result<(), String> {
    if key.is_empty() || key.len() > 4096 || !key.bytes().all(|b| b.is_ascii_graphic()) {
        return Err("Provider key must be a bounded nonempty token".into());
    }
    Ok(())
}

fn validate_env_name(name: &str) -> Result<(), String> {
    let mut bytes = name.bytes();
    let valid_start = bytes
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_');
    if !valid_start || name.len() > 128 || !bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return Err("Provider key variable must be a valid environment variable name".into());
    }
    Ok(())
}

/// https everywhere; plain http only for hosts that stay on this machine or
/// network (local gateways such as magpie, Ollama or LM Studio).
fn validate_base_url(raw: &str) -> Result<String, String> {
    let raw = raw.trim();
    if raw.len() > 2048 {
        return Err("Base URL is too long".into());
    }
    let url = reqwest::Url::parse(raw).map_err(|_| "Base URL is not a valid URL")?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err("Base URL must not contain credentials".into());
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err("Base URL must not contain a query or fragment".into());
    }
    if url.host_str().is_none_or(str::is_empty) {
        return Err("Base URL must name a host".into());
    }
    match url.scheme() {
        "https" => {}
        "http" if local_url(&url) => {}
        "http" => {
            return Err(
                "Plain http is allowed only for loopback, private-network or .local hosts".into(),
            )
        }
        _ => return Err("Base URL must use https".into()),
    }
    Ok(url.as_str().trim_end_matches('/').to_owned())
}

pub(super) fn local_base_url(raw: &str) -> bool {
    reqwest::Url::parse(raw).is_ok_and(|url| local_url(&url))
}

fn local_url(url: &reqwest::Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    let host = host.to_ascii_lowercase();
    if host == "localhost" || host.ends_with(".localhost") || host.ends_with(".local") {
        return true;
    }
    match host.trim_start_matches('[').trim_end_matches(']').parse() {
        Ok(IpAddr::V4(ip)) => ip.is_loopback() || ip.is_private() || ip.is_link_local(),
        Ok(IpAddr::V6(ip)) => {
            ip.is_loopback() || ip.is_unique_local() || ip.is_unicast_link_local()
        }
        Err(_) => false,
    }
}

/// Client for one provider. Redirects are refused so a key is never sent to
/// another host, and a shell proxy must not intercept a gateway on this
/// machine or network.
pub(super) fn http_client(
    base_url: &str,
    timeout: Option<Duration>,
) -> reqwest::Result<reqwest::Client> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut builder = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10));
    if let Some(timeout) = timeout {
        builder = builder.timeout(timeout);
    }
    if local_base_url(base_url) {
        builder = builder.no_proxy();
    }
    builder.build()
}

/// Lists models: no tokens are spent and no model name is needed.
pub(crate) async fn test_provider(provider: &ResolvedProvider) -> ProviderTestResult {
    let client = match http_client(&provider.base_url, Some(Duration::from_secs(20))) {
        Ok(client) => client,
        Err(_) => return outcome(ProviderTestOutcome::Unreachable, None),
    };
    test_with_client(provider, &client).await
}

async fn test_with_client(
    provider: &ResolvedProvider,
    client: &reqwest::Client,
) -> ProviderTestResult {
    let local = local_base_url(&provider.base_url);
    let key = match provider.api_key.as_deref() {
        Some(key) => Some(key),
        None if local => None,
        None => return outcome(ProviderTestOutcome::KeyMissing, None),
    };
    let request = match provider.kind {
        ProviderKind::Anthropic => {
            let request = client
                .get(format!("{}/v1/models", provider.base_url))
                .header("anthropic-version", "2023-06-01");
            match key {
                Some(key) => match sensitive(key) {
                    Some(value) => request.header("x-api-key", value),
                    None => return outcome(ProviderTestOutcome::KeyMissing, None),
                },
                None => request,
            }
        }
        ProviderKind::OpenaiResponses | ProviderKind::OpenaiChat => {
            let request = client.get(format!("{}/models", provider.base_url));
            match key {
                Some(key) => match sensitive(&format!("Bearer {key}")) {
                    Some(value) => request.header(reqwest::header::AUTHORIZATION, value),
                    None => return outcome(ProviderTestOutcome::KeyMissing, None),
                },
                None => request,
            }
        }
    };
    let mut response = match request.send().await {
        Ok(response) => response,
        Err(error) => return transport_outcome(&error),
    };
    let status = response.status().as_u16();
    let classified = match status {
        200..=299 => None,
        401 | 403 => Some(ProviderTestOutcome::AuthFailed),
        404 => Some(ProviderTestOutcome::NotFound),
        429 => Some(ProviderTestOutcome::RateLimited),
        _ => Some(ProviderTestOutcome::HttpError),
    };
    if let Some(classified) = classified {
        return outcome(classified, Some(status));
    }
    let mut bytes = Vec::new();
    loop {
        match response.chunk().await {
            Ok(Some(chunk)) if bytes.len() + chunk.len() <= MAX_TEST_RESPONSE_BYTES => {
                bytes.extend_from_slice(&chunk)
            }
            Ok(Some(_)) => return outcome(ProviderTestOutcome::InvalidResponse, Some(status)),
            Ok(None) => break,
            Err(error) => return transport_outcome(&error),
        }
    }
    // Both protocols answer `{"data": [...]}`; anything else is not this API.
    let listed = serde_json::from_slice::<Value>(&bytes)
        .ok()
        .is_some_and(|value| value["data"].is_array());
    let result = if listed {
        ProviderTestOutcome::Ok
    } else {
        ProviderTestOutcome::InvalidResponse
    };
    outcome(result, Some(status))
}

pub(super) fn sensitive(value: &str) -> Option<reqwest::header::HeaderValue> {
    let mut header = reqwest::header::HeaderValue::from_str(value).ok()?;
    header.set_sensitive(true);
    Some(header)
}

fn transport_outcome(error: &reqwest::Error) -> ProviderTestResult {
    if error.is_timeout() {
        outcome(ProviderTestOutcome::Timeout, None)
    } else {
        outcome(ProviderTestOutcome::Unreachable, None)
    }
}

fn outcome(outcome: ProviderTestOutcome, http_status: Option<u16>) -> ProviderTestResult {
    ProviderTestResult {
        outcome,
        http_status,
    }
}

/// Deleting a provider an agent still uses would make that agent's runs fail.
pub(crate) fn ensure_unused(agents: &[crate::models::AgentConfig], id: &str) -> Result<(), String> {
    match agents
        .iter()
        .find(|agent| agent.provider_id.as_deref() == Some(id))
    {
        Some(agent) => Err(format!(
            "Provider is used by agent '{}'; remove it there first",
            agent.name
        )),
        None => Ok(()),
    }
}

fn config_dir<R: tauri::Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    storage::global_config_dir(app)
}

#[tauri::command]
pub fn provider_list(app: AppHandle) -> Result<ProviderList, String> {
    list_at(&config_dir(&app)?, &process_environment)
}

#[tauri::command]
pub fn provider_save(app: AppHandle, input: ProviderInput) -> Result<ProviderList, String> {
    save_at(&config_dir(&app)?, input, &process_environment)
}

#[tauri::command]
pub fn provider_delete(
    app: AppHandle,
    provider_id: String,
    expected_revision: u64,
) -> Result<ProviderList, String> {
    ensure_unused(&super::registry::load_agents(&app)?, &provider_id)?;
    delete_at(
        &config_dir(&app)?,
        &provider_id,
        expected_revision,
        &process_environment,
    )
}

#[tauri::command]
pub async fn provider_test(
    app: AppHandle,
    provider_id: String,
) -> Result<ProviderTestResult, String> {
    let provider = resolve_at(&config_dir(&app)?, &provider_id, &process_environment)?;
    Ok(test_provider(&provider).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::now_ms;
    use std::{
        io::Write,
        net::TcpListener,
        sync::{
            atomic::{AtomicU64, Ordering},
            mpsc,
        },
        thread,
    };

    const KEY: &str = "sk-fixture-secret-0123456789";
    static ROOT: AtomicU64 = AtomicU64::new(0);

    fn test_root(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "loom-providers-{label}-{}-{}",
            now_ms(),
            ROOT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }
    fn no_environment(_: &str) -> Option<String> {
        None
    }
    fn input(id: &str, revision: u64, base_url: &str, key: KeyChange) -> ProviderInput {
        ProviderInput {
            id: id.into(),
            expected_revision: revision,
            name: "Fixture".into(),
            kind: ProviderKind::OpenaiResponses,
            base_url: base_url.into(),
            model: Some("gpt-5".into()),
            key,
            key_env: None,
        }
    }

    #[test]
    fn stored_keys_stay_private_and_out_of_every_view() {
        let root = test_root("private");
        let list = save_at(
            &root,
            input(
                "openai",
                0,
                "https://api.openai.com/v1/",
                KeyChange::Set(format!(" {KEY} ")),
            ),
            &no_environment,
        )
        .unwrap();

        let view = &list.providers[0];
        assert_eq!(list.revision, 1);
        assert_eq!(view.base_url, "https://api.openai.com/v1");
        assert!(view.stored_key && view.configured);
        assert_eq!(view.key_source, KeySource::Stored);
        assert!(!serde_json::to_string(&list).unwrap().contains(KEY));
        let store = load(&root).unwrap();
        assert!(!format!("{store:?}").contains(KEY));
        assert_eq!(
            resolve_at(&root, "openai", &no_environment)
                .unwrap()
                .api_key
                .as_deref(),
            Some(KEY)
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |name| fs::metadata(root.join(name)).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(PROVIDERS_FILE), 0o600);
            assert_eq!(mode(PROVIDERS_LOCK), 0o600);
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn key_changes_keep_clear_and_prefer_the_named_environment_variable() {
        let root = test_root("keys");
        save_at(
            &root,
            input(
                "openai",
                0,
                "https://api.openai.com/v1",
                KeyChange::Set(KEY.into()),
            ),
            &no_environment,
        )
        .unwrap();
        let mut renamed = input("openai", 1, "https://api.openai.com/v1", KeyChange::Keep);
        renamed.key_env = Some("FIXTURE_PROVIDER_KEY".into());
        let environment =
            |name: &str| (name == "FIXTURE_PROVIDER_KEY").then(|| "env-key".to_string());
        let list = save_at(&root, renamed, &environment).unwrap();
        assert!(list.providers[0].stored_key);
        assert_eq!(list.providers[0].key_source, KeySource::Environment);
        assert_eq!(
            resolve_at(&root, "openai", &environment)
                .unwrap()
                .api_key
                .as_deref(),
            Some("env-key")
        );
        // With the variable unset, the stored key is the fallback.
        assert_eq!(
            list_at(&root, &no_environment).unwrap().providers[0].key_source,
            KeySource::Stored
        );

        let list = save_at(
            &root,
            input("openai", 2, "https://api.openai.com/v1", KeyChange::Clear),
            &no_environment,
        )
        .unwrap();
        assert!(!list.providers[0].stored_key);
        assert_eq!(list.providers[0].key_source, KeySource::None);
        assert!(!list.providers[0].configured);
        assert!(!fs::read_to_string(root.join(PROVIDERS_FILE))
            .unwrap()
            .contains(KEY));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn writers_check_the_revision_and_the_cross_process_lock() {
        let root = test_root("revision");
        let base = "https://api.openai.com/v1";
        save_at(&root, input("a", 0, base, KeyChange::Keep), &no_environment).unwrap();
        let stale = save_at(&root, input("b", 0, base, KeyChange::Keep), &no_environment);
        assert!(stale.unwrap_err().contains("changed"));

        let held = lock(&root).unwrap();
        let busy = save_at(&root, input("b", 1, base, KeyChange::Keep), &no_environment);
        assert!(busy.unwrap_err().contains("Another Loom process"));
        drop(held);

        let list = save_at(&root, input("b", 1, base, KeyChange::Keep), &no_environment).unwrap();
        assert_eq!(list.providers.len(), 2);
        assert!(delete_at(&root, "missing", 2, &no_environment)
            .unwrap_err()
            .contains("not found"));
        let list = delete_at(&root, "a", 2, &no_environment).unwrap();
        assert_eq!(list.revision, 3);
        assert_eq!(list.providers[0].id, "b");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_input_is_rejected_before_any_write() {
        let root = test_root("invalid");
        let base = "https://api.openai.com/v1";
        for bad in [
            input("Upper", 0, base, KeyChange::Keep),
            input("-lead", 0, base, KeyChange::Keep),
            input("ok", 0, base, KeyChange::Set("has space".into())),
            input("ok", 0, base, KeyChange::Set(String::new())),
        ] {
            assert!(save_at(&root, bad, &no_environment).is_err());
        }
        let mut bad_env = input("ok", 0, base, KeyChange::Keep);
        bad_env.key_env = Some("1BAD".into());
        assert!(save_at(&root, bad_env, &no_environment).is_err());
        let mut bad_model = input("ok", 0, base, KeyChange::Keep);
        bad_model.model = Some("--flag".into());
        assert!(save_at(&root, bad_model, &no_environment).is_err());
        assert!(!root.join(PROVIDERS_FILE).exists());
        assert!(!root.join(PROVIDERS_LOCK).exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn malformed_stores_are_refused_without_quoting_them() {
        let root = test_root("malformed");
        fs::write(
            root.join(PROVIDERS_FILE),
            format!(
                r#"{{"schemaVersion":1,"revision":1,"providers":[{{"id":"x","apiKey":"{KEY}"}}]}}"#
            ),
        )
        .unwrap();
        let error = list_at(&root, &no_environment).unwrap_err();
        assert!(error.contains("malformed") && !error.contains(KEY));
        fs::write(
            root.join(PROVIDERS_FILE),
            r#"{"schemaVersion":2,"revision":0,"providers":[]}"#,
        )
        .unwrap();
        assert!(list_at(&root, &no_environment)
            .unwrap_err()
            .contains("schema"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn base_urls_need_https_unless_the_host_is_local() {
        for (raw, expected) in [
            ("https://api.anthropic.com/", "https://api.anthropic.com"),
            ("http://127.0.0.1:8317/v1", "http://127.0.0.1:8317/v1"),
            ("http://localhost:11434/v1/", "http://localhost:11434/v1"),
            ("http://192.168.1.20:1234/v1", "http://192.168.1.20:1234/v1"),
            ("http://10.0.0.5/v1", "http://10.0.0.5/v1"),
            ("http://[::1]:8080", "http://[::1]:8080"),
            ("http://[fd00::1]/v1", "http://[fd00::1]/v1"),
            ("http://studio.local:1234/v1", "http://studio.local:1234/v1"),
        ] {
            assert_eq!(validate_base_url(raw).unwrap(), expected, "{raw}");
        }
        for raw in [
            "http://api.openai.com/v1",
            "http://8.8.8.8/v1",
            "http://[2001:db8::1]/v1",
            "ftp://127.0.0.1/",
            "https://user:pass@api.openai.com/v1",
            "https://api.openai.com/v1?key=1",
            "https://api.openai.com/v1#frag",
            "not a url",
            "file:///tmp/socket",
        ] {
            assert!(validate_base_url(raw).is_err(), "{raw}");
        }
        assert!(local_base_url("http://127.0.0.1:8317/v1"));
        assert!(!local_base_url("https://api.openai.com/v1"));
    }

    #[test]
    fn a_local_provider_without_a_key_counts_as_configured() {
        let root = test_root("local");
        let list = save_at(
            &root,
            input("ollama", 0, "http://127.0.0.1:11434/v1", KeyChange::Keep),
            &no_environment,
        )
        .unwrap();
        assert_eq!(list.providers[0].key_source, KeySource::None);
        assert!(list.providers[0].configured);
        fs::remove_dir_all(root).unwrap();
    }

    /// Serve one request and reply with `status` and `body`; the request head
    /// comes back through the receiver.
    fn server(status: u16, body: &'static str) -> (String, mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 4096];
            while !request.windows(4).any(|v| v == b"\r\n\r\n") {
                let n = socket.read(&mut buffer).unwrap();
                if n == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..n]);
            }
            let _ = tx.send(String::from_utf8_lossy(&request).into_owned());
            let response = format!(
                "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = socket.write_all(response.as_bytes());
        });
        (url, rx)
    }
    fn resolved(kind: ProviderKind, base_url: String, api_key: Option<&str>) -> ResolvedProvider {
        ResolvedProvider {
            kind,
            base_url,
            model: None,
            api_key: api_key.map(str::to_owned),
        }
    }
    fn client() -> reqwest::Client {
        let _ = rustls::crypto::ring::default_provider().install_default();
        reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap()
    }

    #[tokio::test]
    async fn connection_test_lists_models_with_each_protocols_credential() {
        let (url, request) = server(200, r#"{"data":[{"id":"gpt-5"}]}"#);
        let result = test_with_client(
            &resolved(ProviderKind::OpenaiChat, format!("{url}/v1"), Some(KEY)),
            &client(),
        )
        .await;
        assert_eq!(result.outcome, ProviderTestOutcome::Ok);
        assert_eq!(result.http_status, Some(200));
        let head = request.recv().unwrap().to_ascii_lowercase();
        assert!(head.starts_with("get /v1/models "));
        assert!(head.contains(&format!(
            "authorization: bearer {}",
            KEY.to_ascii_lowercase()
        )));

        let (url, request) = server(200, r#"{"data":[],"has_more":false}"#);
        let result = test_with_client(
            &resolved(ProviderKind::Anthropic, url, Some(KEY)),
            &client(),
        )
        .await;
        assert_eq!(result.outcome, ProviderTestOutcome::Ok);
        let head = request.recv().unwrap().to_ascii_lowercase();
        assert!(head.starts_with("get /v1/models "));
        assert!(head.contains(&format!("x-api-key: {}", KEY.to_ascii_lowercase())));
        assert!(head.contains("anthropic-version: 2023-06-01"));
        assert!(!head.contains("authorization:"));
    }

    #[tokio::test]
    async fn connection_test_classifies_failures_without_the_upstream_body() {
        for (status, body, expected) in [
            (
                401,
                r#"{"error":"bad key sk-echo"}"#,
                ProviderTestOutcome::AuthFailed,
            ),
            (404, "{}", ProviderTestOutcome::NotFound),
            (429, "{}", ProviderTestOutcome::RateLimited),
            (500, "{}", ProviderTestOutcome::HttpError),
            (
                200,
                "<html>gateway</html>",
                ProviderTestOutcome::InvalidResponse,
            ),
        ] {
            let (url, _request) = server(status, body);
            let result = test_with_client(
                &resolved(ProviderKind::OpenaiResponses, url, Some(KEY)),
                &client(),
            )
            .await;
            assert_eq!(result.outcome, expected, "{status}");
            assert_eq!(result.http_status, Some(status));
            assert!(!serde_json::to_string(&result).unwrap().contains("sk-echo"));
        }

        // A remote provider without a key never sends a request.
        let result = test_with_client(
            &resolved(
                ProviderKind::OpenaiResponses,
                "https://api.openai.com/v1".into(),
                None,
            ),
            &client(),
        )
        .await;
        assert_eq!(result.outcome, ProviderTestOutcome::KeyMissing);
        assert_eq!(result.http_status, None);

        let closed = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", closed.local_addr().unwrap());
        drop(closed);
        let result =
            test_with_client(&resolved(ProviderKind::OpenaiChat, url, None), &client()).await;
        assert_eq!(result.outcome, ProviderTestOutcome::Unreachable);
    }

    #[test]
    fn providers_used_by_an_agent_cannot_be_deleted() {
        let mut agent: crate::models::AgentConfig = serde_json::from_value(serde_json::json!({
            "id":"agent","name":"Claude","command":"claude","args":[],"workingDirectoryPolicy":"project_root","capabilities":[],"adapterType":"claude_code_cli","canWriteFiles":false,"canRunCommands":false,"enabled":true,"available":true,"providerId":"relay"
        }))
        .unwrap();
        assert_eq!(
            ensure_unused(std::slice::from_ref(&agent), "relay").unwrap_err(),
            "Provider is used by agent 'Claude'; remove it there first"
        );
        assert!(ensure_unused(std::slice::from_ref(&agent), "other").is_ok());
        agent.provider_id = None;
        assert!(ensure_unused(&[agent], "relay").is_ok());
    }
}
