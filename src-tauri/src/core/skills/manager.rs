//! Skills manager with per-cwd caching.
//!
//! Manages skill loading, caching, and lifecycle. Supports both synchronous
//! and async loading paths, extra user roots, and disabled-path tracking.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use tracing::info;

use super::invocation_utils::build_implicit_skill_path_indexes;
use super::loader::{load_skills_from_roots, skill_roots_for_cwd, SkillRoot};
use super::model::{SkillLoadOutcome, SkillScope};
use super::system;

/// Manages skill loading with per-cwd caching.
pub struct SkillsManager {
    codex_home: PathBuf,
    cache_by_cwd: RwLock<HashMap<PathBuf, SkillLoadOutcome>>,
}

impl SkillsManager {
    pub fn new(codex_home: PathBuf) -> Self {
        if let Err(e) = system::install_system_skills(&codex_home) {
            tracing::warn!("failed to install system skills: {e}");
        }
        Self {
            codex_home,
            cache_by_cwd: RwLock::new(HashMap::new()),
        }
    }

    /// Load skills for a given working directory, using cache if available.
    pub fn skills_for_cwd(&self, cwd: &Path, force_reload: bool) -> SkillLoadOutcome {
        if !force_reload {
            if let Some(cached) = self.cached(cwd) {
                return cached;
            }
        }
        let roots = skill_roots_for_cwd(&self.codex_home, cwd);
        let outcome = finalize_outcome(load_skills_from_roots(roots));
        self.store(cwd, outcome.clone());
        outcome
    }

    /// Load skills with additional user-provided root directories.
    pub fn skills_for_cwd_with_extra_roots(
        &self,
        cwd: &Path,
        extra_roots: &[PathBuf],
        force_reload: bool,
    ) -> SkillLoadOutcome {
        if !force_reload {
            if let Some(cached) = self.cached(cwd) {
                return cached;
            }
        }
        let mut roots = skill_roots_for_cwd(&self.codex_home, cwd);
        roots.extend(
            normalize_extra_roots(extra_roots)
                .into_iter()
                .map(|p| SkillRoot {
                    path: p,
                    scope: SkillScope::User,
                }),
        );
        let mut outcome = load_skills_from_roots(roots);
        if !extra_roots.is_empty() {
            outcome.skills.retain(|s| s.scope != SkillScope::System);
        }
        let outcome = finalize_outcome(outcome);
        self.store(cwd, outcome.clone());
        outcome
    }

    /// Async variant of `skills_for_cwd` for use in async contexts.
    pub async fn skills_for_cwd_async(&self, cwd: &Path, force_reload: bool) -> SkillLoadOutcome {
        self.skills_for_cwd(cwd, force_reload)
    }

    /// Clear the entire skills cache.
    pub fn clear_cache(&self) {
        let mut cache = self.cache_by_cwd.write().unwrap_or_else(|e| e.into_inner());
        let count = cache.len();
        cache.clear();
        info!("skills cache cleared ({count} entries)");
    }

    /// Apply disabled paths to an existing outcome.
    pub fn apply_disabled_paths(&self, outcome: &mut SkillLoadOutcome, disabled: HashSet<PathBuf>) {
        outcome.disabled_paths = disabled;
    }

    fn cached(&self, cwd: &Path) -> Option<SkillLoadOutcome> {
        self.cache_by_cwd
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(cwd)
            .cloned()
    }

    fn store(&self, cwd: &Path, outcome: SkillLoadOutcome) {
        let mut cache = self.cache_by_cwd.write().unwrap_or_else(|e| e.into_inner());
        cache.insert(cwd.to_path_buf(), outcome);
    }
}

fn finalize_outcome(mut outcome: SkillLoadOutcome) -> SkillLoadOutcome {
    let implicit = outcome.allowed_skills_for_implicit_invocation();
    let (by_scripts, by_doc) = build_implicit_skill_path_indexes(implicit);
    outcome.implicit_skills_by_scripts_dir = Arc::new(by_scripts);
    outcome.implicit_skills_by_doc_path = Arc::new(by_doc);
    outcome
}

