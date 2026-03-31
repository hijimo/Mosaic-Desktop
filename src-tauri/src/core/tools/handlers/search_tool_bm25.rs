use async_trait::async_trait;
use serde::Deserialize;

use crate::core::tools::{ToolHandler, ToolKind};
use crate::protocol::error::{CodexError, ErrorCode};

pub struct SearchToolBm25Handler;

pub const SEARCH_TOOL_BM25_TOOL_NAME: &str = "search_tool_bm25";
pub const SEARCH_TOOL_BM25_DEFAULT_LIMIT: usize = 8;

fn default_limit() -> usize {
    SEARCH_TOOL_BM25_DEFAULT_LIMIT
}

#[derive(Deserialize)]
struct SearchToolBm25Args {
    query: String,
    #[serde(default = "default_limit")]
    limit: usize,
}

/// Represents a tool entry for BM25 indexing.
#[derive(Clone)]
pub struct ToolEntry {
    pub name: String,
    pub server_name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub connector_name: Option<String>,
    pub input_keys: Vec<String>,
    pub search_text: String,
}

impl ToolEntry {
    pub fn build_search_text(
        name: &str,
        server_name: &str,
        title: Option<&str>,
        description: Option<&str>,
        input_keys: &[String],
    ) -> String {
        let mut parts = vec![name.to_string(), server_name.to_string()];
        if let Some(t) = title {
            if !t.trim().is_empty() {
                parts.push(t.to_string());
            }
        }
        if let Some(d) = description {
            if !d.trim().is_empty() {
                parts.push(d.to_string());
            }
        }
        parts.extend(input_keys.iter().cloned());
        parts.join(" ")
    }
}

#[async_trait]
impl ToolHandler for SearchToolBm25Handler {
    fn matches_kind(&self, kind: &ToolKind) -> bool {
        matches!(kind, ToolKind::Builtin(n) if n == SEARCH_TOOL_BM25_TOOL_NAME)
    }

    fn kind(&self) -> ToolKind {
        ToolKind::Builtin(SEARCH_TOOL_BM25_TOOL_NAME.to_string())
    }

    async fn handle(&self, args: serde_json::Value) -> Result<serde_json::Value, CodexError> {
        let params: SearchToolBm25Args = serde_json::from_value(args).map_err(|e| {
            CodexError::new(
                ErrorCode::InvalidInput,
                format!("invalid search_tool_bm25 args: {e}"),
            )
        })?;

        let query = params.query.trim();
        if query.is_empty() {
            return Err(CodexError::new(
                ErrorCode::InvalidInput,
                "query must not be empty",
            ));
        }
        if params.limit == 0 {
            return Err(CodexError::new(
                ErrorCode::InvalidInput,
                "limit must be greater than zero",
            ));
        }

        // Full implementation builds a BM25 search engine from MCP tools via:
        //   1. session.services.mcp_connection_manager.list_all_tools()
        //   2. Build ToolEntry index
        //   3. SearchEngineBuilder::<usize>::with_documents(Language::English, documents).build()
        //   4. search_engine.search(query, limit)
        // TODO: wire to actual MCP connection manager and bm25 crate

        Ok(serde_json::json!({
            "query": query,
            "total_tools": 0,
            "active_selected_tools": [],
            "tools": [],
        }))
    }
}
