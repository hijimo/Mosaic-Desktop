use std::collections::HashMap;

use super::toml_types::ConfigToml;

/// Configuration layer priority (highest to lowest):
/// Mdm > System > User > Project > Session
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ConfigLayer {
    Session = 0,
    Project = 1,
    User = 2,
    System = 3,
    Mdm = 4,
}

/// Layered configuration stack that merges multiple ConfigToml
/// layers by priority. Higher-priority layers override lower ones.
#[derive(Debug, Clone)]
pub struct ConfigLayerStack {
    layers: Vec<(ConfigLayer, ConfigToml)>,
}

impl ConfigLayerStack {
    pub fn new() -> Self {
        Self { layers: Vec::new() }
    }

    /// Add a configuration layer. Layers are stored and merged
    /// by priority order during `merge()`.
    pub fn add_layer(&mut self, layer: ConfigLayer, config: ConfigToml) {
        self.layers.push((layer, config));
    }

    /// Merge all layers by priority (highest wins).
    /// For scalar fields, the highest-priority non-None value wins.
    /// For mcp_servers maps, entries are merged with higher-priority
    /// layers overriding individual server configs.
    pub fn merge(&self) -> ConfigToml {
        let mut sorted: Vec<_> = self.layers.clone();
        // Sort ascending by priority so we can fold from lowest to highest,
        // letting higher-priority values overwrite.
        sorted.sort_by_key(|(layer, _)| *layer);

        let mut result = ConfigToml::default();

        for (_layer, config) in &sorted {
            if config.model.is_some() {
                result.model = config.model.clone();
            }
            if config.approval_policy.is_some() {
                result.approval_policy = config.approval_policy.clone();
            }
            if config.sandbox_policy.is_some() {
                result.sandbox_policy = config.sandbox_policy.clone();
            }
            if let Some(servers) = &config.mcp_servers {
                let merged = result.mcp_servers.get_or_insert_with(HashMap::new);
                for (name, server_config) in servers {
                    merged.insert(name.clone(), server_config.clone());
                }
            }
            if config.profiles.is_some() {
                result.profiles = config.profiles.clone();
            }
        }

        result
    }

    /// Merge all layers then apply a named profile on top.
    /// Profile values override the base merged config.
    pub fn resolve_with_profile(&self, profile_name: &str) -> ConfigToml {
        let mut base = self.merge();

        if let Some(profiles) = &base.profiles {
            if let Some(profile) = profiles.get(profile_name) {
                if profile.model.is_some() {
                    base.model = profile.model.clone();
                }
                if profile.approval_policy.is_some() {
                    base.approval_policy = profile.approval_policy.clone();
                }
                if profile.sandbox_policy.is_some() {
                    base.sandbox_policy = profile.sandbox_policy.clone();
                }
                if let Some(servers) = &profile.mcp_servers {
                    let merged = base.mcp_servers.get_or_insert_with(HashMap::new);
                    for (name, server_config) in servers {
                        merged.insert(name.clone(), server_config.clone());
                    }
                }
            }
        }

        base
    }
}

impl Default for ConfigLayerStack {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::toml_types::{McpServerConfig, McpServerTransportConfig, McpToolFilter};

    fn make_config(model: Option<&str>, policy: Option<&str>) -> ConfigToml {
        ConfigToml {
            model: model.map(String::from),
            approval_policy: policy.map(String::from),
            sandbox_policy: None,
            mcp_servers: None,
            profiles: None,
        }
    }

    #[test]
    fn empty_stack_returns_default() {
        let stack = ConfigLayerStack::new();
        assert_eq!(stack.merge(), ConfigToml::default());
    }

    #[test]
    fn single_layer() {
        let mut stack = ConfigLayerStack::new();
        stack.add_layer(
            ConfigLayer::User,
            make_config(Some("gpt-4"), Some("always")),
        );
        let merged = stack.merge();
        assert_eq!(merged.model, Some("gpt-4".to_string()));
        assert_eq!(merged.approval_policy, Some("always".to_string()));
    }

    #[test]
    fn higher_priority_wins() {
        let mut stack = ConfigLayerStack::new();
        stack.add_layer(
            ConfigLayer::Session,
            make_config(Some("gpt-3.5"), Some("never")),
        );
        stack.add_layer(ConfigLayer::Mdm, make_config(Some("gpt-4-mdm"), None));
        let merged = stack.merge();
        // Mdm has higher priority, overrides model
        assert_eq!(merged.model, Some("gpt-4-mdm".to_string()));
        // Mdm has no approval_policy, so Session's value persists
        assert_eq!(merged.approval_policy, Some("never".to_string()));
    }

