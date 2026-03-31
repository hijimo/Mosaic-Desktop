use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use rmcp::model::{CallToolRequest, CallToolRequestParams, ListToolsRequest, ServerResult};
use rmcp::service::{serve_client, RoleClient, RunningService};
use rmcp::transport::streamable_http_client::{
    StreamableHttpClientTransport, StreamableHttpClientTransportConfig,
};
use rmcp::transport::TokioChildProcess;
use rmcp::ServiceExt;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use tokio::process::Command;
use tokio::sync::Mutex;

use crate::config::toml_types::{McpServerConfig, McpServerTransportConfig};
use crate::protocol::error::{CodexError, ErrorCode};
use crate::protocol::event::{ErrorEvent, Event, EventMsg, McpStartupUpdateEvent};
use crate::protocol::types::{McpStartupStatus, SandboxPolicy};

const MAX_QUALIFIED_NAME_LEN: usize = 64;

/// Default timeout for initializing MCP server & initially listing tools.
pub const DEFAULT_STARTUP_TIMEOUT: Duration = Duration::from_secs(10);

/// Default timeout for individual tool calls.
pub const DEFAULT_TOOL_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpConnectionState {
    Disconnected,
    Connecting,
    Ready,
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct McpToolInfo {
    pub name: String,
    pub qualified_name: String,
    pub server_name: String,
    pub description: String,
    pub input_schema: Option<serde_json::Value>,
}

/// Sandbox state pushed to MCP servers that support it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SandboxState {
    pub sandbox_policy: SandboxPolicy,
}

type RmcpClient = RunningService<RoleClient, ()>;

struct McpConnection {
    state: McpConnectionState,
    config: McpServerConfig,
    tools: HashMap<String, McpToolInfo>,
    client: Option<Arc<RmcpClient>>,
    tool_timeout: Duration,
}

pub struct McpConnectionManager {
    connections: Arc<Mutex<HashMap<String, McpConnection>>>,
    tx_event: Option<async_channel::Sender<Event>>,
    next_event_id: Arc<Mutex<u64>>,
}

impl McpConnectionManager {
    pub fn new() -> Self {
        Self {
            connections: Arc::new(Mutex::new(HashMap::new())),
            tx_event: None,
            next_event_id: Arc::new(Mutex::new(1)),
        }
    }

    pub fn with_event_sender(tx_event: async_channel::Sender<Event>) -> Self {
        Self {
            connections: Arc::new(Mutex::new(HashMap::new())),
            tx_event: Some(tx_event),
            next_event_id: Arc::new(Mutex::new(1)),
        }
    }

