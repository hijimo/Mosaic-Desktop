//! GitHub source adapter — mirrors Hermes `GitHubSource` + `GitHubAuth`.

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;

use super::super::models::*;
use super::traits::SkillSource;

// ---------------------------------------------------------------------------
// GitHubAuth — mirrors Hermes GitHubAuth (PAT / gh CLI / anonymous)
// ---------------------------------------------------------------------------

pub struct GitHubAuth {
    cached_token: Mutex<Option<String>>,
}

impl GitHubAuth {
    pub fn new() -> Self {
        Self { cached_token: Mutex::new(None) }
    }

    pub fn get_headers(&self) -> Vec<(&'static str, String)> {
        let token = self.resolve_token();
        let mut headers = vec![("Accept", "application/vnd.github.v3+json".into())];
        if let Some(ref t) = token {
            tracing::debug!(token_len = t.len(), "GitHubAuth: using token");
            headers.push(("Authorization", format!("token {t}")));
        } else {
            tracing::warn!("GitHubAuth: no token found, using anonymous access");
        }
        headers
    }

    fn resolve_token(&self) -> Option<String> {
        let mut cached = self.cached_token.lock().unwrap();
        if let Some(ref t) = *cached {
            return Some(t.clone());
        }
        // 1. Environment variable
        if let Ok(t) = std::env::var("GITHUB_TOKEN") {
            *cached = Some(t.clone());
            return Some(t);
        }
        if let Ok(t) = std::env::var("GH_TOKEN") {
            *cached = Some(t.clone());
            return Some(t);
        }
        // 2. gh CLI
        if let Some(t) = Self::try_gh_cli() {
            *cached = Some(t.clone());
            return Some(t);
        }
        None
    }

