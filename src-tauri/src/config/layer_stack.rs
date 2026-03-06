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

    pub fn add_layer(&mut self, layer: ConfigLayer, config: ConfigToml) {
        self.layers.push((layer, config));
    }

    /// Merge all layers by priority (highest wins).
    /// Uses serde_json merge: serialize each layer to JSON Value, then
    /// overlay non-null fields from higher-priority layers.
    pub fn merge(&self) -> ConfigToml {
        let mut sorted: Vec<_> = self.layers.clone();
        sorted.sort_by_key(|(layer, _)| *layer);

        let mut base = serde_json::to_value(ConfigToml::default()).unwrap();

        for (_layer, config) in &sorted {
            let overlay = serde_json::to_value(config).unwrap();
            merge_json(&mut base, &overlay);
        }

        serde_json::from_value(base).unwrap_or_default()
    }

    /// Merge all layers then apply a named profile on top.
    pub fn resolve_with_profile(&self, profile_name: &str) -> ConfigToml {
        let base = self.merge();

        if let Some(profile) = base.profiles.get(profile_name) {
            let mut base_val = serde_json::to_value(&base).unwrap();
            let profile_val = serde_json::to_value(profile).unwrap();
            merge_json(&mut base_val, &profile_val);
            serde_json::from_value(base_val).unwrap_or(base)
        } else {
            base
        }
    }
}

/// Recursively merge `overlay` into `base`. Non-null overlay values win.
/// For objects, merge recursively. For arrays/scalars, overlay replaces base.
fn merge_json(base: &mut serde_json::Value, overlay: &serde_json::Value) {
    match (base, overlay) {
        (serde_json::Value::Object(base_map), serde_json::Value::Object(overlay_map)) => {
            for (key, overlay_val) in overlay_map {
                if overlay_val.is_null() {
                    continue;
                }
                let entry = base_map
                    .entry(key.clone())
                    .or_insert(serde_json::Value::Null);
                if overlay_val.is_object() && entry.is_object() {
                    merge_json(entry, overlay_val);
                } else {
                    *entry = overlay_val.clone();
                }
            }
        }
        (base, overlay) => {
            if !overlay.is_null() {
                *base = overlay.clone();
            }
        }
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
    use crate::config::toml_types::McpServerConfig;
    use crate::config::toml_types::McpServerTransportConfig;
    use crate::protocol::types::Effort;
    use std::collections::HashMap;

    fn make_config(model: Option<&str>) -> ConfigToml {
        ConfigToml {
            model: model.map(String::from),
            ..Default::default()
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
        stack.add_layer(ConfigLayer::User, make_config(Some("gpt-4")));
        let merged = stack.merge();
        assert_eq!(merged.model, Some("gpt-4".to_string()));
    }

    #[test]
    fn higher_priority_wins() {
        let mut stack = ConfigLayerStack::new();
        stack.add_layer(ConfigLayer::Session, make_config(Some("gpt-3.5")));
        stack.add_layer(ConfigLayer::Mdm, make_config(Some("gpt-4-mdm")));
        let merged = stack.merge();
        assert_eq!(merged.model, Some("gpt-4-mdm".to_string()));
    }

    #[test]
    fn three_layer_priority() {
        let mut stack = ConfigLayerStack::new();
        stack.add_layer(ConfigLayer::Session, make_config(Some("session-model")));
        stack.add_layer(ConfigLayer::User, make_config(Some("user-model")));
        stack.add_layer(ConfigLayer::System, make_config(Some("system-model")));
        let merged = stack.merge();
        assert_eq!(merged.model, Some("system-model".to_string()));
    }

    #[test]
    fn mcp_servers_merge() {
        let mut stack = ConfigLayerStack::new();

        let server_a = McpServerConfig {
            transport: McpServerTransportConfig::Stdio {
                command: "node".into(),
                args: vec![],
                env: HashMap::new(),
            },
            disabled: false,
            disabled_reason: None,
            tool_filter: None,
        };

        let mut config_user = ConfigToml::default();
        config_user.mcp_servers = HashMap::from([("srv-a".into(), server_a.clone())]);

        stack.add_layer(ConfigLayer::User, config_user);
        let merged = stack.merge();
        assert!(merged.mcp_servers.contains_key("srv-a"));
    }

    #[test]
    fn profile_overrides_base() {
        use crate::config::toml_types::ConfigProfile;

        let profile = ConfigProfile {
            model: Some("fast-model".into()),
            ..Default::default()
        };
        let config = ConfigToml {
            model: Some("default-model".into()),
            profiles: HashMap::from([("fast".into(), profile)]),
            ..Default::default()
        };

        let mut stack = ConfigLayerStack::new();
        stack.add_layer(ConfigLayer::User, config);

        let resolved = stack.resolve_with_profile("fast");
        assert_eq!(resolved.model, Some("fast-model".to_string()));
    }

    #[test]
    fn nonexistent_profile_returns_base() {
        let mut stack = ConfigLayerStack::new();
        stack.add_layer(ConfigLayer::User, make_config(Some("gpt-4")));
        let resolved = stack.resolve_with_profile("nonexistent");
        assert_eq!(resolved.model, Some("gpt-4".to_string()));
    }

    #[test]
    fn new_fields_merge_correctly() {
        let mut stack = ConfigLayerStack::new();
        let mut c1 = ConfigToml::default();
        c1.model_reasoning_effort = Some(Effort::Low);
        c1.web_search = Some(crate::protocol::types::WebSearchMode::Cached);

        let mut c2 = ConfigToml::default();
        c2.model_reasoning_effort = Some(Effort::High);

        stack.add_layer(ConfigLayer::Session, c1);
        stack.add_layer(ConfigLayer::System, c2);

        let merged = stack.merge();
        assert_eq!(merged.model_reasoning_effort, Some(Effort::High));
        assert_eq!(
            merged.web_search,
            Some(crate::protocol::types::WebSearchMode::Cached)
        );
    }
}
