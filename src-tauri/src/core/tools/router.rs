use std::collections::HashMap;

use crate::core::mcp_client::McpConnectionManager;
use crate::core::tools::{ToolInfo, ToolKind, ToolRegistry};
use crate::protocol::error::{CodexError, ErrorCode};
use crate::protocol::types::DynamicToolSpec;

/// Routes tool calls to the correct handler by priority:
/// 1. Built-in ToolRegistry handlers
/// 2. MCP tools (via McpConnectionManager)
/// 3. Dynamically registered tools
pub struct ToolRouter {
    registry: ToolRegistry,
    mcp_manager: McpConnectionManager,
    dynamic_tools: HashMap<String, DynamicToolSpec>,
}

impl ToolRouter {
    pub fn new(registry: ToolRegistry, mcp_manager: McpConnectionManager) -> Self {
        Self {
            registry,
            mcp_manager,
            dynamic_tools: HashMap::new(),
        }
    }

    /// Route a tool call to the appropriate handler.
    ///
    /// Priority: built-in → MCP → dynamic.
    /// Returns `ToolExecutionFailed` if no handler is found.
    pub async fn route_tool_call(
        &self,
        tool_name: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, CodexError> {
        // 1. Try built-in registry
        let builtin_kind = ToolKind::Builtin(tool_name.to_string());
        if self.registry.find(&builtin_kind).is_some() {
            return self.registry.dispatch(&builtin_kind, args).await;
        }

        // 2. Try MCP tool (format: mcp__{server}__{tool})
        if let Some((server, tool)) = parse_mcp_tool_name(tool_name) {
            let mcp_kind = ToolKind::Mcp {
                server: server.to_string(),
                tool: tool.to_string(),
            };
            if self.registry.find(&mcp_kind).is_some() {
                return self.registry.dispatch(&mcp_kind, args).await;
            }
            // Check if the MCP server is connected even without a registry entry
            let connected = self.mcp_manager.connected_servers().await;
            if connected.contains(&server.to_string()) {
                // Server is connected — delegate would happen here in full impl.
                // For now, return a placeholder indicating the tool was found on MCP.
                return Err(CodexError::new(
                    ErrorCode::ToolExecutionFailed,
                    format!("MCP tool '{tool_name}' found on server '{server}' but call delegation is not yet implemented"),
                ));
            }
        }

        // 3. Try dynamic tools
        if self.dynamic_tools.contains_key(tool_name) {
            let dynamic_kind = ToolKind::Dynamic(tool_name.to_string());
            if self.registry.find(&dynamic_kind).is_some() {
                return self.registry.dispatch(&dynamic_kind, args).await;
            }
            // Dynamic tool is registered but has no handler in registry yet —
            // the caller (codex.rs) is responsible for sending DynamicToolCallRequest
            // and awaiting DynamicToolResponse. Return a sentinel error so the caller
            // knows to initiate the dynamic tool call protocol.
            return Err(CodexError::new(
                ErrorCode::ToolExecutionFailed,
                format!("dynamic tool '{tool_name}' requires external invocation via DynamicToolCallRequest"),
            ));
        }

        Err(CodexError::new(
            ErrorCode::ToolExecutionFailed,
            format!("no handler found for tool: {tool_name}"),
        ))
    }

    /// Register a dynamic tool spec. The tool becomes immediately available for routing.
    pub fn register_dynamic_tool(&mut self, spec: DynamicToolSpec) {
        self.dynamic_tools.insert(spec.name.clone(), spec);
    }

    /// Remove a previously registered dynamic tool.
    pub fn unregister_dynamic_tool(&mut self, name: &str) -> Option<DynamicToolSpec> {
        self.dynamic_tools.remove(name)
    }

    /// Check whether a dynamic tool with the given name is registered.
    pub fn has_dynamic_tool(&self, name: &str) -> bool {
        self.dynamic_tools.contains_key(name)
    }

    /// Get a reference to a registered dynamic tool spec.
    pub fn get_dynamic_tool(&self, name: &str) -> Option<&DynamicToolSpec> {
        self.dynamic_tools.get(name)
    }

    /// List all available tools across built-in, MCP, and dynamic sources.
    pub fn list_all_tools(&self) -> Vec<ToolInfo> {
        let mut tools = Vec::new();

        // Built-in tools from registry
        for kind in self.registry.registered_kinds() {
            tools.push(ToolInfo {
                name: kind.name(),
                description: String::new(),
                kind,
                input_schema: None,
            });
        }

        // Dynamic tools
        for spec in self.dynamic_tools.values() {
            tools.push(ToolInfo {
                name: spec.name.clone(),
                description: spec.description.clone(),
                kind: ToolKind::Dynamic(spec.name.clone()),
                input_schema: Some(spec.input_schema.clone()),
            });
        }

        tools
    }

    /// Access the underlying built-in tool registry.
    pub fn registry(&self) -> &ToolRegistry {
        &self.registry
    }

    /// Mutable access to the underlying built-in tool registry.
    pub fn registry_mut(&mut self) -> &mut ToolRegistry {
        &mut self.registry
    }

    /// Access the MCP connection manager.
    pub fn mcp_manager(&self) -> &McpConnectionManager {
        &self.mcp_manager
    }
}

/// Parse an MCP-qualified tool name (`mcp__{server}__{tool}`) into (server, tool).
fn parse_mcp_tool_name(name: &str) -> Option<(&str, &str)> {
    let rest = name.strip_prefix("mcp__")?;
    let sep_idx = rest.find("__")?;
    let server = &rest[..sep_idx];
    let tool = &rest[sep_idx + 2..];
    if server.is_empty() || tool.is_empty() {
        return None;
    }
    Some((server, tool))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::tools::ToolHandler;
    use async_trait::async_trait;

    struct FakeTool {
        tool_name: String,
    }

    #[async_trait]
    impl ToolHandler for FakeTool {
        fn matches_kind(&self, kind: &ToolKind) -> bool {
            matches!(kind, ToolKind::Builtin(n) if n == &self.tool_name)
        }
        fn kind(&self) -> ToolKind {
            ToolKind::Builtin(self.tool_name.clone())
        }
        async fn handle(&self, args: serde_json::Value) -> Result<serde_json::Value, CodexError> {
            Ok(serde_json::json!({"handled_by": self.tool_name, "args": args}))
        }
    }

    fn make_router_with_builtin(name: &str) -> ToolRouter {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(FakeTool {
            tool_name: name.to_string(),
        }));
        ToolRouter::new(registry, McpConnectionManager::new())
    }

