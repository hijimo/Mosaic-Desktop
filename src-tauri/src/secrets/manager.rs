use std::path::PathBuf;
use std::sync::Arc;

use super::backend::LocalSecretsBackend;
use super::{SecretListEntry, SecretName, SecretScope, SecretsBackend, SecretsBackendKind};

/// High-level secrets manager that delegates to a backend.
#[derive(Clone)]
pub struct SecretsManager {
    backend: Arc<dyn SecretsBackend>,
}

impl SecretsManager {
    pub fn new(codex_home: PathBuf, backend_kind: SecretsBackendKind) -> Self {
        let backend: Arc<dyn SecretsBackend> = match backend_kind {
            SecretsBackendKind::Local => Arc::new(LocalSecretsBackend::new(codex_home)),
        };
        Self { backend }
    }

    /// Create with a custom backend (useful for testing).
    pub fn new_with_backend(backend: Arc<dyn SecretsBackend>) -> Self {
        Self { backend }
    }

    /// Create a manager backed by the OS keyring integration.
    ///
    /// Currently delegates to `LocalSecretsBackend` (in-memory).
    /// TODO(H7): Replace with actual keyring + age-encrypted backend.
    pub fn new_with_keyring(codex_home: PathBuf) -> Self {
        Self::new(codex_home, SecretsBackendKind::Local)
    }

    pub fn set(&self, scope: &SecretScope, name: &SecretName, value: &str) -> Result<(), String> {
        self.backend.set(scope, name, value)
    }

    pub fn get(&self, scope: &SecretScope, name: &SecretName) -> Result<Option<String>, String> {
        self.backend.get(scope, name)
    }

    pub fn delete(&self, scope: &SecretScope, name: &SecretName) -> Result<bool, String> {
        self.backend.delete(scope, name)
    }

    pub fn list(&self, scope_filter: Option<&SecretScope>) -> Result<Vec<SecretListEntry>, String> {
        self.backend.list(scope_filter)
    }

    /// Alias matching the design doc interface.
    pub fn set_secret(
        &self,
        name: &SecretName,
        scope: &SecretScope,
        value: &str,
    ) -> Result<(), String> {
        self.backend.set(scope, name, value)
    }

    /// Alias matching the design doc interface.
    pub fn get_secret(
        &self,
        name: &SecretName,
        scope: &SecretScope,
    ) -> Result<Option<String>, String> {
        self.backend.get(scope, name)
    }

    /// Alias matching the design doc interface.
    pub fn delete_secret(&self, name: &SecretName, scope: &SecretScope) -> Result<bool, String> {
        self.backend.delete(scope, name)
    }

    /// Alias matching the design doc interface.
    pub fn list_secrets(&self, scope: &SecretScope) -> Result<Vec<SecretListEntry>, String> {
        self.backend.list(Some(scope))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manager_roundtrip() {
        let manager =
            SecretsManager::new(PathBuf::from("/tmp/test-mgr"), SecretsBackendKind::Local);
        let scope = SecretScope::Global;
        let name = SecretName::new("GITHUB_TOKEN").unwrap();

        manager.set(&scope, &name, "token-1").unwrap();
        assert_eq!(
            manager.get(&scope, &name).unwrap(),
            Some("token-1".to_string())
        );

        let listed = manager.list(None).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, name);

        assert!(manager.delete(&scope, &name).unwrap());
        assert_eq!(manager.get(&scope, &name).unwrap(), None);
    }

    #[test]
    fn new_with_keyring_creates_manager() {
        let manager = SecretsManager::new_with_keyring(PathBuf::from("/tmp/test-keyring"));
        let scope = SecretScope::Global;
        let name = SecretName::new("TEST_KEY").unwrap();

        manager.set_secret(&name, &scope, "value-1").unwrap();
        assert_eq!(
            manager.get_secret(&name, &scope).unwrap(),
            Some("value-1".to_string())
        );
    }

    #[test]
    fn design_doc_api_roundtrip() {
        let manager =
            SecretsManager::new(PathBuf::from("/tmp/test-api"), SecretsBackendKind::Local);
        let scope = SecretScope::Global;
        let name = SecretName::new("API_TOKEN").unwrap();

        manager.set_secret(&name, &scope, "secret-val").unwrap();
        assert_eq!(
            manager.get_secret(&name, &scope).unwrap(),
            Some("secret-val".to_string())
        );

        let listed = manager.list_secrets(&scope).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, name);

        assert!(manager.delete_secret(&name, &scope).unwrap());
        assert_eq!(manager.get_secret(&name, &scope).unwrap(), None);
    }

    #[test]
    fn new_with_backend_custom() {
        let backend = Arc::new(LocalSecretsBackend::new(PathBuf::from("/tmp/custom")));
        let manager = SecretsManager::new_with_backend(backend);
        let scope = SecretScope::Global;
        let name = SecretName::new("CUSTOM").unwrap();

        manager.set_secret(&name, &scope, "custom-val").unwrap();
        assert_eq!(
            manager.get_secret(&name, &scope).unwrap(),
            Some("custom-val".to_string())
        );
    }
}
