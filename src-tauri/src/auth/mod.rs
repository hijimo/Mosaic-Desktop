pub mod storage;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::protocol::error::{CodexError, ErrorCode};
use storage::{AuthDotJson, AuthStorageBackend, FileAuthStorage};

// ── AuthMode ─────────────────────────────────────────────────────

/// Account type for the current user.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthMode {
    ApiKey,
    Chatgpt,
}

// ── CodexAuth ────────────────────────────────────────────────────

/// Authentication mechanism used by the current user.
#[derive(Debug, Clone)]
pub enum CodexAuth {
    ApiKey(ApiKeyAuth),
    Chatgpt(ChatgptAuth),
}

#[derive(Debug, Clone)]
pub struct ApiKeyAuth {
    pub api_key: String,
}

#[derive(Debug, Clone)]
pub struct ChatgptAuth {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub account_id: Option<String>,
}

impl CodexAuth {
    pub fn auth_mode(&self) -> AuthMode {
        match self {
            CodexAuth::ApiKey(_) => AuthMode::ApiKey,
            CodexAuth::Chatgpt(_) => AuthMode::Chatgpt,
        }
    }

    /// Returns the bearer token for API requests.
    pub fn bearer_token(&self) -> &str {
        match self {
            CodexAuth::ApiKey(a) => &a.api_key,
            CodexAuth::Chatgpt(c) => &c.access_token,
        }
    }
}

// ── RefreshTokenError ────────────────────────────────────────────

#[derive(Debug)]
pub enum RefreshTokenError {
    /// Permanent failure — user must re-authenticate.
    Permanent(String),
    /// Transient failure — can retry.
    Transient(String),
}

impl std::fmt::Display for RefreshTokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Permanent(msg) => write!(f, "permanent auth error: {msg}"),
            Self::Transient(msg) => write!(f, "transient auth error: {msg}"),
        }
    }
}

// ── UnauthorizedRecovery ─────────────────────────────────────────

/// Strategy for recovering from a 401 Unauthorized response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnauthorizedRecovery {
    /// Refresh the token and retry.
    RefreshAndRetry,
    /// Cannot recover — surface the error.
    Fail,
}

// ── AuthManager ──────────────────────────────────────────────────

const REFRESH_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const REFRESH_TOKEN_URL_OVERRIDE_ENV_VAR: &str = "CODEX_REFRESH_TOKEN_URL_OVERRIDE";

/// Manages authentication state, token caching, and refresh logic.
pub struct AuthManager {
    codex_home: PathBuf,
    storage: Arc<dyn AuthStorageBackend>,
    auth: Mutex<Option<CodexAuth>>,
    http_client: reqwest::Client,
}

impl std::fmt::Debug for AuthManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthManager")
            .field("codex_home", &self.codex_home)
            .finish_non_exhaustive()
    }
}

impl AuthManager {
    pub fn new(codex_home: PathBuf) -> Self {
        let storage = Arc::new(FileAuthStorage::new(codex_home.clone()));
        Self {
            codex_home,
            storage,
            auth: Mutex::new(None),
            http_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
        }
    }

    #[cfg(test)]
    pub fn new_with_storage(codex_home: PathBuf, storage: Arc<dyn AuthStorageBackend>) -> Self {
        Self {
            codex_home,
            storage,
            auth: Mutex::new(None),
            http_client: reqwest::Client::new(),
        }
    }

    /// Initialize auth from stored credentials or environment.
    pub async fn initialize(&self) -> Result<(), CodexError> {
        // 1. Try loading from storage (auth.json)
        if let Ok(Some(auth_json)) = self.storage.load() {
            let auth = auth_from_stored(&auth_json)?;
            *self.auth.lock().await = Some(auth);
            return Ok(());
        }

        // 2. Try OPENAI_API_KEY env var
        if let Ok(key) = std::env::var("OPENAI_API_KEY") {
            if !key.trim().is_empty() {
                *self.auth.lock().await = Some(CodexAuth::ApiKey(ApiKeyAuth {
                    api_key: key.trim().to_string(),
                }));
                return Ok(());
            }
        }

        Ok(())
    }

