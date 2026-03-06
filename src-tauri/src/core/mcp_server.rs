/// MCP server that exposes Codex tools to external clients.
/// This is the "dual-role" MCP described in the design docs:
/// Codex acts as both MCP client (connecting to tool servers)
/// and MCP server (exposing its own tools).
pub struct McpServer {
    running: bool,
}

impl McpServer {
    pub fn new() -> Self {
        Self { running: false }
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Start serving MCP requests.
    pub async fn start(&mut self) -> Result<(), crate::protocol::error::CodexError> {
        self.running = true;
        // TODO: bind JSON-RPC transport, register tool handlers
        Ok(())
    }

    pub async fn stop(&mut self) {
        self.running = false;
    }
}

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}
