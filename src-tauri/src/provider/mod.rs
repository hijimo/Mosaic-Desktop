use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::protocol::error::{CodexError, ErrorCode};

// ── Constants ────────────────────────────────────────────────────

const DEFAULT_REQUEST_MAX_RETRIES: u64 = 4;
const DEFAULT_STREAM_MAX_RETRIES: u64 = 5;
const DEFAULT_STREAM_IDLE_TIMEOUT_MS: u64 = 300_000;
const MAX_RETRIES_CAP: u64 = 100;

const OPENAI_PROVIDER_NAME: &str = "OpenAI";
pub const OLLAMA_PROVIDER_ID: &str = "ollama";
pub const LMSTUDIO_PROVIDER_ID: &str = "lmstudio";
pub const DEFAULT_OLLAMA_PORT: u16 = 11434;
pub const DEFAULT_LMSTUDIO_PORT: u16 = 1234;

/// Removed provider ID — kept to emit a helpful migration error.
pub const LEGACY_OLLAMA_CHAT_PROVIDER_ID: &str = "ollama-chat";
pub const OLLAMA_CHAT_PROVIDER_REMOVED_ERROR: &str =
    "`ollama-chat` is no longer supported.\n\
     How to fix: replace `ollama-chat` with `ollama` in `model_provider` or `--local-provider`.\n\
     More info: https://github.com/openai/codex/discussions/7782";

const CHAT_WIRE_API_REMOVED_ERROR: &str =
    "`wire_api = \"chat\"` is no longer supported.\n\
     How to fix: set `wire_api = \"responses\"` in your provider config.\n\
     More info: https://github.com/openai/codex/discussions/7782";

// ── WireApi ──────────────────────────────────────────────────────

/// Wire protocol the provider speaks. Only `responses` is supported.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum WireApi {
    #[default]
    Responses,
}

impl<'de> Deserialize<'de> for WireApi {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        match s.as_str() {
            "responses" => Ok(Self::Responses),
            "chat" => Err(serde::de::Error::custom(CHAT_WIRE_API_REMOVED_ERROR)),
            other => Err(serde::de::Error::unknown_variant(other, &["responses"])),
        }
    }
}

// ── ModelProviderInfo ────────────────────────────────────────────

/// User-facing provider definition. Serializable to/from TOML.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct ModelProviderInfo {
    pub name: String,
    pub base_url: Option<String>,
    pub env_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env_key_instructions: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub experimental_bearer_token: Option<String>,
    #[serde(default)]
    pub wire_api: WireApi,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query_params: Option<HashMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http_headers: Option<HashMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env_http_headers: Option<HashMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_max_retries: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_max_retries: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_idle_timeout_ms: Option<u64>,
    #[serde(default)]
    pub requires_openai_auth: bool,
    #[serde(default)]
    pub supports_websockets: bool,
}

impl ModelProviderInfo {
    // ── Accessors ────────────────────────────────────────────────

    pub fn request_max_retries(&self) -> u64 {
        self.request_max_retries
            .unwrap_or(DEFAULT_REQUEST_MAX_RETRIES)
            .min(MAX_RETRIES_CAP)
    }

    pub fn stream_max_retries(&self) -> u64 {
        self.stream_max_retries
            .unwrap_or(DEFAULT_STREAM_MAX_RETRIES)
            .min(MAX_RETRIES_CAP)
    }

    pub fn stream_idle_timeout(&self) -> Duration {
        Duration::from_millis(
            self.stream_idle_timeout_ms
                .unwrap_or(DEFAULT_STREAM_IDLE_TIMEOUT_MS),
        )
    }

    pub fn is_openai(&self) -> bool {
        self.name == OPENAI_PROVIDER_NAME
    }

    /// Resolve the API key from the environment.
    pub fn api_key(&self) -> Result<Option<String>, CodexError> {
        match &self.env_key {
            None => Ok(None),
            Some(var) => {
                let key = std::env::var(var)
                    .ok()
                    .filter(|v| !v.trim().is_empty())
                    .ok_or_else(|| {
                        let msg = match &self.env_key_instructions {
                            Some(instr) => format!("env var `{var}` not set. {instr}"),
                            None => format!("env var `{var}` not set"),
                        };
                        CodexError::new(ErrorCode::ConfigurationError, msg)
                    })?;
                Ok(Some(key))
            }
        }
    }

    /// Build the resolved HTTP headers map (static + env-sourced).
    pub fn resolved_headers(&self) -> HashMap<String, String> {
        let mut out = HashMap::new();
        if let Some(h) = &self.http_headers {
            out.extend(h.clone());
        }
        if let Some(env_h) = &self.env_http_headers {
            for (header, var) in env_h {
                if let Ok(val) = std::env::var(var) {
                    if !val.trim().is_empty() {
                        out.insert(header.clone(), val);
                    }
                }
            }
        }
        out
    }

