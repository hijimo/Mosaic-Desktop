pub mod backend;
pub mod manager;
pub mod sanitizer;

use std::fmt;
use std::path::Path;

use sha2::{Digest, Sha256};

pub use backend::LocalSecretsBackend;
pub use manager::SecretsManager;
pub use sanitizer::redact_secrets;

/// A validated secret name (uppercase ASCII + digits + underscore only).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SecretName(String);

impl SecretName {
    pub fn new(raw: &str) -> Result<Self, String> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err("secret name must not be empty".to_string());
        }
        if !trimmed
            .chars()
            .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_')
        {
            return Err("secret name must contain only A-Z, 0-9, or _".to_string());
        }
        Ok(Self(trimmed.to_string()))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for SecretName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Scope for a secret — global or per-environment.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SecretScope {
    Global,
    Environment(String),
}

impl SecretScope {
    pub fn environment(environment_id: impl Into<String>) -> Result<Self, String> {
        let env_id = environment_id.into();
        let trimmed = env_id.trim();
        if trimmed.is_empty() {
            return Err("environment id must not be empty".to_string());
        }
        Ok(Self::Environment(trimmed.to_string()))
    }

    pub fn canonical_key(&self, name: &SecretName) -> String {
        match self {
            Self::Global => format!("global/{}", name.as_str()),
            Self::Environment(environment_id) => {
                format!("env/{environment_id}/{}", name.as_str())
            }
        }
    }
}

/// Entry in a secrets listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretListEntry {
    pub scope: SecretScope,
    pub name: SecretName,
}

/// Backend kind selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SecretsBackendKind {
    #[default]
    Local,
}

/// Trait for secrets storage backends.
pub trait SecretsBackend: Send + Sync {
    fn set(&self, scope: &SecretScope, name: &SecretName, value: &str) -> Result<(), String>;
    fn get(&self, scope: &SecretScope, name: &SecretName) -> Result<Option<String>, String>;
    fn delete(&self, scope: &SecretScope, name: &SecretName) -> Result<bool, String>;
    fn list(&self, scope_filter: Option<&SecretScope>) -> Result<Vec<SecretListEntry>, String>;
}

/// Derive an environment ID from a working directory.
///
/// Uses the git repo root name if available, otherwise a SHA-256 hash prefix.
pub fn environment_id_from_cwd(cwd: &Path) -> String {
    if let Some(repo_root) = get_git_repo_root(cwd) {
        if let Some(name) = repo_root.file_name() {
            let name = name.to_string_lossy().trim().to_string();
            if !name.is_empty() {
                return name;
            }
        }
    }

    let canonical = cwd
        .canonicalize()
        .unwrap_or_else(|_| cwd.to_path_buf())
        .to_string_lossy()
        .into_owned();
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    let digest = hasher.finalize();
    let hex = format!("{digest:x}");
    let short = hex.get(..12).unwrap_or(hex.as_str());
    format!("cwd-{short}")
}

fn get_git_repo_root(base_dir: &Path) -> Option<std::path::PathBuf> {
    let mut dir = base_dir.to_path_buf();
    loop {
        if dir.join(".git").exists() {
            return Some(dir);
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_name_valid() {
        assert!(SecretName::new("GITHUB_TOKEN").is_ok());
        assert!(SecretName::new("API_KEY_123").is_ok());
    }

    #[test]
    fn secret_name_rejects_lowercase() {
        assert!(SecretName::new("github_token").is_err());
    }

    #[test]
    fn secret_name_rejects_empty() {
        assert!(SecretName::new("").is_err());
        assert!(SecretName::new("  ").is_err());
    }

    #[test]
    fn secret_scope_canonical_key() {
        let name = SecretName::new("TOKEN").unwrap();
        assert_eq!(SecretScope::Global.canonical_key(&name), "global/TOKEN");
        let env = SecretScope::environment("my-project").unwrap();
        assert_eq!(env.canonical_key(&name), "env/my-project/TOKEN");
    }

    #[test]
    fn environment_scope_rejects_empty() {
        assert!(SecretScope::environment("").is_err());
        assert!(SecretScope::environment("  ").is_err());
    }

    #[test]
    fn environment_id_fallback_has_cwd_prefix() {
        let dir = tempfile::tempdir().unwrap();
        let env_id = environment_id_from_cwd(dir.path());
        assert!(env_id.starts_with("cwd-"));
        assert!(env_id.len() > 4);
    }
}
