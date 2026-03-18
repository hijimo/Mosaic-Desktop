//! Agent role system — applies role-specific configuration layers to spawned agents.
//!
//! Roles are selected at spawn time and loaded as high-precedence config layers.
//! Built-in roles (default, explorer, worker) are always available; users can
//! define additional roles in their config.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::LazyLock;

/// Default role name when none is specified.
pub const DEFAULT_ROLE_NAME: &str = "default";

/// Configuration for a single agent role.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentRoleConfig {
    /// Human-readable description shown in the spawn tool spec.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Path to a TOML config file that overrides session config for this role.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_file: Option<PathBuf>,
}

/// Resolve a role name to its config, checking user-defined roles first, then built-ins.
/// Returns `(config, is_built_in)`.
pub fn resolve_role(
    role_name: &str,
    user_roles: &BTreeMap<String, AgentRoleConfig>,
) -> Option<(AgentRoleConfig, bool)> {
    if let Some(role) = user_roles.get(role_name) {
        return Some((role.clone(), false));
    }
    built_in_configs()
        .get(role_name)
        .map(|role| (role.clone(), true))
}

/// Apply a named role to a TOML config table, returning the merged result.
///
/// For roles without a `config_file`, this is a no-op.
/// For roles with a `config_file`, the file contents are read and merged
/// as a high-precedence layer.
pub async fn load_role_config_toml(
    role_name: Option<&str>,
    user_roles: &BTreeMap<String, AgentRoleConfig>,
) -> Result<Option<toml::Value>, String> {
    let role_name = role_name.unwrap_or(DEFAULT_ROLE_NAME);
    let (role, is_built_in) = resolve_role(role_name, user_roles)
        .ok_or_else(|| format!("unknown agent_type '{role_name}'"))?;

    let config_file = match &role.config_file {
        Some(f) => f,
        None => return Ok(None),
    };

    let contents = if is_built_in {
        built_in_config_file_contents(config_file)
            .map(str::to_owned)
            .ok_or_else(|| "agent type is currently not available".to_string())?
    } else {
        tokio::fs::read_to_string(config_file)
            .await
            .map_err(|_| "agent type is currently not available".to_string())?
    };

    let toml_value: toml::Value = toml::from_str(&contents)
        .map_err(|_| "agent type is currently not available".to_string())?;

    Ok(Some(toml_value))
}

/// Build the spawn-agent tool description text from built-in and user-defined roles.
pub mod spawn_tool_spec {
    use super::*;

    pub fn build(user_defined_roles: &BTreeMap<String, AgentRoleConfig>) -> String {
        let built_in = built_in_configs();
        build_from_configs(built_in, user_defined_roles)
    }

    fn build_from_configs(
        built_in_roles: &BTreeMap<String, AgentRoleConfig>,
        user_defined_roles: &BTreeMap<String, AgentRoleConfig>,
    ) -> String {
        let mut seen = std::collections::BTreeSet::new();
        let mut formatted = Vec::new();

        // User-defined roles take precedence.
        for (name, decl) in user_defined_roles {
            if seen.insert(name.as_str()) {
                formatted.push(format_role(name, decl));
            }
        }
        for (name, decl) in built_in_roles {
            if seen.insert(name.as_str()) {
                formatted.push(format_role(name, decl));
            }
        }

        format!(
            "Optional type name for the new agent. If omitted, `{DEFAULT_ROLE_NAME}` is used.\nAvailable roles:\n{}",
            formatted.join("\n"),
        )
    }

    fn format_role(name: &str, decl: &AgentRoleConfig) -> String {
        if let Some(desc) = &decl.description {
            format!("{name}: {{\n{desc}\n}}")
        } else {
            format!("{name}: no description")
        }
    }
}

// ── Built-in roles ───────────────────────────────────────────────

fn built_in_configs() -> &'static BTreeMap<String, AgentRoleConfig> {
    static CONFIGS: LazyLock<BTreeMap<String, AgentRoleConfig>> = LazyLock::new(|| {
        BTreeMap::from([
            (
                DEFAULT_ROLE_NAME.to_string(),
                AgentRoleConfig {
                    description: Some("Default agent.".to_string()),
                    config_file: None,
                },
            ),
            (
                "explorer".to_string(),
                AgentRoleConfig {
                    description: Some(
                        r#"Use `explorer` for specific codebase questions.
Explorers are fast and authoritative.
They must be used to ask specific, well-scoped questions on the codebase.
Rules:
- Do not re-read or re-search code they cover.
- Trust explorer results without verification.
- Run explorers in parallel when useful.
- Reuse existing explorers for related questions."#
                            .to_string(),
                    ),
                    config_file: Some(PathBuf::from("explorer.toml")),
                },
            ),
            (
                "worker".to_string(),
                AgentRoleConfig {
                    description: Some(
                        r#"Use for execution and production work.
Typical tasks:
- Implement part of a feature
- Fix tests or bugs
- Split large refactors into independent chunks
Rules:
- Explicitly assign ownership of the task (files / responsibility).
- Always tell workers they are not alone in the codebase, and they should ignore edits made by others without touching them."#
                            .to_string(),
                    ),
                    config_file: None,
                },
            ),
        ])
    });
    &CONFIGS
}

