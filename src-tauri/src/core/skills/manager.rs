use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use tracing::info;

use super::loader::{load_skills_from_roots, SkillRoot};
use super::model::{build_implicit_skill_path_indexes, SkillLoadOutcome, SkillScope};
use super::system;

/// Manages skill loading with per-cwd caching.
pub struct SkillsManager {
    codex_home: PathBuf,
    cache_by_cwd: RwLock<HashMap<PathBuf, SkillLoadOutcome>>,
}

impl SkillsManager {
    pub fn new(codex_home: PathBuf) -> Self {
        // Install/update embedded system skills on construction.
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

        let roots = self.default_roots(cwd);
        let mut outcome = load_skills_from_roots(roots);
        finalize_outcome(&mut outcome);

        let mut cache = self.cache_by_cwd.write().unwrap_or_else(|e| e.into_inner());
        cache.insert(cwd.to_path_buf(), outcome.clone());
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

        let mut roots = self.default_roots(cwd);
        roots.extend(extra_roots.iter().map(|p| SkillRoot {
            path: p.clone(),
            scope: SkillScope::User,
        }));

        let mut outcome = load_skills_from_roots(roots);
        finalize_outcome(&mut outcome);

        let mut cache = self.cache_by_cwd.write().unwrap_or_else(|e| e.into_inner());
        cache.insert(cwd.to_path_buf(), outcome.clone());
        outcome
    }

    /// Clear the entire skills cache.
    pub fn clear_cache(&self) {
        let mut cache = self.cache_by_cwd.write().unwrap_or_else(|e| e.into_inner());
        let count = cache.len();
        cache.clear();
        info!("skills cache cleared ({count} entries)");
    }

    fn cached(&self, cwd: &Path) -> Option<SkillLoadOutcome> {
        self.cache_by_cwd
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(cwd)
            .cloned()
    }

    fn default_roots(&self, cwd: &Path) -> Vec<SkillRoot> {
        let mut roots = Vec::new();

        // System skills: ~/.codex/skills/.system/
        let system_skills = system::system_cache_root_dir(&self.codex_home);
        if system_skills.is_dir() {
            roots.push(SkillRoot {
                path: system_skills,
                scope: SkillScope::System,
            });
        }

        // User-level skills: ~/.codex/skills/
        let user_skills = self.codex_home.join("skills");
        if user_skills.is_dir() {
            roots.push(SkillRoot {
                path: user_skills,
                scope: SkillScope::User,
            });
        }

        // Project-level skills: <cwd>/.codex/skills/
        let project_skills = cwd.join(".codex").join("skills");
        if project_skills.is_dir() {
            roots.push(SkillRoot {
                path: project_skills,
                scope: SkillScope::Repo,
            });
        }

        roots
    }
}

fn finalize_outcome(outcome: &mut SkillLoadOutcome) {
    let implicit = outcome.allowed_skills_for_implicit_invocation();
    let (by_scripts, by_doc) = build_implicit_skill_path_indexes(implicit);
    outcome.implicit_skills_by_scripts_dir = Arc::new(by_scripts);
    outcome.implicit_skills_by_doc_path = Arc::new(by_doc);
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
            format!("---\nname: {name}\ndescription: {name} skill\nversion: 1.0\n---\n"),
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

        // Write new skill — should not appear without force_reload.
        write_skill(&skills_dir, "beta");
        let o2 = mgr.skills_for_cwd(cwd.path(), false);
        assert_eq!(o1.skills.len(), o2.skills.len());

        // Force reload picks up new skill.
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
        // After clear, next call should reload (no panic).
        let _ = mgr.skills_for_cwd(cwd.path(), false);
    }

    #[test]
    fn extra_roots() {
        let home = TempDir::new().unwrap();
        let cwd = TempDir::new().unwrap();
        let extra = TempDir::new().unwrap();
        write_skill(extra.path(), "extra-skill");

        let mgr = SkillsManager::new(home.path().to_path_buf());
        let outcome = mgr.skills_for_cwd_with_extra_roots(
            cwd.path(),
            &[extra.path().to_path_buf()],
            true,
        );
        assert!(outcome.skills.iter().any(|s| s.name == "extra-skill"));
    }
}
