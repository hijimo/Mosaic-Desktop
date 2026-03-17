//! Coordinates model discovery, caching, and lookup.

use std::path::PathBuf;
use std::time::Duration;

use tokio::sync::RwLock;
use tracing::{error, info};

use crate::provider::ModelProviderInfo;

use super::cache::ModelsCacheManager;
use super::model_info::{ModelDescriptor, ModelsResponse};

const MODEL_CACHE_FILE: &str = "models_cache.json";
const DEFAULT_MODEL_CACHE_TTL: Duration = Duration::from_secs(300);

/// Strategy for refreshing available models.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshStrategy {
    /// Always fetch from the network, ignoring cache.
    Online,
    /// Only use cached data, never fetch from the network.
    Offline,
    /// Use cache if available and fresh, otherwise fetch.
    OnlineIfUncached,
}

/// Coordinates remote model discovery plus cached metadata on disk.
pub struct ModelsManager {
    models: RwLock<Vec<ModelDescriptor>>,
    etag: RwLock<Option<String>>,
    cache_manager: ModelsCacheManager,
    provider: ModelProviderInfo,
    mosaic_home: PathBuf,
}

impl ModelsManager {
    /// Create a new manager.
    ///
    /// If `initial_catalog` is provided, it seeds the model list.
    /// Otherwise starts with an empty list.
    pub fn new(
        mosaic_home: PathBuf,
        provider: ModelProviderInfo,
        initial_catalog: Option<ModelsResponse>,
    ) -> Self {
        let cache_path = mosaic_home.join(MODEL_CACHE_FILE);
        let cache_manager = ModelsCacheManager::new(cache_path, DEFAULT_MODEL_CACHE_TTL);
        let models = initial_catalog
            .map(|c| c.models)
            .unwrap_or_default();
        Self {
            models: RwLock::new(models),
            etag: RwLock::new(None),
            cache_manager,
            provider,
            mosaic_home,
        }
    }

    /// List all available models, refreshing according to the specified strategy.
    pub async fn list_models(&self, strategy: RefreshStrategy) -> Vec<ModelDescriptor> {
        if let Err(e) = self.refresh(strategy).await {
            error!("failed to refresh models: {e}");
        }
        let mut models = self.models.read().await.clone();
        models.sort_by_key(|m| m.priority);
        models
    }

    /// Look up model metadata by slug. Falls back to a minimal descriptor.
    pub async fn get_model_info(&self, slug: &str) -> ModelDescriptor {
        let models = self.models.read().await;
        find_by_longest_prefix(slug, &models)
            .unwrap_or_else(|| ModelDescriptor::fallback(slug))
    }

    /// Get the default model slug.
    pub async fn get_default_model(&self, explicit: &Option<String>, strategy: RefreshStrategy) -> String {
        if let Some(m) = explicit {
            return m.clone();
        }
        let models = self.list_models(strategy).await;
        models
            .iter()
            .find(|m| m.is_default)
            .or_else(|| models.first())
            .map(|m| m.slug.clone())
            .unwrap_or_default()
    }

    /// Refresh if the provided ETag differs from the cached one.
    pub async fn refresh_if_new_etag(&self, etag: String) {
        let current = self.etag.read().await.clone();
        if current.as_deref() == Some(etag.as_str()) {
            if let Err(e) = self.cache_manager.renew_cache_ttl().await {
                error!("failed to renew cache TTL: {e}");
            }
            return;
        }
        if let Err(e) = self.refresh(RefreshStrategy::Online).await {
            error!("failed to refresh models after etag change: {e}");
        }
    }

    /// Apply a new set of models (e.g. from a network fetch).
    pub async fn apply_models(&self, models: Vec<ModelDescriptor>) {
        *self.models.write().await = models;
    }

    /// Get the current provider info.
    pub fn provider(&self) -> &ModelProviderInfo {
        &self.provider
    }

    // ── Internal ─────────────────────────────────────────────────

    async fn refresh(&self, strategy: RefreshStrategy) -> Result<(), String> {
        match strategy {
            RefreshStrategy::Offline => {
                self.try_load_cache().await;
                Ok(())
            }
            RefreshStrategy::OnlineIfUncached => {
                if self.try_load_cache().await {
                    return Ok(());
                }
                self.fetch_remote_models().await
            }
            RefreshStrategy::Online => self.fetch_remote_models().await,
        }
    }

    async fn try_load_cache(&self) -> bool {
        let version = super::client_version_to_whole();
        let Some(cache) = self.cache_manager.load_fresh(&version).await else {
            return false;
        };
        *self.etag.write().await = cache.etag.clone();
        *self.models.write().await = cache.models;
        info!("models cache: loaded from disk");
        true
    }