    /// Get the current auth, if any.
    pub async fn auth(&self) -> Option<CodexAuth> {
        self.auth.lock().await.clone()
    }

    /// Set auth directly (e.g., from external token provider).
    pub async fn set_auth(&self, auth: CodexAuth) {
        *self.auth.lock().await = Some(auth);
    }

    /// Returns the bearer token, refreshing if needed.
    pub async fn bearer_token(&self) -> Result<String, CodexError> {
        let auth = self.auth.lock().await;
        match auth.as_ref() {
            Some(a) => Ok(a.bearer_token().to_string()),
            None => Err(CodexError::new(
                ErrorCode::ConfigurationError,
                "no authentication configured".to_string(),
            )),
        }
    }

    /// Determine recovery strategy for a 401 response.
    pub async fn unauthorized_recovery(&self) -> UnauthorizedRecovery {
        let auth = self.auth.lock().await;
        match auth.as_ref() {
            Some(CodexAuth::Chatgpt(c)) if c.refresh_token.is_some() => {
                UnauthorizedRecovery::RefreshAndRetry
            }
            _ => UnauthorizedRecovery::Fail,
        }
    }

    /// Attempt to refresh the ChatGPT access token.
    pub async fn refresh_token(&self) -> Result<(), RefreshTokenError> {
        let mut auth_guard = self.auth.lock().await;
        let auth = auth_guard
            .as_ref()
            .ok_or_else(|| RefreshTokenError::Permanent("no auth configured".into()))?;

        let refresh_token = match auth {
            CodexAuth::Chatgpt(c) => c
                .refresh_token
                .as_ref()
                .ok_or_else(|| RefreshTokenError::Permanent("no refresh token".into()))?,
            CodexAuth::ApiKey(_) => {
                return Err(RefreshTokenError::Permanent(
                    "API key auth does not support token refresh".into(),
                ));
            }
        };

        let url = std::env::var(REFRESH_TOKEN_URL_OVERRIDE_ENV_VAR)
            .unwrap_or_else(|_| REFRESH_TOKEN_URL.to_string());

        let resp = self
            .http_client
            .post(&url)
            .json(&serde_json::json!({
                "grant_type": "refresh_token",
                "refresh_token": refresh_token,
                "client_id": "app_codex",
            }))
            .send()
            .await
            .map_err(|e| RefreshTokenError::Transient(format!("HTTP error: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(if status.as_u16() == 400 || status.as_u16() == 401 {
                RefreshTokenError::Permanent(format!(
                    "refresh token rejected ({status}): {body}"
                ))
            } else {
                RefreshTokenError::Transient(format!("refresh failed ({status}): {body}"))
            });
        }

        #[derive(Deserialize)]
        struct RefreshResponse {
            access_token: String,
            refresh_token: Option<String>,
        }

        let refresh_resp: RefreshResponse = resp
            .json()
            .await
            .map_err(|e| RefreshTokenError::Transient(format!("parse error: {e}")))?;

        let old_auth = auth_guard.as_ref().unwrap();
        let new_auth = match old_auth {
            CodexAuth::Chatgpt(c) => CodexAuth::Chatgpt(ChatgptAuth {
                access_token: refresh_resp.access_token,
                refresh_token: refresh_resp.refresh_token.or(c.refresh_token.clone()),
                account_id: c.account_id.clone(),
            }),
            _ => unreachable!(),
        };

        // Persist to storage
        let auth_json = AuthDotJson {
            auth_mode: Some(AuthMode::Chatgpt),
            openai_api_key: None,
            access_token: Some(new_auth.bearer_token().to_string()),
            refresh_token: match &new_auth {
                CodexAuth::Chatgpt(c) => c.refresh_token.clone(),
                _ => None,
            },
        };
        let _ = self.storage.save(&auth_json);

        *auth_guard = Some(new_auth);
        Ok(())
    }
}

