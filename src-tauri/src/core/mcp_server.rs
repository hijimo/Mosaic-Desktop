use serde::{Deserialize, Serialize};

use crate::core::tools::router::ToolRouter;
use crate::core::tools::ToolKind;
use crate::protocol::error::CodexError;

// ── JSON-RPC 2.0 wire types ────────────────────────────────────────────

/// JSON-RPC 2.0 request envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<serde_json::Value>,
    pub method: String,
    #[serde(default)]
    pub params: Option<serde_json::Value>,
}

/// JSON-RPC 2.0 success response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<serde_json::Value>,
    pub result: serde_json::Value,
}

/// JSON-RPC 2.0 error detail.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcErrorData {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// JSON-RPC 2.0 error response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcErrorResponse {
    pub jsonrpc: String,
    pub id: Option<serde_json::Value>,
    pub error: JsonRpcErrorData,
}

// ── MCP-specific payload types ──────────────────────────────────────────

/// Server capabilities returned by `initialize`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerCapabilities {
    pub tools: McpToolsCapability,
}

/// Indicates the server supports tool listing and calling.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolsCapability {
    pub list_changed: bool,
}

/// Result of `initialize`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    pub protocol_version: String,
    pub capabilities: McpServerCapabilities,
    pub server_info: McpServerInfo,
}

/// Server identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerInfo {
    pub name: String,
    pub version: String,
}

/// A single tool descriptor returned by `tools/list`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolDescriptor {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Result of `tools/list`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolsListResult {
    pub tools: Vec<McpToolDescriptor>,
}

/// Parameters for `tools/call`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolsCallParams {
    pub name: String,
    #[serde(default)]
    pub arguments: serde_json::Value,
}

/// A content item in the `tools/call` result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResultContent {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: String,
}

/// Result of `tools/call`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolsCallResult {
    pub content: Vec<ToolResultContent>,
    pub is_error: bool,
}

// ── JSON-RPC error codes ────────────────────────────────────────────────

/// Standard JSON-RPC: method not found.
const METHOD_NOT_FOUND: i32 = -32601;
/// Standard JSON-RPC: invalid params (used for unknown tool).
const INVALID_PARAMS: i32 = -32602;
/// Standard JSON-RPC: internal error (reserved for future use).
#[allow(dead_code)]
const INTERNAL_ERROR: i32 = -32603;

// ── McpServer ───────────────────────────────────────────────────────────