    #[test]
    fn three_layer_priority() {
        let mut stack = ConfigLayerStack::new();
        stack.add_layer(
            ConfigLayer::Session,
            make_config(Some("session-model"), None),
        );
        stack.add_layer(
            ConfigLayer::User,
            make_config(Some("user-model"), Some("user-policy")),
        );
        stack.add_layer(ConfigLayer::System, make_config(Some("system-model"), None));
        let merged = stack.merge();
        // System > User > Session
        assert_eq!(merged.model, Some("system-model".to_string()));
        assert_eq!(merged.approval_policy, Some("user-policy".to_string()));
    }

    #[test]
    fn mcp_servers_merge() {
        let mut stack = ConfigLayerStack::new();

        let server_a = McpServerConfig {
            transport: McpServerTransportConfig::Stdio {
                command: "node".to_string(),
                args: vec![],
                env: HashMap::new(),
            },
            disabled: false,
            disabled_reason: None,
            tool_filter: None,
        };
        let server_b = McpServerConfig {
            transport: McpServerTransportConfig::Http {
                url: "https://b.com".to_string(),
                headers: HashMap::new(),
            },
            disabled: false,
            disabled_reason: None,
            tool_filter: None,
        };

        let mut config_user = ConfigToml::default();
        config_user.mcp_servers = Some(HashMap::from([("srv-a".to_string(), server_a.clone())]));

        let mut config_project = ConfigToml::default();
        config_project.mcp_servers = Some(HashMap::from([("srv-b".to_string(), server_b.clone())]));

        stack.add_layer(ConfigLayer::User, config_user);
        stack.add_layer(ConfigLayer::Project, config_project);

        let merged = stack.merge();
        let servers = merged.mcp_servers.unwrap();
        assert_eq!(servers.len(), 2);
        assert_eq!(servers.get("srv-a").unwrap(), &server_a);
        assert_eq!(servers.get("srv-b").unwrap(), &server_b);
    }

    #[test]
    fn mcp_server_override_by_priority() {
        let mut stack = ConfigLayerStack::new();

        let server_low = McpServerConfig {
            transport: McpServerTransportConfig::Stdio {
                command: "old".to_string(),
                args: vec![],
                env: HashMap::new(),
            },
            disabled: false,
            disabled_reason: None,
            tool_filter: None,
        };
        let server_high = McpServerConfig {
            transport: McpServerTransportConfig::Stdio {
                command: "new".to_string(),
                args: vec!["--flag".to_string()],
                env: HashMap::new(),
            },
            disabled: true,
            disabled_reason: Some("upgraded".to_string()),
            tool_filter: Some(McpToolFilter {
                enabled: Some(vec!["tool1".to_string()]),
                disabled: None,
            }),
        };

        let mut config_session = ConfigToml::default();
        config_session.mcp_servers = Some(HashMap::from([("srv".to_string(), server_low)]));

        let mut config_system = ConfigToml::default();
        config_system.mcp_servers = Some(HashMap::from([("srv".to_string(), server_high.clone())]));

        stack.add_layer(ConfigLayer::Session, config_session);
        stack.add_layer(ConfigLayer::System, config_system);

        let merged = stack.merge();
        let servers = merged.mcp_servers.unwrap();
        assert_eq!(servers.get("srv").unwrap(), &server_high);
    }

    #[test]
    fn profile_overrides_base() {
        let profile = ConfigToml {
            model: Some("fast-model".to_string()),
            approval_policy: None,
            sandbox_policy: Some("danger".to_string()),
            mcp_servers: None,
            profiles: None,
        };
        let base = ConfigToml {
            model: Some("default-model".to_string()),
            approval_policy: Some("always".to_string()),
            sandbox_policy: Some("read-only".to_string()),
            mcp_servers: None,
            profiles: Some(HashMap::from([("fast".to_string(), profile)])),
        };

        let mut stack = ConfigLayerStack::new();
        stack.add_layer(ConfigLayer::User, base);

        let resolved = stack.resolve_with_profile("fast");
        assert_eq!(resolved.model, Some("fast-model".to_string()));
        assert_eq!(resolved.approval_policy, Some("always".to_string()));
        assert_eq!(resolved.sandbox_policy, Some("danger".to_string()));
    }

    #[test]
    fn nonexistent_profile_returns_base() {
        let mut stack = ConfigLayerStack::new();
        stack.add_layer(
            ConfigLayer::User,
            make_config(Some("gpt-4"), Some("always")),
        );
        let resolved = stack.resolve_with_profile("nonexistent");
        assert_eq!(resolved.model, Some("gpt-4".to_string()));
        assert_eq!(resolved.approval_policy, Some("always".to_string()));
    }
}
