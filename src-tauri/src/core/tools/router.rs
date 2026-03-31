use std::collections::HashMap;

use crate::core::mcp_client::McpConnectionManager;
use crate::core::tools::spec::{build_specs, ConfiguredToolSpec, ToolSpec, ToolsConfig};
use crate::core::tools::{ToolInfo, ToolKind, ToolRegistry};
use crate::protocol::error::{CodexError, ErrorCode};
use crate::protocol::types::DynamicToolSpec;

/// Result of a tool routing attempt.
pub enum RouteResult {
    /// Tool was handled by a built-in or MCP handler; contains the result.
    Handled(Result<serde_json::Value, CodexError>),
    /// Tool is a registered dynamic tool that requires external invocation
    /// via the DynamicToolCallRequest/DynamicToolResponse protocol.
    /// Contains the `DynamicToolSpec` name for the caller to initiate the protocol.
    DynamicTool(String),
    /// No handler found for the tool.
    NotFound(String),
}

impl std::fmt::Debug for RouteResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RouteResult::Handled(Ok(v)) => write!(f, "Handled(Ok({v}))"),
            RouteResult::Handled(Err(e)) => write!(f, "Handled(Err({}))", e.message),
            RouteResult::DynamicTool(name) => write!(f, "DynamicTool({name})"),
            RouteResult::NotFound(name) => write!(f, "NotFound({name})"),
        }
    }
}

/// Routes tool calls to the correct handler by priority:
/// 1. Built-in ToolRegistry handlers
/// 2. MCP tools (via McpConnectionManager)
/// 3. Dynamically registered tools
pub struct ToolRouter {
    registry: ToolRegistry,
    mcp_manager: McpConnectionManager,
    dynamic_tools: HashMap<String, DynamicToolSpec>,
    configured_specs: Vec<ConfiguredToolSpec>,
}

impl ToolRouter {
    pub fn new(registry: ToolRegistry, mcp_manager: McpConnectionManager) -> Self {
        Self {
            registry,
            mcp_manager,
            dynamic_tools: HashMap::new(),
            configured_specs: Vec::new(),
        }
    }

    pub fn from_config(config: ToolsConfig, has_agent_control: bool) -> Self {
        let assembled = build_specs(&config, has_agent_control);
        Self {
            registry: assembled.registry,
            mcp_manager: McpConnectionManager::new(),
            dynamic_tools: HashMap::new(),
            configured_specs: assembled.configured_specs,
        }
    }

