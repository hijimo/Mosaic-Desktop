use std::collections::{HashMap, HashSet};

use tracing::warn;

use crate::config::toml_types::{McpServerConfig, McpServerTransportConfig};
use crate::core::skills::model::{SkillMetadata, SkillToolDependency};

/// Collect MCP server dependencies from mentioned skills that are not yet installed.
///
/// Uses canonical keys (transport + identifier) to match dependencies against
/// installed servers, so a dependency is satisfied even if the server name differs.
pub fn collect_missing_mcp_dependencies(
    mentioned_skills: &[SkillMetadata],
    installed: &HashMap<String, McpServerConfig>,
) -> HashMap<String, McpServerConfig> {
    let installed_keys: HashSet<String> = installed
        .iter()
        .map(|(name, config)| canonical_server_key(name, config))
        .collect();
    let mut seen_keys = HashSet::new();
    let mut missing = HashMap::new();

    for skill in mentioned_skills {
        let Some(deps) = skill.dependencies.as_ref() else {
            continue;
        };
        for tool in &deps.tools {
            if !tool.r#type.eq_ignore_ascii_case("mcp") {
                continue;
            }
            let dep_key = match canonical_dependency_key(tool) {
                Ok(k) => k,
                Err(e) => {
                    warn!(
                        "unable to resolve MCP dependency {} for skill {}: {e}",
                        tool.value, skill.name
                    );
                    continue;
                }
            };
            if installed_keys.contains(&dep_key) || seen_keys.contains(&dep_key) {
                continue;
            }
            let config = match dependency_to_server_config(tool) {
                Ok(c) => c,
                Err(e) => {
                    warn!(
                        "unable to build config for MCP dependency {} for skill {}: {e}",
                        tool.value, skill.name
                    );
                    continue;
                }
            };
            missing.insert(tool.value.clone(), config);
            seen_keys.insert(dep_key);
        }
    }

    missing
}

fn canonical_key(transport: &str, identifier: &str, fallback: &str) -> String {
    let id = identifier.trim();
    if id.is_empty() {
        fallback.to_string()
    } else {
        format!("mcp__{transport}__{id}")
    }
}

fn canonical_server_key(name: &str, config: &McpServerConfig) -> String {
    match &config.transport {
        McpServerTransportConfig::Stdio { command, .. } => canonical_key("stdio", command, name),
        McpServerTransportConfig::StreamableHttp { url, .. } => {
            canonical_key("streamable_http", url, name)
        }
    }
}

fn canonical_dependency_key(dep: &SkillToolDependency) -> Result<String, String> {
    let transport = dep.transport.as_deref().unwrap_or("streamable_http");
    if transport.eq_ignore_ascii_case("streamable_http") {
        let url = dep
            .url
            .as_ref()
            .ok_or("missing url for streamable_http dependency")?;
        return Ok(canonical_key("streamable_http", url, &dep.value));
    }
    if transport.eq_ignore_ascii_case("stdio") {
        let command = dep
            .command
            .as_ref()
            .ok_or("missing command for stdio dependency")?;
        return Ok(canonical_key("stdio", command, &dep.value));
    }
    Err(format!("unsupported transport {transport}"))
}

fn dependency_to_server_config(dep: &SkillToolDependency) -> Result<McpServerConfig, String> {
    let transport = dep.transport.as_deref().unwrap_or("streamable_http");
    if transport.eq_ignore_ascii_case("streamable_http") {
        let url = dep
            .url
            .as_ref()
            .ok_or("missing url for streamable_http dependency")?;
        return Ok(McpServerConfig {
            transport: McpServerTransportConfig::StreamableHttp {
                url: url.clone(),
                bearer_token_env_var: None,
                http_headers: None,
                env_http_headers: None,
            },
            enabled: true,
            required: false,
            disabled_reason: None,
            startup_timeout_sec: None,
            tool_timeout_sec: None,
            enabled_tools: None,
            disabled_tools: None,
            scopes: None,
            oauth_resource: None,
        });
    }
    if transport.eq_ignore_ascii_case("stdio") {
        let command = dep
            .command
            .as_ref()
            .ok_or("missing command for stdio dependency")?;
        return Ok(McpServerConfig {
            transport: McpServerTransportConfig::Stdio {
                command: command.clone(),
                args: Vec::new(),
                env: HashMap::new(),
            },
            enabled: true,
            required: false,
            disabled_reason: None,
            startup_timeout_sec: None,
            tool_timeout_sec: None,
            enabled_tools: None,
            disabled_tools: None,
            scopes: None,
            oauth_resource: None,
        });
    }
    Err(format!("unsupported transport {transport}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::skills::model::{SkillDependencies, SkillScope};
    use std::path::PathBuf;

    fn skill_with_tools(tools: Vec<SkillToolDependency>) -> SkillMetadata {
        SkillMetadata {
            name: "skill".into(),
            short_description: None,
            description: "skill".into(),
            version: "1.0".into(),
            triggers: vec![],
            interface: None,
            dependencies: Some(SkillDependencies { tools }),
            policy: None,
            permission_profile: None,
            path_to_skills_md: PathBuf::from("skill"),
            scope: SkillScope::User,
        }
    }

    #[test]
    fn collect_missing_respects_canonical_key() {
        let url = "https://example.com/mcp".to_string();
        let skills = vec![skill_with_tools(vec![SkillToolDependency {
            r#type: "mcp".into(),
            value: "github".into(),
            description: None,
            transport: Some("streamable_http".into()),
            command: None,
            url: Some(url.clone()),
        }])];
        let installed = HashMap::from([(
            "alias".to_string(),
            McpServerConfig {
                transport: McpServerTransportConfig::StreamableHttp {
                    url,
                    bearer_token_env_var: None,
                    http_headers: None,
                    env_http_headers: None,
                },
                enabled: true,
                required: false,
                disabled_reason: None,
                startup_timeout_sec: None,
                tool_timeout_sec: None,
                enabled_tools: None,
                disabled_tools: None,
                scopes: None,
                oauth_resource: None,
            },
        )]);
        assert!(collect_missing_mcp_dependencies(&skills, &installed).is_empty());
    }

    #[test]
    fn collect_missing_dedupes_by_canonical_key() {
        let url = "https://example.com/one".to_string();
        let skills = vec![skill_with_tools(vec![
            SkillToolDependency {
                r#type: "mcp".into(),
                value: "alias-one".into(),
                description: None,
                transport: Some("streamable_http".into()),
                command: None,
                url: Some(url.clone()),
            },
            SkillToolDependency {
                r#type: "mcp".into(),
                value: "alias-two".into(),
                description: None,
                transport: Some("streamable_http".into()),
                command: None,
                url: Some(url.clone()),
            },
        ])];
        let missing = collect_missing_mcp_dependencies(&skills, &HashMap::new());
        assert_eq!(missing.len(), 1);
        assert!(missing.contains_key("alias-one"));
    }

    #[test]
    fn non_mcp_deps_ignored() {
        let skills = vec![skill_with_tools(vec![SkillToolDependency {
            r#type: "builtin".into(),
            value: "shell".into(),
            description: None,
            transport: None,
            command: None,
            url: None,
        }])];
        assert!(collect_missing_mcp_dependencies(&skills, &HashMap::new()).is_empty());
    }
}
