use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::config::toml_types::{McpServerConfig, McpServerTransportConfig};

/// OAuth authentication status for an MCP server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpAuthStatus {
    /// Server does not use OAuth.
    Unsupported,
    /// OAuth is supported and credentials are valid.
    Authenticated,
    /// OAuth is supported but credentials are missing or expired.
    NeedsAuth,
    /// Could not determine auth status.
    Unknown,
}

/// Auth status entry for a single MCP server.
#[derive(Debug, Clone)]
pub struct McpAuthStatusEntry {
    pub config: McpServerConfig,
    pub auth_status: McpAuthStatus,
}

/// Compute auth statuses for all configured MCP servers.
///
/// For Stdio transports, OAuth is unsupported. For StreamableHttp, we check
/// whether a bearer token env var is configured (authenticated) or not.
pub async fn compute_auth_statuses<'a, I>(servers: I) -> HashMap<String, McpAuthStatusEntry>
where
    I: IntoIterator<Item = (&'a String, &'a McpServerConfig)>,
{
    let mut result = HashMap::new();
    for (name, config) in servers {
        let auth_status = match &config.transport {
            McpServerTransportConfig::Stdio { .. } => McpAuthStatus::Unsupported,
            McpServerTransportConfig::StreamableHttp {
                bearer_token_env_var,
                ..
            } => {
                if bearer_token_env_var.is_some() {
                    // Has a configured token env var — assume authenticated.
                    McpAuthStatus::Authenticated
                } else if config.oauth_resource.is_some() {
                    // OAuth resource configured but no token — needs auth.
                    McpAuthStatus::NeedsAuth
                } else {
                    McpAuthStatus::Unsupported
                }
            }
        };
        result.insert(
            name.clone(),
            McpAuthStatusEntry {
                config: config.clone(),
                auth_status,
            },
        );
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stdio_config() -> McpServerConfig {
        McpServerConfig {
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
        }
    }

    fn http_config(bearer: Option<&str>, oauth_resource: Option<&str>) -> McpServerConfig {
        McpServerConfig {
            transport: McpServerTransportConfig::StreamableHttp {
                url: "https://example.com/mcp".into(),
                bearer_token_env_var: bearer.map(String::from),
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
            oauth_resource: oauth_resource.map(String::from),
        }
    }

    #[tokio::test]
    async fn stdio_is_unsupported() {
        let servers = HashMap::from([("srv".to_string(), stdio_config())]);
        let statuses = compute_auth_statuses(servers.iter()).await;
        assert_eq!(statuses["srv"].auth_status, McpAuthStatus::Unsupported);
    }

    #[tokio::test]
    async fn http_with_bearer_is_authenticated() {
        let servers = HashMap::from([("srv".to_string(), http_config(Some("MY_TOKEN"), None))]);
        let statuses = compute_auth_statuses(servers.iter()).await;
        assert_eq!(statuses["srv"].auth_status, McpAuthStatus::Authenticated);
    }

    #[tokio::test]
    async fn http_with_oauth_resource_needs_auth() {
        let servers = HashMap::from([(
            "srv".to_string(),
            http_config(None, Some("https://api.example.com")),
        )]);
        let statuses = compute_auth_statuses(servers.iter()).await;
        assert_eq!(statuses["srv"].auth_status, McpAuthStatus::NeedsAuth);
    }

    #[tokio::test]
    async fn http_plain_is_unsupported() {
        let servers = HashMap::from([("srv".to_string(), http_config(None, None))]);
        let statuses = compute_auth_statuses(servers.iter()).await;
        assert_eq!(statuses["srv"].auth_status, McpAuthStatus::Unsupported);
    }
}