    fn try_gh_cli() -> Option<String> {
        std::process::Command::new("gh")
            .args(["auth", "token"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }
}

// ---------------------------------------------------------------------------
// GitHubSource
// ---------------------------------------------------------------------------

pub struct GitHubSource {
    auth: GitHubAuth,
    client: Client,
    taps: Vec<TapConfig>,
    rate_limited: Mutex<bool>,
}

#[derive(Clone)]
struct TapConfig {
    repo: String,
    path: String,
}

impl GitHubSource {
    pub fn new(extra_taps: Vec<(String, String)>) -> Self {
        let mut taps = vec![
            TapConfig { repo: "openai/skills".into(), path: "skills/".into() },
            TapConfig { repo: "anthropics/skills".into(), path: "skills/".into() },
        ];
        for (repo, path) in extra_taps {
            taps.push(TapConfig { repo, path });
        }
        Self {
            auth: GitHubAuth::new(),
            client: Client::new(),
            taps,
            rate_limited: Mutex::new(false),
        }
    }

    pub fn is_rate_limited(&self) -> bool {
        *self.rate_limited.lock().unwrap()
    }

    fn build_request(&self, url: &str) -> reqwest::RequestBuilder {
        let mut req = self.client.get(url)
            .header("User-Agent", "mosaic-desktop/1.0");
        for (k, v) in self.auth.get_headers() {
            req = req.header(k, v);
        }
        req
    }

    fn check_rate_limit(&self, headers: &reqwest::header::HeaderMap) {
        if let Some(remaining) = headers.get("X-RateLimit-Remaining") {
            if remaining.to_str().unwrap_or("1") == "0" {
                *self.rate_limited.lock().unwrap() = true;
            }
        }
    }

    /// Fetch a single file's raw content from GitHub.
    async fn fetch_file_content(&self, repo: &str, path: &str) -> Option<String> {
        let url = format!("https://api.github.com/repos/{repo}/contents/{path}");
        let resp = match self.build_request(&url)
            .header("Accept", "application/vnd.github.v3.raw")
            .timeout(std::time::Duration::from_secs(15))
            .send().await {
            Ok(r) => r,
            Err(e) => { tracing::warn!(repo, path, error = %e, "fetch_file_content: request failed"); return None; }
        };
        if resp.status() == 403 { self.check_rate_limit(resp.headers()); tracing::warn!(repo, path, "fetch_file_content: 403"); }
        if !resp.status().is_success() { tracing::warn!(repo, path, status = %resp.status(), "fetch_file_content: non-success"); return None; }
        resp.text().await.ok()
    }

    /// Get recursive tree for a repo (single API call).
    async fn get_repo_tree(&self, repo: &str) -> Option<(String, Vec<TreeEntry>)> {
        // Get default branch
        let repo_url = format!("https://api.github.com/repos/{repo}");
        let resp = self.build_request(&repo_url)
            .timeout(std::time::Duration::from_secs(15))
            .send().await.ok()?;
        if !resp.status().is_success() {
            let status = resp.status();
            if status == 403 { self.check_rate_limit(resp.headers()); }
            tracing::warn!(repo, %status, "get_repo_tree: repo info failed");
            return None;
        }
        let repo_data: serde_json::Value = resp.json().await.ok()?;
        let branch = repo_data["default_branch"].as_str().unwrap_or("main");

        // Get tree
        let tree_url = format!("https://api.github.com/repos/{repo}/git/trees/{branch}?recursive=1");
        let resp = self.build_request(&tree_url)
            .timeout(std::time::Duration::from_secs(30))
            .send().await.ok()?;
        if resp.status() == 403 { self.check_rate_limit(resp.headers()); tracing::warn!(repo, "get_repo_tree: 403 on tree"); }
        if !resp.status().is_success() { tracing::warn!(repo, status = %resp.status(), "get_repo_tree: tree fetch failed"); return None; }
        let tree_data: TreeResponse = resp.json().await.ok()?;
        if tree_data.truncated.unwrap_or(false) { tracing::warn!(repo, "get_repo_tree: tree truncated"); return None; }
        tracing::info!(repo, branch, entries = tree_data.tree.len(), "get_repo_tree: success");
        Some((branch.to_string(), tree_data.tree))
    }

    /// Download all files in a directory using the tree API.
    async fn download_directory(&self, repo: &str, path: &str) -> HashMap<String, String> {
        let path = path.trim_end_matches('/');
        let tree = self.get_repo_tree(repo).await;
        if let Some((_branch, entries)) = tree {
            let prefix = format!("{path}/");
            let matching: Vec<_> = entries.iter().filter(|e| e.r#type == "blob" && e.path.starts_with(&prefix)).collect();
            tracing::debug!(repo, path, tree_entries = entries.len(), matching = matching.len(), "download_directory: tree API result");
            let mut files = HashMap::new();
            for entry in &matching {
                let rel = &entry.path[prefix.len()..];
                if let Some(content) = self.fetch_file_content(repo, &entry.path).await {
                    files.insert(rel.to_string(), content);
                }
            }
            if !files.is_empty() { return files; }
        } else {
            tracing::debug!(repo, path, "download_directory: tree API returned None, falling back to Contents API");
        }
        // Fallback: Contents API
        self.download_directory_recursive(repo, path).await
    }

    async fn download_directory_recursive(&self, repo: &str, path: &str) -> HashMap<String, String> {
        Box::pin(self.download_directory_recursive_inner(repo, path)).await
    }

    fn download_directory_recursive_inner<'a>(&'a self, repo: &'a str, path: &'a str) -> std::pin::Pin<Box<dyn std::future::Future<Output = HashMap<String, String>> + Send + 'a>> {
        Box::pin(async move {
        let url = format!("https://api.github.com/repos/{repo}/contents/{}", path.trim_end_matches('/'));
        let resp = match self.build_request(&url)
            .timeout(std::time::Duration::from_secs(15))
            .send().await {
            Ok(r) if r.status().is_success() => r,
            _ => return HashMap::new(),
        };
        let entries: Vec<ContentsEntry> = match resp.json().await {
            Ok(e) => e,
            Err(_) => return HashMap::new(),
        };
        let mut files = HashMap::new();
        for entry in entries {
            match entry.r#type.as_str() {
                "file" => {
                    if let Some(content) = self.fetch_file_content(repo, &entry.path).await {
                        files.insert(entry.name, content);
                    }
                }
                "dir" => {
                    let sub = self.download_directory_recursive_inner(repo, &entry.path).await;
                    for (k, v) in sub {
                        files.insert(format!("{}/{k}", entry.name), v);
                    }
                }
                _ => {}
            }
        }
        files
        })
    }

