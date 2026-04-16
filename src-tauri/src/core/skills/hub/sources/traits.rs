//! SkillSource trait — mirrors Hermes `SkillSource` ABC.

use async_trait::async_trait;

use super::super::models::{SkillBundle, SkillHubMeta, TrustLevel};

/// Abstract interface for all skill registry adapters.
#[async_trait]
pub trait SkillSource: Send + Sync {
    /// Search for skills matching a query string.
    async fn search(&self, query: &str, limit: usize) -> Vec<SkillHubMeta>;

    /// Download a skill bundle by identifier.
    async fn fetch(&self, identifier: &str) -> Option<SkillBundle>;

    /// Fetch metadata for a skill without downloading all files.
    async fn inspect(&self, identifier: &str) -> Option<SkillHubMeta>;

    /// Unique identifier for this source (e.g. "github", "skills-sh").
    fn source_id(&self) -> &str;

    /// Determine trust level for a skill from this source.
    fn trust_level_for(&self, _identifier: &str) -> TrustLevel {
        TrustLevel::Community
    }
}
