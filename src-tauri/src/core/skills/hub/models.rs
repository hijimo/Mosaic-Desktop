//! Hub data models — mirrors Hermes `tools/skills_hub.py` SkillMeta/SkillBundle
//! and `tools/skills_guard.py` Finding/ScanResult.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Trust levels — mirrors Hermes INSTALL_POLICY keys
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustLevel {
    Builtin,
    Trusted,
    Community,
    AgentCreated,
}

impl TrustLevel {
    /// Numeric rank for dedup (higher = prefer). Mirrors Hermes `_TRUST_RANK`.
    pub fn rank(self) -> u8 {
        match self {
            Self::Builtin => 3,
            Self::Trusted => 2,
            Self::Community => 1,
            Self::AgentCreated => 0,
        }
    }
}

impl std::fmt::Display for TrustLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Builtin => write!(f, "builtin"),
            Self::Trusted => write!(f, "trusted"),
            Self::Community => write!(f, "community"),
            Self::AgentCreated => write!(f, "agent-created"),
        }
    }
}

// ---------------------------------------------------------------------------
// SkillHubMeta — mirrors Hermes SkillMeta dataclass
// ---------------------------------------------------------------------------

/// Minimal metadata returned by search results.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillHubMeta {
    pub name: String,
    pub description: String,
    /// e.g. "official", "github", "skills-sh", "clawhub", "lobehub"
    pub source: String,
    /// Source-specific ID (e.g. "openai/skills/skill-creator")
    pub identifier: String,
    pub trust_level: TrustLevel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub extra: HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// SkillBundle — mirrors Hermes SkillBundle dataclass
// ---------------------------------------------------------------------------

/// A downloaded skill ready for quarantine/scanning/installation.
#[derive(Debug, Clone)]
pub struct SkillBundle {
    pub name: String,
    /// relative_path -> file content (text)
    pub files: HashMap<String, String>,
    pub source: String,
    pub identifier: String,
    pub trust_level: TrustLevel,
    pub metadata: HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Scan result models — mirrors Hermes skills_guard.py
// ---------------------------------------------------------------------------

/// Severity of a scan finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Critical,
    High,
    Medium,
    Low,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Critical => write!(f, "CRITICAL"),
            Self::High => write!(f, "HIGH"),
            Self::Medium => write!(f, "MEDIUM"),
            Self::Low => write!(f, "LOW"),
        }
    }
}

/// A single finding from the security scanner.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub pattern_id: String,
    pub severity: Severity,
    pub category: String,
    pub file: String,
    pub line: usize,
    #[serde(rename = "match")]
    pub matched: String,
    pub description: String,
}

/// Overall scan verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Safe,
    Caution,
    Dangerous,
}

impl std::fmt::Display for Verdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Safe => write!(f, "SAFE"),
            Self::Caution => write!(f, "CAUTION"),
            Self::Dangerous => write!(f, "DANGEROUS"),
        }
    }
}

/// Result of scanning a skill. Mirrors Hermes `ScanResult`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub skill_name: String,
    pub source: String,
    pub trust_level: TrustLevel,
    pub verdict: Verdict,
    #[serde(default)]
    pub findings: Vec<Finding>,
    pub scanned_at: String,
    pub summary: String,
}

/// Install policy decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallDecision {
    Allow,
    Block,
    Ask,
}

