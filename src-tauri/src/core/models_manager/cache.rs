//! Disk-backed model metadata cache with TTL.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::time::Duration;
use tokio::fs;
use tracing::error;

use super::model_info::ModelDescriptor;

/// Manages loading and saving of models cache to disk.
#[derive(Debug)]
pub struct ModelsCacheManager {
    cache_path: PathBuf,
    cache_ttl: Duration,
}

impl ModelsCacheManager {
    pub fn new(cache_path: PathBuf, cache_ttl: Duration) -> Self {
        Self {
            cache_path,
            cache_ttl,
        }
    }

    /// Load a fresh cache entry. Returns `None` if missing, stale, or version-mismatched.
    pub async fn load_fresh(&self, expected_version: &str) -> Option<ModelsCache> {
        let cache = match self.load().await {
            Ok(Some(c)) => c,
            _ => return None,
        };
        if cache.client_version.as_deref() != Some(expected_version) {
            return None;
        }
        if !cache.is_fresh(self.cache_ttl) {
            return None;
        }
        Some(cache)
    }

    /// Persist models to disk.
    pub async fn persist_cache(
        &self,
        models: &[ModelDescriptor],
        etag: Option<String>,
        client_version: String,
    ) {
        let cache = ModelsCache {
            fetched_at: Utc::now(),
            etag,
            client_version: Some(client_version),
            models: models.to_vec(),
        };
        if let Err(err) = self.save(&cache).await {
            error!("failed to write models cache: {err}");
        }
    }

    /// Renew the cache TTL by bumping `fetched_at` to now.
    pub async fn renew_cache_ttl(&self) -> io::Result<()> {
        let mut cache = self
            .load()
            .await?
            .ok_or_else(|| io::Error::new(ErrorKind::NotFound, "cache not found"))?;
        cache.fetched_at = Utc::now();
        self.save(&cache).await
    }

    async fn load(&self) -> io::Result<Option<ModelsCache>> {
        match fs::read(&self.cache_path).await {
            Ok(bytes) => {
                let cache = serde_json::from_slice(&bytes)
                    .map_err(|e| io::Error::new(ErrorKind::InvalidData, e.to_string()))?;
                Ok(Some(cache))
            }
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    async fn save(&self, cache: &ModelsCache) -> io::Result<()> {
        if let Some(parent) = self.cache_path.parent() {
            fs::create_dir_all(parent).await?;
        }
        let json = serde_json::to_vec_pretty(cache)
            .map_err(|e| io::Error::new(ErrorKind::InvalidData, e.to_string()))?;
        fs::write(&self.cache_path, json).await
    }
}

/// Serialized snapshot of models cached on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelsCache {
    pub fetched_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_version: Option<String>,
    pub models: Vec<ModelDescriptor>,
}

impl ModelsCache {
    fn is_fresh(&self, ttl: Duration) -> bool {
        if ttl.is_zero() {
            return false;
        }
        let Ok(ttl_chrono) = chrono::Duration::from_std(ttl) else {
            return false;
        };
        Utc::now().signed_duration_since(self.fetched_at) <= ttl_chrono
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn persist_and_load_round_trip() {
        let tmp = TempDir::new().unwrap();
        let cache_path = tmp.path().join("cache.json");
        let mgr = ModelsCacheManager::new(cache_path, Duration::from_secs(300));

        let models = vec![ModelDescriptor::fallback("test-model")];
        mgr.persist_cache(&models, Some("etag-1".into()), "0.1.0".into())
            .await;

        let loaded = mgr.load_fresh("0.1.0").await.unwrap();
        assert_eq!(loaded.models.len(), 1);
        assert_eq!(loaded.models[0].slug, "test-model");
        assert_eq!(loaded.etag, Some("etag-1".into()));
    }

    #[tokio::test]
    async fn stale_cache_returns_none() {
        let tmp = TempDir::new().unwrap();
        let cache_path = tmp.path().join("cache.json");
        let mgr = ModelsCacheManager::new(cache_path, Duration::from_secs(0));

        let models = vec![ModelDescriptor::fallback("m")];
        mgr.persist_cache(&models, None, "0.1.0".into()).await;

        assert!(mgr.load_fresh("0.1.0").await.is_none());
    }

    #[tokio::test]
    async fn version_mismatch_returns_none() {
        let tmp = TempDir::new().unwrap();
        let cache_path = tmp.path().join("cache.json");
        let mgr = ModelsCacheManager::new(cache_path, Duration::from_secs(300));

        let models = vec![ModelDescriptor::fallback("m")];
        mgr.persist_cache(&models, None, "0.1.0".into()).await;

        assert!(mgr.load_fresh("0.2.0").await.is_none());
    }
}
