use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::core::tools::{ToolHandler, ToolKind};
use crate::protocol::error::{CodexError, ErrorCode};

pub struct McpResourceHandler;

#[derive(Debug, Deserialize, Default)]
struct ListResourcesArgs {
    #[serde(default)]
    server: Option<String>,
    #[serde(default)]
    cursor: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct ListResourceTemplatesArgs {
    #[serde(default)]
    server: Option<String>,
    #[serde(default)]
    cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ReadResourceArgs {
    server: String,
    uri: String,
}

#[derive(Debug, Serialize)]
struct ResourceWithServer {
    server: String,
    uri: String,
    name: Option<String>,
    description: Option<String>,
    mime_type: Option<String>,
}

#[derive(Debug, Serialize)]
struct ResourceTemplateWithServer {
    server: String,
    uri_template: String,
    name: Option<String>,
    description: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ListResourcesPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    server: Option<String>,
    resources: Vec<ResourceWithServer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_cursor: Option<String>,
}

impl ListResourcesPayload {
    /// Build payload from a single server's resource list.
    fn from_single_server(
        server: String,
        resources: Vec<ResourceWithServer>,
        next_cursor: Option<String>,
    ) -> Self {
        Self { server: Some(server), resources, next_cursor }
    }

    /// Build payload aggregating resources from all servers.
    fn from_all_servers(resources_by_server: std::collections::HashMap<String, Vec<ResourceWithServer>>) -> Self {
        let mut entries: Vec<_> = resources_by_server.into_iter().collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        let resources = entries.into_iter().flat_map(|(_, r)| r).collect();
        Self { server: None, resources, next_cursor: None }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ListResourceTemplatesPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    server: Option<String>,
    resource_templates: Vec<ResourceTemplateWithServer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_cursor: Option<String>,
}

impl ListResourceTemplatesPayload {
    fn from_single_server(
        server: String,
        templates: Vec<ResourceTemplateWithServer>,
        next_cursor: Option<String>,
    ) -> Self {
        Self { server: Some(server), resource_templates: templates, next_cursor }
    }

    fn from_all_servers(templates_by_server: std::collections::HashMap<String, Vec<ResourceTemplateWithServer>>) -> Self {
        let mut entries: Vec<_> = templates_by_server.into_iter().collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        let resource_templates = entries.into_iter().flat_map(|(_, t)| t).collect();
        Self { server: None, resource_templates, next_cursor: None }
    }
}

#[async_trait]
impl ToolHandler for McpResourceHandler {
    fn matches_kind(&self, kind: &ToolKind) -> bool {
        matches!(kind, ToolKind::Builtin(n) if matches!(n.as_str(),
            "list_mcp_resources" | "list_mcp_resource_templates" | "read_mcp_resource"
        ))
    }

    fn kind(&self) -> ToolKind {
        ToolKind::Builtin("list_mcp_resources".to_string())
    }

    async fn handle(&self, args: serde_json::Value) -> Result<serde_json::Value, CodexError> {
        // Determine which operation based on args shape
        if args.get("uri").is_some() {
            let params: ReadResourceArgs = serde_json::from_value(args).map_err(|e| {
                CodexError::new(ErrorCode::InvalidInput, format!("invalid read_resource args: {e}"))
            })?;
            return handle_read_resource(params).await;
        }

        // list_resources or list_resource_templates
        let params: ListResourcesArgs = serde_json::from_value(args).map_err(|e| {
            CodexError::new(ErrorCode::InvalidInput, format!("invalid list_resources args: {e}"))
        })?;
        handle_list_resources(params).await
    }
}

async fn handle_list_resources(params: ListResourcesArgs) -> Result<serde_json::Value, CodexError> {
    // Full implementation iterates MCP servers via McpConnectionManager.
    // TODO: wire to actual MCP connection manager
    let payload = ListResourcesPayload {
        server: params.server,
        resources: Vec::new(),
        next_cursor: None,
    };
    serde_json::to_value(&payload).map_err(|e| {
        CodexError::new(ErrorCode::ToolExecutionFailed, format!("serialization error: {e}"))
    })
}

async fn handle_read_resource(params: ReadResourceArgs) -> Result<serde_json::Value, CodexError> {
    // Full implementation calls server.read_resource() via McpConnectionManager.
    // TODO: wire to actual MCP connection manager
    Err(CodexError::new(
        ErrorCode::ToolExecutionFailed,
        format!("read_mcp_resource for server={} uri={} requires MCP connection manager", params.server, params.uri),
    ))
}
