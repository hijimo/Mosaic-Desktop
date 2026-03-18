use std::path::PathBuf;

use super::model::SkillMetadata;

/// Simplified permission profile compiled from a skill's declared permissions.
/// On Mosaic we don't have the full seatbelt/sandbox infrastructure, so this
/// captures the intent: what filesystem and network access the skill requests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillPermissions {
    /// Whether the skill requests network access.
    pub network: bool,
    /// Readable filesystem paths.
    pub readable_paths: Vec<PathBuf>,
    /// Writable filesystem paths.
    pub writable_paths: Vec<PathBuf>,
}

impl Default for SkillPermissions {
    fn default() -> Self {
        Self {
            network: false,
            readable_paths: Vec::new(),
            writable_paths: Vec::new(),
        }
    }
}

/// Compile a skill's `permission_profile` field into a [`SkillPermissions`].
///
/// In the source project this maps to full seatbelt sandbox policies. Here we
/// extract the declared intent so callers can enforce it as appropriate.
pub fn compile_skill_permissions(skill: &SkillMetadata) -> Option<SkillPermissions> {
    let profile = skill.permission_profile.as_deref()?;
    match profile {
        "network" => Some(SkillPermissions {
            network: true,
            ..Default::default()
        }),
        "elevated" => Some(SkillPermissions {
            network: true,
            readable_paths: vec![],
            writable_paths: vec![],
        }),
        "read-only" | "readonly" => Some(SkillPermissions::default()),
        _ => {
            tracing::warn!("unknown skill permission profile: {profile}");
            None
        }
    }
}

/// Check whether a skill's declared dependencies include env-var requirements.
pub fn collect_env_var_dependencies(skill: &SkillMetadata) -> Vec<EnvVarDependency> {
    let Some(deps) = &skill.dependencies else {
        return Vec::new();
    };
    deps.tools
        .iter()
        .filter(|t| t.r#type == "env_var" && !t.value.is_empty())
        .map(|t| EnvVarDependency {
            skill_name: skill.name.clone(),
            var_name: t.value.clone(),
            description: t.description.clone(),
        })
        .collect()
}

/// An environment variable required by a skill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvVarDependency {
    pub skill_name: String,
    pub var_name: String,
    pub description: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::skills::model::*;

    fn make_skill(profile: Option<&str>) -> SkillMetadata {
        SkillMetadata {
            name: "test".into(),
            short_description: None,
            description: "test".into(),
            version: "1.0".into(),
            triggers: vec![],
            interface: None,
            dependencies: None,
            policy: None,
            permission_profile: profile.map(String::from),
            path_to_skills_md: PathBuf::from("/tmp/SKILL.md"),
            scope: SkillScope::Repo,
        }
    }

    #[test]
    fn no_profile_returns_none() {
        assert!(compile_skill_permissions(&make_skill(None)).is_none());
    }

    #[test]
    fn network_profile() {
        let perms = compile_skill_permissions(&make_skill(Some("network"))).unwrap();
        assert!(perms.network);
    }

    #[test]
    fn readonly_profile() {
        let perms = compile_skill_permissions(&make_skill(Some("read-only"))).unwrap();
        assert!(!perms.network);
    }

    #[test]
    fn env_var_deps() {
        let mut skill = make_skill(None);
        skill.dependencies = Some(SkillDependencies {
            tools: vec![
                SkillToolDependency {
                    r#type: "env_var".into(),
                    value: "API_KEY".into(),
                    description: Some("API key".into()),
                    transport: None,
                    command: None,
                    url: None,
                },
                SkillToolDependency {
                    r#type: "tool".into(),
                    value: "cargo".into(),
                    description: None,
                    transport: None,
                    command: None,
                    url: None,
                },
            ],
        });
        let deps = collect_env_var_dependencies(&skill);
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].var_name, "API_KEY");
    }
}
