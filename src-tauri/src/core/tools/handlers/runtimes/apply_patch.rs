//! Apply-patch runtime — applies unified diffs via `git apply`.

use crate::core::tools::sandboxing::{SandboxAttempt, ToolError, ToolRuntime};
use crate::protocol::error::{CodexError, ErrorCode};

pub struct ApplyPatchRuntime;

#[async_trait::async_trait]
impl ToolRuntime for ApplyPatchRuntime {
    async fn run(
        &self,
        args: serde_json::Value,
        _attempt: &SandboxAttempt,
    ) -> Result<serde_json::Value, ToolError> {
        let patch = args
            .get("patch")
            .or_else(|| args.get("input"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if patch.is_empty() {
            return Err(ToolError::Codex(CodexError::new(
                ErrorCode::InvalidInput,
                "empty patch",
            )));
        }

        let mut cmd = tokio::process::Command::new("git");
        cmd.args(["apply", "--verbose", "-"]);
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| {
            ToolError::Codex(CodexError::new(ErrorCode::ToolExecutionFailed, format!("{e}")))
        })?;

        if let Some(mut stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            let _ = stdin.write_all(patch.as_bytes()).await;
        }

        let output = child.wait_with_output().await.map_err(|e| {
            ToolError::Codex(CodexError::new(ErrorCode::ToolExecutionFailed, format!("{e}")))
        })?;

        Ok(serde_json::json!({
            "exit_code": output.status.code().unwrap_or(-1),
            "stdout": String::from_utf8_lossy(&output.stdout),
            "stderr": String::from_utf8_lossy(&output.stderr),
        }))
    }
}
