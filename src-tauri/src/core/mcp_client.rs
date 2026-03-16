use std::collections::HashMap;
use std::sync::Arc;

use rmcp::model::{CallToolRequestParams, ListToolsRequest, CallToolRequest, ServerResult, ListToolsResult, CallToolResult};
use rmcp::service::{RoleClient, RunningService, serve_client};
use rmcp::transport::TokioChildProcess;
use rmcp::transport::streamable_http_client::{
    StreamableHttpClientTransport, StreamableHttpClientTransportConfig,
};
use rmcp::ServiceExt;
use sha1::{Digest, Sha1};
use tokio::process::Command;
use tokio::sync::Mutex;

use crate::config::toml_types::{McpServerConfig, McpServerTransportConfig, McpToolFilter};
use crate::protocol::error::{CodexError, ErrorCode};
use crate::protocol::event::{ErrorEvent, Event, EventMsg, McpStartupUpdateEvent};
use crate::protocol::types::McpStartupStatus;

const MAX_QUALIFIED_NAME_LEN: usize = 64;

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
    pub description: String,
    pub input_schema: Option<serde_json::Value>,
}

// Use a type alias to avoid naming the generic parameter everywhere.
type RmcpClient = RunningService<RoleClient, ()>;

struct McpConnection {
    state: McpConnectionState,
    config: McpServerConfig,
    tools: HashMap<String, McpToolInfo>,
    /// Live rmcp client — present when state == Ready.
    client: Option<Arc<RmcpClient>>,
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

