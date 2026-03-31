use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// Scope / priority of a skill root (Repo > User > System > Admin).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum SkillScope {
    Repo,
    User,
    System,
    Admin,
}

impl SkillScope {
    pub fn priority(&self) -> u8 {
        match self {
            Self::Repo => 0,
            Self::User => 1,
            Self::System => 2,
            Self::Admin => 3,
        }
    }
}

/// UI display configuration for a skill.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillInterface {
    pub display_name: Option<String>,
    pub short_description: Option<String>,
    pub icon: Option<String>,
    pub icon_large: Option<String>,
    pub brand_color: Option<String>,
    pub default_prompt: Option<String>,
}

/// A single tool dependency required by a skill.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillToolDependency {
    pub r#type: String,
    pub value: String,
    pub description: Option<String>,
    pub transport: Option<String>,
    pub command: Option<String>,
    pub url: Option<String>,
}

/// Tool dependencies required by a skill.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillDependencies {
    pub tools: Vec<SkillToolDependency>,
}

/// Invocation policy for a skill.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SkillPolicy {
    pub allow_implicit_invocation: Option<bool>,
}

/// Complete metadata for a discovered skill.
#[derive(Debug, Clone, PartialEq)]
pub struct SkillMetadata {
    pub name: String,
    pub short_description: Option<String>,
    pub description: String,
    pub version: String,
    pub triggers: Vec<String>,
    pub interface: Option<SkillInterface>,
    pub dependencies: Option<SkillDependencies>,
    pub policy: Option<SkillPolicy>,
    pub permission_profile: Option<String>,
    pub path_to_skills_md: PathBuf,
    pub scope: SkillScope,
}

impl SkillMetadata {
    pub fn allow_implicit_invocation(&self) -> bool {
        self.policy
            .and_then(|p| p.allow_implicit_invocation)
            .unwrap_or(true)
    }
}

/// Error encountered while loading a skill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillError {
    pub path: PathBuf,
    pub message: String,
}

/// Result of loading skills from multiple roots.
#[derive(Debug, Clone, Default)]
pub struct SkillLoadOutcome {
    pub skills: Vec<SkillMetadata>,
    pub errors: Vec<SkillError>,
    pub disabled_paths: HashSet<PathBuf>,
    pub implicit_skills_by_scripts_dir: Arc<HashMap<PathBuf, SkillMetadata>>,
    pub implicit_skills_by_doc_path: Arc<HashMap<PathBuf, SkillMetadata>>,
}

impl SkillLoadOutcome {
    pub fn is_skill_enabled(&self, skill: &SkillMetadata) -> bool {
        !self.disabled_paths.contains(&skill.path_to_skills_md)
    }

    pub fn is_skill_allowed_for_implicit_invocation(&self, skill: &SkillMetadata) -> bool {
        self.is_skill_enabled(skill) && skill.allow_implicit_invocation()
    }

    pub fn allowed_skills_for_implicit_invocation(&self) -> Vec<SkillMetadata> {
        self.skills
            .iter()
            .filter(|s| self.is_skill_allowed_for_implicit_invocation(s))
            .cloned()
            .collect()
    }

    pub fn skills_with_enabled(&self) -> impl Iterator<Item = (&SkillMetadata, bool)> {
        self.skills.iter().map(|s| (s, self.is_skill_enabled(s)))
    }
}

/// Build path indexes for implicit skill invocation detection.
/// (Moved to invocation_utils.rs — this is kept as a re-export for backward compat.)
pub fn build_implicit_skill_path_indexes(
    skills: Vec<SkillMetadata>,
) -> (
    HashMap<PathBuf, SkillMetadata>,
    HashMap<PathBuf, SkillMetadata>,
) {
    super::invocation_utils::build_implicit_skill_path_indexes(skills)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_skill(name: &str, path: &str) -> SkillMetadata {
        SkillMetadata {
            name: name.into(),
            short_description: None,
            description: format!("{name} skill"),
            version: "1.0".into(),
            triggers: vec![],
            interface: None,
            dependencies: None,
            policy: None,
            permission_profile: None,
            path_to_skills_md: PathBuf::from(path),
            scope: SkillScope::User,
        }
    }

    #[test]
    fn disabled_skill_not_enabled() {
        let skill = make_skill("a", "/tmp/a/SKILL.md");
        let outcome = SkillLoadOutcome {
            skills: vec![skill.clone()],
            disabled_paths: HashSet::from([PathBuf::from("/tmp/a/SKILL.md")]),
            ..Default::default()
        };
        assert!(!outcome.is_skill_enabled(&skill));
    }

    #[test]
    fn implicit_invocation_respects_policy() {
        let mut skill = make_skill("a", "/tmp/a/SKILL.md");
        skill.policy = Some(SkillPolicy {
            allow_implicit_invocation: Some(false),
        });
        let outcome = SkillLoadOutcome {
            skills: vec![skill.clone()],
            ..Default::default()
        };
        assert!(!outcome.is_skill_allowed_for_implicit_invocation(&skill));
    }

    #[test]
    fn build_indexes() {
        let skill = make_skill("a", "/tmp/a/SKILL.md");
        let (by_scripts, by_doc) =
            crate::core::skills::invocation_utils::build_implicit_skill_path_indexes(vec![skill]);
        assert!(by_doc.contains_key(&PathBuf::from("/tmp/a/SKILL.md")));
        assert!(by_scripts.contains_key(&PathBuf::from("/tmp/a/scripts")));
    }
}