fn built_in_config_file_contents(path: &std::path::Path) -> Option<&'static str> {
    match path.to_str()? {
        "explorer.toml" => Some(include_str!("builtins/explorer.toml")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_built_in_default_role() {
        let user_roles = BTreeMap::new();
        let (role, is_built_in) = resolve_role("default", &user_roles).unwrap();
        assert!(is_built_in);
        assert!(role.config_file.is_none());
    }

    #[test]
    fn resolve_built_in_explorer_role() {
        let user_roles = BTreeMap::new();
        let (role, is_built_in) = resolve_role("explorer", &user_roles).unwrap();
        assert!(is_built_in);
        assert!(role.config_file.is_some());
    }

    #[test]
    fn resolve_unknown_role_returns_none() {
        let user_roles = BTreeMap::new();
        assert!(resolve_role("nonexistent", &user_roles).is_none());
    }

    #[test]
    fn user_defined_role_overrides_built_in() {
        let mut user_roles = BTreeMap::new();
        user_roles.insert(
            "explorer".to_string(),
            AgentRoleConfig {
                description: Some("user override".to_string()),
                config_file: None,
            },
        );
        let (role, is_built_in) = resolve_role("explorer", &user_roles).unwrap();
        assert!(!is_built_in);
        assert_eq!(role.description.as_deref(), Some("user override"));
    }

    #[test]
    fn spawn_tool_spec_deduplicates_roles() {
        let mut user_roles = BTreeMap::new();
        user_roles.insert(
            "explorer".to_string(),
            AgentRoleConfig {
                description: Some("user override".to_string()),
                config_file: None,
            },
        );
        user_roles.insert("researcher".to_string(), AgentRoleConfig::default());

        let spec = spawn_tool_spec::build(&user_roles);
        assert!(spec.contains("researcher: no description"));
        assert!(spec.contains("explorer: {\nuser override\n}"));
        assert!(spec.contains("default: {\nDefault agent.\n}"));
        // Built-in explorer description should NOT appear.
        assert!(!spec.contains("Explorers are fast and authoritative."));
    }

    #[test]
    fn spawn_tool_spec_user_roles_before_built_ins() {
        let mut user_roles = BTreeMap::new();
        user_roles.insert(
            "aaa".to_string(),
            AgentRoleConfig {
                description: Some("first".to_string()),
                config_file: None,
            },
        );

        let spec = spawn_tool_spec::build(&user_roles);
        let user_idx = spec.find("aaa: {\nfirst\n}").expect("find user role");
        let built_in_idx = spec
            .find("default: {\nDefault agent.\n}")
            .expect("find built-in");
        assert!(user_idx < built_in_idx);
    }

    #[tokio::test]
    async fn load_role_config_default_returns_none() {
        let user_roles = BTreeMap::new();
        let result = load_role_config_toml(None, &user_roles).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn load_role_config_unknown_returns_error() {
        let user_roles = BTreeMap::new();
        let err = load_role_config_toml(Some("missing"), &user_roles)
            .await
            .unwrap_err();
        assert!(err.contains("unknown agent_type"));
    }

    #[tokio::test]
    async fn load_role_config_missing_file_returns_error() {
        let mut user_roles = BTreeMap::new();
        user_roles.insert(
            "custom".to_string(),
            AgentRoleConfig {
                description: None,
                config_file: Some(PathBuf::from("/nonexistent/role.toml")),
            },
        );
        let err = load_role_config_toml(Some("custom"), &user_roles)
            .await
            .unwrap_err();
        assert_eq!(err, "agent type is currently not available");
    }

    #[tokio::test]
    async fn load_role_config_valid_user_file() {
        let dir = tempfile::tempdir().unwrap();
        let role_path = dir.path().join("test-role.toml");
        tokio::fs::write(&role_path, "model = \"test-model\"\n")
            .await
            .unwrap();

        let mut user_roles = BTreeMap::new();
        user_roles.insert(
            "custom".to_string(),
            AgentRoleConfig {
                description: None,
                config_file: Some(role_path),
            },
        );

        let result = load_role_config_toml(Some("custom"), &user_roles)
            .await
            .unwrap();
        assert!(result.is_some());
        let toml = result.unwrap();
        assert_eq!(
            toml.get("model").and_then(|v| v.as_str()),
            Some("test-model")
        );
    }
}