/// Build a set of disabled skill paths from a list of `(path, enabled)` entries.
/// This mirrors the source project's `disabled_paths_from_stack` but works with
/// a flat list instead of a `ConfigLayerStack`.
pub fn disabled_paths_from_entries(entries: &[(PathBuf, bool)]) -> HashSet<PathBuf> {
    let mut configs: HashMap<PathBuf, bool> = HashMap::new();
    for (path, enabled) in entries {
        let normalized = dunce::canonicalize(path).unwrap_or_else(|_| path.clone());
        configs.insert(normalized, *enabled);
    }
    configs
        .into_iter()
        .filter(|(_, enabled)| !enabled)
        .map(|(path, _)| path)
        .collect()
}

fn normalize_extra_roots(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut normalized: Vec<PathBuf> = roots
        .iter()
        .map(|p| dunce::canonicalize(p).unwrap_or_else(|_| p.clone()))
        .collect();
    normalized.sort_unstable();
    normalized.dedup();
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write_skill(root: &Path, name: &str) {
        let dir = root.join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {name} skill\n---\n"),
        )
        .unwrap();
    }

    #[test]
    fn caches_by_cwd() {
        let home = TempDir::new().unwrap();
        let cwd = TempDir::new().unwrap();
        let skills_dir = home.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        write_skill(&skills_dir, "alpha");

        let mgr = SkillsManager::new(home.path().to_path_buf());
        let o1 = mgr.skills_for_cwd(cwd.path(), false);
        assert!(!o1.skills.is_empty());

        write_skill(&skills_dir, "beta");
        let o2 = mgr.skills_for_cwd(cwd.path(), false);
        assert_eq!(o1.skills.len(), o2.skills.len());

        let o3 = mgr.skills_for_cwd(cwd.path(), true);
        assert!(o3.skills.len() > o1.skills.len());
    }

    #[test]
    fn clear_cache_works() {
        let home = TempDir::new().unwrap();
        let cwd = TempDir::new().unwrap();
        let mgr = SkillsManager::new(home.path().to_path_buf());
        let _ = mgr.skills_for_cwd(cwd.path(), false);
        mgr.clear_cache();
        let _ = mgr.skills_for_cwd(cwd.path(), false);
    }

    #[test]
    fn extra_roots() {
        let home = TempDir::new().unwrap();
        let cwd = TempDir::new().unwrap();
        let extra = TempDir::new().unwrap();
        write_skill(extra.path(), "extra-skill");

        let mgr = SkillsManager::new(home.path().to_path_buf());
        let outcome =
            mgr.skills_for_cwd_with_extra_roots(cwd.path(), &[extra.path().to_path_buf()], true);
        assert!(outcome.skills.iter().any(|s| s.name == "extra-skill"));
    }

    #[test]
    fn normalize_extra_roots_deduplicates() {
        let a = PathBuf::from("/tmp/a");
        let b = PathBuf::from("/tmp/b");
        let first = normalize_extra_roots(&[a.clone(), b.clone(), a.clone()]);
        let second = normalize_extra_roots(&[b, a]);
        assert_eq!(first, second);
    }

    #[test]
    fn disabled_paths_from_entries_filters_disabled() {
        let entries = vec![
            (PathBuf::from("/tmp/a/SKILL.md"), false),
            (PathBuf::from("/tmp/b/SKILL.md"), true),
            (PathBuf::from("/tmp/c/SKILL.md"), false),
        ];
        let disabled = disabled_paths_from_entries(&entries);
        assert!(disabled.contains(&PathBuf::from("/tmp/a/SKILL.md")));
        assert!(!disabled.contains(&PathBuf::from("/tmp/b/SKILL.md")));
        assert!(disabled.contains(&PathBuf::from("/tmp/c/SKILL.md")));
    }

    #[test]
    fn disabled_paths_later_entry_overrides_earlier() {
        let entries = vec![
            (PathBuf::from("/tmp/a/SKILL.md"), false),
            (PathBuf::from("/tmp/a/SKILL.md"), true),
        ];
        let disabled = disabled_paths_from_entries(&entries);
        assert!(!disabled.contains(&PathBuf::from("/tmp/a/SKILL.md")));
    }
}
