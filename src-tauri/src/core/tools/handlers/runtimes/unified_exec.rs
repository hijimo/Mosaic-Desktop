//! Unified exec runtime — runs commands via PTY session management.

use std::collections::HashMap;
use std::sync::Arc;

use crate::core::tools::sandboxing::{SandboxAttempt, ToolError};
use crate::core::unified_exec::{apply_exec_env, UnifiedExecProcessManager};
use crate::protocol::error::{CodexError, ErrorCode};

/// Runtime that delegates to the shared `UnifiedExecProcessManager`.
pub struct UnifiedExecRuntime {
    manager: Arc<UnifiedExecProcessManager>,
}

impl UnifiedExecRuntime {
    pub fn new(manager: Arc<UnifiedExecProcessManager>) -> Self {
        Self { manager }
    }
}

#[async_trait::async_trait]
impl crate::core::tools::sandboxing::ToolRuntime for UnifiedExecRuntime {
    async fn run(
        &self,
        args: serde_json::Value,
        _attempt: &SandboxAttempt,
    ) -> Result<serde_json::Value, ToolError> {
        let cmd_str = args
            .get("cmd")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if cmd_str.is_empty() {
            return Err(ToolError::Codex(CodexError::new(
                ErrorCode::InvalidInput,
                "empty cmd",
            )));
        }

        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
        let tty = args
            .get("tty")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let command = vec![shell, "-c".to_string(), cmd_str.to_string()];
        let cwd = args
            .get("workdir")
            .and_then(|v| v.as_str())
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        let env = apply_exec_env(HashMap::new());

        let process = self
            .manager
            .open_session_with_exec_env(&command, &cwd, &env, tty)
            .await
            .map_err(|e| {
                ToolError::Codex(CodexError::new(
                    ErrorCode::ToolExecutionFailed,
                    format!("{e}"),
                ))
            })?;

        // Wait for completion with a timeout
        let timeout_ms = args
            .get("timeout_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(120_000);

        let deadline =
            tokio::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
        let handles = process.output_handles();
        let collected =
            crate::core::unified_exec::UnifiedExecProcessManager::collect_output_until_deadline(
                &handles.output_buffer,
                &handles.output_notify,
                &handles.output_closed,
                &handles.output_closed_notify,
                &handles.cancellation_token,
                deadline,
            )
            .await;

        let exit_code = process.exit_code().unwrap_or(-1);
        let output = String::from_utf8_lossy(&collected).to_string();

        Ok(serde_json::json!({
            "exit_code": exit_code,
            "output": output,
        }))
    }
}
