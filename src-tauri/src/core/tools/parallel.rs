//! Parallel / serial tool call execution runtime.
//!
//! Adapted from Codex `tools/parallel.rs`. Provides `ToolCallRuntime` that
//! dispatches tool calls with parallel-vs-serial locking semantics.

use crate::core::tools::{ToolKind, ToolRegistry};
use crate::protocol::error::CodexError;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Runtime that dispatches tool calls, using a read/write lock to allow
/// parallel-safe tools to run concurrently while serial tools get exclusive
/// access.
pub struct ToolCallRuntime {
    registry: Arc<ToolRegistry>,
    /// Tools that support parallel execution acquire a read lock;
    /// serial tools acquire a write lock.
    lock: Arc<RwLock<()>>,
    /// Tool names that support parallel execution.
    parallel_tools: Vec<String>,
}

impl ToolCallRuntime {
    pub fn new(registry: Arc<ToolRegistry>, parallel_tools: Vec<String>) -> Self {
        Self {
            registry,
            lock: Arc::new(RwLock::new(())),
            parallel_tools,
        }
    }

    /// Dispatch a tool call, respecting parallel/serial semantics.
    pub async fn dispatch(
        &self,
        kind: &ToolKind,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, CodexError> {
        let supports_parallel = self.parallel_tools.contains(&kind.name());

        if supports_parallel {
            let _guard = self.lock.read().await;
            self.registry.dispatch(kind, args).await
        } else {
            let _guard = self.lock.write().await;
            self.registry.dispatch(kind, args).await
        }
    }
}
