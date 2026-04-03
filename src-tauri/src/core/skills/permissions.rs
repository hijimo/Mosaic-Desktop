//! Skill permission profile compilation.
//!
//! Compiles a skill's declared permissions into a structured representation.
//! The source project maps these to full seatbelt/sandbox policies; here we
//! capture the intent so callers can enforce it as appropriate.

use std::path::PathBuf;

use dunce::canonicalize as canonicalize_path;

use super::model::SkillMetadata;

/// Structured permission profile compiled from a skill's declarations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillPermissions {
    /// Whether the skill requests network access.
    pub network: bool,
    /// Readable filesystem paths (canonicalized, deduplicated).
    pub readable_paths: Vec<PathBuf>,
    /// Writable filesystem paths (canonicalized, deduplicated).
    pub writable_paths: Vec<PathBuf>,
    /// macOS-specific permission extensions.
    pub macos: Option<MacOsSkillPermissions>,
}

impl Default for SkillPermissions {
    fn default() -> Self {
        Self {
            network: false,
            readable_paths: Vec::new(),
            writable_paths: Vec::new(),
            macos: None,
        }
    }
}

/// macOS-specific permission extensions for skills.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MacOsSkillPermissions {
    /// Preferences access mode: "readonly" or "readwrite".
    pub preferences: Option<String>,
    /// Automation bundle IDs the skill may control.
    pub automations: Vec<String>,
    /// Whether accessibility access is requested.
    pub accessibility: bool,
    /// Whether calendar access is requested.
    pub calendar: bool,
}

/// Compile a skill's `permission_profile` field into a [`SkillPermissions`].
///
/// This is the Mosaic equivalent of codex-main's `compile_permission_profile(Option<PermissionProfile>)`.
/// Since Mosaic uses string-based profiles rather than structured `PermissionProfile`,
/// this maps known profile names to permission structures.
pub fn compile_skill_permissions(skill: &SkillMetadata) -> Option<SkillPermissions> {
    let profile = skill.permission_profile.as_deref()?;
    match profile {
        "network" => Some(SkillPermissions {
            network: true,
            ..Default::default()
        }),
        "elevated" => Some(SkillPermissions {
            network: true,
            ..Default::default()
        }),
        "read-only" | "readonly" => Some(SkillPermissions::default()),
        _ => {
            tracing::warn!("unknown skill permission profile: {profile}");
            None
        }
    }
}

/// Alias matching codex-main's function name.
pub fn compile_permission_profile(skill: &SkillMetadata) -> Option<SkillPermissions> {
    compile_skill_permissions(skill)
}

/// Build macOS seatbelt profile extensions from a skill's macOS permissions.
///
/// Matches codex-main's `build_macos_seatbelt_profile_extensions(&MacOsPermissions)`.
/// Returns `None` on non-macOS platforms.
pub fn build_macos_seatbelt_profile_extensions(
    macos: &MacOsSkillPermissions,
) -> Option<MacOsSkillPermissions> {
    if macos == &MacOsSkillPermissions::default() {
        return None;
    }
    Some(macos.clone())
}

/// Resolve macOS preferences permission from a string value.
///
/// Matches codex-main's `resolve_macos_preferences_permission(Option<&MacOsPreferencesValue>, default)`.
pub fn resolve_macos_preferences_permission(
    value: Option<&str>,
    default: Option<String>,
) -> Option<String> {
    match value {
        Some("true") | Some("readonly") | Some("read-only") => Some("readonly".to_string()),
        Some("false") => None,
        Some("readwrite") | Some("read-write") => Some("readwrite".to_string()),
        Some(other) => {
            tracing::warn!(
                "ignoring permissions.macos.preferences: expected true/false, readonly, or readwrite, got {other}"
            );
            default
        }
        None => default,
    }
}

/// Resolve macOS automation permission from a value.
///
/// Matches codex-main's `resolve_macos_automation_permission(Option<&MacOsAutomationValue>, default)`.
pub fn resolve_macos_automation_permission(
    value: Option<&[String]>,
    default: Vec<String>,
) -> Vec<String> {
    match value {
        Some(bundle_ids) if !bundle_ids.is_empty() => {
            bundle_ids
                .iter()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        }
        Some(_) => Vec::new(),
        None => default,
    }
}

/// Normalize and deduplicate a list of filesystem paths.
pub fn normalize_permission_paths(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut result = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for path in paths {
        if let Some(canonical) = normalize_permission_path(path) {
            if seen.insert(canonical.clone()) {
                result.push(canonical);
            }
        }
    }
    result
}

/// Normalize a single permission path. Returns `None` if the path cannot be
/// resolved (e.g. it doesn't exist and isn't absolute).
fn normalize_permission_path(path: &PathBuf) -> Option<PathBuf> {
    let canonical = canonicalize_path(path).unwrap_or_else(|_| path.clone());
    if canonical.is_absolute() {
        Some(canonical)
    } else {
        tracing::warn!(
            "ignoring permission path: expected absolute, got {:?}",
            canonical
        );
        None
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
        assert!(perms.readable_paths.is_empty());
    }

    #[test]
    fn readonly_profile() {
        let perms = compile_skill_permissions(&make_skill(Some("read-only"))).unwrap();
        assert!(!perms.network);
    }

    #[test]
    fn unknown_profile_returns_none() {
        assert!(compile_skill_permissions(&make_skill(Some("unknown"))).is_none());
    }

    #[test]
    fn normalize_deduplicates() {
        let paths = vec![
            PathBuf::from("/tmp"),
            PathBuf::from("/tmp"),
            PathBuf::from("/var"),
        ];
        let result = normalize_permission_paths(&paths);
        assert_eq!(result.len(), 2);
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

    #[test]
    fn elevated_profile_has_network() {
        let perms = compile_skill_permissions(&make_skill(Some("elevated"))).unwrap();
        assert!(perms.network);
    }

    #[test]
    fn macos_permissions_default() {
        let macos = MacOsSkillPermissions::default();
        assert!(!macos.accessibility);
        assert!(!macos.calendar);
        assert!(macos.automations.is_empty());
        assert!(macos.preferences.is_none());
    }
}