    pub async fn connect(
        &self,
        server_name: &str,
        config: &McpServerConfig,
    ) -> Result<(), CodexError> {
        if !config.enabled {
            let mut conns = self.connections.lock().await;
            conns.insert(
                server_name.to_string(),
                McpConnection {
                    state: McpConnectionState::Disconnected,
                    config: config.clone(),
                    tools: HashMap::new(),
                    client: None,
                    tool_timeout: config.tool_timeout_sec.unwrap_or(DEFAULT_TOOL_TIMEOUT),
                },
            );
            return Ok(());
        }

        self.emit_startup_update(server_name, McpStartupStatus::Starting)
            .await;
        {
            let mut conns = self.connections.lock().await;
            conns.insert(
                server_name.to_string(),
                McpConnection {
                    state: McpConnectionState::Connecting,
                    config: config.clone(),
                    tools: HashMap::new(),
                    client: None,
                    tool_timeout: config.tool_timeout_sec.unwrap_or(DEFAULT_TOOL_TIMEOUT),
                },
            );
        }

        let timeout = config
            .startup_timeout_sec
            .unwrap_or(DEFAULT_STARTUP_TIMEOUT);
        let client =
            match tokio::time::timeout(timeout, self.build_client(server_name, &config.transport))
                .await
            {
                Ok(Ok(c)) => c,
                Ok(Err(e)) => {
                    let msg = format!("MCP connect '{server_name}' failed: {e}");
                    self.mark_failed(server_name, &msg).await;
                    self.emit_error(&msg).await;
                    self.emit_startup_update(
                        server_name,
                        McpStartupStatus::Failed { error: msg.clone() },
                    )
                    .await;
                    return Err(CodexError::new(ErrorCode::McpServerUnavailable, msg));
                }
                Err(_) => {
                    let msg = format!("MCP connect '{server_name}' timed out after {timeout:?}");
                    self.mark_failed(server_name, &msg).await;
                    self.emit_error(&msg).await;
                    self.emit_startup_update(
                        server_name,
                        McpStartupStatus::Failed { error: msg.clone() },
                    )
                    .await;
                    return Err(CodexError::new(ErrorCode::McpServerUnavailable, msg));
                }
            };

        let client = Arc::new(client);
        {
            let mut conns = self.connections.lock().await;
            if let Some(conn) = conns.get_mut(server_name) {
                conn.state = McpConnectionState::Ready;
                conn.client = Some(client.clone());
            }
        }

        if let Err(e) = self.discover_tools_internal(server_name).await {
            self.emit_error(&format!("Tool discovery failed for '{server_name}': {e}"))
                .await;
        }

        self.emit_startup_update(server_name, McpStartupStatus::Ready)
            .await;
        Ok(())
    }

    pub async fn disconnect(&self, server_name: &str) {
        let mut conns = self.connections.lock().await;
        if let Some(conn) = conns.get_mut(server_name) {
            conn.state = McpConnectionState::Disconnected;
            conn.tools.clear();
            conn.client = None;
        }
    }

    pub async fn state(&self, server_name: &str) -> McpConnectionState {
        let conns = self.connections.lock().await;
        conns
            .get(server_name)
            .map(|c| c.state.clone())
            .unwrap_or(McpConnectionState::Disconnected)
    }

    pub async fn connected_servers(&self) -> Vec<String> {
        let conns = self.connections.lock().await;
        conns
            .iter()
            .filter(|(_, c)| matches!(c.state, McpConnectionState::Ready))
            .map(|(n, _)| n.clone())
            .collect()
    }

    pub async fn is_disabled(&self, server_name: &str) -> bool {
        let conns = self.connections.lock().await;
        conns
            .get(server_name)
            .map(|c| !c.config.enabled)
            .unwrap_or(false)
    }

    pub async fn disabled_reason(&self, server_name: &str) -> Option<String> {
        let conns = self.connections.lock().await;
        conns
            .get(server_name)
            .and_then(|c| c.config.disabled_reason.clone())
    }

    pub async fn discover_tools(&self, server_name: &str) -> Result<Vec<McpToolInfo>, CodexError> {
        self.discover_tools_internal(server_name).await
    }

    async fn discover_tools_internal(
        &self,
        server_name: &str,
    ) -> Result<Vec<McpToolInfo>, CodexError> {
        let (client, config) = {
            let conns = self.connections.lock().await;
            let conn = conns.get(server_name);
            (
                conn.and_then(|c| c.client.clone()),
                conn.map(|c| c.config.clone()),
            )
        };
        let Some(client) = client else {
            return Ok(vec![]);
        };

        let result = client
            .peer()
            .send_request(ListToolsRequest::default().into())
            .await
            .map_err(|e| {
                CodexError::new(
                    ErrorCode::McpServerUnavailable,
                    format!("tools/list failed: {e}"),
                )
            })?;

        let tools_list = match result {
            ServerResult::ListToolsResult(r) => r.tools,
            _ => {
                return Err(CodexError::new(
                    ErrorCode::McpServerUnavailable,
                    "unexpected response to tools/list".to_string(),
                ))
            }
        };

        let mut discovered = Vec::new();
        for tool in tools_list {
            let tool_name = tool.name.to_string();
            if !is_tool_allowed_by_config(&tool_name, config.as_ref()) {
                continue;
            }
            let qualified = qualify_tool_name(server_name, &tool_name);
            let input_schema = serde_json::to_value(&tool.input_schema).ok();
            discovered.push(McpToolInfo {
                name: tool_name,
                qualified_name: qualified,
                server_name: server_name.to_string(),
                description: tool.description.as_deref().unwrap_or("").to_string(),
                input_schema,
            });
        }

        {
            let mut conns = self.connections.lock().await;
            if let Some(conn) = conns.get_mut(server_name) {
                conn.tools.clear();
                for t in &discovered {
                    conn.tools.insert(t.qualified_name.clone(), t.clone());
                }
            }
        }

        Ok(discovered)
    }