    /// Convert to the runtime `Provider`.
    pub fn to_provider(&self) -> Provider {
        let base_url = self
            .base_url
            .clone()
            .unwrap_or_else(|| "https://api.openai.com/v1".to_string());

        Provider {
            name: self.name.clone(),
            base_url,
            query_params: self.query_params.clone(),
            headers: self.resolved_headers(),
            retry: RetryConfig {
                max_attempts: self.request_max_retries(),
                base_delay: Duration::from_millis(200),
                retry_429: false,
                retry_5xx: true,
                retry_transport: true,
            },
            stream_idle_timeout: self.stream_idle_timeout(),
        }
    }

    // ── Built-in constructors ────────────────────────────────────

    pub fn create_openai() -> Self {
        Self {
            name: OPENAI_PROVIDER_NAME.into(),
            base_url: std::env::var("OPENAI_BASE_URL")
                .ok()
                .filter(|v| !v.trim().is_empty()),
            env_key: None,
            env_key_instructions: None,
            experimental_bearer_token: None,
            wire_api: WireApi::Responses,
            query_params: None,
            // Inject the package version so the server can identify the client.
            http_headers: Some(HashMap::from([(
                "version".into(),
                env!("CARGO_PKG_VERSION").into(),
            )])),
            env_http_headers: Some(HashMap::from([
                ("OpenAI-Organization".into(), "OPENAI_ORGANIZATION".into()),
                ("OpenAI-Project".into(), "OPENAI_PROJECT".into()),
            ])),
            request_max_retries: None,
            stream_max_retries: None,
            stream_idle_timeout_ms: None,
            requires_openai_auth: true,
            supports_websockets: true,
        }
    }

    pub fn create_oss(base_url: &str) -> Self {
        Self {
            name: "gpt-oss".into(),
            base_url: Some(base_url.into()),
            env_key: None,
            env_key_instructions: None,
            experimental_bearer_token: None,
            wire_api: WireApi::Responses,
            query_params: None,
            http_headers: None,
            env_http_headers: None,
            request_max_retries: None,
            stream_max_retries: None,
            stream_idle_timeout_ms: None,
            requires_openai_auth: false,
            supports_websockets: false,
        }
    }
}

// ── Provider (runtime) ───────────────────────────────────────────

/// Runtime HTTP endpoint configuration used by the API client.
#[derive(Debug, Clone)]
pub struct Provider {
    pub name: String,
    pub base_url: String,
    pub query_params: Option<HashMap<String, String>>,
    /// Resolved static + env-sourced headers (header-name → value).
    pub headers: HashMap<String, String>,
    pub retry: RetryConfig,
    pub stream_idle_timeout: Duration,
}

impl Provider {
    /// Build the full URL for a given path, appending query params if present.
    pub fn url_for_path(&self, path: &str) -> String {
        let base = self.base_url.trim_end_matches('/');
        let path = path.trim_start_matches('/');
        let mut url = if path.is_empty() {
            base.to_string()
        } else {
            format!("{base}/{path}")
        };
        if let Some(params) = &self.query_params {
            if !params.is_empty() {
                let qs = params
                    .iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect::<Vec<_>>()
                    .join("&");
                url.push('?');
                url.push_str(&qs);
            }
        }
        url
    }

    /// Convert http/https scheme to ws/wss for WebSocket connections.
    pub fn websocket_url_for_path(&self, path: &str) -> Result<String, String> {
        let url = self.url_for_path(path);
        if url.starts_with("https://") {
            Ok(url.replacen("https://", "wss://", 1))
        } else if url.starts_with("http://") {
            Ok(url.replacen("http://", "ws://", 1))
        } else {
            Ok(url)
        }
    }

    /// Returns true if this provider is an Azure OpenAI endpoint.
    pub fn is_azure_responses_endpoint(&self) -> bool {
        is_azure_responses_wire_base_url(&self.name, Some(&self.base_url))
    }
}

/// Detect Azure OpenAI endpoints by name or URL pattern.
pub fn is_azure_responses_wire_base_url(name: &str, base_url: Option<&str>) -> bool {
    if name.eq_ignore_ascii_case("azure") {
        return true;
    }
    let Some(url) = base_url else { return false };
    let lower = url.to_ascii_lowercase();
    lower.contains("openai.azure.")
        || lower.contains("cognitiveservices.azure.")
        || lower.contains("aoai.azure.")
        || lower.contains("azure-api.")
        || lower.contains("azurefd.")
        || lower.contains("windows.net/openai")
}

// ── RetryConfig ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_attempts: u64,
    pub base_delay: Duration,
    /// Retry on HTTP 429 (rate limit).
    pub retry_429: bool,
    /// Retry on HTTP 5xx.
    pub retry_5xx: bool,
    /// Retry on transport-level errors.
    pub retry_transport: bool,
}

// ── Built-in provider registry ───────────────────────────────────

fn oss_base_url(default_port: u16) -> String {
    let default = format!(
        "http://localhost:{}/v1",
        std::env::var("CODEX_OSS_PORT")
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .unwrap_or(default_port)
    );
    std::env::var("CODEX_OSS_BASE_URL")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or(default)
}

