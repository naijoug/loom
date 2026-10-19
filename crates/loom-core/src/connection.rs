use crate::*;
use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionProvider {
    Claude,
    Pi,
    PiCompat,
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionAuth {
    ApiKey,
    BearerToken,
    None,
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EndpointApi {
    AnthropicMessages,
    OpenaiCompletions,
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MidStreamBehavior {
    Queue,
    Steer,
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectionConfig {
    pub name: String,
    pub provider: ConnectionProvider,
    pub auth: ConnectionAuth,
    pub endpoint: Option<String>,
    pub pi_provider: Option<String>,
    pub custom_api: Option<EndpointApi>,
    pub models: Vec<String>,
    pub default_model: String,
    pub supports_images: bool,
    pub mid_stream_behavior: MidStreamBehavior,
}
impl ConnectionConfig {
    pub fn validate(&self) -> Result<()> {
        crate::projection::validate_name(&self.name)?;
        crate::projection::validate_name(&self.default_model)?;
        if self.models.is_empty()
            || self.models.len() > 100
            || !self.models.contains(&self.default_model)
        {
            return Err(CoreError::invalid(
                "connection needs 1–100 models including its default",
            ));
        }
        let mut models = std::collections::BTreeSet::new();
        for model in &self.models {
            crate::projection::validate_name(model)?;
            if !models.insert(model) {
                return Err(CoreError::invalid("duplicate model ID"));
            }
        }
        match self.provider {
            ConnectionProvider::Claude => {
                if self.custom_api.is_some()
                    || self.pi_provider.is_some()
                    || self.auth == ConnectionAuth::None
                {
                    return Err(CoreError::invalid(
                        "Claude connection requires API key/bearer auth without Pi options",
                    ));
                }
            }
            ConnectionProvider::Pi => {
                let provider = self
                    .pi_provider
                    .as_ref()
                    .ok_or_else(|| CoreError::invalid("native Pi provider ID is required"))?;
                crate::projection::validate_name(provider)?;
                if self.custom_api.is_some() {
                    return Err(CoreError::invalid("use PiCompat for a custom protocol"));
                }
            }
            ConnectionProvider::PiCompat => {
                if self.endpoint.is_none()
                    || self.custom_api.is_none()
                    || self.pi_provider.is_some()
                {
                    return Err(CoreError::invalid(
                        "custom Pi connection requires endpoint/protocol and no native provider ID",
                    ));
                }
            }
        }
        if let Some(endpoint) = &self.endpoint {
            if endpoint.len() > 4096 {
                return Err(CoreError::invalid("endpoint exceeds 4 KiB"));
            }
            let url = url::Url::parse(endpoint)
                .map_err(|_| CoreError::invalid("invalid model endpoint URL"))?;
            let loopback = match url.host() {
                Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
                Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
                Some(url::Host::Domain(host)) => host == "localhost",
                _ => false,
            };
            if url.host().is_none()
                || !(url.scheme() == "https" || url.scheme() == "http" && loopback)
                || !url.username().is_empty()
                || url.password().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
            {
                return Err(CoreError::invalid("endpoint must use HTTPS (or loopback HTTP), without embedded credentials, query or fragment"));
            }
        }
        Ok(())
    }
    pub fn same_credential_destination(&self, other: &Self) -> bool {
        self.provider == other.provider
            && self.auth == other.auth
            && self.endpoint == other.endpoint
            && self.pi_provider == other.pi_provider
            && self.custom_api == other.custom_api
    }
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelConnection {
    pub id: Id,
    pub revision: u64,
    pub operation_id: Id,
    pub config: ConnectionConfig,
    pub credential_ref: Option<Id>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}
impl ModelConnection {
    pub fn snapshot_digest(&self) -> Result<String> {
        crate::projection::checksum(self)
    }
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveConnectionMetadata {
    pub id: Id,
    pub operation_id: Id,
    pub expected_revision: Option<u64>,
    pub config: ConnectionConfig,
    pub credential_ref: Option<Id>,
}

// Secret-bearing host input: intentionally neither Serialize nor Debug.
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CredentialChange {
    Keep,
    Replace {
        #[serde(deserialize_with = "secret_string")]
        value: String,
    },
    Clear,
}
fn secret_string<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<String, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::String(value) => Ok(value),
        _ => Err(serde::de::Error::custom("credential must be a string")),
    }
}
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveConnectionRequest {
    pub id: Id,
    pub operation_id: Id,
    pub expected_revision: Option<u64>,
    pub config: ConnectionConfig,
    pub credential: CredentialChange,
}