// ---------------------------------------------------------------------------
// Lock file entry — mirrors Hermes HubLockFile installed entries
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockEntry {
    pub source: String,
    pub identifier: String,
    pub trust_level: String,
    pub scan_verdict: String,
    pub content_hash: String,
    pub install_path: String,
    pub files: Vec<String>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
    pub installed_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockFileData {
    pub version: u32,
    pub installed: HashMap<String, LockEntry>,
}

impl Default for LockFileData {
    fn default() -> Self {
        Self {
            version: 1,
            installed: HashMap::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Path validation — mirrors Hermes `_normalize_bundle_path`
// ---------------------------------------------------------------------------

/// Validate and normalize a bundle-controlled path before touching disk.
/// Rejects absolute paths, `..` traversal, Windows drive letters, and
/// (optionally) nested paths.
pub fn normalize_bundle_path(
    path_value: &str,
    field_name: &str,
    allow_nested: bool,
) -> Result<String, String> {
    let raw = path_value.trim();
    if raw.is_empty() {
        return Err(format!("Unsafe {field_name}: empty path"));
    }

    let normalized = raw.replace('\\', "/");

    if normalized.starts_with('/') {
        return Err(format!("Unsafe {field_name}: {path_value}"));
    }

    let parts: Vec<&str> = normalized
        .split('/')
        .filter(|p| !p.is_empty() && *p != ".")
        .collect();

    if parts.is_empty() {
        return Err(format!("Unsafe {field_name}: {path_value}"));
    }
    if parts.iter().any(|p| *p == "..") {
        return Err(format!("Unsafe {field_name}: {path_value}"));
    }
    // Reject Windows drive letters like "C:"
    if parts[0].len() == 2 && parts[0].as_bytes()[1] == b':' && parts[0].as_bytes()[0].is_ascii_alphabetic() {
        return Err(format!("Unsafe {field_name}: {path_value}"));
    }
    if !allow_nested && parts.len() != 1 {
        return Err(format!("Unsafe {field_name}: {path_value}"));
    }

    Ok(parts.join("/"))
}

pub fn validate_skill_name(name: &str) -> Result<String, String> {
    normalize_bundle_path(name, "skill name", false)
}

pub fn validate_category_name(category: &str) -> Result<String, String> {
    normalize_bundle_path(category, "category", false)
}

pub fn validate_bundle_rel_path(rel_path: &str) -> Result<String, String> {
    normalize_bundle_path(rel_path, "bundle file path", true)
}

// ---------------------------------------------------------------------------
// Hub directory paths — mirrors Hermes HERMES_HOME / HUB_DIR / etc.
// ---------------------------------------------------------------------------

/// Resolve `~/.mosaic/skills/.hub/` and sub-paths.
pub struct HubPaths {
    pub skills_dir: PathBuf,
    pub hub_dir: PathBuf,
    pub lock_file: PathBuf,
    pub quarantine_dir: PathBuf,
    pub audit_log: PathBuf,
    pub taps_file: PathBuf,
    pub index_cache_dir: PathBuf,
}

impl HubPaths {
    pub fn new(mosaic_home: &Path) -> Self {
        let skills_dir = mosaic_home.join("skills");
        let hub_dir = skills_dir.join(".hub");
        Self {
            lock_file: hub_dir.join("lock.json"),
            quarantine_dir: hub_dir.join("quarantine"),
            audit_log: hub_dir.join("audit.log"),
            taps_file: hub_dir.join("taps.json"),
            index_cache_dir: hub_dir.join("index-cache"),
            skills_dir,
            hub_dir,
        }
    }

    pub fn from_default() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        Self::new(&home.join(".codex"))
    }
}

/// Cache TTL for remote index fetches (seconds). Mirrors Hermes INDEX_CACHE_TTL.
pub const INDEX_CACHE_TTL: u64 = 3600;

/// Trusted repos that get `TrustLevel::Trusted`. Mirrors Hermes TRUSTED_REPOS.
pub const TRUSTED_REPOS: &[&str] = &["openai/skills", "anthropics/skills"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_rejects_absolute() {
        assert!(normalize_bundle_path("/etc/passwd", "test", true).is_err());
    }

    #[test]
    fn normalize_rejects_traversal() {
        assert!(normalize_bundle_path("../secret", "test", true).is_err());
        assert!(normalize_bundle_path("foo/../../bar", "test", true).is_err());
    }

    #[test]
    fn normalize_rejects_windows_drive() {
        assert!(normalize_bundle_path("C:foo", "test", true).is_err());
    }

    #[test]
    fn normalize_rejects_nested_when_disallowed() {
        assert!(normalize_bundle_path("a/b", "test", false).is_err());
    }

    #[test]
    fn normalize_accepts_simple_name() {
        assert_eq!(
            normalize_bundle_path("my-skill", "test", false).unwrap(),
            "my-skill"
        );
    }

    #[test]
    fn normalize_accepts_nested_when_allowed() {
        assert_eq!(
            normalize_bundle_path("refs/api.md", "test", true).unwrap(),
            "refs/api.md"
        );
    }

    #[test]
    fn normalize_strips_dots_and_backslash() {
        assert_eq!(
            normalize_bundle_path("./foo\\bar.md", "test", true).unwrap(),
            "foo/bar.md"
        );
    }

    #[test]
    fn trust_level_rank_order() {
        assert!(TrustLevel::Builtin.rank() > TrustLevel::Trusted.rank());
        assert!(TrustLevel::Trusted.rank() > TrustLevel::Community.rank());
    }

    #[test]
    fn lock_file_data_default() {
        let d = LockFileData::default();
        assert_eq!(d.version, 1);
        assert!(d.installed.is_empty());
    }

    #[test]
    fn hub_paths_structure() {
        let paths = HubPaths::new(Path::new("/tmp/mosaic"));
        assert_eq!(paths.hub_dir, PathBuf::from("/tmp/mosaic/skills/.hub"));
        assert_eq!(
            paths.lock_file,
            PathBuf::from("/tmp/mosaic/skills/.hub/lock.json")
        );
        assert_eq!(
            paths.quarantine_dir,
            PathBuf::from("/tmp/mosaic/skills/.hub/quarantine")
        );
    }
}
