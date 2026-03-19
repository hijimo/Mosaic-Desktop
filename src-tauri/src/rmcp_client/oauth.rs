//! OAuth credential management for MCP servers.
//! Stores credentials in MOSAIC_HOME/.credentials.json (file-based only).

use anyhow::Context;
use anyhow::Result;
use oauth2::AccessToken;
use oauth2::RefreshToken;
use oauth2::Scope;
use oauth2::TokenResponse;
use oauth2::basic::BasicTokenType;
use rmcp::transport::auth::AuthorizationManager;
use rmcp::transport::auth::OAuthTokenResponse;
use rmcp::transport::auth::VendorExtraTokenFields;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use serde_json::map::Map as JsonMap;
use sha2::Digest;
use sha2::Sha256;
use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;
use tokio::sync::Mutex;
use tracing::warn;

#[allow(dead_code)]
const REFRESH_SKEW_MILLIS: u64 = 30_000;
const FALLBACK_FILENAME: &str = ".credentials.json";
const MCP_SERVER_TYPE: &str = "http";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoredOAuthTokens {
    pub server_name: String,
    pub url: String,
    pub client_id: String,
    pub token_response: WrappedOAuthTokenResponse,
    #[serde(default)]
    pub expires_at: Option<u64>,
}

/// Wrap OAuthTokenResponse to allow for partial equality comparison.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WrappedOAuthTokenResponse(pub OAuthTokenResponse);

impl PartialEq for WrappedOAuthTokenResponse {
    fn eq(&self, other: &Self) -> bool {
        match (serde_json::to_string(self), serde_json::to_string(other)) {
            (Ok(s1), Ok(s2)) => s1 == s2,
            _ => false,
        }
    }
}

fn find_mosaic_home() -> Result<PathBuf> {
    if let Ok(home) = std::env::var("MOSAIC_HOME") {
        return Ok(PathBuf::from(home));
    }
    if let Ok(home) = std::env::var("CODEX_HOME") {
        return Ok(PathBuf::from(home));
    }
    let home = dirs::home_dir().context("cannot determine home directory")?;
    Ok(home.join(".mosaic"))
}

fn fallback_file_path() -> Result<PathBuf> {
    let mut path = find_mosaic_home()?;
    path.push(FALLBACK_FILENAME);
    Ok(path)
}

type FallbackFile = BTreeMap<String, FallbackTokenEntry>;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FallbackTokenEntry {
    server_name: String,
    server_url: String,
    client_id: String,
    access_token: String,
    #[serde(default)]
    expires_at: Option<u64>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    scopes: Vec<String>,
}

pub(crate) fn load_oauth_tokens(
    server_name: &str,
    url: &str,
) -> Result<Option<StoredOAuthTokens>> {
    load_oauth_tokens_from_file(server_name, url)
}

pub(crate) fn has_oauth_tokens(server_name: &str, url: &str) -> Result<bool> {
    Ok(load_oauth_tokens(server_name, url)?.is_some())
}

pub fn save_oauth_tokens(
    _server_name: &str,
    tokens: &StoredOAuthTokens,
) -> Result<()> {
    save_oauth_tokens_to_file(tokens)
}

pub fn delete_oauth_tokens(server_name: &str, url: &str) -> Result<bool> {
    let key = compute_store_key(server_name, url)?;
    delete_oauth_tokens_from_file(&key)
}

fn refresh_expires_in_from_timestamp(tokens: &mut StoredOAuthTokens) {
    let Some(expires_at) = tokens.expires_at else {
        return;
    };
    match expires_in_from_timestamp(expires_at) {
        Some(seconds) => {
            let duration = Duration::from_secs(seconds);
            tokens.token_response.0.set_expires_in(Some(&duration));
        }
        None => {
            tokens.token_response.0.set_expires_in(None);
        }
    }
}

