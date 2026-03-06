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
}
