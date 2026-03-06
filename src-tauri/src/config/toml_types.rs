use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Top-level TOML configuration structure.
/// Uses kebab-case naming to match TOML conventions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub struct ConfigToml {
    pub model: Option<String>,
    pub approval_policy: Option<String>,
    pub sandbox_policy: Option<String>,
    pub mcp_servers: Option<HashMap<String, McpServerConfig>>,
    pub profiles: Option<HashMap<String, ConfigToml>>,
}

/// MCP server transport configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case", tag = "type")]
pub enum McpServerTransportConfig {
    Stdio {
        command: String,
        args: Vec<String>,
        env: HashMap<String, String>,
    },
    Http {
        url: String,
        headers: HashMap<String, String>,
    },
    OAuth {
        url: String,
        client_id: String,
        client_secret: String,
        token_url: String,
    },
}

/// MCP server configuration with disabled tracking.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct McpServerConfig {
    pub transport: McpServerTransportConfig,
    pub disabled: bool,
    pub disabled_reason: Option<String>,
    pub tool_filter: Option<McpToolFilter>,
}

/// MCP tool filter — enabled/disabled lists.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct McpToolFilter {
    pub enabled: Option<Vec<String>>,
    pub disabled: Option<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config_toml_roundtrip() {
        let config = ConfigToml::default();
        let toml_str = toml::to_string(&config).unwrap();
        let decoded: ConfigToml = toml::from_str(&toml_str).unwrap();
        assert_eq!(config, decoded);
    }

    #[test]
    fn config_toml_with_model_and_policies() {
        let config = ConfigToml {
            model: Some("gpt-4".to_string()),
            approval_policy: Some("always".to_string()),
            sandbox_policy: Some("read-only".to_string()),
            mcp_servers: None,
            profiles: None,
        };
        let toml_str = toml::to_string(&config).unwrap();
        let decoded: ConfigToml = toml::from_str(&toml_str).unwrap();
        assert_eq!(config, decoded);
    }

    #[test]
    fn stdio_transport_roundtrip() {
        let transport = McpServerTransportConfig::Stdio {
            command: "node".to_string(),
            args: vec!["server.js".to_string()],
            env: HashMap::from([("KEY".to_string(), "val".to_string())]),
        };
        let server = McpServerConfig {
            transport,
            disabled: false,
            disabled_reason: None,
            tool_filter: None,
        };
        let config = ConfigToml {
            model: None,
            approval_policy: None,
            sandbox_policy: None,
            mcp_servers: Some(HashMap::from([("test".to_string(), server.clone())])),
            profiles: None,
        };
        let toml_str = toml::to_string(&config).unwrap();
        let decoded: ConfigToml = toml::from_str(&toml_str).unwrap();
        assert_eq!(config, decoded);
    }

    #[test]
    fn http_transport_roundtrip() {
        let server = McpServerConfig {
            transport: McpServerTransportConfig::Http {
                url: "https://example.com/mcp".to_string(),
                headers: HashMap::from([("Authorization".to_string(), "Bearer tok".to_string())]),
            },
            disabled: false,
            disabled_reason: None,
            tool_filter: Some(McpToolFilter {
                enabled: Some(vec!["tool_a".to_string()]),
                disabled: None,
            }),
        };
        let config = ConfigToml {
            model: None,
            approval_policy: None,
            sandbox_policy: None,
            mcp_servers: Some(HashMap::from([("http-srv".to_string(), server)])),
            profiles: None,
        };
        let toml_str = toml::to_string(&config).unwrap();
        let decoded: ConfigToml = toml::from_str(&toml_str).unwrap();
        assert_eq!(config, decoded);
    }

    #[test]
    fn oauth_transport_roundtrip() {
        let server = McpServerConfig {
            transport: McpServerTransportConfig::OAuth {
                url: "https://mcp.example.com".to_string(),
                client_id: "cid".to_string(),
                client_secret: "csecret".to_string(),
                token_url: "https://auth.example.com/token".to_string(),
            },
            disabled: true,
            disabled_reason: Some("maintenance".to_string()),
            tool_filter: None,
        };
        let config = ConfigToml {
            model: None,
            approval_policy: None,
            sandbox_policy: None,
            mcp_servers: Some(HashMap::from([("oauth-srv".to_string(), server)])),
            profiles: None,
        };
        let toml_str = toml::to_string(&config).unwrap();
        let decoded: ConfigToml = toml::from_str(&toml_str).unwrap();
        assert_eq!(config, decoded);
    }

    #[test]
    fn tool_filter_both_lists() {
        let filter = McpToolFilter {
            enabled: Some(vec!["a".to_string(), "b".to_string()]),
            disabled: Some(vec!["c".to_string()]),
        };
        let json = serde_json::to_string(&filter).unwrap();
        let decoded: McpToolFilter = serde_json::from_str(&json).unwrap();
        assert_eq!(filter, decoded);
    }

    #[test]
    fn config_with_profiles() {
        let profile = ConfigToml {
            model: Some("gpt-3.5".to_string()),
            approval_policy: None,
            sandbox_policy: None,
            mcp_servers: None,
            profiles: None,
        };
        let config = ConfigToml {
            model: Some("gpt-4".to_string()),
            approval_policy: None,
            sandbox_policy: None,
            mcp_servers: None,
            profiles: Some(HashMap::from([("fast".to_string(), profile)])),
        };
        let toml_str = toml::to_string(&config).unwrap();
        let decoded: ConfigToml = toml::from_str(&toml_str).unwrap();
        assert_eq!(config, decoded);
    }

    #[test]
    fn kebab_case_serialization() {
        let config = ConfigToml {
            model: Some("test".to_string()),
            approval_policy: Some("always".to_string()),
            sandbox_policy: Some("read-only".to_string()),
            mcp_servers: None,
            profiles: None,
        };
        let toml_str = toml::to_string(&config).unwrap();
        assert!(toml_str.contains("approval-policy"));
        assert!(toml_str.contains("sandbox-policy"));
        assert!(!toml_str.contains("approval_policy"));
        assert!(!toml_str.contains("sandbox_policy"));
    }
}