fn load_oauth_tokens_from_file(server_name: &str, url: &str) -> Result<Option<StoredOAuthTokens>> {
    let Some(store) = read_fallback_file()? else {
        return Ok(None);
    };
    let key = compute_store_key(server_name, url)?;
    for entry in store.values() {
        let entry_key = compute_store_key(&entry.server_name, &entry.server_url)?;
        if entry_key != key {
            continue;
        }
        let mut token_response = OAuthTokenResponse::new(
            AccessToken::new(entry.access_token.clone()),
            BasicTokenType::Bearer,
            VendorExtraTokenFields(Default::default()),
        );
        if let Some(refresh) = entry.refresh_token.clone() {
            token_response.set_refresh_token(Some(RefreshToken::new(refresh)));
        }
        let scopes = entry.scopes.clone();
        if !scopes.is_empty() {
            token_response.set_scopes(Some(scopes.into_iter().map(Scope::new).collect()));
        }
        let mut stored = StoredOAuthTokens {
            server_name: entry.server_name.clone(),
            url: entry.server_url.clone(),
            client_id: entry.client_id.clone(),
            token_response: WrappedOAuthTokenResponse(token_response),
            expires_at: entry.expires_at,
        };
        refresh_expires_in_from_timestamp(&mut stored);
        return Ok(Some(stored));
    }
    Ok(None)
}

fn save_oauth_tokens_to_file(tokens: &StoredOAuthTokens) -> Result<()> {
    let key = compute_store_key(&tokens.server_name, &tokens.url)?;
    let mut store = read_fallback_file()?.unwrap_or_default();
    let token_response = &tokens.token_response.0;
    let expires_at = tokens
        .expires_at
        .or_else(|| compute_expires_at_millis(token_response));
    let refresh_token = token_response
        .refresh_token()
        .map(|token| token.secret().to_string());
    let scopes = token_response
        .scopes()
        .map(|s| s.iter().map(|s| s.to_string()).collect())
        .unwrap_or_default();
    let entry = FallbackTokenEntry {
        server_name: tokens.server_name.clone(),
        server_url: tokens.url.clone(),
        client_id: tokens.client_id.clone(),
        access_token: token_response.access_token().secret().to_string(),
        expires_at,
        refresh_token,
        scopes,
    };
    store.insert(key, entry);
    write_fallback_file(&store)
}

fn delete_oauth_tokens_from_file(key: &str) -> Result<bool> {
    let mut store = match read_fallback_file()? {
        Some(store) => store,
        None => return Ok(false),
    };
    let removed = store.remove(key).is_some();
    if removed {
        write_fallback_file(&store)?;
    }
    Ok(removed)
}

pub(crate) fn compute_expires_at_millis(response: &OAuthTokenResponse) -> Option<u64> {
    let expires_in = response.expires_in()?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0));
    let expiry = now.checked_add(expires_in)?;
    let millis = expiry.as_millis();
    if millis > u128::from(u64::MAX) {
        Some(u64::MAX)
    } else {
        Some(millis as u64)
    }
}

fn expires_in_from_timestamp(expires_at: u64) -> Option<u64> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0));
    let now_ms = now.as_millis() as u64;
    if expires_at <= now_ms {
        None
    } else {
        Some((expires_at - now_ms) / 1000)
    }
}

#[allow(dead_code)]
fn token_needs_refresh(expires_at: Option<u64>) -> bool {
    let Some(expires_at) = expires_at else {
        return false;
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_millis() as u64;
    now.saturating_add(REFRESH_SKEW_MILLIS) >= expires_at
}

fn compute_store_key(server_name: &str, server_url: &str) -> Result<String> {
    let mut payload = JsonMap::new();
    payload.insert("type".to_string(), Value::String(MCP_SERVER_TYPE.to_string()));
    payload.insert("url".to_string(), Value::String(server_url.to_string()));
    payload.insert("headers".to_string(), Value::Object(JsonMap::new()));
    let truncated = sha_256_prefix(&Value::Object(payload))?;
    Ok(format!("{server_name}|{truncated}"))
}

fn read_fallback_file() -> Result<Option<FallbackFile>> {
    let path = fallback_file_path()?;
    let contents = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(err).context(format!(
                "failed to read credentials file at {}",
                path.display()
            ));
        }
    };
    match serde_json::from_str::<FallbackFile>(&contents) {
        Ok(store) => Ok(Some(store)),
        Err(e) => Err(e).context(format!(
            "failed to parse credentials file at {}",
            path.display()
        )),
    }
}