/// MCP server that exposes Codex tools to external clients via JSON-RPC.
///
/// Implements three MCP methods:
/// - `initialize` — returns server capabilities and info
/// - `tools/list` — returns all registered tools with name, description, and input schema
/// - `tools/call` — dispatches a tool call and returns the result
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
    pub async fn start(&mut self) -> Result<(), CodexError> {
        self.running = true;
        Ok(())
    }

    pub async fn stop(&mut self) {
        self.running = false;
    }

    /// Handle a single JSON-RPC request and produce a response.
    ///
    /// Dispatches to `initialize`, `tools/list`, or `tools/call`.
    /// Unknown methods return JSON-RPC error -32601 (method not found).
    pub async fn handle_request(
        &self,
        request: &JsonRpcRequest,
        router: &ToolRouter,
    ) -> Result<JsonRpcResponse, JsonRpcErrorResponse> {
        match request.method.as_str() {
            "initialize" => self.handle_initialize(request),
            "tools/list" => self.handle_tools_list(request, router),
            "tools/call" => self.handle_tools_call(request, router).await,
            _ => Err(make_error_response(
                request.id.clone(),
                METHOD_NOT_FOUND,
                format!("method not found: {}", request.method),
                None,
            )),
        }
    }

    /// `initialize` — returns protocol version, capabilities, and server info.
    fn handle_initialize(
        &self,
        request: &JsonRpcRequest,
    ) -> Result<JsonRpcResponse, JsonRpcErrorResponse> {
        let result = InitializeResult {
            protocol_version: "2024-11-05".to_string(),
            capabilities: McpServerCapabilities {
                tools: McpToolsCapability {
                    list_changed: false,
                },
            },
            server_info: McpServerInfo {
                name: "mosaic".to_string(),
                version: "0.1.0".to_string(),
            },
        };

        Ok(JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id.clone(),
            result: serde_json::to_value(result).unwrap_or_default(),
        })
    }

    /// `tools/list` — returns all registered tool handlers with name and input schema.
    fn handle_tools_list(
        &self,
        request: &JsonRpcRequest,
        router: &ToolRouter,
    ) -> Result<JsonRpcResponse, JsonRpcErrorResponse> {
        let all_tools = router.list_all_tools();
        let descriptors: Vec<McpToolDescriptor> = all_tools
            .into_iter()
            .map(|info| McpToolDescriptor {
                name: info.name,
                description: info.description,
                input_schema: info
                    .input_schema
                    .unwrap_or_else(|| serde_json::json!({"type": "object"})),
            })
            .collect();

        let result = ToolsListResult { tools: descriptors };

        Ok(JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id.clone(),
            result: serde_json::to_value(result).unwrap_or_default(),
        })
    }

    /// `tools/call` — dispatches to the matching tool handler and returns the result.
    ///
    /// Unknown tool names produce JSON-RPC error -32602 (invalid params).
    async fn handle_tools_call(
        &self,
        request: &JsonRpcRequest,
        router: &ToolRouter,
    ) -> Result<JsonRpcResponse, JsonRpcErrorResponse> {
        let params: ToolsCallParams = match &request.params {
            Some(p) => serde_json::from_value(p.clone()).map_err(|e| {
                make_error_response(
                    request.id.clone(),
                    INVALID_PARAMS,
                    format!("invalid tools/call params: {e}"),
                    None,
                )
            })?,
            None => {
                return Err(make_error_response(
                    request.id.clone(),
                    INVALID_PARAMS,
                    "tools/call requires params with 'name' field".to_string(),
                    None,
                ));
            }
        };

        // Check if the tool exists before attempting dispatch.
        let tool_exists = router
            .list_all_tools()
            .iter()
            .any(|t| t.name == params.name);
        if !tool_exists {
            // Also check the built-in registry directly.
            let builtin_kind = ToolKind::Builtin(params.name.clone());
            if router.registry().find(&builtin_kind).is_none() {
                return Err(make_error_response(
                    request.id.clone(),
                    INVALID_PARAMS,
                    format!("unknown tool: {}", params.name),
                    None,
                ));
            }
        }

        use crate::core::tools::router::RouteResult;

        match router.route_tool_call(&params.name, params.arguments).await {
            RouteResult::Handled(Ok(result)) => {
                let text = match result {
                    serde_json::Value::String(s) => s,
                    other => serde_json::to_string(&other).unwrap_or_default(),
                };
                let call_result = ToolsCallResult {
                    content: vec![ToolResultContent {
                        content_type: "text".to_string(),
                        text,
                    }],
                    is_error: false,
                };
                Ok(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: request.id.clone(),
                    result: serde_json::to_value(call_result).unwrap_or_default(),
                })
            }
            RouteResult::Handled(Err(err)) => {
                let call_result = ToolsCallResult {
                    content: vec![ToolResultContent {
                        content_type: "text".to_string(),
                        text: err.message.clone(),
                    }],
                    is_error: true,
                };
                // Tool execution errors are returned as successful JSON-RPC responses
                // with `isError: true` in the MCP result, per MCP spec.
                Ok(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: request.id.clone(),
                    result: serde_json::to_value(call_result).unwrap_or_default(),
                })
            }
            RouteResult::DynamicTool(name) => {
                // Dynamic tools require the DynamicToolCallRequest/Response protocol
                // which is not available in the MCP server context. Return an error.
                let call_result = ToolsCallResult {
                    content: vec![ToolResultContent {
                        content_type: "text".to_string(),
                        text: format!("dynamic tool '{name}' cannot be invoked via MCP server"),
                    }],
                    is_error: true,
                };
                Ok(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: request.id.clone(),
                    result: serde_json::to_value(call_result).unwrap_or_default(),
                })
            }
            RouteResult::NotFound(name) => Err(make_error_response(
                request.id.clone(),
                INVALID_PARAMS,
                format!("unknown tool: {name}"),
                None,
            )),
        }
    }
}

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper to construct a JSON-RPC error response.
fn make_error_response(
    id: Option<serde_json::Value>,
    code: i32,
    message: String,
    data: Option<serde_json::Value>,
) -> JsonRpcErrorResponse {
    JsonRpcErrorResponse {
        jsonrpc: "2.0".to_string(),
        id,
        error: JsonRpcErrorData {
            code,
            message,
            data,
        },
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::mcp_client::McpConnectionManager;
    use crate::core::tools::{ToolHandler, ToolKind, ToolRegistry};
    use crate::protocol::error::CodexError;
    use crate::protocol::types::DynamicToolSpec;
    use async_trait::async_trait;

    // ── Test helpers ────────────────────────────────────────────────────

    struct FakeTool {
        name: String,
    }

    #[async_trait]
    impl ToolHandler for FakeTool {
        fn matches_kind(&self, kind: &ToolKind) -> bool {
            matches!(kind, ToolKind::Builtin(n) if n == &self.name)
        }
        fn kind(&self) -> ToolKind {
            ToolKind::Builtin(self.name.clone())
        }
        async fn handle(&self, args: serde_json::Value) -> Result<serde_json::Value, CodexError> {
            Ok(serde_json::json!({"tool": self.name, "args": args}))
        }
    }

    struct FailingTool;

    #[async_trait]
    impl ToolHandler for FailingTool {
        fn matches_kind(&self, kind: &ToolKind) -> bool {
            matches!(kind, ToolKind::Builtin(n) if n == "fail_tool")
        }
        fn kind(&self) -> ToolKind {
            ToolKind::Builtin("fail_tool".to_string())
        }
        async fn handle(&self, _args: serde_json::Value) -> Result<serde_json::Value, CodexError> {
            Err(CodexError::new(
                crate::protocol::error::ErrorCode::ToolExecutionFailed,
                "intentional failure",
            ))
        }
    }

    fn make_request(method: &str, params: Option<serde_json::Value>) -> JsonRpcRequest {
        JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::json!(1)),
            method: method.to_string(),
            params,
        }
    }

    fn make_router_with_tools(names: &[&str]) -> ToolRouter {
        let mut registry = ToolRegistry::new();
        for name in names {
            registry.register(Box::new(FakeTool {
                name: name.to_string(),
            }));
        }
        ToolRouter::new(registry, McpConnectionManager::new())
    }

    // ── initialize ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn initialize_returns_server_info() {
        let server = McpServer::new();
        let router = make_router_with_tools(&[]);
        let req = make_request("initialize", None);

        let resp = server.handle_request(&req, &router).await.unwrap();
        let result: InitializeResult = serde_json::from_value(resp.result).unwrap();

        assert_eq!(result.server_info.name, "mosaic");
        assert_eq!(result.protocol_version, "2024-11-05");
    }

    // ── tools/list ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn tools_list_returns_all_registered_handlers() {
        let server = McpServer::new();
        let router = make_router_with_tools(&["read_file", "write_file"]);
        let req = make_request("tools/list", None);

        let resp = server.handle_request(&req, &router).await.unwrap();
        let result: ToolsListResult = serde_json::from_value(resp.result).unwrap();

        let names: Vec<&str> = result.tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"read_file"));
        assert!(names.contains(&"write_file"));
        assert_eq!(result.tools.len(), 2);
    }

    #[tokio::test]
    async fn tools_list_includes_dynamic_tools() {
        let server = McpServer::new();
        let mut router = make_router_with_tools(&["builtin"]);
        router.register_dynamic_tool(DynamicToolSpec {
            name: "dynamic_one".to_string(),
            description: "a dynamic tool".to_string(),
            input_schema: serde_json::json!({"type": "object", "properties": {}}),
        });
        let req = make_request("tools/list", None);

        let resp = server.handle_request(&req, &router).await.unwrap();
        let result: ToolsListResult = serde_json::from_value(resp.result).unwrap();

        let names: Vec<&str> = result.tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"builtin"));
        assert!(names.contains(&"dynamic_one"));
    }

    #[tokio::test]
    async fn tools_list_empty_when_no_handlers() {
        let server = McpServer::new();
        let router = make_router_with_tools(&[]);
        let req = make_request("tools/list", None);

        let resp = server.handle_request(&req, &router).await.unwrap();
        let result: ToolsListResult = serde_json::from_value(resp.result).unwrap();

        assert!(result.tools.is_empty());
    }

    // ── tools/call ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn tools_call_dispatches_to_handler() {
        let server = McpServer::new();
        let router = make_router_with_tools(&["echo"]);
        let req = make_request(
            "tools/call",
            Some(serde_json::json!({"name": "echo", "arguments": {"msg": "hi"}})),
        );

        let resp = server.handle_request(&req, &router).await.unwrap();
        let result: ToolsCallResult = serde_json::from_value(resp.result).unwrap();

        assert!(!result.is_error);
        assert_eq!(result.content.len(), 1);
        assert_eq!(result.content[0].content_type, "text");
        // The FakeTool returns JSON with tool name and args
        assert!(result.content[0].text.contains("echo"));
    }

    #[tokio::test]
    async fn tools_call_unknown_tool_returns_invalid_params() {
        let server = McpServer::new();
        let router = make_router_with_tools(&[]);
        let req = make_request(
            "tools/call",
            Some(serde_json::json!({"name": "nonexistent", "arguments": {}})),
        );

        let err = server.handle_request(&req, &router).await.unwrap_err();

        assert_eq!(err.error.code, INVALID_PARAMS);
        assert!(err.error.message.contains("nonexistent"));
    }

    #[tokio::test]
    async fn tools_call_missing_params_returns_error() {
        let server = McpServer::new();
        let router = make_router_with_tools(&[]);
        let req = make_request("tools/call", None);

        let err = server.handle_request(&req, &router).await.unwrap_err();

        assert_eq!(err.error.code, INVALID_PARAMS);
    }

    #[tokio::test]
    async fn tools_call_handler_error_returns_is_error_true() {
        let server = McpServer::new();
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(FailingTool));
        let router = ToolRouter::new(registry, McpConnectionManager::new());

        let req = make_request(
            "tools/call",
            Some(serde_json::json!({"name": "fail_tool", "arguments": {}})),
        );

        let resp = server.handle_request(&req, &router).await.unwrap();
        let result: ToolsCallResult = serde_json::from_value(resp.result).unwrap();

        assert!(result.is_error);
        assert!(result.content[0].text.contains("intentional failure"));
    }

    // ── unknown method ──────────────────────────────────────────────────

    #[tokio::test]
    async fn unknown_method_returns_method_not_found() {
        let server = McpServer::new();
        let router = make_router_with_tools(&[]);
        let req = make_request("unknown/method", None);

        let err = server.handle_request(&req, &router).await.unwrap_err();

        assert_eq!(err.error.code, METHOD_NOT_FOUND);
    }

    // ── start / stop lifecycle ──────────────────────────────────────────

    #[tokio::test]
    async fn start_stop_lifecycle() {
        let mut server = McpServer::new();
        assert!(!server.is_running());

        server.start().await.unwrap();
        assert!(server.is_running());

        server.stop().await;
        assert!(!server.is_running());
    }

    // ── JSON-RPC response structure ─────────────────────────────────────

    #[tokio::test]
    async fn response_has_correct_jsonrpc_version() {
        let server = McpServer::new();
        let router = make_router_with_tools(&[]);
        let req = make_request("initialize", None);

        let resp = server.handle_request(&req, &router).await.unwrap();

        assert_eq!(resp.jsonrpc, "2.0");
        assert_eq!(resp.id, Some(serde_json::json!(1)));
    }

    #[tokio::test]
    async fn error_response_preserves_request_id() {
        let server = McpServer::new();
        let router = make_router_with_tools(&[]);
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::json!("req-42")),
            method: "tools/call".to_string(),
            params: Some(serde_json::json!({"name": "missing"})),
        };

        let err = server.handle_request(&req, &router).await.unwrap_err();

        assert_eq!(err.id, Some(serde_json::json!("req-42")));
    }
}
