//! Unified exec runtime — runs commands with PTY session management.

use crate::core::tools::sandboxing::{SandboxAttempt, ToolError, ToolRuntime};
use crate::protocol::error::{CodexError, ErrorCode};

pub struct UnifiedExecRuntime;

#[async_trait::async_trait]
impl ToolRuntime for UnifiedExecRuntime {
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
        let mut cmd = tokio::process::Command::new(&shell);
        cmd.arg("-c").arg(cmd_str);
        if let Some(wd) = args.get("workdir").and_then(|v| v.as_str()) {
            cmd.current_dir(wd);
        }
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let timeout_ms = args
            .get("timeout_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(120_000);

        let child = cmd.spawn().map_err(|e| {
            ToolError::Codex(CodexError::new(ErrorCode::ToolExecutionFailed, format!("{e}")))
        })?;

        let output = match tokio::time::timeout(
            std::time::Duration::from_millis(timeout_ms),
            child.wait_with_output(),
        )
        .await
        {
            Ok(Ok(o)) => o,
            Ok(Err(e)) => {
                return Err(ToolError::Codex(CodexError::new(
                    ErrorCode::ToolExecutionFailed,
                    format!("{e}"),
                )));
            }
            Err(_) => {
                return Ok(serde_json::json!({
                    "exit_code": -1,
                    "output": "command timed out",
                    "timed_out": true,
                }));
            }
        };

        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );

        Ok(serde_json::json!({
            "exit_code": output.status.code().unwrap_or(-1),
            "output": combined,
        }))
    }
}
