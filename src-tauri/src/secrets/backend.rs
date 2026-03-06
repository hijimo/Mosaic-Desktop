use std::collections::BTreeMap;
use std::path::PathBuf;

use super::{SecretListEntry, SecretName, SecretScope, SecretsBackend};

/// In-memory secrets backend for local development and testing.
///
/// TODO(H7): The reference implementation uses `age`-encrypted files + OS keyring
/// (`codex-keyring-store` crate) for persistent, encrypted secret storage.
/// This simplified version stores secrets in memory (suitable for development;
/// a persistent encrypted backend should replace this for production use).
/// Key missing features:
/// - `age` encryption with scrypt for `local.age` file
/// - OS keyring integration for passphrase storage
/// - `SecretsManager::new_with_keyring_store` constructor
/// - Atomic file operations for corruption prevention
#[derive(Debug, Clone)]
pub struct LocalSecretsBackend {
    codex_home: PathBuf,
    secrets: std::sync::Arc<std::sync::Mutex<BTreeMap<String, String>>>,
}

impl LocalSecretsBackend {
    pub fn new(codex_home: PathBuf) -> Self {
        Self {
            codex_home,
            secrets: std::sync::Arc::new(std::sync::Mutex::new(BTreeMap::new())),
        }
    }

    pub fn codex_home(&self) -> &PathBuf {
        &self.codex_home
    }
}

impl SecretsBackend for LocalSecretsBackend {
    fn set(&self, scope: &SecretScope, name: &SecretName, value: &str) -> Result<(), String> {
        if value.is_empty() {
            return Err("secret value must not be empty".to_string());
        }
        let key = scope.canonical_key(name);
        let mut secrets = self.secrets.lock().map_err(|e| e.to_string())?;
        secrets.insert(key, value.to_string());
        Ok(())
    }

    fn get(&self, scope: &SecretScope, name: &SecretName) -> Result<Option<String>, String> {
        let key = scope.canonical_key(name);
        let secrets = self.secrets.lock().map_err(|e| e.to_string())?;
        Ok(secrets.get(&key).cloned())
    }

    fn delete(&self, scope: &SecretScope, name: &SecretName) -> Result<bool, String> {
        let key = scope.canonical_key(name);
        let mut secrets = self.secrets.lock().map_err(|e| e.to_string())?;
        Ok(secrets.remove(&key).is_some())
    }

    fn list(&self, scope_filter: Option<&SecretScope>) -> Result<Vec<SecretListEntry>, String> {
        let secrets = self.secrets.lock().map_err(|e| e.to_string())?;
        let mut entries = Vec::new();
        for canonical_key in secrets.keys() {
            if let Some(entry) = parse_canonical_key(canonical_key) {
                if let Some(scope) = scope_filter {
                    if entry.scope != *scope {
                        continue;
                    }
                }
                entries.push(entry);
            }
        }
        Ok(entries)
    }
}

fn parse_canonical_key(canonical_key: &str) -> Option<SecretListEntry> {
    let mut parts = canonical_key.split('/');
    let scope_kind = parts.next()?;
    match scope_kind {
        "global" => {
            let name_str = parts.next()?;
            if parts.next().is_some() {
                return None;
            }
            let name = SecretName::new(name_str).ok()?;
            Some(SecretListEntry {
                scope: SecretScope::Global,
                name,
            })
        }
        "env" => {
            let environment_id = parts.next()?;
            let name_str = parts.next()?;
            if parts.next().is_some() {
                return None;
            }
            let name = SecretName::new(name_str).ok()?;
            let scope = SecretScope::environment(environment_id.to_string()).ok()?;
            Some(SecretListEntry { scope, name })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_delete_roundtrip() {
        let backend = LocalSecretsBackend::new(PathBuf::from("/tmp/test"));
        let scope = SecretScope::Global;
        let name = SecretName::new("TOKEN").unwrap();

        backend.set(&scope, &name, "secret-value").unwrap();
        assert_eq!(
            backend.get(&scope, &name).unwrap(),
            Some("secret-value".to_string())
        );

        assert!(backend.delete(&scope, &name).unwrap());
        assert_eq!(backend.get(&scope, &name).unwrap(), None);
    }

    #[test]
    fn set_rejects_empty_value() {
        let backend = LocalSecretsBackend::new(PathBuf::from("/tmp/test"));
        let scope = SecretScope::Global;
        let name = SecretName::new("TOKEN").unwrap();
        assert!(backend.set(&scope, &name, "").is_err());
    }

    #[test]
    fn list_with_scope_filter() {
        let backend = LocalSecretsBackend::new(PathBuf::from("/tmp/test"));
        let global = SecretScope::Global;
        let env = SecretScope::environment("proj").unwrap();
        let name1 = SecretName::new("A").unwrap();
        let name2 = SecretName::new("B").unwrap();

        backend.set(&global, &name1, "val1").unwrap();
        backend.set(&env, &name2, "val2").unwrap();

        let all = backend.list(None).unwrap();
        assert_eq!(all.len(), 2);

        let global_only = backend.list(Some(&global)).unwrap();
        assert_eq!(global_only.len(), 1);
        assert_eq!(global_only[0].name, name1);
    }

    #[test]
    fn delete_nonexistent_returns_false() {
        let backend = LocalSecretsBackend::new(PathBuf::from("/tmp/test"));
        let scope = SecretScope::Global;
        let name = SecretName::new("NOPE").unwrap();
        assert!(!backend.delete(&scope, &name).unwrap());
    }
}
