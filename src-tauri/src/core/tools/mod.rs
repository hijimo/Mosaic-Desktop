pub mod handlers;
pub mod router;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::protocol::error::{CodexError, ErrorCode};

/// Identifies the kind of tool — used for registry lookup and dispatch.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolKind {
    /// A built-in tool identified by name.
    Builtin(String),
    /// An MCP tool qualified as `mcp__{server}__{tool}`.
    Mcp { server: String, tool: String },
    /// A dynamically registered tool.
    Dynamic(String),
}

impl ToolKind {
    /// Returns the display name for this tool kind.
    pub fn name(&self) -> String {
        match self {
            ToolKind::Builtin(name) => name.clone(),
            ToolKind::Mcp { server, tool } => format!("mcp__{server}__{tool}"),
            ToolKind::Dynamic(name) => name.clone(),
        }
    }
}

/// Tool metadata returned by listing operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub kind: ToolKind,
    pub input_schema: Option<serde_json::Value>,
}

/// Async trait that all tool handlers must implement.
///
/// - `matches_kind`: returns true if this handler can service the given `ToolKind`.
/// - `kind`: returns the canonical `ToolKind` for this handler.
/// - `handle`: executes the tool with the provided JSON arguments.
#[async_trait]
pub trait ToolHandler: Send + Sync {
    fn matches_kind(&self, kind: &ToolKind) -> bool;
    fn kind(&self) -> ToolKind;
    async fn handle(&self, args: serde_json::Value) -> Result<serde_json::Value, CodexError>;
}

/// Registry of built-in tool handlers.
///
/// Supports runtime registration and dispatch by `ToolKind`.
pub struct ToolRegistry {
    handlers: Vec<Box<dyn ToolHandler>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            handlers: Vec::new(),
        }
    }

    /// Register a new tool handler.
    pub fn register(&mut self, handler: Box<dyn ToolHandler>) {
        self.handlers.push(handler);
    }

    /// Dispatch a tool call to the first handler whose `matches_kind` returns true.
    pub async fn dispatch(
        &self,
        kind: &ToolKind,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, CodexError> {
        for handler in &self.handlers {
            if handler.matches_kind(kind) {
                return handler.handle(args).await;
            }
        }
        Err(CodexError::new(
            ErrorCode::ToolExecutionFailed,
            format!("no handler registered for tool kind: {}", kind.name()),
        ))
    }

    /// Find a handler matching the given kind.
    pub fn find(&self, kind: &ToolKind) -> Option<&dyn ToolHandler> {
        self.handlers
            .iter()
            .find(|h| h.matches_kind(kind))
            .map(|h| h.as_ref())
    }

    /// List all registered handler kinds.
    pub fn registered_kinds(&self) -> Vec<ToolKind> {
        self.handlers.iter().map(|h| h.kind()).collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EchoTool;

    #[async_trait]
    impl ToolHandler for EchoTool {
        fn matches_kind(&self, kind: &ToolKind) -> bool {
            matches!(kind, ToolKind::Builtin(name) if name == "echo")
        }

        fn kind(&self) -> ToolKind {
            ToolKind::Builtin("echo".to_string())
        }

        async fn handle(&self, args: serde_json::Value) -> Result<serde_json::Value, CodexError> {
            Ok(args)
        }
    }

    #[tokio::test]
    async fn register_and_dispatch() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(EchoTool));

        let kind = ToolKind::Builtin("echo".to_string());
        let input = serde_json::json!({"msg": "hello"});
        let result = registry.dispatch(&kind, input.clone()).await.unwrap();
        assert_eq!(result, input);
    }

    #[tokio::test]
    async fn dispatch_unknown_returns_error() {
        let registry = ToolRegistry::new();
        let kind = ToolKind::Builtin("nonexistent".to_string());
        let result = registry.dispatch(&kind, serde_json::Value::Null).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, ErrorCode::ToolExecutionFailed);
    }

    #[test]
    fn find_returns_matching_handler() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(EchoTool));

        let kind = ToolKind::Builtin("echo".to_string());
        assert!(registry.find(&kind).is_some());

        let missing = ToolKind::Builtin("missing".to_string());
        assert!(registry.find(&missing).is_none());
    }

    #[test]
    fn registered_kinds_lists_all() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(EchoTool));
        let kinds = registry.registered_kinds();
        assert_eq!(kinds.len(), 1);
        assert_eq!(kinds[0], ToolKind::Builtin("echo".to_string()));
    }

    #[test]
    fn tool_kind_name_formats_correctly() {
        assert_eq!(ToolKind::Builtin("read".to_string()).name(), "read");
        assert_eq!(
            ToolKind::Mcp {
                server: "srv".to_string(),
                tool: "fetch".to_string()
            }
            .name(),
            "mcp__srv__fetch"
        );
        assert_eq!(ToolKind::Dynamic("custom".to_string()).name(), "custom");
    }
}