    pub async fn connect(&self, server_name: &str, config: &McpServerConfig) -> Result<(), CodexError> {
        if config.disabled {
            let mut conns = self.connections.lock().await;
            conns.insert(server_name.to_string(), McpConnection {
                state: McpConnectionState::Disconnected,
                config: config.clone(),
                tools: HashMap::new(),
                client: None,
            });
            return Ok(());
        }

        self.emit_startup_update(server_name, McpStartupStatus::Starting).await;
        {
            let mut conns = self.connections.lock().await;
            conns.insert(server_name.to_string(), McpConnection {
                state: McpConnectionState::Connecting,
                config: config.clone(),
                tools: HashMap::new(),
                client: None,
            });
        }

        let client = match self.build_client(server_name, &config.transport).await {
            Ok(c) => c,
            Err(e) => {
                let msg = format!("MCP connect '{server_name}' failed: {e}");
                self.mark_failed(server_name, &msg).await;
                self.emit_error(&msg).await;
                self.emit_startup_update(server_name, McpStartupStatus::Failed { error: msg.clone() }).await;
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

        if let Err(e) = self.discover_tools_internal(server_name, config.tool_filter.as_ref()).await {
            self.emit_error(&format!("Tool discovery failed for '{server_name}': {e}")).await;
        }

        self.emit_startup_update(server_name, McpStartupStatus::Ready).await;
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
        conns.get(server_name).map(|c| c.state.clone()).unwrap_or(McpConnectionState::Disconnected)
    }

    pub async fn connected_servers(&self) -> Vec<String> {
        let conns = self.connections.lock().await;
        conns.iter()
            .filter(|(_, c)| matches!(c.state, McpConnectionState::Ready))
            .map(|(n, _)| n.clone())
            .collect()
    }

    pub async fn is_disabled(&self, server_name: &str) -> bool {
        let conns = self.connections.lock().await;
        conns.get(server_name).map(|c| c.config.disabled).unwrap_or(false)
    }

    pub async fn disabled_reason(&self, server_name: &str) -> Option<String> {
        let conns = self.connections.lock().await;
        conns.get(server_name).and_then(|c| c.config.disabled_reason.clone())
    }

    pub async fn discover_tools(&self, server_name: &str) -> Result<Vec<McpToolInfo>, CodexError> {
        let filter = {
            let conns = self.connections.lock().await;
            conns.get(server_name).and_then(|c| c.config.tool_filter.clone())
        };
        self.discover_tools_internal(server_name, filter.as_ref()).await
    }

    async fn discover_tools_internal(
        &self,
        server_name: &str,
        tool_filter: Option<&McpToolFilter>,
    ) -> Result<Vec<McpToolInfo>, CodexError> {
        let client = {
            let conns = self.connections.lock().await;
            conns.get(server_name).and_then(|c| c.client.clone())
        };
        let Some(client) = client else {
            return Ok(vec![]);
        };

        let result = client.peer()
            .send_request(ListToolsRequest::default().into())
            .await
            .map_err(|e| CodexError::new(ErrorCode::McpServerUnavailable, format!("tools/list failed: {e}")))?;

        let tools_list = match result {
            ServerResult::ListToolsResult(r) => r.tools,
            _ => return Err(CodexError::new(ErrorCode::McpServerUnavailable, "unexpected response to tools/list".to_string())),
        };

        let mut discovered = Vec::new();
        for tool in tools_list {
            let tool_name = tool.name.to_string();
            if !is_tool_allowed(&tool_name, tool_filter) {
                continue;
            }
            let qualified = qualify_tool_name(server_name, &tool_name);
            let input_schema = serde_json::to_value(&tool.input_schema).ok();
            discovered.push(McpToolInfo {
                name: tool_name,
                qualified_name: qualified,
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
        conns.values()
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
        let client = {
            let conns = self.connections.lock().await;
            let conn = conns.get(server_name).ok_or_else(|| {
                CodexError::new(ErrorCode::McpServerUnavailable, format!("MCP server '{server_name}' not registered"))
            })?;
            if !matches!(conn.state, McpConnectionState::Ready) {
                return Err(CodexError::new(ErrorCode::McpServerUnavailable, format!("MCP server '{server_name}' not ready")));
            }
            conn.client.clone()
        };

        let Some(client) = client else {
            return Err(CodexError::new(ErrorCode::McpServerUnavailable, format!("MCP server '{server_name}' has no client")));
        };

        let arguments = match args {
            serde_json::Value::Object(map) => Some(map),
            serde_json::Value::Null => None,
            other => return Err(CodexError::new(ErrorCode::InvalidInput, format!("tool args must be object, got {other}"))),
        };

        let mut params = CallToolRequestParams::new(tool_name.to_string());
        if let Some(args) = arguments {
            params = params.with_arguments(args);
        }

        let result = client.peer()
            .send_request(CallToolRequest::new(params).into())
            .await
            .map_err(|e| CodexError::new(ErrorCode::ToolExecutionFailed, format!("tools/call failed: {e}")))?;

        let call_result = match result {
            ServerResult::CallToolResult(r) => r,
            _ => return Err(CodexError::new(ErrorCode::ToolExecutionFailed, "unexpected response to tools/call".to_string())),
        };

        // Convert CallToolResult content to JSON
        let content: Vec<serde_json::Value> = call_result.content.iter().map(|c| {
            serde_json::to_value(c).unwrap_or(serde_json::Value::Null)
        }).collect();

        Ok(serde_json::json!({
            "content": content,
            "isError": call_result.is_error.unwrap_or(false)
        }))
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
                let transport = TokioChildProcess::new(cmd)
                    .map_err(|e| CodexError::new(ErrorCode::McpServerUnavailable, format!("spawn failed: {e}")))?;
                serve_client((), transport)
                    .await
                    .map_err(|e| CodexError::new(ErrorCode::McpServerUnavailable, format!("MCP init failed: {e}")))
            }
            McpServerTransportConfig::Http { url, headers } => {
                let mut config = StreamableHttpClientTransportConfig::with_uri(url.clone());
                // Apply auth header if Authorization is present
                if let Some(auth) = headers.get("Authorization").or_else(|| headers.get("authorization")) {
                    config = config.auth_header(auth.clone());
                }
                let transport = StreamableHttpClientTransport::from_config(config);
                serve_client((), transport)
                    .await
                    .map_err(|e| CodexError::new(ErrorCode::McpServerUnavailable, format!("MCP init failed: {e}")))
            }
            McpServerTransportConfig::OAuth { url, client_id, client_secret, token_url } => {
                let token = fetch_oauth_token(token_url, client_id, client_secret).await?;
                let config = StreamableHttpClientTransportConfig::with_uri(url.clone())
                    .auth_header(token);
                let transport = StreamableHttpClientTransport::from_config(config);
                serve_client((), transport)
                    .await
                    .map_err(|e| CodexError::new(ErrorCode::McpServerUnavailable, format!("MCP init failed: {e}")))
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
            let _ = tx.send(Event {
                id: id.to_string(),
                msg: EventMsg::McpStartupUpdate(McpStartupUpdateEvent {
                    server: server_name.to_string(),
                    status,
                }),
            }).await;
        }
    }

    async fn emit_error(&self, message: &str) {
        if let Some(tx) = &self.tx_event {
            let id = self.next_event_id().await;
            let _ = tx.send(Event {
                id: id.to_string(),
                msg: EventMsg::Error(ErrorEvent {
                    message: message.to_string(),
                    codex_error_info: None,
                }),
            }).await;
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
    if full.len() <= MAX_QUALIFIED_NAME_LEN {
        return full;
    }
    let mut hasher = Sha1::new();
    hasher.update(full.as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    let hash_suffix = &hash[..8];
    let truncate_len = MAX_QUALIFIED_NAME_LEN - 1 - hash_suffix.len();
    let truncated = &full[..truncate_len];
    format!("{truncated}_{hash_suffix}")
}

pub fn is_tool_allowed(tool_name: &str, filter: Option<&McpToolFilter>) -> bool {
    let Some(filter) = filter else { return true; };
    if let Some(enabled) = &filter.enabled {
        return enabled.iter().any(|e| e == tool_name);
    }
    if let Some(disabled) = &filter.disabled {
        return !disabled.iter().any(|d| d == tool_name);
    }
    true
}

async fn fetch_oauth_token(token_url: &str, client_id: &str, client_secret: &str) -> Result<String, CodexError> {
    let response = reqwest::Client::new()
        .post(token_url)
        .form(&[
            ("grant_type", "client_credentials"),
            ("client_id", client_id),
            ("client_secret", client_secret),
        ])
        .send()
        .await
        .map_err(|e| CodexError::new(ErrorCode::McpServerUnavailable, format!("token request failed: {e}")))?;

    if !response.status().is_success() {
        return Err(CodexError::new(ErrorCode::McpServerUnavailable, format!("token endpoint returned {}", response.status())));
    }

    let body: serde_json::Value = response.json().await
        .map_err(|e| CodexError::new(ErrorCode::McpServerUnavailable, format!("token parse failed: {e}")))?;

    body["access_token"].as_str().map(String::from).ok_or_else(|| {
        CodexError::new(ErrorCode::McpServerUnavailable, "token response missing access_token".to_string())
    })
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
        assert!(is_tool_allowed("anything", None));
    }

    #[test]
    fn enabled_list_allows_only_listed() {
        let filter = McpToolFilter {
            enabled: Some(vec!["fetch".to_string()]),
            disabled: None,
        };
        assert!(is_tool_allowed("fetch", Some(&filter)));
        assert!(!is_tool_allowed("delete", Some(&filter)));
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
            disabled: true,
            disabled_reason: Some("test".to_string()),
            tool_filter: None,
        };
        mgr.connect("srv", &config).await.unwrap();
        assert!(mgr.is_disabled("srv").await);
        assert!(mgr.connected_servers().await.is_empty());
    }
}