    /// Parse YAML frontmatter quickly (for inspect).
    fn parse_frontmatter_quick(content: &str) -> HashMap<String, String> {


        let mut map = HashMap::new();
        if !content.starts_with("---") { return map; }
        let rest = &content[3..];
        let end = rest.find("\n---");
        if let Some(pos) = end {
            let yaml = &rest[..pos];
            for line in yaml.lines() {
                if let Some((k, v)) = line.split_once(':') {
                    let k = k.trim();
                    let v = v.trim().trim_matches('"').trim_matches('\'');
                    if !k.is_empty() && !v.is_empty() {
                        map.insert(k.to_string(), v.to_string());
                    }
                }
            }
        }
        map
    }

    /// Fetch a skill by finding it in the repo tree (single API call for discovery).
    /// Looks for `<skill_name>/SKILL.md` anywhere in the tree.
    pub async fn fetch_by_tree_lookup(&self, repo: &str, skill_name: &str) -> Option<SkillBundle> {
        let (_, entries) = self.get_repo_tree(repo).await?;

        // Find SKILL.md inside a directory named skill_name
        let suffix = format!("/{skill_name}/SKILL.md");
        let root_match = format!("{skill_name}/SKILL.md");
        let skill_dir = entries.iter().find_map(|e| {
            if e.r#type != "blob" { return None; }
            if e.path.ends_with(&suffix) || e.path == root_match {
                Some(e.path.trim_end_matches("/SKILL.md"))
            } else {
                None
            }
        })?;

        tracing::info!(repo, skill_dir, "fetch_by_tree_lookup: found skill directory");

        // Collect all files under that directory
        let prefix = format!("{skill_dir}/");
        let mut files = HashMap::new();
        for entry in &entries {
            if entry.r#type != "blob" || !entry.path.starts_with(&prefix) { continue; }
            let rel = &entry.path[prefix.len()..];
            if let Some(content) = self.fetch_file_content(repo, &entry.path).await {
                files.insert(rel.to_string(), content);
            }
        }

        if !files.contains_key("SKILL.md") { return None; }

        let trust = self.trust_level_for(&format!("{repo}/{skill_dir}"));
        Some(SkillBundle {
            name: skill_name.to_string(),
            files,
            source: "github".into(),
            identifier: format!("{repo}/{skill_dir}"),
            trust_level: trust,
            metadata: HashMap::new(),
        })
    }

}

#[derive(Deserialize)]
struct TreeResponse {
    tree: Vec<TreeEntry>,
    truncated: Option<bool>,
}

#[derive(Deserialize)]
struct TreeEntry {
    path: String,
    r#type: String,
}

#[derive(Deserialize)]
struct ContentsEntry {
    name: String,
    path: String,
    r#type: String,
}

// ---------------------------------------------------------------------------
// SkillSource impl
// ---------------------------------------------------------------------------

#[async_trait]
impl SkillSource for GitHubSource {
    fn source_id(&self) -> &str { "github" }

    fn trust_level_for(&self, identifier: &str) -> TrustLevel {
        let parts: Vec<&str> = identifier.splitn(3, '/').collect();
        if parts.len() >= 2 {
            let repo = format!("{}/{}", parts[0], parts[1]);
            if TRUSTED_REPOS.contains(&repo.as_str()) {
                return TrustLevel::Trusted;
            }
        }
        TrustLevel::Community
    }

    async fn search(&self, query: &str, limit: usize) -> Vec<SkillHubMeta> {
        let query_lower = query.to_lowercase();
        let mut results = Vec::new();

        for tap in &self.taps {
            let url = format!(
                "https://api.github.com/repos/{}/contents/{}",
                tap.repo, tap.path.trim_end_matches('/')
            );
            let resp = match self.build_request(&url)
                .timeout(std::time::Duration::from_secs(15))
                .send().await {
                Ok(r) if r.status().is_success() => r,
                Ok(r) => { if r.status().as_u16() == 403 { self.check_rate_limit(r.headers()); } continue; }
                Err(_) => continue,
            };
            let entries: Vec<ContentsEntry> = match resp.json().await {
                Ok(e) => e,
                Err(_) => continue,
            };
            for entry in entries {
                if entry.r#type != "dir" || entry.name.starts_with('.') || entry.name.starts_with('_') {
                    continue;
                }
                let prefix = tap.path.trim_end_matches('/');
                let identifier = if prefix.is_empty() {
                    format!("{}/{}", tap.repo, entry.name)
                } else {
                    format!("{}/{}/{}", tap.repo, prefix, entry.name)
                };
                // Quick inspect for metadata
                if let Some(meta) = self.inspect(&identifier).await {
                    let searchable = format!("{} {} {}", meta.name, meta.description, meta.tags.join(" ")).to_lowercase();
                    if query_lower.is_empty() || searchable.contains(&query_lower) {
                        results.push(meta);
                    }
                }
                if results.len() >= limit { break; }
            }
            if results.len() >= limit { break; }
        }