    /// Route a tool call to the appropriate handler.
    ///
    /// Priority: built-in → MCP → dynamic.
    /// Returns `RouteResult::NotFound` if no handler is found.
    pub async fn route_tool_call(&self, tool_name: &str, args: serde_json::Value) -> RouteResult {
        // 1. Try built-in registry
        let builtin_kind = ToolKind::Builtin(tool_name.to_string());
        if self.registry.find(&builtin_kind).is_some() {
            return RouteResult::Handled(self.registry.dispatch(&builtin_kind, args).await);
        }

        // 2. Try MCP tool (format: mcp__{server}__{tool})
        if let Some((server, tool)) = parse_mcp_tool_name(tool_name) {
            let mcp_kind = ToolKind::Mcp {
                server: server.to_string(),
                tool: tool.to_string(),
            };
            if self.registry.find(&mcp_kind).is_some() {
                return RouteResult::Handled(self.registry.dispatch(&mcp_kind, args).await);
            }
            // Check if the MCP server is connected even without a registry entry
            let connected = self.mcp_manager.connected_servers().await;
            if connected.contains(&server.to_string()) {
                return RouteResult::Handled(Err(CodexError::new(
                    ErrorCode::ToolExecutionFailed,
                    format!("MCP tool '{tool_name}' found on server '{server}' but call delegation is not yet implemented"),
                )));
            }
        }

        // 3. Try dynamic tools — signal the caller to use the DynamicToolHandler protocol
        if self.dynamic_tools.contains_key(tool_name) {
            return RouteResult::DynamicTool(tool_name.to_string());
        }

        RouteResult::NotFound(tool_name.to_string())
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

    pub fn configured_specs(&self) -> &[ConfiguredToolSpec] {
        &self.configured_specs
    }

    /// List all available tools across built-in, MCP, and dynamic sources.
    pub fn list_all_tools(&self) -> Vec<ToolInfo> {
        let mut tools = Vec::new();

        // Built-in tools from configured specs when available.
        if self.configured_specs.is_empty() {
            for kind in self.registry.registered_kinds() {
                tools.push(ToolInfo {
                    name: kind.name(),
                    description: String::new(),
                    kind,
                    input_schema: None,
                });
            }
        } else {
            for configured in &self.configured_specs {
                tools.push(tool_info_from_spec(&configured.spec));
            }
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

    /// Collect all tool specs for sending to the model API.
    /// Aggregates built-in registry specs + dynamic tool specs.
    pub fn collect_tool_specs(&self) -> Vec<serde_json::Value> {
        let mut specs = if self.configured_specs.is_empty() {
            self.registry.collect_tool_specs()
        } else {
            self.configured_specs
                .iter()
                .map(|configured| tool_spec_to_json(&configured.spec))
                .collect()
        };

        // Add dynamic tools
        for spec in self.dynamic_tools.values() {
            specs.push(serde_json::json!({
                "type": "function",
                "name": spec.name,
                "description": spec.description,
                "parameters": spec.input_schema,
            }));
        }

        specs
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

fn tool_spec_to_json(spec: &ToolSpec) -> serde_json::Value {
    match spec {
        ToolSpec::Function {
            name,
            description,
            strict,
            parameters,
        } => serde_json::json!({
            "type": "function",
            "name": name,
            "description": description,
            "strict": strict,
            "parameters": parameters,
        }),
        ToolSpec::WebSearch {
            external_web_access,
        } => {
            let mut value = serde_json::json!({
                "type": "web_search",
            });
            if let Some(external_web_access) = external_web_access {
                value["external_web_access"] = serde_json::Value::Bool(*external_web_access);
            }
            value
        }
    }
}

fn tool_info_from_spec(spec: &ToolSpec) -> ToolInfo {
    match spec {
        ToolSpec::Function {
            name,
            description,
            parameters,
            ..
        } => ToolInfo {
            name: name.clone(),
            description: description.clone(),
            kind: ToolKind::Builtin(name.clone()),
            input_schema: Some(serde_json::to_value(parameters).unwrap_or(serde_json::Value::Null)),
        },
        ToolSpec::WebSearch { .. } => ToolInfo {
            name: "web_search".to_string(),
            description: String::new(),
            kind: ToolKind::Builtin("web_search".to_string()),
            input_schema: None,
        },
    }
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
            .await;
        match result {
            RouteResult::Handled(Ok(value)) => assert_eq!(value["handled_by"], "read_file"),
            other => panic!("expected Handled(Ok), got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn unknown_tool_returns_not_found() {
        let router = ToolRouter::new(ToolRegistry::new(), McpConnectionManager::new());
        let result = router
            .route_tool_call("nonexistent", serde_json::Value::Null)
            .await;
        assert!(matches!(result, RouteResult::NotFound(_)));
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

    #[tokio::test]
    async fn dynamic_tool_returns_dynamic_route_result() {
        let mut router = ToolRouter::new(ToolRegistry::new(), McpConnectionManager::new());
        router.register_dynamic_tool(DynamicToolSpec {
            name: "my_dyn".to_string(),
            description: "dynamic".to_string(),
            input_schema: serde_json::json!({}),
        });
        let result = router
            .route_tool_call("my_dyn", serde_json::json!({"x": 1}))
            .await;
        match result {
            RouteResult::DynamicTool(name) => assert_eq!(name, "my_dyn"),
            other => panic!("expected DynamicTool, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn builtin_takes_priority_over_dynamic() {
        let mut router = make_router_with_builtin("overlap");
        router.register_dynamic_tool(DynamicToolSpec {
            name: "overlap".to_string(),
            description: "also dynamic".to_string(),
            input_schema: serde_json::json!({}),
        });
        let result = router
            .route_tool_call("overlap", serde_json::json!({}))
            .await;
        // Built-in should win
        match result {
            RouteResult::Handled(Ok(value)) => assert_eq!(value["handled_by"], "overlap"),
            other => panic!("expected Handled(Ok) from builtin, got: {other:?}"),
        }
    }

    #[test]
    fn from_config_collects_stable_specs() {
        let config = crate::core::tools::spec::ToolsConfig::default();
        let router = ToolRouter::from_config(config, false);
        let names: Vec<String> = router
            .configured_specs()
            .iter()
            .map(|spec| spec.spec.name().to_string())
            .collect();

        assert!(names.contains(&"shell".to_string()));
        assert!(names.contains(&"apply_patch".to_string()));
        assert!(names.contains(&"list_dir".to_string()));
        assert!(names.contains(&"read_file".to_string()));
        assert!(names.contains(&"grep_files".to_string()));
        assert!(!names.contains(&"shell_command".to_string()));
    }

    #[test]
    fn list_all_tools_prefers_configured_specs_when_present() {
        let router = ToolRouter::from_config(
            crate::core::tools::spec::ToolsConfig {
                shell_enabled: false,
                shell_command_enabled: false,
                apply_patch_enabled: false,
                list_dir_enabled: false,
                read_file_enabled: false,
                grep_files_enabled: false,
                mcp_resources_enabled: false,
                unified_exec_enabled: true,
                update_plan_enabled: false,
                view_image_enabled: false,
                request_user_input_enabled: false,
                js_repl_enabled: false,
                test_sync_enabled: false,
                agent_jobs_enabled: false,
                agent_jobs_worker_enabled: false,
                collab_tools: false,
                search_tool: false,
                presentation_artifact: false,
                web_search_mode: None,
            },
            false,
        );
        let names: Vec<String> = router
            .list_all_tools()
            .into_iter()
            .map(|t| t.name)
            .collect();

        assert!(names.contains(&"exec_command".to_string()));
        assert!(names.contains(&"write_stdin".to_string()));
    }

    #[tokio::test]
    async fn from_config_router_still_routes_builtin_tools() {
        let router =
            ToolRouter::from_config(crate::core::tools::spec::ToolsConfig::default(), false);
        let result = router
            .route_tool_call(
                "read_file",
                serde_json::json!({"file_path": "/tmp/missing"}),
            )
            .await;

        assert!(matches!(result, RouteResult::Handled(_)));
    }

    #[tokio::test]
    async fn from_config_router_routes_update_plan_when_enabled() {
        let router = ToolRouter::from_config(
            crate::core::tools::spec::ToolsConfig {
                shell_enabled: false,
                shell_command_enabled: false,
                apply_patch_enabled: false,
                list_dir_enabled: false,
                read_file_enabled: false,
                grep_files_enabled: false,
                mcp_resources_enabled: false,
                unified_exec_enabled: false,
                update_plan_enabled: true,
                view_image_enabled: false,
                request_user_input_enabled: false,
                js_repl_enabled: false,
                test_sync_enabled: false,
                agent_jobs_enabled: false,
                agent_jobs_worker_enabled: false,
                collab_tools: false,
                search_tool: false,
                presentation_artifact: false,
                web_search_mode: None,
            },
            false,
        );
        let result = router
            .route_tool_call(
                "update_plan",
                serde_json::json!({
                    "plan": [
                        {"step": "wire plan tool", "status": "in_progress"}
                    ]
                }),
            )
            .await;

        match result {
            RouteResult::Handled(Ok(value)) => assert_eq!(value["status"], "Plan updated"),
            other => panic!("expected Handled(Ok) from update_plan, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn from_config_router_routes_view_image_when_enabled() {
        let dir = tempfile::tempdir().unwrap();
        let image_path = dir.path().join("fixture.png");
        std::fs::write(&image_path, b"png-bytes").unwrap();

        let router = ToolRouter::from_config(
            crate::core::tools::spec::ToolsConfig {
                shell_enabled: false,
                shell_command_enabled: false,
                apply_patch_enabled: false,
                list_dir_enabled: false,
                read_file_enabled: false,
                grep_files_enabled: false,
                mcp_resources_enabled: false,
                unified_exec_enabled: false,
                update_plan_enabled: false,
                view_image_enabled: true,
                request_user_input_enabled: false,
                js_repl_enabled: false,
                test_sync_enabled: false,
                agent_jobs_enabled: false,
                agent_jobs_worker_enabled: false,
                collab_tools: false,
                search_tool: false,
                presentation_artifact: false,
                web_search_mode: None,
            },
            false,
        );
        let result = router
            .route_tool_call(
                "view_image",
                serde_json::json!({"path": image_path.to_string_lossy()}),
            )
            .await;

        match result {
            RouteResult::Handled(Ok(value)) => {
                assert_eq!(value["size_bytes"], 9);
                assert_eq!(value["content"][0]["type"], "input_image");
            }
            other => panic!("expected Handled(Ok) from view_image, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn from_config_router_routes_request_user_input_when_enabled() {
        let router = ToolRouter::from_config(
            crate::core::tools::spec::ToolsConfig {
                shell_enabled: false,
                shell_command_enabled: false,
                apply_patch_enabled: false,
                list_dir_enabled: false,
                read_file_enabled: false,
                grep_files_enabled: false,
                mcp_resources_enabled: false,
                unified_exec_enabled: false,
                update_plan_enabled: false,
                view_image_enabled: false,
                request_user_input_enabled: true,
                js_repl_enabled: false,
                test_sync_enabled: false,
                agent_jobs_enabled: false,
                agent_jobs_worker_enabled: false,
                collab_tools: false,
                search_tool: false,
                presentation_artifact: false,
                web_search_mode: None,
            },
            false,
        );
        let result = router
            .route_tool_call(
                "request_user_input",
                serde_json::json!({
                    "questions": [
                        {
                            "text": "继续哪一阶段？",
                            "options": ["phase-2"]
                        }
                    ]
                }),
            )
            .await;

        match result {
            RouteResult::Handled(Err(err)) => {
                assert_eq!(err.code, ErrorCode::ToolExecutionFailed);
                assert!(err.message.contains("request_user_input is unavailable"));
            }
            other => panic!("expected Handled(Err) from request_user_input, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn from_config_router_routes_shell_command_when_enabled() {
        let tempdir = tempfile::tempdir().unwrap();
        let router = ToolRouter::from_config(
            crate::core::tools::spec::ToolsConfig {
                shell_enabled: false,
                shell_command_enabled: true,
                apply_patch_enabled: false,
                list_dir_enabled: false,
                read_file_enabled: false,
                grep_files_enabled: false,
                mcp_resources_enabled: false,
                unified_exec_enabled: false,
                update_plan_enabled: false,
                view_image_enabled: false,
                request_user_input_enabled: false,
                js_repl_enabled: false,
                test_sync_enabled: false,
                agent_jobs_enabled: false,
                agent_jobs_worker_enabled: false,
                collab_tools: false,
                search_tool: false,
                presentation_artifact: false,
                web_search_mode: None,
            },
            false,
        );
        let result = router
            .route_tool_call(
                "shell_command",
                serde_json::json!({
                    "command": "printf shell_command_ready",
                    "workdir": tempdir.path().to_string_lossy(),
                }),
            )
            .await;

        match result {
            RouteResult::Handled(Ok(value)) => {
                assert_eq!(value["exit_code"], 0);
                assert_eq!(value["stdout"], "shell_command_ready");
            }
            other => panic!("expected Handled(Ok) from shell_command, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn from_config_router_routes_presentation_artifact_when_enabled() {
        let router = ToolRouter::from_config(
            crate::core::tools::spec::ToolsConfig {
                shell_enabled: false,
                shell_command_enabled: false,
                apply_patch_enabled: false,
                list_dir_enabled: false,
                read_file_enabled: false,
                grep_files_enabled: false,
                mcp_resources_enabled: false,
                unified_exec_enabled: false,
                update_plan_enabled: false,
                view_image_enabled: false,
                request_user_input_enabled: false,
                js_repl_enabled: false,
                test_sync_enabled: false,
                agent_jobs_enabled: false,
                agent_jobs_worker_enabled: false,
                collab_tools: false,
                search_tool: false,
                presentation_artifact: true,
                web_search_mode: None,
            },
            false,
        );
        let result = router
            .route_tool_call(
                "presentation_artifact",
                serde_json::json!({
                    "action": "list",
                }),
            )
            .await;

        match result {
            RouteResult::Handled(Err(err)) => {
                assert_eq!(err.code, ErrorCode::ToolExecutionFailed);
                assert!(err
                    .message
                    .contains("presentation_artifact execution requires the artifact subsystem"));
            }
            other => panic!("expected Handled(Err) from presentation_artifact, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn from_config_router_routes_exec_command_when_enabled() {
        let router = ToolRouter::from_config(
            crate::core::tools::spec::ToolsConfig {
                shell_enabled: false,
                shell_command_enabled: false,
                apply_patch_enabled: false,
                list_dir_enabled: false,
                read_file_enabled: false,
                grep_files_enabled: false,
                mcp_resources_enabled: false,
                unified_exec_enabled: true,
                update_plan_enabled: false,
                view_image_enabled: false,
                request_user_input_enabled: false,
                js_repl_enabled: false,
                test_sync_enabled: false,
                agent_jobs_enabled: false,
                agent_jobs_worker_enabled: false,
                collab_tools: false,
                search_tool: false,
                presentation_artifact: false,
                web_search_mode: None,
            },
            false,
        );
        let result = router
            .route_tool_call(
                "exec_command",
                serde_json::json!({
                    "cmd": "printf unified_exec_ready",
                    "yield_time_ms": 250,
                }),
            )
            .await;

        match result {
            RouteResult::Handled(Ok(value)) => {
                let output = value
                    .as_str()
                    .expect("exec_command should serialize as string");
                assert!(output.contains("Process exited with code 0"));
                assert!(output.contains("unified_exec_ready"));
            }
            other => panic!("expected Handled(Ok) from exec_command, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn from_config_router_routes_write_stdin_when_enabled() {
        let router = ToolRouter::from_config(
            crate::core::tools::spec::ToolsConfig {
                shell_enabled: false,
                shell_command_enabled: false,
                apply_patch_enabled: false,
                list_dir_enabled: false,
                read_file_enabled: false,
                grep_files_enabled: false,
                mcp_resources_enabled: false,
                unified_exec_enabled: true,
                update_plan_enabled: false,
                view_image_enabled: false,
                request_user_input_enabled: false,
                js_repl_enabled: false,
                test_sync_enabled: false,
                agent_jobs_enabled: false,
                agent_jobs_worker_enabled: false,
                collab_tools: false,
                search_tool: false,
                presentation_artifact: false,
                web_search_mode: None,
            },
            false,
        );
        let result = router
            .route_tool_call(
                "write_stdin",
                serde_json::json!({
                    "session_id": 99999,
                }),
            )
            .await;

        match result {
            RouteResult::Handled(Err(err)) => {
                assert_eq!(err.code, ErrorCode::ToolExecutionFailed);
                assert!(err.message.contains("Unknown process id"));
            }
            other => panic!("expected Handled(Err) from write_stdin, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn from_config_router_routes_list_mcp_resources_when_enabled() {
        let router = ToolRouter::from_config(
            crate::core::tools::spec::ToolsConfig {
                shell_enabled: false,
                shell_command_enabled: false,
                apply_patch_enabled: false,
                list_dir_enabled: false,
                read_file_enabled: false,
                grep_files_enabled: false,
                mcp_resources_enabled: true,
                unified_exec_enabled: false,
                update_plan_enabled: false,
                view_image_enabled: false,
                request_user_input_enabled: false,
                js_repl_enabled: false,
                test_sync_enabled: false,
                agent_jobs_enabled: false,
                agent_jobs_worker_enabled: false,
                collab_tools: false,
                search_tool: false,
                presentation_artifact: false,
                web_search_mode: None,
            },
            false,
        );
        let result = router
            .route_tool_call("list_mcp_resources", serde_json::json!({}))
            .await;

        match result {
            RouteResult::Handled(Ok(value)) => {
                assert_eq!(value["resources"], serde_json::json!([]));
                assert!(value.get("server").is_none());
            }
            other => panic!("expected Handled(Ok) from list_mcp_resources, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn from_config_router_routes_list_mcp_resource_templates_when_enabled() {
        let router = ToolRouter::from_config(
            crate::core::tools::spec::ToolsConfig {
                shell_enabled: false,
                shell_command_enabled: false,
                apply_patch_enabled: false,
                list_dir_enabled: false,
                read_file_enabled: false,
                grep_files_enabled: false,
                mcp_resources_enabled: true,
                unified_exec_enabled: false,
                update_plan_enabled: false,
                view_image_enabled: false,
                request_user_input_enabled: false,
                js_repl_enabled: false,
                test_sync_enabled: false,
                agent_jobs_enabled: false,
                agent_jobs_worker_enabled: false,
                collab_tools: false,
                search_tool: false,
                presentation_artifact: false,
                web_search_mode: None,
            },
            false,
        );
        let result = router
            .route_tool_call("list_mcp_resource_templates", serde_json::json!({}))
            .await;

        match result {
            RouteResult::Handled(Ok(value)) => {
                assert_eq!(value["resourceTemplates"], serde_json::json!([]));
                assert!(value.get("server").is_none());
            }
            other => {
                panic!("expected Handled(Ok) from list_mcp_resource_templates, got: {other:?}")
            }
        }
    }

    #[tokio::test]
    async fn from_config_router_routes_read_mcp_resource_when_enabled() {
        let router = ToolRouter::from_config(
            crate::core::tools::spec::ToolsConfig {
                shell_enabled: false,
                shell_command_enabled: false,
                apply_patch_enabled: false,
                list_dir_enabled: false,
                read_file_enabled: false,
                grep_files_enabled: false,
                mcp_resources_enabled: true,
                unified_exec_enabled: false,
                update_plan_enabled: false,
                view_image_enabled: false,
                request_user_input_enabled: false,
                js_repl_enabled: false,
                test_sync_enabled: false,
                agent_jobs_enabled: false,
                agent_jobs_worker_enabled: false,
                collab_tools: false,
                search_tool: false,
                presentation_artifact: false,
                web_search_mode: None,
            },
            false,
        );
        let result = router
            .route_tool_call(
                "read_mcp_resource",
                serde_json::json!({
                    "server": "demo",
                    "uri": "app://resource/demo"
                }),
            )
            .await;

        match result {
            RouteResult::Handled(Err(err)) => {
                assert_eq!(err.code, ErrorCode::ToolExecutionFailed);
                assert!(err.message.contains("requires MCP connection manager"));
            }
            other => panic!("expected Handled(Err) from read_mcp_resource, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn from_config_router_routes_search_tool_bm25_when_enabled() {
        let router = ToolRouter::from_config(
            crate::core::tools::spec::ToolsConfig {
                shell_enabled: false,
                shell_command_enabled: false,
                apply_patch_enabled: false,
                list_dir_enabled: false,
                read_file_enabled: false,
                grep_files_enabled: false,
                mcp_resources_enabled: false,
                unified_exec_enabled: false,
                update_plan_enabled: false,
                view_image_enabled: false,
                request_user_input_enabled: false,
                js_repl_enabled: false,
                test_sync_enabled: false,
                agent_jobs_enabled: false,
                agent_jobs_worker_enabled: false,
                collab_tools: false,
                search_tool: true,
                presentation_artifact: false,
                web_search_mode: None,
            },
            false,
        );
        let result = router
            .route_tool_call(
                "search_tool_bm25",
                serde_json::json!({
                    "query": "calendar",
                    "limit": 5,
                }),
            )
            .await;

        match result {
            RouteResult::Handled(Ok(value)) => {
                assert_eq!(value["query"], "calendar");
                assert_eq!(value["total_tools"], 0);
                assert_eq!(value["active_selected_tools"], serde_json::json!([]));
                assert_eq!(value["tools"], serde_json::json!([]));
            }
            other => panic!("expected Handled(Ok) from search_tool_bm25, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn from_config_router_routes_js_repl_tools_when_enabled() {
        let router = ToolRouter::from_config(
            crate::core::tools::spec::ToolsConfig {
                shell_enabled: false,
                shell_command_enabled: false,
                apply_patch_enabled: false,
                list_dir_enabled: false,
                read_file_enabled: false,
                grep_files_enabled: false,
                mcp_resources_enabled: false,
                unified_exec_enabled: false,
                update_plan_enabled: false,
                view_image_enabled: false,
                request_user_input_enabled: false,
                js_repl_enabled: true,
                test_sync_enabled: false,
                agent_jobs_enabled: false,
                agent_jobs_worker_enabled: false,
                collab_tools: false,
                search_tool: false,
                presentation_artifact: false,
                web_search_mode: None,
            },
            false,
        );

        let repl_result = router
            .route_tool_call("js_repl", serde_json::json!({"code": "1 + 1"}))
            .await;
        match repl_result {
            RouteResult::Handled(Err(err)) => {
                assert_eq!(err.code, ErrorCode::ToolExecutionFailed);
                assert!(err
                    .message
                    .contains("js_repl requires the JavaScript REPL runtime"));
            }
            other => panic!("expected Handled(Err) from js_repl, got: {other:?}"),
        }

        let reset_result = router
            .route_tool_call("js_repl_reset", serde_json::json!({}))
            .await;
        match reset_result {
            RouteResult::Handled(Err(err)) => {
                assert_eq!(err.code, ErrorCode::ToolExecutionFailed);
                assert!(err
                    .message
                    .contains("js_repl_reset requires the JavaScript REPL runtime"));
            }
            other => panic!("expected Handled(Err) from js_repl_reset, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn from_config_router_routes_test_sync_tool_when_enabled() {
        let router = ToolRouter::from_config(
            crate::core::tools::spec::ToolsConfig {
                shell_enabled: false,
                shell_command_enabled: false,
                apply_patch_enabled: false,
                list_dir_enabled: false,
                read_file_enabled: false,
                grep_files_enabled: false,
                mcp_resources_enabled: false,
                unified_exec_enabled: false,
                update_plan_enabled: false,
                view_image_enabled: false,
                request_user_input_enabled: false,
                js_repl_enabled: false,
                test_sync_enabled: true,
                agent_jobs_enabled: false,
                agent_jobs_worker_enabled: false,
                collab_tools: false,
                search_tool: false,
                presentation_artifact: false,
                web_search_mode: None,
            },
            false,
        );

        let result = router
            .route_tool_call(
                "test_sync_tool",
                serde_json::json!({
                    "sleep_before_ms": 1
                }),
            )
            .await;

        match result {
            RouteResult::Handled(Ok(value)) => assert_eq!(value["status"], "ok"),
            other => panic!("expected Handled(Ok) from test_sync_tool, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn from_config_router_routes_spawn_agents_on_csv_when_enabled() {
        let router = ToolRouter::from_config(
            crate::core::tools::spec::ToolsConfig {
                shell_enabled: false,
                shell_command_enabled: false,
                apply_patch_enabled: false,
                list_dir_enabled: false,
                read_file_enabled: false,
                grep_files_enabled: false,
                mcp_resources_enabled: false,
                unified_exec_enabled: false,
                update_plan_enabled: false,
                view_image_enabled: false,
                request_user_input_enabled: false,
                js_repl_enabled: false,
                test_sync_enabled: false,
                agent_jobs_enabled: true,
                agent_jobs_worker_enabled: true,
                collab_tools: false,
                search_tool: false,
                presentation_artifact: false,
                web_search_mode: None,
            },
            false,
        );

        let result = router
            .route_tool_call(
                "spawn_agents_on_csv",
                serde_json::json!({
                    "csv_path": "/tmp/demo.csv",
                    "instruction": "process row",
                    "max_concurrency": 3,
                }),
            )
            .await;

        match result {
            RouteResult::Handled(Err(err)) => {
                assert_eq!(err.code, ErrorCode::ToolExecutionFailed);
                assert!(err
                    .message
                    .contains("spawn_agents_on_csv requires the agent subsystem"));
                assert!(err.message.contains("csv=/tmp/demo.csv"));
                assert!(err.message.contains("concurrency=3"));
            }
            other => panic!("expected Handled(Err) from spawn_agents_on_csv, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn from_config_router_routes_report_agent_job_result_when_enabled() {
        let router = ToolRouter::from_config(
            crate::core::tools::spec::ToolsConfig {
                shell_enabled: false,
                shell_command_enabled: false,
                apply_patch_enabled: false,
                list_dir_enabled: false,
                read_file_enabled: false,
                grep_files_enabled: false,
                mcp_resources_enabled: false,
                unified_exec_enabled: false,
                update_plan_enabled: false,
                view_image_enabled: false,
                request_user_input_enabled: false,
                js_repl_enabled: false,
                test_sync_enabled: false,
                agent_jobs_enabled: true,
                agent_jobs_worker_enabled: true,
                collab_tools: false,
                search_tool: false,
                presentation_artifact: false,
                web_search_mode: None,
            },
            false,
        );

        let result = router
            .route_tool_call(
                "report_agent_job_result",
                serde_json::json!({
                    "job_id": "job-1",
                    "item_id": "item-1",
                    "result": {"ok": true},
                }),
            )
            .await;

        match result {
            RouteResult::Handled(Err(err)) => {
                assert_eq!(err.code, ErrorCode::ToolExecutionFailed);
                assert!(err
                    .message
                    .contains("report_agent_job_result requires the agent subsystem"));
            }
            other => panic!("expected Handled(Err) from report_agent_job_result, got: {other:?}"),
        }
    }

    #[test]
    fn from_config_collects_cached_web_search_spec_when_enabled() {
        let router = ToolRouter::from_config(
            crate::core::tools::spec::ToolsConfig {
                shell_enabled: false,
                shell_command_enabled: false,
                apply_patch_enabled: false,
                list_dir_enabled: false,
                read_file_enabled: false,
                grep_files_enabled: false,
                mcp_resources_enabled: false,
                unified_exec_enabled: false,
                update_plan_enabled: false,
                view_image_enabled: false,
                request_user_input_enabled: false,
                js_repl_enabled: false,
                test_sync_enabled: false,
                agent_jobs_enabled: false,
                agent_jobs_worker_enabled: false,
                collab_tools: false,
                search_tool: false,
                presentation_artifact: false,
                web_search_mode: Some(crate::protocol::types::WebSearchMode::Cached),
            },
            false,
        );

        let specs = router.collect_tool_specs();
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0]["type"], "web_search");
        assert_eq!(specs[0]["external_web_access"], false);

        let names: Vec<String> = router
            .list_all_tools()
            .into_iter()
            .map(|t| t.name)
            .collect();
        assert_eq!(names, vec!["web_search".to_string()]);
    }

    #[test]
    fn from_config_collects_live_web_search_spec_when_enabled() {
        let router = ToolRouter::from_config(
            crate::core::tools::spec::ToolsConfig {
                shell_enabled: false,
                shell_command_enabled: false,
                apply_patch_enabled: false,
                list_dir_enabled: false,
                read_file_enabled: false,
                grep_files_enabled: false,
                mcp_resources_enabled: false,
                unified_exec_enabled: false,
                update_plan_enabled: false,
                view_image_enabled: false,
                request_user_input_enabled: false,
                js_repl_enabled: false,
                test_sync_enabled: false,
                agent_jobs_enabled: false,
                agent_jobs_worker_enabled: false,
                collab_tools: false,
                search_tool: false,
                presentation_artifact: false,
                web_search_mode: Some(crate::protocol::types::WebSearchMode::Live),
            },
            false,
        );

        let specs = router.collect_tool_specs();
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0]["type"], "web_search");
        assert_eq!(specs[0]["external_web_access"], true);
    }
}
