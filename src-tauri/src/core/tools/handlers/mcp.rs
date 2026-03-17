use async_trait::async_trait;

use crate::core::tools::{ToolHandler, ToolKind};
use crate::protocol::error::{CodexError, ErrorCode};

/// Handler for MCP tool calls. Delegates to the MCP connection manager.
pub struct McpHandler;

#[async_trait]
impl ToolHandler for McpHandler {
    fn matches_kind(&self, kind: &ToolKind) -> bool {
        matches!(kind, ToolKind::Mcp { .. })
    }

    fn kind(&self) -> ToolKind {
        ToolKind::Mcp {
            server: "__mcp__".to_string(),
            tool: "__handler__".to_string(),
        }
    }

    async fn handle(&self, args: serde_json::Value) -> Result<serde_json::Value, CodexError> {
        // MCP tool calls are dispatched through the McpConnectionManager in the router.
        // This handler is a placeholder for the registry; actual MCP dispatch happens
        // at the router level via mcp_manager.call_tool().
        Err(CodexError::new(
            ErrorCode::ToolExecutionFailed,
            format!("MCP tool calls should be dispatched via McpConnectionManager, not directly. Args: {args}"),
        ))
    }
}
