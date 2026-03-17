pub mod backend;
pub mod manager;
pub mod sanitizer;

use std::fmt;
use std::ops::Range;
use std::path::Path;

use regex::Regex;
use sha2::{Digest, Sha256};
use std::sync::LazyLock;

pub use backend::LocalSecretsBackend;
pub use manager::SecretsManager;
pub use sanitizer::redact_known_secrets;
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

/// A match found by `scan_for_secrets`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretMatch {
    /// Kind of secret detected (e.g. "openai_api_key", "aws_access_key", "bearer_token", "private_key", "secret_assignment").
    pub kind: String,
    /// Byte range in the original input where the secret was found.
    pub range: Range<usize>,
    /// Redacted representation of the matched secret.
    pub redacted: String,
}

static SCAN_OPENAI_KEY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"sk-[A-Za-z0-9]{20,}").unwrap());
static SCAN_AWS_ACCESS_KEY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bAKIA[0-9A-Z]{16}\b").unwrap());
static SCAN_BEARER_TOKEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bBearer\s+[A-Za-z0-9._\-]{16,}\b").unwrap());
static SCAN_PRIVATE_KEY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"-----BEGIN\s+(RSA\s+)?PRIVATE\s+KEY-----").unwrap());
static SCAN_SECRET_ASSIGNMENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)\b(api[_-]?key|token|secret|password)\b(\s*[:=]\s*)(["']?)([^\s"']{8,})"#)
        .unwrap()
});

/// Scan content for embedded secrets and sensitive patterns.
///
/// Returns a `Vec<SecretMatch>` for each detected pattern, with the matched
/// range and a redacted placeholder. Patterns detected include OpenAI API keys,
/// AWS access key IDs, bearer tokens, private key headers, and common
/// secret-assignment patterns.
pub fn scan_for_secrets(content: &str) -> Vec<SecretMatch> {
    let mut matches = Vec::new();

    for m in SCAN_OPENAI_KEY.find_iter(content) {
        matches.push(SecretMatch {
            kind: "openai_api_key".to_string(),
            range: m.start()..m.end(),
            redacted: "[REDACTED_OPENAI_KEY]".to_string(),
        });
    }

    for m in SCAN_AWS_ACCESS_KEY.find_iter(content) {
        matches.push(SecretMatch {
            kind: "aws_access_key".to_string(),
            range: m.start()..m.end(),
            redacted: "[REDACTED_AWS_KEY]".to_string(),
        });
    }

    for m in SCAN_BEARER_TOKEN.find_iter(content) {
        matches.push(SecretMatch {
            kind: "bearer_token".to_string(),
            range: m.start()..m.end(),
            redacted: "Bearer [REDACTED_TOKEN]".to_string(),
        });
    }

    for m in SCAN_PRIVATE_KEY.find_iter(content) {
        matches.push(SecretMatch {
            kind: "private_key".to_string(),
            range: m.start()..m.end(),
            redacted: "[REDACTED_PRIVATE_KEY]".to_string(),
        });
    }

    for m in SCAN_SECRET_ASSIGNMENT.find_iter(content) {
        matches.push(SecretMatch {
            kind: "secret_assignment".to_string(),
            range: m.start()..m.end(),
            redacted: "[REDACTED_SECRET]".to_string(),
        });
    }

    // Sort by start position for deterministic output
    matches.sort_by_key(|m| m.range.start);
    matches
}

/// Derive an environment ID from a working directory.
///
/// Uses the git repo root name if available, otherwise a SHA-256 hash prefix.
pub fn environment_id_from_cwd(cwd: &Path) -> String {
    if let Some(repo_root) = crate::core::git_info::get_git_repo_root(cwd) {
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

    #[test]
    fn scan_detects_openai_key() {
        let input = "my key is sk-abcdefghijklmnopqrstuvwxyz here";
        let matches = scan_for_secrets(input);
        assert!(!matches.is_empty());
        assert_eq!(matches[0].kind, "openai_api_key");
        assert!(!matches[0]
            .redacted
            .contains("sk-abcdefghijklmnopqrstuvwxyz"));
    }

    #[test]
    fn scan_detects_aws_access_key() {
        let input = "key AKIAIOSFODNN7EXAMPLE end";
        let matches = scan_for_secrets(input);
        assert!(!matches.is_empty());
        assert_eq!(matches[0].kind, "aws_access_key");
    }

    #[test]
    fn scan_detects_bearer_token() {
        let input = "Authorization: Bearer eyJhbGciOiJIUzI1NiJ9.test";
        let matches = scan_for_secrets(input);
        assert!(!matches.is_empty());
        assert_eq!(matches[0].kind, "bearer_token");
    }

    #[test]
    fn scan_detects_private_key() {
        let input = "-----BEGIN RSA PRIVATE KEY-----\nMIIE...";
        let matches = scan_for_secrets(input);
        assert!(!matches.is_empty());
        assert_eq!(matches[0].kind, "private_key");
    }

    #[test]
    fn scan_detects_secret_assignment() {
        let input = "api_key = 'my_super_secret_value_here'";
        let matches = scan_for_secrets(input);
        assert!(!matches.is_empty());
        assert_eq!(matches[0].kind, "secret_assignment");
    }

    #[test]
    fn scan_returns_empty_for_clean_text() {
        let input = "hello world, nothing secret here";
        let matches = scan_for_secrets(input);
        assert!(matches.is_empty());
    }

    #[test]
    fn scan_redacted_does_not_contain_original() {
        let secret = "sk-abcdefghijklmnopqrstuvwxyz";
        let input = format!("key is {secret}");
        let matches = scan_for_secrets(&input);
        for m in &matches {
            assert!(!m.redacted.contains(secret));
        }
    }

    #[test]
    fn scan_range_matches_input_slice() {
        let secret = "sk-abcdefghijklmnopqrstuvwxyz";
        let input = format!("prefix {secret} suffix");
        let matches = scan_for_secrets(&input);
        assert!(!matches.is_empty());
        let m = &matches[0];
        assert_eq!(&input[m.range.clone()], secret);
    }
}
