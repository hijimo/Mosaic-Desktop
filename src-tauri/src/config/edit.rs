use std::collections::HashMap;

use crate::protocol::types::{
    AskForApproval, Effort, Personality, ReasoningSummary, SandboxMode, ServiceTier, Verbosity,
    WebSearchMode,
};

use super::toml_types::{ConfigToml, McpServerConfig};

/// Builder for atomic configuration modifications.
#[derive(Debug, Clone, Default)]
pub struct ConfigEdit {
    model: Option<Option<String>>,
    approval_policy: Option<Option<AskForApproval>>,
    sandbox_mode: Option<Option<SandboxMode>>,
    model_reasoning_effort: Option<Option<Effort>>,
    model_reasoning_summary: Option<Option<ReasoningSummary>>,
    model_verbosity: Option<Option<Verbosity>>,
    personality: Option<Option<Personality>>,
    service_tier: Option<Option<ServiceTier>>,
    instructions: Option<Option<String>>,
    developer_instructions: Option<Option<String>>,
    web_search: Option<Option<WebSearchMode>>,
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

    pub fn set_approval_policy(mut self, policy: AskForApproval) -> Self {
        self.approval_policy = Some(Some(policy));
        self
    }

    pub fn set_sandbox_mode(mut self, mode: SandboxMode) -> Self {
        self.sandbox_mode = Some(Some(mode));
        self
    }

    pub fn set_reasoning_effort(mut self, effort: Effort) -> Self {
        self.model_reasoning_effort = Some(Some(effort));
        self
    }

    pub fn set_reasoning_summary(mut self, summary: ReasoningSummary) -> Self {
        self.model_reasoning_summary = Some(Some(summary));
        self
    }

    pub fn set_verbosity(mut self, verbosity: Verbosity) -> Self {
        self.model_verbosity = Some(Some(verbosity));
        self
    }

    pub fn set_personality(mut self, personality: Personality) -> Self {
        self.personality = Some(Some(personality));
        self
    }

    pub fn set_service_tier(mut self, tier: ServiceTier) -> Self {
        self.service_tier = Some(Some(tier));
        self
    }

    pub fn set_instructions(mut self, instructions: impl Into<String>) -> Self {
        self.instructions = Some(Some(instructions.into()));
        self
    }

    pub fn set_developer_instructions(mut self, instructions: impl Into<String>) -> Self {
        self.developer_instructions = Some(Some(instructions.into()));
        self
    }

    pub fn set_web_search(mut self, mode: WebSearchMode) -> Self {
        self.web_search = Some(Some(mode));
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
        if let Some(mode) = self.sandbox_mode {
            result.sandbox_mode = mode;
        }
        if let Some(effort) = self.model_reasoning_effort {
            result.model_reasoning_effort = effort;
        }
        if let Some(summary) = self.model_reasoning_summary {
            result.model_reasoning_summary = summary;
        }
        if let Some(verbosity) = self.model_verbosity {
            result.model_verbosity = verbosity;
        }
        if let Some(personality) = self.personality {
            result.personality = personality;
        }
        if let Some(tier) = self.service_tier {
            result.service_tier = tier;
        }
        if let Some(instructions) = self.instructions {
            result.instructions = instructions;
        }
        if let Some(dev_instructions) = self.developer_instructions {
            result.developer_instructions = dev_instructions;
        }
        if let Some(web_search) = self.web_search {
            result.web_search = web_search;
        }

        if !self.mcp_server_updates.is_empty() {
            for (name, update) in self.mcp_server_updates {
                match update {
                    McpServerUpdate::Set(config) => {
                        result.mcp_servers.insert(name, config);
                    }
                    McpServerUpdate::Remove => {
                        result.mcp_servers.remove(&name);
                    }
                }
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
            model: Some("gpt-4".into()),
            ..Default::default()
        };
        let result = ConfigEdit::new().apply(&config);
        assert_eq!(result, config);
    }

    #[test]
    fn set_model() {
        let result = ConfigEdit::new()
            .set_model("gpt-4o")
            .apply(&ConfigToml::default());
        assert_eq!(result.model, Some("gpt-4o".to_string()));
    }

    #[test]
    fn clear_model() {
        let config = ConfigToml {
            model: Some("gpt-4".into()),
            ..Default::default()
        };
        let result = ConfigEdit::new().clear_model().apply(&config);
        assert_eq!(result.model, None);
    }

    #[test]
    fn set_new_fields() {
        let result = ConfigEdit::new()
            .set_reasoning_effort(Effort::High)
            .set_web_search(WebSearchMode::Live)
            .set_personality(Personality::Friendly)
            .apply(&ConfigToml::default());
        assert_eq!(result.model_reasoning_effort, Some(Effort::High));
        assert_eq!(result.web_search, Some(WebSearchMode::Live));
        assert_eq!(result.personality, Some(Personality::Friendly));
    }

    #[test]
    fn add_mcp_server() {
        let server = McpServerConfig {
            transport: McpServerTransportConfig::Stdio {
                command: "node".into(),
                args: vec![],
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
        };
        let result = ConfigEdit::new()
            .set_mcp_server("my-server", server.clone())
            .apply(&ConfigToml::default());
        assert_eq!(result.mcp_servers.get("my-server").unwrap(), &server);
    }

    #[test]
    fn remove_mcp_server() {
        let server = McpServerConfig {
            transport: McpServerTransportConfig::Stdio {
                command: "node".into(),
                args: vec![],
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
        };
        let config = ConfigToml {
            mcp_servers: HashMap::from([("srv".into(), server)]),
            ..Default::default()
        };
        let result = ConfigEdit::new().remove_mcp_server("srv").apply(&config);
        assert!(result.mcp_servers.is_empty());
    }
}