    pub async fn all_tools(&self) -> Vec<McpToolInfo> {
        let conns = self.connections.lock().await;
        conns
            .values()
            .filter(|c| matches!(c.state, McpConnectionState::Ready))
            .flat_map(|c| c.tools.values().cloned())
            .collect()
    }

    pub async fn find_server_for_tool(&self, qualified_name: &str) -> Option<String> {
        let conns = self.connections.lock().await;
        for (name, conn) in conns.iter() {
            if conn.tools.contains_key(qualified_name) {
                return Some(name.clone());
            }
        }
        None
    }

    pub async fn call_tool(
        &self,
        server_name: &str,
        tool_name: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, CodexError> {
        let (client, tool_timeout) = {
            let conns = self.connections.lock().await;
            let conn = conns.get(server_name).ok_or_else(|| {
                CodexError::new(
                    ErrorCode::McpServerUnavailable,
                    format!("MCP server '{server_name}' not registered"),
                )
            })?;
            if !matches!(conn.state, McpConnectionState::Ready) {
                return Err(CodexError::new(
                    ErrorCode::McpServerUnavailable,
                    format!("MCP server '{server_name}' not ready"),
                ));
            }
            (conn.client.clone(), conn.tool_timeout)
        };

        let Some(client) = client else {
            return Err(CodexError::new(
                ErrorCode::McpServerUnavailable,
                format!("MCP server '{server_name}' has no client"),
            ));
        };

        let arguments = match args {
            serde_json::Value::Object(map) => Some(map),
            serde_json::Value::Null => None,
            other => {
                return Err(CodexError::new(
                    ErrorCode::InvalidInput,
                    format!("tool args must be object, got {other}"),
                ))
            }
        };

        let mut params = CallToolRequestParams::new(tool_name.to_string());
        if let Some(args) = arguments {
            params = params.with_arguments(args);
        }

        let result = tokio::time::timeout(
            tool_timeout,
            client
                .peer()
                .send_request(CallToolRequest::new(params).into()),
        )
        .await
        .map_err(|_| {
            CodexError::new(
                ErrorCode::ToolExecutionFailed,
                format!("tool call timed out after {tool_timeout:?}"),
            )
        })?
        .map_err(|e| {
            CodexError::new(
                ErrorCode::ToolExecutionFailed,
                format!("tools/call failed: {e}"),
            )
        })?;

        let call_result = match result {
            ServerResult::CallToolResult(r) => r,
            _ => {
                return Err(CodexError::new(
                    ErrorCode::ToolExecutionFailed,
                    "unexpected response to tools/call".to_string(),
                ))
            }
        };

        let content: Vec<serde_json::Value> = call_result
            .content
            .iter()
            .map(|c| serde_json::to_value(c).unwrap_or(serde_json::Value::Null))
            .collect();

        Ok(serde_json::json!({
            "content": content,
            "isError": call_result.is_error.unwrap_or(false)
        }))
    }

    /// Get required servers that failed to start.
    pub async fn required_startup_failures(&self) -> Vec<(String, String)> {
        let conns = self.connections.lock().await;
        conns
            .iter()
            .filter(|(_, c)| c.config.required && matches!(c.state, McpConnectionState::Failed(_)))
            .map(|(name, c)| {
                let error = match &c.state {
                    McpConnectionState::Failed(e) => e.clone(),
                    _ => "unknown".to_string(),
                };
                (name.clone(), error)
            })
            .collect()
    }

    // ── Private helpers ──────────────────────────────────────────

    async fn build_client(
        &self,
        _server_name: &str,
        transport: &McpServerTransportConfig,
    ) -> Result<RmcpClient, CodexError> {
        match transport {
            McpServerTransportConfig::Stdio { command, args, env } => {
                let mut cmd = Command::new(command);
                cmd.args(args).envs(env).kill_on_drop(true);
                let transport = TokioChildProcess::new(cmd).map_err(|e| {
                    CodexError::new(
                        ErrorCode::McpServerUnavailable,
                        format!("spawn failed: {e}"),
                    )
                })?;
                serve_client((), transport).await.map_err(|e| {
                    CodexError::new(
                        ErrorCode::McpServerUnavailable,
                        format!("MCP init failed: {e}"),
                    )
                })
            }
            McpServerTransportConfig::StreamableHttp {
                url,
                bearer_token_env_var,
                http_headers,
                ..
            } => {
                let mut config = StreamableHttpClientTransportConfig::with_uri(url.clone());
                // Resolve bearer token from env var.
                if let Some(env_var) = bearer_token_env_var {
                    if let Ok(token) = std::env::var(env_var) {
                        config = config.auth_header(format!("Bearer {token}"));
                    }
                }
                // Apply explicit auth header if present.
                if let Some(headers) = http_headers {
                    if let Some(auth) = headers
                        .get("Authorization")
                        .or_else(|| headers.get("authorization"))
                    {
                        config = config.auth_header(auth.clone());
                    }
                }
                let transport = StreamableHttpClientTransport::from_config(config);
                serve_client((), transport).await.map_err(|e| {
                    CodexError::new(
                        ErrorCode::McpServerUnavailable,
                        format!("MCP init failed: {e}"),
                    )
                })
            }
        }
    }

    async fn mark_failed(&self, server_name: &str, error: &str) {
        let mut conns = self.connections.lock().await;
        if let Some(conn) = conns.get_mut(server_name) {
            conn.state = McpConnectionState::Failed(error.to_string());
        }
    }

    async fn emit_startup_update(&self, server_name: &str, status: McpStartupStatus) {
        if let Some(tx) = &self.tx_event {
            let id = self.next_event_id().await;
            let _ = tx
                .send(Event {
                    id: id.to_string(),
                    msg: EventMsg::McpStartupUpdate(McpStartupUpdateEvent {
                        server: server_name.to_string(),
                        status,
                    }),
                })
                .await;
        }
    }

    async fn emit_error(&self, message: &str) {
        if let Some(tx) = &self.tx_event {
            let id = self.next_event_id().await;
            let _ = tx
                .send(Event {
                    id: id.to_string(),
                    msg: EventMsg::Error(ErrorEvent {
                        message: message.to_string(),
                        codex_error_info: None,
                    }),
                })
                .await;
        }
    }

    async fn next_event_id(&self) -> u64 {
        let mut id = self.next_event_id.lock().await;
        let current = *id;
        *id += 1;
        current
    }
}

impl Default for McpConnectionManager {
    fn default() -> Self {
        Self::new()
    }
}

// ── Free functions ───────────────────────────────────────────────

pub fn qualify_tool_name(server: &str, tool: &str) -> String {
    let full = format!("mcp__{server}__{tool}");
    let sanitized = sanitize_tool_name(&full);
    if sanitized.len() <= MAX_QUALIFIED_NAME_LEN {
        return sanitized;
    }
    let mut hasher = Sha1::new();
    hasher.update(full.as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    let prefix_len = MAX_QUALIFIED_NAME_LEN - hash.len();
    format!("{}{}", &sanitized[..prefix_len], hash)
}

fn sanitize_tool_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

pub fn is_tool_allowed(
    tool_name: &str,
    enabled: Option<&[String]>,
    disabled: Option<&[String]>,
) -> bool {
    if let Some(enabled) = enabled {
        return enabled.iter().any(|e| e == tool_name);
    }
    if let Some(disabled) = disabled {
        return !disabled.iter().any(|d| d == tool_name);
    }
    true
}

fn is_tool_allowed_by_config(tool_name: &str, config: Option<&McpServerConfig>) -> bool {
    let Some(config) = config else {
        return true;
    };
    is_tool_allowed(
        tool_name,
        config.enabled_tools.as_deref(),
        config.disabled_tools.as_deref(),
    )
}

/// Split a qualified tool name `mcp__server__tool` into (server, tool).
pub fn split_qualified_tool_name(qualified_name: &str) -> Option<(String, String)> {
    let mut parts = qualified_name.split("__");
    let prefix = parts.next()?;
    if prefix != "mcp" {
        return None;
    }
    let server_name = parts.next()?;
    let tool_name: String = parts.collect::<Vec<_>>().join("__");
    if tool_name.is_empty() {
        return None;
    }
    Some((server_name.to_string(), tool_name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn qualify_short_name() {
        assert_eq!(qualify_tool_name("srv", "fetch"), "mcp__srv__fetch");
    }

    #[test]
    fn qualify_name_exceeds_limit_uses_hash() {
        let server = "a".repeat(30);
        let tool = "b".repeat(30);
        let result = qualify_tool_name(&server, &tool);
        assert!(result.len() <= MAX_QUALIFIED_NAME_LEN);
        let result2 = qualify_tool_name(&server, &"c".repeat(30));
        assert_ne!(result, result2);
    }

    proptest! {
        #[test]
        fn qualify_name_never_exceeds_max_len(server in "[a-z]{1,50}", tool in "[a-z]{1,50}") {
            let result = qualify_tool_name(&server, &tool);
            prop_assert!(result.len() <= MAX_QUALIFIED_NAME_LEN);
        }
    }

    #[test]
    fn no_filter_allows_all() {
        assert!(is_tool_allowed("anything", None, None));
    }

    #[test]
    fn enabled_list_allows_only_listed() {
        let enabled = vec!["fetch".to_string()];
        assert!(is_tool_allowed("fetch", Some(&enabled), None));
        assert!(!is_tool_allowed("delete", Some(&enabled), None));
    }

    #[test]
    fn disabled_list_blocks_listed() {
        let disabled = vec!["delete".to_string()];
        assert!(is_tool_allowed("fetch", None, Some(&disabled)));
        assert!(!is_tool_allowed("delete", None, Some(&disabled)));
    }

    #[test]
    fn split_qualified_name() {
        assert_eq!(
            split_qualified_tool_name("mcp__alpha__do_thing"),
            Some(("alpha".to_string(), "do_thing".to_string()))
        );
        assert_eq!(split_qualified_tool_name("other__alpha__do_thing"), None);
        assert_eq!(split_qualified_tool_name("mcp__alpha__"), None);
    }

    #[tokio::test]
    async fn connect_disabled_server_records_state() {
        let mgr = McpConnectionManager::new();
        let config = McpServerConfig {
            transport: McpServerTransportConfig::Stdio {
                command: "node".to_string(),
                args: vec![],
                env: HashMap::new(),
            },
            enabled: false,
            required: false,
            disabled_reason: Some("test".to_string()),
            startup_timeout_sec: None,
            tool_timeout_sec: None,
            enabled_tools: None,
            disabled_tools: None,
            scopes: None,
            oauth_resource: None,
        };
        mgr.connect("srv", &config).await.unwrap();
        assert!(mgr.is_disabled("srv").await);
        assert!(mgr.connected_servers().await.is_empty());
    }
}
