//! skills.sh source adapter — mirrors Hermes `SkillsShSource`.
//! Discovers skills via skills.sh API and fetches from underlying GitHub repos.

use std::collections::HashMap;

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;

use super::super::models::*;
use super::github::GitHubSource;
use super::traits::SkillSource;

const BASE_URL: &str = "https://skills.sh";
const SEARCH_URL: &str = "https://skills.sh/api/search";

pub struct SkillsShSource {
    client: Client,
    github: GitHubSource,
}

impl SkillsShSource {
    pub fn new(github: GitHubSource) -> Self {
        Self { client: Client::new(), github }
    }

    /// Normalize identifier by stripping "skills-sh/" or "skills.sh/" prefix.
    fn normalize_id(identifier: &str) -> &str {
        identifier
            .strip_prefix("skills-sh/")
            .or_else(|| identifier.strip_prefix("skills.sh/"))
            .unwrap_or(identifier)
    }

    fn wrap_id(canonical: &str) -> String {
        format!("skills-sh/{canonical}")
    }

    /// Generate candidate GitHub identifiers for a skills.sh entry.
    fn candidate_identifiers(identifier: &str) -> Vec<String> {
        let parts: Vec<&str> = identifier.splitn(3, '/').collect();
        if parts.len() < 3 { return vec![identifier.to_string()]; }
        let repo = format!("{}/{}", parts[0], parts[1]);
        let skill_path = parts[2].trim_start_matches('/');
        let mut candidates = vec![
            format!("{repo}/{skill_path}"),
            format!("{repo}/skills/{skill_path}"),
            format!("{repo}/.agents/skills/{skill_path}"),
            format!("{repo}/.claude/skills/{skill_path}"),
        ];
        candidates.dedup();
        candidates
    }

    /// Fetch featured skills from the homepage.
    async fn featured_skills(&self, limit: usize) -> Vec<SkillHubMeta> {
        let resp = match self.client.get(BASE_URL)
            .timeout(std::time::Duration::from_secs(20))
            .send().await {
            Ok(r) if r.status().is_success() => r,
            _ => return vec![],
        };
        let html = resp.text().await.unwrap_or_default();

        // Extract skill links: href="/owner/repo/skill"
        let re = regex::Regex::new(r#"href="/(?P<id>(?!agents/|_next/|api/)[^"'/]+/[^"'/]+/[^"'/]+)""#).unwrap();
        let mut seen = std::collections::HashSet::new();
        let mut results = Vec::new();

        for cap in re.captures_iter(&html) {
            let canonical = &cap["id"];
            if !seen.insert(canonical.to_string()) { continue; }
            let parts: Vec<&str> = canonical.splitn(3, '/').collect();
            if parts.len() < 3 { continue; }
            let repo = format!("{}/{}", parts[0], parts[1]);
            let skill_path = parts[2];
            results.push(SkillHubMeta {
                name: skill_path.rsplit('/').next().unwrap_or(skill_path).to_string(),
                description: format!("Featured on skills.sh from {repo}"),
                source: "skills.sh".into(),
                identifier: Self::wrap_id(canonical),
                trust_level: self.github.trust_level_for(canonical),
                repo: Some(repo),
                path: Some(skill_path.to_string()),
                tags: vec![],
                extra: HashMap::new(),
            });
            if results.len() >= limit { break; }
        }
        results
    }
}

#[async_trait]
impl SkillSource for SkillsShSource {
    fn source_id(&self) -> &str { "skills-sh" }

    fn trust_level_for(&self, identifier: &str) -> TrustLevel {
        self.github.trust_level_for(Self::normalize_id(identifier))
    }

