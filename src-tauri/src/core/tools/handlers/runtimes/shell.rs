//! Shell runtime — executes commands via the system shell.

use crate::core::tools::sandboxing::{SandboxAttempt, ToolError, ToolRuntime};
use crate::protocol::error::{CodexError, ErrorCode};

pub struct ShellRuntime;

#[async_trait::async_trait]
impl ToolRuntime for ShellRuntime {
    async fn run(
        &self,
        args: serde_json::Value,
        _attempt: &SandboxAttempt,
    ) -> Result<serde_json::Value, ToolError> {
        let command = args
            .get("command")
            .and_then(|v| {
                v.as_array()
                    .map(|a| a.iter().filter_map(|s| s.as_str().map(String::from)).collect::<Vec<_>>())
            })
            .unwrap_or_default();

        if command.is_empty() {
            return Err(ToolError::Codex(CodexError::new(
                ErrorCode::InvalidInput,
                "empty command",
            )));
        }

        let mut cmd = tokio::process::Command::new(&command[0]);
        cmd.args(&command[1..]);
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let output = cmd.output().await.map_err(|e| {
            ToolError::Codex(CodexError::new(
                ErrorCode::ToolExecutionFailed,
                format!("{e}"),
            ))
        })?;

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