    async fn fetch_remote_models(&self) -> Result<(), String> {
        let provider = self.provider.to_provider();
        let url = provider.url_for_path("models");
        let api_key = self.provider.api_key().map_err(|e| e.to_string())?;

        let client = reqwest::Client::new();
        let mut req = client.get(&url);
        if let Some(key) = &api_key {
            req = req.bearer_auth(key);
        }
        for (k, v) in &provider.headers {
            req = req.header(k.as_str(), v.as_str());
        }

        let resp = tokio::time::timeout(Duration::from_secs(5), req.send())
            .await
            .map_err(|_| "timeout fetching models".to_string())?
            .map_err(|e| format!("request failed: {e}"))?;

        let etag = resp
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(String::from);

        let body: ModelsResponse = resp
            .json()
            .await
            .map_err(|e| format!("failed to parse models response: {e}"))?;

        *self.models.write().await = body.models.clone();
        *self.etag.write().await = etag.clone();

        let version = super::client_version_to_whole();
        self.cache_manager
            .persist_cache(&body.models, etag, version)
            .await;

        info!(count = body.models.len(), "models: fetched from remote");
        Ok(())
    }
}

fn find_by_longest_prefix(slug: &str, candidates: &[ModelDescriptor]) -> Option<ModelDescriptor> {
    candidates
        .iter()
        .filter(|c| slug.starts_with(&c.slug))
        .max_by_key(|c| c.slug.len())
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_models() -> Vec<ModelDescriptor> {
        vec![
            ModelDescriptor {
                slug: "gpt-4o".into(),
                display_name: "GPT-4o".into(),
                description: None,
                priority: 1,
                context_window: Some(128_000),
                is_default: true,
                show_in_picker: true,
                supports_parallel_tool_calls: true,
                supports_reasoning_summaries: false,
            },
            ModelDescriptor {
                slug: "gpt-4o-mini".into(),
                display_name: "GPT-4o Mini".into(),
                description: None,
                priority: 2,
                context_window: Some(128_000),
                is_default: false,
                show_in_picker: true,
                supports_parallel_tool_calls: true,
                supports_reasoning_summaries: false,
            },
        ]
    }

    #[test]
    fn find_by_longest_prefix_exact_match() {
        let models = test_models();
        let found = find_by_longest_prefix("gpt-4o", &models).unwrap();
        assert_eq!(found.slug, "gpt-4o");
    }

    #[test]
    fn find_by_longest_prefix_prefers_longer() {
        let models = test_models();
        let found = find_by_longest_prefix("gpt-4o-mini-2024", &models).unwrap();
        assert_eq!(found.slug, "gpt-4o-mini");
    }

    #[test]
    fn find_by_longest_prefix_no_match() {
        let models = test_models();
        assert!(find_by_longest_prefix("claude-3", &models).is_none());
    }

    #[tokio::test]
    async fn manager_list_models_offline() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = ModelsManager::new(
            tmp.path().to_path_buf(),
            ModelProviderInfo::create_openai(),
            Some(ModelsResponse {
                models: test_models(),
            }),
        );
        let models = mgr.list_models(RefreshStrategy::Offline).await;
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].slug, "gpt-4o"); // priority 1 first
    }

    #[tokio::test]
    async fn manager_get_default_model() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = ModelsManager::new(
            tmp.path().to_path_buf(),
            ModelProviderInfo::create_openai(),
            Some(ModelsResponse {
                models: test_models(),
            }),
        );
        let default = mgr.get_default_model(&None, RefreshStrategy::Offline).await;
        assert_eq!(default, "gpt-4o");
    }

    #[tokio::test]
    async fn manager_get_default_model_explicit() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = ModelsManager::new(
            tmp.path().to_path_buf(),
            ModelProviderInfo::create_openai(),
            None,
        );
        let default = mgr
            .get_default_model(&Some("custom-model".into()), RefreshStrategy::Offline)
            .await;
        assert_eq!(default, "custom-model");
    }

    #[tokio::test]
    async fn manager_get_model_info_fallback() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = ModelsManager::new(
            tmp.path().to_path_buf(),
            ModelProviderInfo::create_openai(),
            Some(ModelsResponse {
                models: test_models(),
            }),
        );
        let info = mgr.get_model_info("unknown-model").await;
        assert_eq!(info.slug, "unknown-model");
        assert_eq!(info.priority, 99); // fallback
    }
}