        // Dedup by name, prefer higher trust
        let mut seen: HashMap<String, SkillHubMeta> = HashMap::new();
        for r in results {
            let existing = seen.get(&r.name);
            if existing.is_none() || r.trust_level.rank() > existing.unwrap().trust_level.rank() {
                seen.insert(r.name.clone(), r);
            }
        }
        seen.into_values().take(limit).collect()
    }

    async fn fetch(&self, identifier: &str) -> Option<SkillBundle> {
        // Skip identifiers meant for other sources
        if identifier.starts_with("skills-sh/") || identifier.starts_with("skills.sh/") {
            return None;
        }
        let parts: Vec<&str> = identifier.splitn(3, '/').collect();
        if parts.len() < 3 {
            tracing::warn!(identifier, "github fetch: identifier has < 3 parts");
            return None;
        }
        let repo = format!("{}/{}", parts[0], parts[1]);
        let skill_path = parts[2];

        tracing::info!(repo, skill_path, "github fetch: downloading directory");
        let files = self.download_directory(&repo, skill_path).await;
        tracing::info!(repo, skill_path, file_count = files.len(), has_skill_md = files.contains_key("SKILL.md"), "github fetch: download result");
        if files.is_empty() || !files.contains_key("SKILL.md") { return None; }

        let skill_name = skill_path.trim_end_matches('/').rsplit('/').next().unwrap_or(skill_path);
        let trust = self.trust_level_for(identifier);

        Some(SkillBundle {
            name: skill_name.to_string(),
            files,
            source: "github".into(),
            identifier: identifier.into(),
            trust_level: trust,
            metadata: HashMap::new(),
        })
    }

    async fn inspect(&self, identifier: &str) -> Option<SkillHubMeta> {
        if identifier.starts_with("skills-sh/") || identifier.starts_with("skills.sh/") {
            return None;
        }
        let parts: Vec<&str> = identifier.splitn(3, '/').collect();
        if parts.len() < 3 { return None; }
        let repo = format!("{}/{}", parts[0], parts[1]);
        let skill_path = parts[2].trim_end_matches('/');
        let skill_md_path = format!("{skill_path}/SKILL.md");

        let content = self.fetch_file_content(&repo, &skill_md_path).await?;
        let fm = Self::parse_frontmatter_quick(&content);
        let skill_name = fm.get("name")
            .cloned()
            .unwrap_or_else(|| skill_path.rsplit('/').next().unwrap_or(skill_path).to_string());
        let description = fm.get("description").cloned().unwrap_or_default();

        Some(SkillHubMeta {
            name: skill_name,
            description,
            source: "github".into(),
            identifier: identifier.into(),
            trust_level: self.trust_level_for(identifier),
            repo: Some(repo),
            path: Some(skill_path.to_string()),
            tags: vec![],
            extra: HashMap::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trust_level_for_trusted_repos() {
        let src = GitHubSource::new(vec![]);
        assert_eq!(src.trust_level_for("openai/skills/some-skill"), TrustLevel::Trusted);
        assert_eq!(src.trust_level_for("anthropics/skills/foo"), TrustLevel::Trusted);
        assert_eq!(src.trust_level_for("random/repo/bar"), TrustLevel::Community);
    }

    #[test]
    fn parse_frontmatter() {
        let content = "---\nname: test-skill\ndescription: A test\n---\n# Hello";
        let fm = GitHubSource::parse_frontmatter_quick(content);
        assert_eq!(fm.get("name").unwrap(), "test-skill");
        assert_eq!(fm.get("description").unwrap(), "A test");
    }

    #[test]
    fn parse_frontmatter_no_frontmatter() {
        let fm = GitHubSource::parse_frontmatter_quick("# Just markdown");
        assert!(fm.is_empty());
    }
}