/// Convert stored auth.json into a CodexAuth.
fn auth_from_stored(json: &AuthDotJson) -> Result<CodexAuth, CodexError> {
    match json.auth_mode {
        Some(AuthMode::Chatgpt) | None if json.access_token.is_some() => {
            Ok(CodexAuth::Chatgpt(ChatgptAuth {
                access_token: json.access_token.clone().unwrap(),
                refresh_token: json.refresh_token.clone(),
                account_id: None,
            }))
        }
        _ => {
            let key = json
                .openai_api_key
                .as_ref()
                .ok_or_else(|| {
                    CodexError::new(
                        ErrorCode::ConfigurationError,
                        "auth.json has no API key or access token".to_string(),
                    )
                })?
                .clone();
            Ok(CodexAuth::ApiKey(ApiKeyAuth { api_key: key }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use storage::MemoryAuthStorage;

    #[tokio::test]
    async fn api_key_from_env() {
        std::env::set_var("OPENAI_API_KEY", "sk-test-key-123");
        let mgr = AuthManager::new_with_storage(
            PathBuf::from("/tmp/test-codex"),
            Arc::new(MemoryAuthStorage::new()),
        );
        mgr.initialize().await.unwrap();
        let token = mgr.bearer_token().await.unwrap();
        assert_eq!(token, "sk-test-key-123");
        std::env::remove_var("OPENAI_API_KEY");
    }

    #[tokio::test]
    async fn auth_from_storage() {
        let storage = Arc::new(MemoryAuthStorage::new());
        storage
            .save(&AuthDotJson {
                auth_mode: Some(AuthMode::ApiKey),
                openai_api_key: Some("sk-stored".into()),
                access_token: None,
                refresh_token: None,
            })
            .unwrap();

        let mgr = AuthManager::new_with_storage(PathBuf::from("/tmp"), storage);
        mgr.initialize().await.unwrap();
        assert_eq!(mgr.bearer_token().await.unwrap(), "sk-stored");
    }

    #[tokio::test]
    async fn chatgpt_auth_from_storage() {
        let storage = Arc::new(MemoryAuthStorage::new());
        storage
            .save(&AuthDotJson {
                auth_mode: Some(AuthMode::Chatgpt),
                openai_api_key: None,
                access_token: Some("chatgpt-token".into()),
                refresh_token: Some("refresh-token".into()),
            })
            .unwrap();

        let mgr = AuthManager::new_with_storage(PathBuf::from("/tmp"), storage);
        mgr.initialize().await.unwrap();
        let auth = mgr.auth().await.unwrap();
        assert_eq!(auth.auth_mode(), AuthMode::Chatgpt);
        assert_eq!(auth.bearer_token(), "chatgpt-token");
    }

    #[tokio::test]
    async fn unauthorized_recovery_for_chatgpt() {
        let mgr = AuthManager::new_with_storage(
            PathBuf::from("/tmp"),
            Arc::new(MemoryAuthStorage::new()),
        );
        mgr.set_auth(CodexAuth::Chatgpt(ChatgptAuth {
            access_token: "tok".into(),
            refresh_token: Some("ref".into()),
            account_id: None,
        }))
        .await;
        assert_eq!(
            mgr.unauthorized_recovery().await,
            UnauthorizedRecovery::RefreshAndRetry
        );
    }

    #[tokio::test]
    async fn unauthorized_recovery_for_api_key() {
        let mgr = AuthManager::new_with_storage(
            PathBuf::from("/tmp"),
            Arc::new(MemoryAuthStorage::new()),
        );
        mgr.set_auth(CodexAuth::ApiKey(ApiKeyAuth {
            api_key: "sk-x".into(),
        }))
        .await;
        assert_eq!(
            mgr.unauthorized_recovery().await,
            UnauthorizedRecovery::Fail
        );
    }

    #[tokio::test]
    async fn no_auth_returns_error() {
        let mgr = AuthManager::new_with_storage(
            PathBuf::from("/tmp"),
            Arc::new(MemoryAuthStorage::new()),
        );
        assert!(mgr.bearer_token().await.is_err());
    }

    #[test]
    fn auth_mode_serialization() {
        assert_eq!(
            serde_json::to_string(&AuthMode::ApiKey).unwrap(),
            "\"apikey\""
        );
        assert_eq!(
            serde_json::to_string(&AuthMode::Chatgpt).unwrap(),
            "\"chatgpt\""
        );
    }
}
