//! Environment variable dependency resolution for skills.
//!
//! Skills can declare `env_var` dependencies in their `openai.yaml` metadata.
//! This module collects those dependencies, checks the environment and a
//! session-level cache, and identifies which values are still missing.

use std::collections::{HashMap, HashSet};
use std::env;

use tracing::warn;

use super::model::SkillMetadata;

/// Information about a single env-var dependency required by a skill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillDependencyInfo {
    pub skill_name: String,
    pub name: String,
    pub description: Option<String>,
}

/// Collect all `env_var` dependencies from the given skills.
pub fn collect_env_var_dependencies(
    mentioned_skills: &[SkillMetadata],
) -> Vec<SkillDependencyInfo> {
    let mut deps = Vec::new();
    for skill in mentioned_skills {
        let Some(skill_deps) = &skill.dependencies else {
            continue;
        };
        for tool in &skill_deps.tools {
            if tool.r#type != "env_var" || tool.value.is_empty() {
                continue;
            }
            deps.push(SkillDependencyInfo {
                skill_name: skill.name.clone(),
                name: tool.value.clone(),
                description: tool.description.clone(),
            });
        }
    }
    deps
}

/// Result of resolving skill dependencies against the environment and cache.
#[derive(Debug, Default)]
pub struct ResolvedDependencies {
    /// Values that were found in the environment (not yet in the session cache).
    pub loaded_from_env: HashMap<String, String>,
    /// Dependencies that are missing from both the cache and the environment.
    pub missing: Vec<SkillDependencyInfo>,
}

/// Resolve required dependency values against a session cache and the environment.
///
/// - `existing_env`: values already stored in the session-level dependency cache.
/// - Returns which values were found in the OS environment (to be added to the
///   cache) and which are still missing (to be prompted from the user).
pub fn resolve_dependencies(
    dependencies: &[SkillDependencyInfo],
    existing_env: &HashMap<String, String>,
) -> ResolvedDependencies {
    if dependencies.is_empty() {
        return ResolvedDependencies::default();
    }

    let mut loaded = HashMap::new();
    let mut missing = Vec::new();
    let mut seen = HashSet::new();

    for dep in dependencies {
        if !seen.insert(dep.name.clone()) {
            continue;
        }
        if existing_env.contains_key(&dep.name) {
            continue;
        }
        match env::var(&dep.name) {
            Ok(value) => {
                loaded.insert(dep.name.clone(), value);
            }
            Err(env::VarError::NotPresent) => {
                missing.push(dep.clone());
            }
            Err(err) => {
                warn!("failed to read env var {}: {err}", dep.name);
                missing.push(dep.clone());
            }
        }
    }

    ResolvedDependencies {
        loaded_from_env: loaded,
        missing,
    }
}

/// Resolve required dependency values for a turn, using a session-level cache
/// and the OS environment. Missing values are collected for prompting.
///
/// Matches codex-main's `resolve_skill_dependencies_for_turn(sess, turn_context, dependencies)`.
///
/// - `dependency_env`: mutable session-level cache of resolved values.
/// - `dependencies`: the env-var dependencies to resolve.
/// - Returns the list of still-missing dependencies after resolution.
pub async fn resolve_skill_dependencies_for_turn(
    dependency_env: &tokio::sync::Mutex<HashMap<String, String>>,
    dependencies: &[SkillDependencyInfo],
) -> Vec<SkillDependencyInfo> {
    if dependencies.is_empty() {
        return Vec::new();
    }

    let existing = dependency_env.lock().await.clone();
    let resolved = resolve_dependencies(dependencies, &existing);

    if !resolved.loaded_from_env.is_empty() {
        let mut env_cache = dependency_env.lock().await;
        env_cache.extend(resolved.loaded_from_env);
    }

    resolved.missing
}

/// Build user-facing prompts for missing skill dependencies.
///
/// Matches codex-main's `request_skill_dependencies(sess, turn_context, dependencies)`.
/// Returns a list of `(id, header, question, is_secret)` tuples suitable for
/// presenting to the user via a UI prompt or elicitation request.
pub fn request_skill_dependencies(
    dependencies: &[SkillDependencyInfo],
) -> Vec<(String, String, String, bool)> {
    dependencies
        .iter()
        .map(|dep| {
            let requirement = dep.description.as_ref().map_or_else(
                || {
                    format!(
                        "The skill \"{}\" requires \"{}\" to be set.",
                        dep.skill_name, dep.name
                    )
                },
                |description| {
                    format!(
                        "The skill \"{}\" requires \"{}\" to be set ({}).",
                        dep.skill_name, dep.name, description
                    )
                },
            );
            let question = format!(
                "{requirement} This is an experimental internal feature. The value is stored in memory for this session only.",
            );
            (
                dep.name.clone(),
                "Skill requires environment variable".to_string(),
                question,
                true, // is_secret
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::skills::model::*;
    use std::path::PathBuf;

    fn make_skill_with_deps(deps: Vec<SkillToolDependency>) -> SkillMetadata {
        SkillMetadata {
            name: "test-skill".into(),
            description: "test".into(),
            short_description: None,
            version: "1.0".into(),
            triggers: vec![],
            interface: None,
            dependencies: Some(SkillDependencies { tools: deps }),
            policy: None,
            permission_profile: None,
            path_to_skills_md: PathBuf::from("/tmp/SKILL.md"),
            scope: SkillScope::User,
        }
    }

    #[test]
    fn collects_env_var_deps_only() {
        let skill = make_skill_with_deps(vec![
            SkillToolDependency {
                r#type: "env_var".into(),
                value: "API_KEY".into(),
                description: Some("key".into()),
                transport: None,
                command: None,
                url: None,
            },
            SkillToolDependency {
                r#type: "mcp".into(),
                value: "github".into(),
                description: None,
                transport: None,
                command: None,
                url: None,
            },
        ]);
        let deps = collect_env_var_dependencies(&[skill]);
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].name, "API_KEY");
    }

    #[test]
    fn resolve_skips_cached_values() {
        let deps = vec![SkillDependencyInfo {
            skill_name: "s".into(),
            name: "CACHED_VAR".into(),
            description: None,
        }];
        let mut cache = HashMap::new();
        cache.insert("CACHED_VAR".into(), "val".into());
        let result = resolve_dependencies(&deps, &cache);
        assert!(result.loaded_from_env.is_empty());
        assert!(result.missing.is_empty());
    }

    #[test]
    fn resolve_deduplicates() {
        let deps = vec![
            SkillDependencyInfo {
                skill_name: "a".into(),
                name: "X".into(),
                description: None,
            },
            SkillDependencyInfo {
                skill_name: "b".into(),
                name: "X".into(),
                description: None,
            },
        ];
        let result = resolve_dependencies(&deps, &HashMap::new());
        // X appears only once in missing (assuming not set in env).
        assert!(result.missing.len() <= 1);
    }

    #[test]
    fn empty_deps_returns_default() {
        let result = resolve_dependencies(&[], &HashMap::new());
        assert!(result.loaded_from_env.is_empty());
        assert!(result.missing.is_empty());
    }
}
