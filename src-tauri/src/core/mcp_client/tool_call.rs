use std::time::Instant;

use tracing::warn;

use super::connection_manager::McpConnectionManager;
use crate::protocol::error::CodexError;

/// Result of an MCP tool call with timing metadata.
#[derive(Debug, Clone)]
pub struct McpToolCallResult {
    pub output: Result<serde_json::Value, String>,
    pub duration: std::time::Duration,
}

/// Handle an MCP tool call with timing and error wrapping.
pub async fn handle_mcp_tool_call(
    manager: &McpConnectionManager,
    server: &str,
    tool_name: &str,
    arguments: serde_json::Value,
) -> McpToolCallResult {
    let start = Instant::now();
    let output = manager
        .call_tool(server, tool_name, arguments)
        .await
        .map_err(|e| format!("tool call error: {e}"));

    if let Err(ref e) = output {
        warn!("MCP tool call {server}/{tool_name} failed: {e}");
    }

    McpToolCallResult {
        output,
        duration: start.elapsed(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn call_to_unknown_server_returns_error() {
        let mgr = McpConnectionManager::new();
        let result =
            handle_mcp_tool_call(&mgr, "nonexistent", "tool", serde_json::Value::Null).await;
        assert!(result.output.is_err());
    }
}
