use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::Mutex;

use crate::protocol::error::CodexError;

/// MCP client connection state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpConnectionState {
    Disconnected,
    Connecting,
    Ready,
    Failed(String),
}

/// Manages connections to external MCP servers.
pub struct McpConnectionManager {
    connections: Arc<Mutex<HashMap<String, McpConnectionState>>>,
}

impl McpConnectionManager {
    pub fn new() -> Self {
        Self {
            connections: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn connect(&self, server_name: &str) -> Result<(), CodexError> {
        let mut conns = self.connections.lock().await;
        conns.insert(server_name.to_string(), McpConnectionState::Connecting);
        // TODO: actual MCP protocol handshake
        conns.insert(server_name.to_string(), McpConnectionState::Ready);
        Ok(())
    }

    pub async fn disconnect(&self, server_name: &str) {
        let mut conns = self.connections.lock().await;
        conns.insert(server_name.to_string(), McpConnectionState::Disconnected);
    }

    pub async fn state(&self, server_name: &str) -> McpConnectionState {
        let conns = self.connections.lock().await;
        conns
            .get(server_name)
            .cloned()
            .unwrap_or(McpConnectionState::Disconnected)
    }

    pub async fn connected_servers(&self) -> Vec<String> {
        let conns = self.connections.lock().await;
        conns
            .iter()
            .filter(|(_, state)| matches!(state, McpConnectionState::Ready))
            .map(|(name, _)| name.clone())
            .collect()
    }
}

impl Default for McpConnectionManager {
    fn default() -> Self {
        Self::new()
    }
}
