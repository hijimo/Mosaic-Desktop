//! Tool invocation context types.
//!
//! Adapted from Codex `tools/context.rs`. Simplified to use Mosaic's own
//! type system without external protocol crates.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Shared mutable tracker for file diffs accumulated during a turn.
pub type SharedTurnDiffTracker = Arc<Mutex<TurnDiffTracker>>;

/// Minimal diff tracker that records file paths modified during a turn.
#[derive(Debug, Default)]
pub struct TurnDiffTracker {
    pub modified_files: Vec<String>,
}

impl TurnDiffTracker {
    pub fn record(&mut self, path: impl Into<String>) {
        self.modified_files.push(path.into());
    }
}

/// Where a tool call originated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolCallSource {
    Direct,
    JsRepl,
}

/// Describes the payload of a single tool invocation.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ToolPayload {
    /// Standard JSON function call.
    Function { arguments: String },
    /// Freeform (custom) tool input (e.g. `apply_patch`, `js_repl`).
    Custom { input: String },
    /// Shell-style command.
    Shell { command: Vec<String> },
    /// MCP tool call.
    Mcp {
        server: String,
        tool: String,
        raw_arguments: String,
    },
}

/// Result of executing a tool.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ToolOutput {
    /// Textual output (most tools).
    Text { content: String, success: bool },
    /// Structured JSON output.
    Json {
        value: serde_json::Value,
        success: bool,
    },
}

impl ToolOutput {
    pub fn text(content: impl Into<String>) -> Self {
        Self::Text {
            content: content.into(),
            success: true,
        }
    }

    pub fn error(content: impl Into<String>) -> Self {
        Self::Text {
            content: content.into(),
            success: false,
        }
    }

    pub fn is_success(&self) -> bool {
        match self {
            Self::Text { success, .. } | Self::Json { success, .. } => *success,
        }
    }
}