    #[tokio::test]
    async fn routes_to_builtin_handler() {
        let router = make_router_with_builtin("read_file");
        let result = router
            .route_tool_call("read_file", serde_json::json!({"path": "/tmp"}))
            .await
            .unwrap();
        assert_eq!(result["handled_by"], "read_file");
    }

    #[tokio::test]
    async fn unknown_tool_returns_error() {
        let router = ToolRouter::new(ToolRegistry::new(), McpConnectionManager::new());
        let result = router
            .route_tool_call("nonexistent", serde_json::Value::Null)
            .await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, ErrorCode::ToolExecutionFailed);
    }

    #[test]
    fn register_and_query_dynamic_tool() {
        let mut router = ToolRouter::new(ToolRegistry::new(), McpConnectionManager::new());
        let spec = DynamicToolSpec {
            name: "my_tool".to_string(),
            description: "a custom tool".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        };
        router.register_dynamic_tool(spec);
        assert!(router.has_dynamic_tool("my_tool"));
        assert!(!router.has_dynamic_tool("other"));
        assert_eq!(
            router.get_dynamic_tool("my_tool").unwrap().description,
            "a custom tool"
        );
    }

    #[test]
    fn unregister_dynamic_tool() {
        let mut router = ToolRouter::new(ToolRegistry::new(), McpConnectionManager::new());
        router.register_dynamic_tool(DynamicToolSpec {
            name: "tmp".to_string(),
            description: String::new(),
            input_schema: serde_json::Value::Null,
        });
        assert!(router.has_dynamic_tool("tmp"));
        let removed = router.unregister_dynamic_tool("tmp");
        assert!(removed.is_some());
        assert!(!router.has_dynamic_tool("tmp"));
    }

    #[test]
    fn list_all_tools_includes_builtin_and_dynamic() {
        let mut router = make_router_with_builtin("grep");
        router.register_dynamic_tool(DynamicToolSpec {
            name: "custom".to_string(),
            description: "desc".to_string(),
            input_schema: serde_json::json!({}),
        });
        let tools = router.list_all_tools();
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"grep"));
        assert!(names.contains(&"custom"));
    }

    #[test]
    fn parse_mcp_tool_name_valid() {
        assert_eq!(
            parse_mcp_tool_name("mcp__server1__fetch"),
            Some(("server1", "fetch"))
        );
        assert_eq!(parse_mcp_tool_name("mcp__a__b__c"), Some(("a", "b__c")));
    }

    #[test]
    fn parse_mcp_tool_name_invalid() {
        assert_eq!(parse_mcp_tool_name("not_mcp"), None);
        assert_eq!(parse_mcp_tool_name("mcp__"), None);
        assert_eq!(parse_mcp_tool_name("mcp____tool"), None);
        assert_eq!(parse_mcp_tool_name("mcp__server__"), None);
    }
}