    async fn search(&self, query: &str, limit: usize) -> Vec<SkillHubMeta> {
        if query.trim().is_empty() {
            return self.featured_skills(limit).await;
        }

        // Use the search API
        let resp = match self.client.get(SEARCH_URL)
            .query(&[("q", query), ("limit", &limit.to_string())])
            .timeout(std::time::Duration::from_secs(20))
            .send().await {
            Ok(r) if r.status().is_success() => r,
            _ => return vec![],
        };

        let data: SearchResponse = match resp.json().await {
            Ok(d) => d,
            Err(_) => return vec![],
        };

        let items = data.skills.unwrap_or_default();
        items.into_iter().take(limit).filter_map(|item| {
            // Build canonical from id or source+skillId
            let canonical = item.id.as_deref()
                .filter(|s| s.matches('/').count() >= 2)
                .map(|s| s.to_string())
                .or_else(|| {
                    let repo = item.source.as_deref()?;
                    let skill = item.skill_id.as_deref()?;
                    Some(format!("{repo}/{skill}"))
                })?;

            let parts: Vec<&str> = canonical.splitn(3, '/').collect();
            if parts.len() < 3 { return None; }
            let repo = format!("{}/{}", parts[0], parts[1]);
            let skill_path = parts[2];
            let name = item.name.unwrap_or_else(|| skill_path.rsplit('/').next().unwrap_or(skill_path).to_string());
            let installs_label = item.installs.map(|i| format!(" · {i} installs")).unwrap_or_default();

            Some(SkillHubMeta {
                name,
                description: format!("Indexed by skills.sh from {repo}{installs_label}"),
                source: "skills.sh".into(),
                identifier: Self::wrap_id(&canonical),
                trust_level: self.github.trust_level_for(&canonical),
                repo: Some(repo.clone()),
                path: Some(skill_path.to_string()),
                tags: vec![],
                extra: {
                    let mut m = HashMap::new();
                    m.insert("detail_url".into(), serde_json::json!(format!("{BASE_URL}/{canonical}")));
                    m.insert("repo_url".into(), serde_json::json!(format!("https://github.com/{repo}")));
                    m
                },
            })
        }).collect()
    }

    async fn fetch(&self, identifier: &str) -> Option<SkillBundle> {
        let canonical = Self::normalize_id(identifier);
        let parts: Vec<&str> = canonical.splitn(3, '/').collect();
        if parts.len() < 3 {
            tracing::warn!(identifier, "skills-sh fetch: identifier has < 3 parts");
            return None;
        }
        let repo = format!("{}/{}", parts[0], parts[1]);
        let skill_token = parts[2].rsplit('/').next().unwrap_or(parts[2]);

        tracing::info!(identifier, canonical, repo, skill_token, "skills-sh fetch: using tree lookup");

        // Single tree fetch for the repo, then find the skill in it
        if let Some(mut bundle) = self.github.fetch_by_tree_lookup(&repo, skill_token).await {
            bundle.source = "skills.sh".into();
            bundle.identifier = Self::wrap_id(canonical);
            return Some(bundle);
        }

        // Fallback: try direct GitHub fetch with candidate paths
        tracing::warn!(identifier, canonical, "skills-sh fetch: tree lookup failed, trying candidate paths");
        for candidate in Self::candidate_identifiers(canonical) {
            tracing::info!(candidate, "skills-sh fetch: trying candidate via github.fetch");
            if let Some(mut bundle) = self.github.fetch(&candidate).await {
                bundle.source = "skills.sh".into();
                bundle.identifier = Self::wrap_id(canonical);
                return Some(bundle);
            }
        }

        tracing::warn!(identifier, canonical, "skills-sh fetch: all attempts failed");
        None
    }

    async fn inspect(&self, identifier: &str) -> Option<SkillHubMeta> {
        let canonical = Self::normalize_id(identifier);
        for candidate in Self::candidate_identifiers(canonical) {
            if let Some(mut meta) = self.github.inspect(&candidate).await {
                meta.source = "skills.sh".into();
                meta.identifier = Self::wrap_id(canonical);
                meta.trust_level = self.trust_level_for(identifier);
                return Some(meta);
            }
        }
        None
    }
}

#[derive(Deserialize)]
struct SearchResponse {
    skills: Option<Vec<SearchItem>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchItem {
    id: Option<String>,
    name: Option<String>,
    source: Option<String>,
    skill_id: Option<String>,
    installs: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_prefix() {
        assert_eq!(SkillsShSource::normalize_id("skills-sh/owner/repo/skill"), "owner/repo/skill");
        assert_eq!(SkillsShSource::normalize_id("skills.sh/owner/repo/skill"), "owner/repo/skill");
        assert_eq!(SkillsShSource::normalize_id("owner/repo/skill"), "owner/repo/skill");
    }

    #[test]
    fn candidate_identifiers_generates_paths() {
        let candidates = SkillsShSource::candidate_identifiers("owner/repo/my-skill");
        assert!(candidates.contains(&"owner/repo/my-skill".to_string()));
        assert!(candidates.contains(&"owner/repo/skills/my-skill".to_string()));
        assert!(candidates.contains(&"owner/repo/.agents/skills/my-skill".to_string()));
    }

    #[test]
    fn wrap_id_adds_prefix() {
        assert_eq!(SkillsShSource::wrap_id("o/r/s"), "skills-sh/o/r/s");
    }
}
