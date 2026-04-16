//! Skill source adapters.

pub mod github;
pub mod skills_sh;
pub mod traits;

pub use traits::SkillSource;

use std::collections::HashMap;
use std::sync::Arc;

use super::dirs::TapsManager;
use super::models::*;
use github::GitHubSource;
use skills_sh::SkillsShSource;

/// Create all configured source adapters. Mirrors Hermes `create_source_router()`.
pub fn create_source_router(paths: &HubPaths) -> Vec<Arc<dyn SkillSource>> {
    let taps_mgr = TapsManager::new(paths.taps_file.clone());
    let extra_taps: Vec<(String, String)> = taps_mgr.load()
        .into_iter()
        .map(|t| (t.repo, t.path))
        .collect();

    let github = GitHubSource::new(extra_taps);
    let skills_sh = SkillsShSource::new(GitHubSource::new(vec![]));

    vec![
        Arc::new(skills_sh) as Arc<dyn SkillSource>,
        Arc::new(github) as Arc<dyn SkillSource>,
    ]
}

/// Search all sources in parallel and merge results.
/// Mirrors Hermes `parallel_search_sources()` + `unified_search()`.
pub async fn unified_search(
    sources: &[Arc<dyn SkillSource>],
    query: &str,
    source_filter: &str,
    limit: usize,
) -> Vec<SkillHubMeta> {
    let mut handles = Vec::new();

    for src in sources {
        let sid = src.source_id().to_string();
        if source_filter != "all" && sid != source_filter {
            continue;
        }
        let src = Arc::clone(src);
        let query = query.to_string();
        handles.push(tokio::spawn(async move {
            tokio::time::timeout(
                std::time::Duration::from_secs(30),
                src.search(&query, 50),
            ).await.unwrap_or_default()
        }));
    }

    let mut all_results = Vec::new();
    for handle in handles {
        if let Ok(results) = handle.await {
            all_results.extend(results);
        }
    }

    // Dedup by name, prefer higher trust level
    let mut seen: HashMap<String, SkillHubMeta> = HashMap::new();
    for r in all_results {
        let existing = seen.get(&r.name);
        if existing.is_none() || r.trust_level.rank() > existing.unwrap().trust_level.rank() {
            seen.insert(r.name.clone(), r);
        }
    }

    let mut deduped: Vec<SkillHubMeta> = seen.into_values().collect();
    deduped.sort_by(|a, b| b.trust_level.rank().cmp(&a.trust_level.rank()).then(a.name.cmp(&b.name)));
    deduped.truncate(limit);
    deduped
}