/// Returns the built-in provider map (openai, ollama, lmstudio).
pub fn built_in_providers() -> HashMap<String, ModelProviderInfo> {
    [
        ("openai".to_string(), ModelProviderInfo::create_openai()),
        (
            OLLAMA_PROVIDER_ID.to_string(),
            ModelProviderInfo::create_oss(&oss_base_url(DEFAULT_OLLAMA_PORT)),
        ),
        (
            LMSTUDIO_PROVIDER_ID.to_string(),
            ModelProviderInfo::create_oss(&oss_base_url(DEFAULT_LMSTUDIO_PORT)),
        ),
    ]
    .into_iter()
    .collect()
}

/// Resolve a provider by ID, merging built-ins with user-defined overrides.
/// User-defined entries take precedence over built-ins.
/// Returns `Err` if the provider ID is a known-removed legacy ID.
pub fn resolve_provider(
    provider_id: &str,
    user_providers: &HashMap<String, ModelProviderInfo>,
) -> Result<Option<ModelProviderInfo>, &'static str> {
    if provider_id == LEGACY_OLLAMA_CHAT_PROVIDER_ID {
        return Err(OLLAMA_CHAT_PROVIDER_REMOVED_ERROR);
    }
    Ok(user_providers
        .get(provider_id)
        .cloned()
        .or_else(|| built_in_providers().remove(provider_id)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_for_path_appends_correctly() {
        let p = ModelProviderInfo::create_openai().to_provider();
        let url = p.url_for_path("/v1/responses");
        assert!(url.ends_with("/v1/responses"));
        assert!(!url.contains("//v1"));
    }

    #[test]
    fn url_for_path_with_query_params() {
        let mut info = ModelProviderInfo::create_openai();
        info.base_url = Some("https://example.com/openai".into());
        info.query_params = Some([("api-version".into(), "2025-04-01".into())].into());
        let p = info.to_provider();
        let url = p.url_for_path("responses");
        assert!(url.contains("api-version=2025-04-01"));
    }

    #[test]
    fn websocket_url_converts_scheme() {
        let mut info = ModelProviderInfo::create_openai();
        info.base_url = Some("https://api.openai.com/v1".into());
        let p = info.to_provider();
        assert_eq!(
            p.websocket_url_for_path("realtime").unwrap(),
            "wss://api.openai.com/v1/realtime"
        );

        let mut info2 = ModelProviderInfo::create_oss("http://localhost:11434/v1");
        info2.base_url = Some("http://localhost:11434/v1".into());
        let p2 = info2.to_provider();
        assert_eq!(
            p2.websocket_url_for_path("realtime").unwrap(),
            "ws://localhost:11434/v1/realtime"
        );
    }

    #[test]
    fn is_azure_responses_endpoint_detects_known_urls() {
        let mut info = ModelProviderInfo::create_openai();
        info.base_url = Some("https://foo.openai.azure.com/openai".into());
        assert!(info.to_provider().is_azure_responses_endpoint());

        let mut info2 = ModelProviderInfo::create_openai();
        info2.name = "Azure".into();
        info2.base_url = Some("https://example.com".into());
        assert!(info2.to_provider().is_azure_responses_endpoint());

        let p = ModelProviderInfo::create_openai().to_provider();
        assert!(!p.is_azure_responses_endpoint());
    }

    #[test]
    fn wire_api_chat_returns_error_with_link() {
        let toml = r#"name = "X"
wire_api = "chat""#;
        let err = toml::from_str::<ModelProviderInfo>(toml).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("no longer supported"));
        assert!(msg.contains("discussions/7782"));
    }

    #[test]
    fn built_in_providers_contains_expected_keys() {
        let providers = built_in_providers();
        assert!(providers.contains_key("openai"));
        assert!(providers.contains_key("ollama"));
        assert!(providers.contains_key("lmstudio"));
    }

    #[test]
    fn openai_provider_has_version_header() {
        let info = ModelProviderInfo::create_openai();
        let headers = info.http_headers.as_ref().unwrap();
        assert!(headers.contains_key("version"));
        assert!(!headers["version"].is_empty());
    }

    #[test]
    fn retry_defaults_and_cap() {
        let info = ModelProviderInfo::create_openai();
        assert_eq!(info.request_max_retries(), DEFAULT_REQUEST_MAX_RETRIES);
        assert_eq!(info.stream_max_retries(), DEFAULT_STREAM_MAX_RETRIES);

        let mut capped = info.clone();
        capped.request_max_retries = Some(999);
        assert_eq!(capped.request_max_retries(), MAX_RETRIES_CAP);
    }

    #[test]
    fn resolve_provider_user_overrides_builtin() {
        let mut user = HashMap::new();
        let mut custom = ModelProviderInfo::create_openai();
        custom.base_url = Some("https://my-proxy.com/v1".into());
        user.insert("openai".to_string(), custom.clone());

        let resolved = resolve_provider("openai", &user).unwrap().unwrap();
        assert_eq!(resolved.base_url, custom.base_url);
    }

    #[test]
    fn resolve_provider_legacy_ollama_chat_returns_error() {
        let user = HashMap::new();
        let result = resolve_provider(LEGACY_OLLAMA_CHAT_PROVIDER_ID, &user);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("ollama-chat"));
    }
}
