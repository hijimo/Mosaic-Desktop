use std::collections::HashMap;

use super::toml_types::{ConfigToml, McpServerConfig};

/// Builder for atomic configuration modifications.
/// Collects changes and applies them to a ConfigToml in one step.
#[derive(Debug, Clone, Default)]
pub struct ConfigEdit {
    model: Option<Option<String>>,
    approval_policy: Option<Option<String>>,
    sandbox_policy: Option<Option<String>>,
    mcp_server_updates: HashMap<String, McpServerUpdate>,
}

#[derive(Debug, Clone)]
enum McpServerUpdate {
    Set(McpServerConfig),
    Remove,
}

impl ConfigEdit {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(Some(model.into()));
        self
    }

    pub fn clear_model(mut self) -> Self {
        self.model = Some(None);
        self
    }

    pub fn set_approval_policy(mut self, policy: impl Into<String>) -> Self {
        self.approval_policy = Some(Some(policy.into()));
        self
    }

    pub fn clear_approval_policy(mut self) -> Self {
        self.approval_policy = Some(None);
        self
    }

    pub fn set_sandbox_policy(mut self, policy: impl Into<String>) -> Self {
        self.sandbox_policy = Some(Some(policy.into()));
        self
    }

    pub fn clear_sandbox_policy(mut self) -> Self {
        self.sandbox_policy = Some(None);
        self
    }

    pub fn set_mcp_server(mut self, name: impl Into<String>, config: McpServerConfig) -> Self {
        self.mcp_server_updates
            .insert(name.into(), McpServerUpdate::Set(config));
        self
    }

    pub fn remove_mcp_server(mut self, name: impl Into<String>) -> Self {
        self.mcp_server_updates
            .insert(name.into(), McpServerUpdate::Remove);
        self
    }

    /// Apply all collected edits to the given config, returning the modified copy.
    pub fn apply(self, config: &ConfigToml) -> ConfigToml {
        let mut result = config.clone();

        if let Some(model) = self.model {
            result.model = model;
        }
        if let Some(policy) = self.approval_policy {
            result.approval_policy = policy;
        }
        if let Some(policy) = self.sandbox_policy {
            result.sandbox_policy = policy;
        }

        if !self.mcp_server_updates.is_empty() {
            let servers = result.mcp_servers.get_or_insert_with(HashMap::new);
            for (name, update) in self.mcp_server_updates {
                match update {
                    McpServerUpdate::Set(config) => {
                        servers.insert(name, config);
                    }
                    McpServerUpdate::Remove => {
                        servers.remove(&name);
                    }
                }
            }
            if servers.is_empty() {
                result.mcp_servers = None;
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::toml_types::McpServerTransportConfig;

    #[test]
    fn no_edits_returns_clone() {
        let config = ConfigToml {
            model: Some("gpt-4".to_string()),
            approval_policy: Some("always".to_string()),
            sandbox_policy: None,
            mcp_servers: None,
            profiles: None,
        };
        let result = ConfigEdit::new().apply(&config);
        assert_eq!(result, config);
    }

    #[test]
    fn set_model() {
        let config = ConfigToml::default();
        let result = ConfigEdit::new().set_model("gpt-4o").apply(&config);
        assert_eq!(result.model, Some("gpt-4o".to_string()));
    }

    #[test]
    fn clear_model() {
        let config = ConfigToml {
            model: Some("gpt-4".to_string()),
            ..Default::default()
        };
        let result = ConfigEdit::new().clear_model().apply(&config);
        assert_eq!(result.model, None);
    }

    #[test]
    fn set_policies() {
        let config = ConfigToml::default();
        let result = ConfigEdit::new()
            .set_approval_policy("on-failure")
            .set_sandbox_policy("workspace-write-only")
            .apply(&config);
        assert_eq!(result.approval_policy, Some("on-failure".to_string()));
        assert_eq!(
            result.sandbox_policy,
            Some("workspace-write-only".to_string())
        );
    }

    #[test]
    fn add_mcp_server() {
        let config = ConfigToml::default();
        let server = McpServerConfig {
            transport: McpServerTransportConfig::Stdio {
                command: "node".to_string(),
                args: vec![],
                env: HashMap::new(),
            },
            disabled: false,
            disabled_reason: None,
            tool_filter: None,
        };
        let result = ConfigEdit::new()
            .set_mcp_server("my-server", server.clone())
            .apply(&config);
        let servers = result.mcp_servers.unwrap();
        assert_eq!(servers.len(), 1);
        assert_eq!(servers.get("my-server").unwrap(), &server);
    }

    #[test]
    fn remove_mcp_server() {
        let server = McpServerConfig {
            transport: McpServerTransportConfig::Stdio {
                command: "node".to_string(),
                args: vec![],
                env: HashMap::new(),
            },
            disabled: false,
            disabled_reason: None,
            tool_filter: None,
        };
        let config = ConfigToml {
            mcp_servers: Some(HashMap::from([("srv".to_string(), server)])),
            ..Default::default()
        };
        let result = ConfigEdit::new().remove_mcp_server("srv").apply(&config);
        // Empty map becomes None
        assert_eq!(result.mcp_servers, None);
    }

    #[test]
    fn multiple_edits_atomic() {
        let config = ConfigToml {
            model: Some("old".to_string()),
            approval_policy: Some("never".to_string()),
            ..Default::default()
        };
        let result = ConfigEdit::new()
            .set_model("new")
            .clear_approval_policy()
            .set_sandbox_policy("read-only")
            .apply(&config);
        assert_eq!(result.model, Some("new".to_string()));
        assert_eq!(result.approval_policy, None);
        assert_eq!(result.sandbox_policy, Some("read-only".to_string()));
    }
}