fn write_fallback_file(store: &FallbackFile) -> Result<()> {
    let path = fallback_file_path()?;
    if store.is_empty() {
        if path.exists() {
            fs::remove_file(path)?;
        }
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let serialized = serde_json::to_string(store)?;
    fs::write(&path, serialized)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o600);
        fs::set_permissions(&path, perms)?;
    }
    Ok(())
}

fn sha_256_prefix(value: &Value) -> Result<String> {
    let serialized =
        serde_json::to_string(&value).context("failed to serialize MCP OAuth key payload")?;
    let mut hasher = Sha256::new();
    hasher.update(serialized.as_bytes());
    let digest = hasher.finalize();
    let hex = format!("{digest:x}");
    let truncated = &hex[..16];
    Ok(truncated.to_string())
}

// ── OAuthPersistor ──────────────────────────────────────────────

#[allow(dead_code)]
#[derive(Clone)]
pub(crate) struct OAuthPersistor {
    inner: Arc<OAuthPersistorInner>,
}

#[allow(dead_code)]
struct OAuthPersistorInner {
    server_name: String,
    url: String,
    authorization_manager: Arc<Mutex<AuthorizationManager>>,
    last_credentials: Mutex<Option<StoredOAuthTokens>>,
}

impl OAuthPersistor {
    pub(crate) fn new(
        server_name: String,
        url: String,
        authorization_manager: Arc<Mutex<AuthorizationManager>>,
        initial_credentials: Option<StoredOAuthTokens>,
    ) -> Self {
        Self {
            inner: Arc::new(OAuthPersistorInner {
                server_name,
                url,
                authorization_manager,
                last_credentials: Mutex::new(initial_credentials),
            }),
        }
    }

    pub(crate) async fn persist_if_needed(&self) -> Result<()> {
        let (client_id, maybe_credentials) = {
            let manager = self.inner.authorization_manager.clone();
            let guard = manager.lock().await;
            guard.get_credentials().await
        }?;

        match maybe_credentials {
            Some(credentials) => {
                let mut last_credentials = self.inner.last_credentials.lock().await;
                let new_token_response = WrappedOAuthTokenResponse(credentials.clone());
                let same_token = last_credentials
                    .as_ref()
                    .map(|prev| prev.token_response == new_token_response)
                    .unwrap_or(false);
                let expires_at = if same_token {
                    last_credentials.as_ref().and_then(|prev| prev.expires_at)
                } else {
                    compute_expires_at_millis(&credentials)
                };
                let stored = StoredOAuthTokens {
                    server_name: self.inner.server_name.clone(),
                    url: self.inner.url.clone(),
                    client_id,
                    token_response: new_token_response,
                    expires_at,
                };
                if last_credentials.as_ref() != Some(&stored) {
                    save_oauth_tokens(&self.inner.server_name, &stored)?;
                    *last_credentials = Some(stored);
                }
            }
            None => {
                let mut last_serialized = self.inner.last_credentials.lock().await;
                if last_serialized.take().is_some() {
                    if let Err(error) =
                        delete_oauth_tokens(&self.inner.server_name, &self.inner.url)
                    {
                        warn!(
                            "failed to remove OAuth tokens for server {}: {error}",
                            self.inner.server_name
                        );
                    }
                }
            }
        }
        Ok(())
    }

    pub(crate) async fn refresh_if_needed(&self) -> Result<()> {
        let expires_at = {
            let guard = self.inner.last_credentials.lock().await;
            guard.as_ref().and_then(|tokens| tokens.expires_at)
        };
        if !token_needs_refresh(expires_at) {
            return Ok(());
        }
        {
            let manager = self.inner.authorization_manager.clone();
            let guard = manager.lock().await;
            guard.refresh_token().await.with_context(|| {
                format!(
                    "failed to refresh OAuth tokens for server {}",
                    self.inner.server_name
                )
            })?;
        }
        self.persist_if_needed().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_store_key_is_deterministic() {
        let k1 = compute_store_key("srv", "https://example.com").unwrap();
        let k2 = compute_store_key("srv", "https://example.com").unwrap();
        assert_eq!(k1, k2);
    }

    #[test]
    fn different_urls_produce_different_keys() {
        let k1 = compute_store_key("srv", "https://a.com").unwrap();
        let k2 = compute_store_key("srv", "https://b.com").unwrap();
        assert_ne!(k1, k2);
    }
}
